package remotecontrol

// Phase 2c: the structured channel — a freshly connected client first receives
// a full session snapshot (from the serve layer's SessionRuntime) and then
// follows the conversation through incremental event frames. The tests below
// drive a real serve stack with scripted streams, mirroring the assembly a
// host binary performs: NewService → CreateSession → SetSessionSource(runtime).

import (
	"context"
	"errors"
	"testing"
	"time"

	"github.com/coder/websocket"
	"github.com/coder/websocket/wsjson"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/provider"
	"github.com/smallnest/pigo/internal/runtime"
	"github.com/smallnest/pigo/internal/serve"
	"github.com/smallnest/pigo/internal/session"
)

// newTestRuntime builds a serve SessionRuntime over a temp store whose model
// stream answers every call with one plain text turn, so a run settles after a
// single assistant message.
func newTestRuntime(t *testing.T) serve.SessionRuntime {
	t.Helper()
	stream := func(ctx context.Context, model string, llm provider.LlmContext, cfg provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
		final := agentcore.AssistantMessage{
			RoleField:  agentcore.RoleAssistant,
			Content:    agentcore.ContentList{agentcore.NewTextContent("reply")},
			StopReason: agentcore.StopReasonEndTurn,
		}
		s := provider.NewAssistantMessageEventStream(0)
		go func() {
			_ = s.Emit(ctx, provider.StreamDoneEvent{Message: final})
			s.Close()
		}()
		return s, nil
	}
	store, err := session.NewStore(t.TempDir())
	if err != nil {
		t.Fatalf("NewStore: %v", err)
	}
	svc, err := serve.NewService(store, func(h session.SessionHeader, th agentcore.ThinkingLevel) runtime.RunConfig {
		return runtime.RunConfig{
			LoopConfig: runtime.LoopConfig{Model: h.Model, Provider: h.Provider, Stream: stream},
		}
	})
	if err != nil {
		t.Fatalf("NewService: %v", err)
	}
	rt, err := svc.CreateSession(context.Background(), serve.SessionConfig{Model: "fake", Provider: "p1"})
	if err != nil {
		t.Fatalf("CreateSession: %v", err)
	}
	return rt
}

// readFrame reads frames until one of the wanted type arrives (skipping
// anything else the wire happens to interleave), failing on the read deadline.
func readFrame(t *testing.T, ctx context.Context, conn *websocket.Conn, typ FrameType) Frame {
	t.Helper()
	for {
		var f Frame
		if err := wsjson.Read(ctx, conn, &f); err != nil {
			t.Fatalf("read %v frame: %v", typ, err)
		}
		if f.Type == typ {
			return f
		}
	}
}

// collectEvents reads FrameEvent frames until one carries the until type,
// returning the events' "type" sequence in arrival order.
func collectEvents(t *testing.T, ctx context.Context, conn *websocket.Conn, until string) []string {
	t.Helper()
	var kinds []string
	for {
		var f Frame
		if err := wsjson.Read(ctx, conn, &f); err != nil {
			t.Fatalf("read event frame: %v", err)
		}
		if f.Type != FrameEvent {
			continue
		}
		kind, _ := f.Event["type"].(string)
		kinds = append(kinds, kind)
		if kind == until {
			return kinds
		}
	}
}

// TestSnapshotAndEventFeed is the phase-2c acceptance: a client joining
// mid-session receives the full snapshot of everything that already happened,
// then follows the next run purely through incremental event frames.
func TestSnapshotAndEventFeed(t *testing.T) {
	rt := newTestRuntime(t)
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	// Round one completes before any client exists — its result lives only on
	// disk, so the mid-session joiner must learn it from the snapshot frame.
	if err := rt.Prompt(ctx, "first"); err != nil {
		t.Fatalf("Prompt 1: %v", err)
	}
	if err := rt.WaitIdle(ctx); err != nil {
		t.Fatalf("WaitIdle 1: %v", err)
	}

	s, pairURL := startTestServer(t, &fakeHandler{})
	s.SetSessionSource(rt)
	base, cred := sessionCred(t, pairURL)
	conn, _, err := dialWS(t, base, cred)
	if err != nil {
		t.Fatalf("dial ws: %v", err)
	}
	defer conn.Close(websocket.StatusNormalClosure, "")

	// Wire order: connected → snapshot → events, strictly.
	var connected Frame
	if err := wsjson.Read(ctx, conn, &connected); err != nil {
		t.Fatalf("read connected: %v", err)
	}
	if connected.Type != FrameStatus || connected.State != StatusConnected {
		t.Fatalf("first frame = %+v, want status/connected", connected)
	}

	snap := readFrame(t, ctx, conn, FrameSnapshot)
	if snap.Snapshot == nil {
		t.Fatalf("snapshot frame without payload: %+v", snap)
	}
	if len(snap.Snapshot.Entries) != 2 { // user prompt + assistant reply
		t.Fatalf("snapshot entries = %d, want 2", len(snap.Snapshot.Entries))
	}
	if snap.Snapshot.LeafID == "" {
		t.Fatal("snapshot leaf must be set after a persisted run")
	}
	if snap.Snapshot.Header.Model != "fake" {
		t.Fatalf("snapshot header model = %q, want fake", snap.Snapshot.Header.Model)
	}
	if msgs := snap.Snapshot.Messages(); len(msgs) != 2 {
		t.Fatalf("snapshot messages = %d, want 2", len(msgs))
	}

	// Round two: the client follows it through incremental events alone.
	if err := rt.Prompt(ctx, "second"); err != nil {
		t.Fatalf("Prompt 2: %v", err)
	}
	kinds := collectEvents(t, ctx, conn, agentcore.EventAgentEnd)
	want := []string{
		agentcore.EventAgentStart, agentcore.EventTurnStart, agentcore.EventMessageEnd,
		agentcore.EventTurnEnd, agentcore.EventTelemetry, agentcore.EventAgentEnd,
	}
	if len(kinds) != len(want) {
		t.Fatalf("event kinds = %v, want %v", kinds, want)
	}
	for i := range want {
		if kinds[i] != want[i] {
			t.Fatalf("event kinds = %v, want %v", kinds, want)
		}
	}
	if err := rt.WaitIdle(ctx); err != nil {
		t.Fatalf("WaitIdle 2: %v", err)
	}
}

// TestEventFrameCarriesWireEncoding verifies one event frame's payload beyond
// its type: the agent_start event names the session so a consumer can
// associate the feed with a resumable session, exactly as headless stream-json
// does — both share runtime.EventEnvelope.
func TestEventFrameCarriesWireEncoding(t *testing.T) {
	rt := newTestRuntime(t)
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	s, pairURL := startTestServer(t, &fakeHandler{})
	s.SetSessionSource(rt)
	base, cred := sessionCred(t, pairURL)
	conn, _, err := dialWS(t, base, cred)
	if err != nil {
		t.Fatalf("dial ws: %v", err)
	}
	defer conn.Close(websocket.StatusNormalClosure, "")

	readFrame(t, ctx, conn, FrameSnapshot) // drain the baseline

	if err := rt.Prompt(ctx, "go"); err != nil {
		t.Fatalf("Prompt: %v", err)
	}
	start := readFrame(t, ctx, conn, FrameEvent)
	if got, _ := start.Event["type"].(string); got != agentcore.EventAgentStart {
		t.Fatalf("first event = %v, want agent_start", start.Event)
	}
	if got, _ := start.Event["sessionId"].(string); got != rt.ID() {
		t.Fatalf("agent_start sessionId = %q, want %q", got, rt.ID())
	}
	if err := rt.WaitIdle(ctx); err != nil {
		t.Fatalf("WaitIdle: %v", err)
	}
}

// failingSource refuses both halves of the structured channel, exercising the
// fallback: the client still gets the legacy ring-replay terminal mirror.
type failingSource struct{}

func (failingSource) Snapshot(ctx context.Context) (serve.SessionSnapshot, error) {
	return serve.SessionSnapshot{}, errors.New("source unavailable")
}

func (failingSource) Subscribe() (*serve.Subscription, error) {
	return nil, errors.New("source unavailable")
}

// TestSourceFailureFallsBackToRing verifies the compatibility path: when the
// structured source is absent or refuses, a connecting client is served the
// rolling byte mirror exactly as before phase 2c.
func TestSourceFailureFallsBackToRing(t *testing.T) {
	s, pairURL := startTestServer(t, &fakeHandler{})
	s.SetSessionSource(failingSource{})
	base, cred := sessionCred(t, pairURL)

	conn, _, err := dialWS(t, base, cred)
	if err != nil {
		t.Fatalf("dial ws: %v", err)
	}
	defer conn.Close(websocket.StatusNormalClosure, "")

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	// No snapshot frame may appear; the terminal mirror still streams.
	s.SendOutput("mirror me")
	out := readFrame(t, ctx, conn, FrameOutput)
	if out.Text != "mirror me" {
		t.Fatalf("output = %q, want mirror me", out.Text)
	}
}

// TestReconnectGetsFreshSnapshot verifies every connection independently
// re-baselines: after a disconnect and further conversation progress, the
// reconnecting client's snapshot reflects the newer state, not the one the
// first client saw.
func TestReconnectGetsFreshSnapshot(t *testing.T) {
	rt := newTestRuntime(t)
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	if err := rt.Prompt(ctx, "first"); err != nil {
		t.Fatalf("Prompt 1: %v", err)
	}
	if err := rt.WaitIdle(ctx); err != nil {
		t.Fatalf("WaitIdle 1: %v", err)
	}

	s, pairURL := startTestServer(t, &fakeHandler{})
	s.SetSessionSource(rt)
	base, cred := sessionCred(t, pairURL)

	conn1, _, err := dialWS(t, base, cred)
	if err != nil {
		t.Fatalf("dial ws 1: %v", err)
	}
	readFrame(t, ctx, conn1, FrameSnapshot)
	conn1.Close(websocket.StatusNormalClosure, "")

	deadline := time.Now().Add(time.Second)
	for s.HasClient() {
		if time.Now().After(deadline) {
			t.Fatal("client 1 slot never released")
		}
		time.Sleep(2 * time.Millisecond)
	}

	// The conversation moves on while nobody is connected.
	if err := rt.Prompt(ctx, "second"); err != nil {
		t.Fatalf("Prompt 2: %v", err)
	}
	if err := rt.WaitIdle(ctx); err != nil {
		t.Fatalf("WaitIdle 2: %v", err)
	}

	conn2, _, err := dialWS(t, base, cred)
	if err != nil {
		t.Fatalf("dial ws 2: %v", err)
	}
	defer conn2.Close(websocket.StatusNormalClosure, "")

	snap := readFrame(t, ctx, conn2, FrameSnapshot)
	if snap.Snapshot == nil || len(snap.Snapshot.Entries) != 4 {
		t.Fatalf("reconnect snapshot entries = %d, want 4", len(snap.Snapshot.Entries))
	}
}
