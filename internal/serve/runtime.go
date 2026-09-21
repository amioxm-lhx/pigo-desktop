// SessionRuntime drives one open conversation: it maps the service-level
// operations onto the runtime's existing seams rather than reimplementing
// any loop logic — Prompt appends the user message and launches
// runtime.StartRun, Steer feeds the RunConfig.GetSteeringMessages channel,
// Abort cancels the run context, and Subscribe fans the run's AgentEvents
// out to any number of observers. The run's result is persisted through
// session.Store.AppendBranch when it finishes, so the on-disk tree keeps
// every branch.
package serve

import (
	"context"
	"errors"
	"sync"
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/runtime"
	"github.com/smallnest/pigo/internal/session"
)

// Phase is the coarse lifecycle state of a SessionRuntime.
type Phase int

const (
	// PhaseIdle means no run is active: the session can be prompted, snap-
	// shotted, reconfigured, or disposed.
	PhaseIdle Phase = iota
	// PhaseRunning means a prompt run is in flight: Prompt returns
	// ErrRunActive until it finishes (or is aborted).
	PhaseRunning
	// PhaseDisposed means the runtime was disposed: every method is closed
	// for business. Dispose is idempotent.
	PhaseDisposed
)

// Sentinel errors returned by SessionRuntime methods.
var (
	// ErrDisposed is returned by any method called after Dispose.
	ErrDisposed = errors.New("serve: session runtime is disposed")
	// ErrRunActive is returned by Prompt when a run is already in flight.
	ErrRunActive = errors.New("serve: a run is already active on this session")
	// ErrNoActiveRun is returned by Steer when no run is in flight to steer.
	ErrNoActiveRun = errors.New("serve: no active run to steer")
)

// subscriberBuffer is the per-subscriber event buffer. A subscriber that
// falls further behind than this loses events rather than stalling the run
// (same containment philosophy as remotecontrol's ring buffer); a live
// consumer that keeps up never drops.
const subscriberBuffer = 128

// SessionRuntime is the live handle on one conversation. It is safe for
// concurrent use. Events are observed via Subscribe (multiple subscribers
// allowed — the "several people watching one session" case), the persisted
// state via Snapshot.
type SessionRuntime interface {
	// ID returns the session id.
	ID() string
	// Snapshot returns the current persisted state: header, entry tree,
	// active leaf.
	Snapshot(ctx context.Context) (SessionSnapshot, error)
	// GetPhase reports the current lifecycle phase.
	GetPhase() Phase
	// Prompt appends the user message and starts a run. It returns
	// immediately; the run's events flow to subscribers and its result is
	// persisted when it ends. Use WaitIdle to await completion.
	Prompt(ctx context.Context, text string) error
	// WaitIdle blocks until no run is active (or ctx ends), returning any
	// persistence error from the run that just finished. It is how the
	// headless driver awaits a prompt synchronously.
	WaitIdle(ctx context.Context) error
	// Steer queues a message to inject into the running conversation
	// between turns (the GetSteeringMessages seam).
	Steer(text string) error
	// Abort cancels the active run; already-produced messages are still
	// persisted. No-op when idle.
	Abort()
	// SetModel switches the model for subsequent runs (recorded in the
	// header; the run-config factory sees it).
	SetModel(model string) error
	// SetThinking switches the thinking level for subsequent runs.
	SetThinking(level agentcore.ThinkingLevel) error
	// Subscribe returns a live event feed. Disposed runtimes refuse.
	Subscribe() (*Subscription, error)
	// Dispose aborts any active run (without persisting it), closes all
	// subscriptions, and permanently closes the runtime. Idempotent.
	Dispose() error
}

// sessionRuntime is the concrete SessionRuntime.
type sessionRuntime struct {
	mu           sync.Mutex
	store        session.Store
	newRunConfig RunConfigFunc

	header   session.SessionHeader
	entries  []session.Entry // persisted tree cache (file order)
	leafID   string          // active leaf entry id
	thinking agentcore.ThinkingLevel

	agentCtx *agentcore.AgentContext // the continuation context (leaf path)

	phase     Phase
	steerQ    []agentcore.AgentMessage // pending steering messages
	runCancel context.CancelFunc
	runDone   chan struct{} // closed when the active run fully finishes

	subs      map[int]*Subscription
	nextSubID int

	persistErr error // last persistence failure, reported once via WaitIdle
}

// newSessionRuntime builds a runtime for a header with no persisted entries
// yet (CreateSession); attach loads a stored tree (OpenSession).
func newSessionRuntime(store session.Store, newRunConfig RunConfigFunc, header session.SessionHeader) *sessionRuntime {
	return &sessionRuntime{
		store:        store,
		newRunConfig: newRunConfig,
		header:       header,
		agentCtx:     &agentcore.AgentContext{SystemPrompt: header.SystemPrompt},
		phase:        PhaseIdle,
		subs:         make(map[int]*Subscription),
	}
}

// attach seeds the runtime from a loaded session tree: the entry cache takes
// the file's tree, and the continuation context becomes the leaf path.
func (rt *sessionRuntime) attach(entries []session.Entry, leafID string) {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	rt.entries = append([]session.Entry(nil), entries...)
	rt.leafID = leafID
	var msgs agentcore.MessageList
	for _, e := range session.PathToLeaf(entries, leafID) {
		msgs = append(msgs, e.Message)
	}
	rt.agentCtx.Messages = msgs
}

// ID returns the session id.
func (rt *sessionRuntime) ID() string {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	return rt.header.ID
}

// Snapshot returns the current persisted state (a copy; safe to retain).
func (rt *sessionRuntime) Snapshot(ctx context.Context) (SessionSnapshot, error) {
	if err := ctx.Err(); err != nil {
		return SessionSnapshot{}, err
	}
	rt.mu.Lock()
	defer rt.mu.Unlock()
	if rt.phase == PhaseDisposed {
		return SessionSnapshot{}, ErrDisposed
	}
	entries := make([]session.Entry, len(rt.entries))
	copy(entries, rt.entries)
	return SessionSnapshot{Header: rt.header, Entries: entries, LeafID: rt.leafID}, nil
}

// GetPhase reports the current lifecycle phase.
func (rt *sessionRuntime) GetPhase() Phase {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	return rt.phase
}

// Prompt appends the user message and starts the run. The run-config factory
// sees the session's current header (model et al.) and thinking level; serve
// fills SessionID and composes the steering queue onto whatever steering the
// factory itself configured.
func (rt *sessionRuntime) Prompt(ctx context.Context, text string) error {
	rt.mu.Lock()
	if rt.phase == PhaseDisposed {
		rt.mu.Unlock()
		return ErrDisposed
	}
	if rt.phase == PhaseRunning {
		rt.mu.Unlock()
		return ErrRunActive
	}
	rt.phase = PhaseRunning

	startIdx := len(rt.agentCtx.Messages)
	rt.agentCtx.Messages = append(rt.agentCtx.Messages, agentcore.UserMessage{
		RoleField: agentcore.RoleUser,
		Content:   agentcore.ContentList{agentcore.NewTextContent(text)},
	})

	runCtx, cancel := context.WithCancel(ctx)
	rt.runCancel = cancel
	done := make(chan struct{})
	rt.runDone = done

	header := rt.header
	thinking := rt.thinking
	agentCtx := rt.agentCtx
	rt.mu.Unlock()

	cfg := rt.newRunConfig(header, thinking)
	cfg.SessionID = header.ID
	inner := cfg.GetSteeringMessages
	cfg.GetSteeringMessages = func(ctx context.Context) []agentcore.AgentMessage {
		var out []agentcore.AgentMessage
		if inner != nil {
			out = append(out, inner(ctx)...)
		}
		return append(out, rt.takeSteering()...)
	}

	stream := runtime.StartRun(runCtx, agentCtx, cfg)

	go func() {
		defer cancel()
		defer close(done)
		for ev := range stream.Events() {
			rt.broadcast(ev)
		}
		rt.finishRun(startIdx)
	}()
	return nil
}

// WaitIdle blocks until the session is idle again (or ctx ends), returning
// any persistence error from the run that just finished (read-once).
func (rt *sessionRuntime) WaitIdle(ctx context.Context) error {
	for {
		rt.mu.Lock()
		if rt.phase == PhaseDisposed {
			rt.mu.Unlock()
			return ErrDisposed
		}
		if rt.phase == PhaseIdle {
			err := rt.persistErr
			rt.persistErr = nil
			rt.mu.Unlock()
			return err
		}
		done := rt.runDone
		rt.mu.Unlock()
		select {
		case <-done: // finishRun has completed; re-check under the lock
		case <-ctx.Done():
			return ctx.Err()
		}
	}
}

// Steer queues a user message to inject between the running run's turns.
func (rt *sessionRuntime) Steer(text string) error {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	switch rt.phase {
	case PhaseDisposed:
		return ErrDisposed
	case PhaseIdle:
		return ErrNoActiveRun
	}
	rt.steerQ = append(rt.steerQ, agentcore.UserMessage{
		RoleField: agentcore.RoleUser,
		Content:   agentcore.ContentList{agentcore.NewTextContent(text)},
	})
	return nil
}

// takeSteering drains the pending steering queue (called by the composed
// GetSteeringMessages during a run).
func (rt *sessionRuntime) takeSteering() []agentcore.AgentMessage {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	out := rt.steerQ
	rt.steerQ = nil
	return out
}

// Abort cancels the active run. The run's already-produced messages are
// still persisted when its stream ends.
func (rt *sessionRuntime) Abort() {
	rt.mu.Lock()
	cancel := rt.runCancel
	rt.mu.Unlock()
	if cancel != nil {
		cancel()
	}
}

// SetModel records the model for subsequent runs.
func (rt *sessionRuntime) SetModel(model string) error {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	if rt.phase == PhaseDisposed {
		return ErrDisposed
	}
	rt.header.Model = model
	return nil
}

// SetThinking records the thinking level for subsequent runs.
func (rt *sessionRuntime) SetThinking(level agentcore.ThinkingLevel) error {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	if rt.phase == PhaseDisposed {
		return ErrDisposed
	}
	rt.thinking = level
	return nil
}

// finishRun persists the messages a run produced (user prompt included) as a
// branch descending from the current leaf, refreshes the entry cache from
// the authoritative file, and returns the session to idle. Disposed runtimes
// skip persistence entirely. Runs under rt.mu so the next Prompt cannot
// interleave with this write.
func (rt *sessionRuntime) finishRun(startIdx int) {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	if rt.phase == PhaseDisposed {
		return
	}
	rt.phase = PhaseIdle
	rt.runCancel = nil
	rt.runDone = nil

	msgs := rt.agentCtx.Messages
	if len(msgs) <= startIdx {
		return
	}
	newMsgs := make(agentcore.MessageList, len(msgs)-startIdx)
	copy(newMsgs, msgs[startIdx:])

	header := rt.header
	header.UpdatedAt = time.Now().UTC()
	leaf, err := rt.store.AppendBranch(header, rt.leafID, newMsgs)
	if err != nil {
		rt.persistErr = err
		return
	}
	rt.leafID = leaf
	rt.header.UpdatedAt = header.UpdatedAt
	if _, entries, err := rt.store.LoadEntries(header.ID); err == nil {
		rt.entries = entries
	} else {
		rt.persistErr = err
	}
}

// Subscribe returns a live event feed for this session's runs. Events are
// buffered (subscriberBuffer); a subscriber that falls further behind drops
// events instead of stalling the run — a consumer that keeps up never drops.
func (rt *sessionRuntime) Subscribe() (*Subscription, error) {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	if rt.phase == PhaseDisposed {
		return nil, ErrDisposed
	}
	rt.nextSubID++
	sub := &Subscription{
		events: make(chan agentcore.AgentEvent, subscriberBuffer),
		rt:     rt,
		id:     rt.nextSubID,
	}
	rt.subs[sub.id] = sub
	return sub, nil
}

// broadcast fans one event out to every subscriber. Called from the run's
// event-forwarding goroutine, one event at a time, in emission order.
func (rt *sessionRuntime) broadcast(ev agentcore.AgentEvent) {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	for _, sub := range rt.subs {
		sub.deliver(ev)
	}
}

// removeSub detaches a subscription: it leaves the fan-out map and its
// events channel is closed, ending any range loop over it. Idempotent.
func (rt *sessionRuntime) removeSub(id int) {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	if sub, ok := rt.subs[id]; ok {
		delete(rt.subs, id)
		close(sub.events)
	}
}

// Dispose aborts any active run without persisting it, closes every
// subscription, and permanently closes the runtime. Idempotent.
func (rt *sessionRuntime) Dispose() error {
	rt.mu.Lock()
	if rt.phase == PhaseDisposed {
		rt.mu.Unlock()
		return nil
	}
	rt.phase = PhaseDisposed
	cancel := rt.runCancel
	subs := make([]*Subscription, 0, len(rt.subs))
	for _, s := range rt.subs {
		subs = append(subs, s)
	}
	rt.mu.Unlock()

	if cancel != nil {
		cancel()
	}
	for _, s := range subs {
		s.Close()
	}
	return nil
}

// Subscription is one observer's event feed. Events are delivered in
// emission order; the channel closes when the subscription is closed or the
// runtime is disposed.
type Subscription struct {
	events    chan agentcore.AgentEvent
	rt        *sessionRuntime
	id        int
	closeOnce sync.Once
	dropped   int // events lost to a full buffer (guarded by rt.mu)
}

// Events returns the read-only event channel.
func (s *Subscription) Events() <-chan agentcore.AgentEvent { return s.events }

// Dropped reports how many events were lost because this subscriber fell
// behind the buffer. A healthy consumer stays at zero.
func (s *Subscription) Dropped() int {
	s.rt.mu.Lock()
	defer s.rt.mu.Unlock()
	return s.dropped
}

// Close detaches the subscription; its Events channel is closed. Idempotent.
func (s *Subscription) Close() {
	s.closeOnce.Do(func() { s.rt.removeSub(s.id) })
}

// deliver forwards one event, dropping it when the buffer is full rather
// than blocking the run's forwarder. Called with rt.mu held.
func (s *Subscription) deliver(ev agentcore.AgentEvent) {
	select {
	case s.events <- ev:
	default:
		s.dropped++
	}
}
