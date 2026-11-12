package im

import (
	"context"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

func testInbound(chat string, n int) Inbound {
	return Inbound{
		Adapter: "test",
		Chat:    ChatRef{ID: chat, Kind: ChatP2P},
		Sender:  UserRef{ID: "u1"},
		MsgID:   chat + "-" + string(rune('a'+n)),
		Text:    "hi",
		Kind:    KindText,
	}
}

// TestQueueRejectsWhenBacklogFull pins the documented policy: a chat may have
// `depth` messages waiting behind the one it is running, and the next one is
// rejected rather than queued.
func TestQueueRejectsWhenBacklogFull(t *testing.T) {
	release := make(chan struct{})
	var handled atomic.Int64
	handler := func(_ context.Context, _ Inbound) {
		handled.Add(1)
		<-release
	}

	var fullCalls atomic.Int64
	var lastBacklog atomic.Int64
	q := NewQueue(2, handler, func(_ Inbound, backlog int) {
		fullCalls.Add(1)
		lastBacklog.Store(int64(backlog))
	})

	if got := q.Submit(testInbound("c1", 0)); got != Accepted {
		t.Fatalf("first submit = %v, want Accepted", got)
	}
	// The first message now occupies the worker; give it a moment to be picked
	// up so the backlog counts below are unambiguous.
	waitFor(t, func() bool { return handled.Load() == 1 })

	if got := q.Submit(testInbound("c1", 1)); got != Accepted {
		t.Fatalf("second submit = %v, want Accepted", got)
	}
	if got := q.Submit(testInbound("c1", 2)); got != Accepted {
		t.Fatalf("third submit = %v, want Accepted", got)
	}
	if got := q.Submit(testInbound("c1", 3)); got != RejectedFull {
		t.Fatalf("fourth submit = %v, want RejectedFull", got)
	}
	if fullCalls.Load() != 1 {
		t.Fatalf("onFull calls = %d, want 1", fullCalls.Load())
	}
	if lastBacklog.Load() != 2 {
		t.Fatalf("reported backlog = %d, want 2", lastBacklog.Load())
	}

	close(release)
	q.Wait()
	if handled.Load() != 3 {
		t.Fatalf("handled = %d, want 3 (the rejected message must not run)", handled.Load())
	}
}

// TestQueueIsolatesChats proves a saturated chat does not cost another chat its
// message. This is the property that makes per-chat lanes worth the bookkeeping.
func TestQueueIsolatesChats(t *testing.T) {
	release := make(chan struct{})
	handler := func(_ context.Context, _ Inbound) { <-release }
	q := NewQueue(1, handler, nil)

	q.Submit(testInbound("busy", 0))
	waitFor(t, func() bool { return q.Busy(testInbound("busy", 0).Chat.Key()) })
	q.Submit(testInbound("busy", 1)) // fills the backlog
	if got := q.Submit(testInbound("busy", 2)); got != RejectedFull {
		t.Fatalf("busy chat = %v, want RejectedFull", got)
	}
	if got := q.Submit(testInbound("calm", 0)); got != Accepted {
		t.Fatalf("other chat = %v, want Accepted", got)
	}

	close(release)
	q.Wait()
}

// TestQueueSerializesPerChat checks that messages from one chat never overlap:
// a session runs one prompt at a time, so the queue must not fan out.
func TestQueueSerializesPerChat(t *testing.T) {
	var inFlight atomic.Int64
	var maxInFlight atomic.Int64
	var mu sync.Mutex
	var order []string

	handler := func(_ context.Context, in Inbound) {
		cur := inFlight.Add(1)
		for {
			prev := maxInFlight.Load()
			if cur <= prev || maxInFlight.CompareAndSwap(prev, cur) {
				break
			}
		}
		mu.Lock()
		order = append(order, in.MsgID)
		mu.Unlock()
		time.Sleep(2 * time.Millisecond)
		inFlight.Add(-1)
	}

	q := NewQueue(16, handler, nil)
	for i := 0; i < 6; i++ {
		q.Submit(testInbound("c1", i))
	}
	q.Wait()

	if maxInFlight.Load() != 1 {
		t.Fatalf("max in-flight = %d, want 1", maxInFlight.Load())
	}
	mu.Lock()
	defer mu.Unlock()
	if len(order) != 6 {
		t.Fatalf("handled %d messages, want 6", len(order))
	}
	for i, id := range order {
		want := "c1-" + string(rune('a'+i))
		if id != want {
			t.Fatalf("order[%d] = %q, want %q", i, id, want)
		}
	}
}

// TestQueueSurvivesHandlerPanic covers the failure mode that would otherwise
// strand a chat forever: a panicking handler must not kill the lane worker.
func TestQueueSurvivesHandlerPanic(t *testing.T) {
	var panics atomic.Int64
	var handled atomic.Int64
	handler := func(_ context.Context, in Inbound) {
		if in.MsgID == "c1-a" {
			panic("boom")
		}
		handled.Add(1)
	}
	q := NewQueue(8, handler, nil)
	q.SetPanicHandler(func(Inbound, any) { panics.Add(1) })

	q.Submit(testInbound("c1", 0))
	q.Submit(testInbound("c1", 1))
	q.Wait()

	if panics.Load() != 1 {
		t.Fatalf("panics = %d, want 1", panics.Load())
	}
	if handled.Load() != 1 {
		t.Fatalf("handled after panic = %d, want 1", handled.Load())
	}
}

// TestQueueCloseRejects checks that a closed queue reports it instead of
// silently accepting work it will never run.
func TestQueueCloseRejects(t *testing.T) {
	q := NewQueue(2, func(context.Context, Inbound) {}, nil)
	q.Close()
	if got := q.Submit(testInbound("c1", 0)); got != RejectedClosed {
		t.Fatalf("submit after close = %v, want RejectedClosed", got)
	}
	q.Wait()
}

// TestQueueDefaultDepth documents that a non-positive depth falls back to the
// package default rather than meaning "unbounded".
func TestQueueDefaultDepth(t *testing.T) {
	q := NewQueue(0, func(context.Context, Inbound) {}, nil)
	if q.Depth() != DefaultQueueDepth {
		t.Fatalf("depth = %d, want %d", q.Depth(), DefaultQueueDepth)
	}
}

func waitFor(t *testing.T, cond func() bool) {
	t.Helper()
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		if cond() {
			return
		}
		time.Sleep(time.Millisecond)
	}
	t.Fatal("condition not reached within 2s")
}
