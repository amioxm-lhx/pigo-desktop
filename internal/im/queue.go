package im

import (
	"context"
	"sync"
)

// DefaultQueueDepth is how many messages may wait behind the one a chat is
// currently running.
//
// The number is a judgement, not a derived value. Three covers the realistic
// pattern ("收到" aside, a follow-up plus a correction plus one more), while
// staying small enough that a user who fired off eight messages understands the
// rejection as feedback on their own burst rather than as a broken bot. Raising
// it trades a longer invisible delay for fewer rejections, which is the wrong
// trade: a queued message the user has forgotten about is worse than an honest
// "try later".
const DefaultQueueDepth = 3

// SubmitResult reports whether the queue accepted a message.
type SubmitResult int

const (
	// Accepted means the message is queued (or already running).
	Accepted SubmitResult = iota
	// RejectedFull means the chat's backlog is at capacity.
	RejectedFull
	// RejectedClosed means the queue is shutting down.
	RejectedClosed
)

// HandlerFunc processes one message. It runs on the chat's lane goroutine, so
// messages from one chat are strictly serialized while different chats proceed
// independently.
type HandlerFunc func(ctx context.Context, in Inbound)

// Queue fans inbound messages into per-chat FIFO lanes with a bounded backlog.
//
// Why per chat: one chat maps to one pigo session, and a session runs one prompt
// at a time (serve.ErrRunActive). Serializing per chat keeps that invariant
// structural instead of turning a second message into an error the user cannot
// see or act on.
//
// Why bounded: a chatty user on a slow task enqueues faster than the agent
// drains. An unbounded queue would grow silently and then run every stale
// message long after it stopped being relevant. The bound converts that into
// immediate, truthful feedback, which is the whole point: the channel says
// "too many queued, try later" while it is still true, instead of pretending to
// accept work it will get to in ten minutes.
//
// The active message does not count toward depth: depth is the waiting backlog,
// so a chat running one prompt may have depth more waiting.
type Queue struct {
	depth   int
	handler HandlerFunc
	onFull  func(in Inbound, backlog int)
	onPanic func(in Inbound, recovered any)

	mu     sync.Mutex
	lanes  map[string]*lane
	closed bool

	ctx    context.Context
	cancel context.CancelFunc
	wg     sync.WaitGroup
}

// lane is one chat's backlog plus whether its worker is alive. The worker exits
// as soon as the backlog drains, so idle chats cost nothing; `running` is the
// handshake that prevents two workers from racing over one backlog.
type lane struct {
	pending []Inbound
	running bool
}

// NewQueue builds a queue. depth <= 0 selects DefaultQueueDepth. handler is
// required. onFull, when set, runs synchronously on the submitting goroutine and
// must therefore stay cheap — it is how the caller learns about a rejection, and
// it is called once per rejected message rather than once per burst (the Router
// throttles the user-visible reply).
func NewQueue(depth int, handler HandlerFunc, onFull func(in Inbound, backlog int)) *Queue {
	if depth <= 0 {
		depth = DefaultQueueDepth
	}
	ctx, cancel := context.WithCancel(context.Background())
	return &Queue{
		depth:   depth,
		handler: handler,
		onFull:  onFull,
		lanes:   make(map[string]*lane),
		ctx:     ctx,
		cancel:  cancel,
	}
}

// Depth reports the configured backlog size.
func (q *Queue) Depth() int { return q.depth }

// Submit enqueues in on its chat's lane. It never blocks on the handler: it
// either queues the message, starts a worker, or reports a rejection.
func (q *Queue) Submit(in Inbound) SubmitResult {
	key := in.Chat.Key()
	if key == "" {
		// A message with no chat cannot be answered and cannot be bound; treat
		// it as a rejection so the caller logs it instead of queueing garbage.
		return RejectedFull
	}

	q.mu.Lock()
	if q.closed {
		q.mu.Unlock()
		return RejectedClosed
	}
	ln := q.lanes[key]
	if ln == nil {
		ln = &lane{}
		q.lanes[key] = ln
	}
	if len(ln.pending) >= q.depth {
		backlog := len(ln.pending)
		q.mu.Unlock()
		if q.onFull != nil {
			q.onFull(in, backlog)
		}
		return RejectedFull
	}
	ln.pending = append(ln.pending, in)
	start := !ln.running
	ln.running = true
	q.mu.Unlock()

	if start {
		q.wg.Add(1)
		go q.worker(key)
	}
	return Accepted
}

// Backlog reports how many messages are waiting for a chat (excluding the one
// currently being handled).
func (q *Queue) Backlog(chatKey string) int {
	q.mu.Lock()
	defer q.mu.Unlock()
	if ln := q.lanes[chatKey]; ln != nil {
		return len(ln.pending)
	}
	return 0
}

// Busy reports whether a chat has a worker running.
func (q *Queue) Busy(chatKey string) bool {
	q.mu.Lock()
	defer q.mu.Unlock()
	if ln := q.lanes[chatKey]; ln != nil {
		return ln.running
	}
	return false
}

// Close stops accepting new messages and cancels in-flight handlers. It does not
// wait for them; callers that need a clean shutdown should cancel their own
// context and call Wait.
func (q *Queue) Close() {
	q.mu.Lock()
	if q.closed {
		q.mu.Unlock()
		return
	}
	q.closed = true
	q.mu.Unlock()
	q.cancel()
}

// Wait blocks until every lane worker has exited.
func (q *Queue) Wait() { q.wg.Wait() }

func (q *Queue) worker(key string) {
	defer q.wg.Done()
	for {
		q.mu.Lock()
		ln := q.lanes[key]
		if ln == nil || len(ln.pending) == 0 {
			if ln != nil {
				ln.running = false
				// Reclaim the lane so a long-lived process does not accumulate
				// one empty struct per chat that ever spoke to it.
				delete(q.lanes, key)
			}
			q.mu.Unlock()
			return
		}
		in := ln.pending[0]
		ln.pending[0] = Inbound{} // release the reference before the next pop
		ln.pending = ln.pending[1:]
		q.mu.Unlock()

		q.dispatch(in)
	}
}

// SetPanicHandler installs the callback invoked when a handler panics. It must
// be set before messages are submitted (the Router sets it at construction).
func (q *Queue) SetPanicHandler(fn func(in Inbound, recovered any)) {
	q.mu.Lock()
	q.onPanic = fn
	q.mu.Unlock()
}

// dispatch isolates one message's failure. A panic in the handler must not kill
// the lane: the chat's worker is the only thing draining its backlog, and losing
// it would silently strand every queued message.
func (q *Queue) dispatch(in Inbound) {
	defer func() {
		rec := recover()
		if rec == nil {
			return
		}
		q.mu.Lock()
		fn := q.onPanic
		q.mu.Unlock()
		if fn != nil {
			fn(in, rec)
		}
	}()
	q.handler(q.ctx, in)
}
