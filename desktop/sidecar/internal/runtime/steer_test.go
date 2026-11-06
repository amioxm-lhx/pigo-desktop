package runtime

import (
	"context"
	"io"
	"log"
	"testing"
	"time"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/provider"
)

func steerTestSession(t *testing.T) (*Session, *captureEmitter) {
	t.Helper()
	params := historyTestParams("u1", "start")
	params.SessionID = "session-steer"
	emitter := &captureEmitter{ch: make(chan protocol.EventEnvelope, 4)}
	session, err := NewSession(
		context.Background(),
		&fakeHost{},
		emitter,
		log.New(io.Discard, "", 0),
		params,
		(&scriptedStream{}).Stream,
		provider.Model{Provider: "desktop", ID: "test-model"},
	)
	if err != nil {
		t.Fatalf("NewSession: %v", err)
	}
	return session, emitter
}

func TestSteerQueuesInputAndEmitsMessageEnd(t *testing.T) {
	session, emitter := steerTestSession(t)
	session.mu.Lock()
	session.running = true
	session.currentTurn = "turn-1"
	session.mu.Unlock()
	defer func() {
		session.mu.Lock()
		session.running = false
		session.currentTurn = ""
		session.mu.Unlock()
	}()

	params := protocol.SteerParams{
		SessionID:      "session-steer",
		ExpectedTurnID: "turn-1",
		Content:        "please continue",
		Attachments: []protocol.PromptAttachment{{
			Kind:     "image",
			MimeType: "image/png",
			Data:     "aGVsbG8=",
		}},
	}
	params.Message.ID = "steer-1"
	params.Message.Content = "please continue"
	params.Message.CreatedAt = "2026-11-02T10:00:00Z"

	reply, err := session.Steer(params)
	if err != nil {
		t.Fatalf("Steer: %v", err)
	}
	if accepted, _ := reply["accepted"].(bool); !accepted {
		t.Fatalf("steer reply = %#v", reply)
	}
	if reply["turnId"] != "turn-1" || reply["messageId"] != "steer-1" {
		t.Fatalf("steer reply = %#v", reply)
	}

	select {
	case envelope := <-emitter.ch:
		end, ok := envelope.Event.(protocol.MessageEndEvent)
		if !ok {
			t.Fatalf("event = %T, want message_end", envelope.Event)
		}
		if end.Message.ID != "steer-1" || end.Message.Content != "please continue" {
			t.Fatalf("steering row = %#v", end.Message)
		}
		if end.Message.Steering == nil || !*end.Message.Steering {
			t.Fatalf("steering flag = %#v", end.Message.Steering)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for steering event")
	}

	queued := session.takeSteeringMessages()
	if len(queued) != 1 {
		t.Fatalf("queued steering messages = %d, want 1", len(queued))
	}
	user, ok := queued[0].(agentcore.UserMessage)
	if !ok {
		t.Fatalf("queued message = %T", queued[0])
	}
	if text := agentcore.ContentToText(user.Content); text != "please continue" {
		t.Fatalf("queued text = %q", text)
	}
	images := 0
	for _, block := range user.Content {
		if image, ok := block.(agentcore.ImageContent); ok {
			images++
			if image.MimeType != "image/png" || image.Data != "aGVsbG8=" {
				t.Fatalf("queued image = %#v", image)
			}
		}
	}
	if images != 1 {
		t.Fatalf("queued images = %d, want 1", images)
	}
}

func TestSteerRejectsMissingOrMismatchedTurn(t *testing.T) {
	session, _ := steerTestSession(t)

	if _, err := session.Steer(protocol.SteerParams{SessionID: "session-steer", Content: "x"}); err == nil {
		t.Fatal("steer without a running turn should fail")
	}

	session.mu.Lock()
	session.running = true
	session.currentTurn = "turn-1"
	session.mu.Unlock()
	defer func() {
		session.mu.Lock()
		session.running = false
		session.currentTurn = ""
		session.mu.Unlock()
	}()

	if _, err := session.Steer(protocol.SteerParams{
		SessionID:      "session-steer",
		ExpectedTurnID: "turn-other",
		Content:        "x",
	}); err == nil {
		t.Fatal("steer with a mismatched turn should fail")
	}
	if queued := session.takeSteeringMessages(); len(queued) != 0 {
		t.Fatalf("mismatched steer queued %d messages", len(queued))
	}
}

func TestRequestGracefulStopOnlyOnce(t *testing.T) {
	session, _ := steerTestSession(t)
	session.mu.Lock()
	session.running = true
	session.currentTurn = "turn-1"
	session.mu.Unlock()
	defer func() {
		session.mu.Lock()
		session.running = false
		session.currentTurn = ""
		session.mu.Unlock()
	}()

	if result := session.RequestGracefulStop(); !result.Requested {
		t.Fatalf("first stop = %+v, want requested", result)
	}
	if result := session.RequestGracefulStop(); result.Requested {
		t.Fatalf("second stop = %+v, want not requested", result)
	}
	if !session.StopRequested() {
		t.Fatal("stop flag was not set")
	}
}

func TestUserContentIncludesOnlyInlinedImages(t *testing.T) {
	content := userContent("look", []protocol.PromptAttachment{
		{Kind: "image", MimeType: "image/png", Data: "aGVsbG8="},
		{Kind: "file", Name: "notes.txt", Data: "ignored"},
	})
	if len(content) != 2 {
		t.Fatalf("content = %#v, want text + image", content)
	}
	if text, ok := content[0].(agentcore.TextContent); !ok || text.Text != "look" {
		t.Fatalf("content[0] = %#v", content[0])
	}
	image, ok := content[1].(agentcore.ImageContent)
	if !ok || image.Data != "aGVsbG8=" || image.MimeType != "image/png" {
		t.Fatalf("content[1] = %#v", content[1])
	}

	if content := userContent("plain", nil); len(content) != 1 {
		t.Fatalf("text-only content = %#v", content)
	}
}

func TestStreamingIdentityDropsGrowingContent(t *testing.T) {
	message := protocol.UiMessage{
		ID:         "row-1",
		Role:       protocol.RoleAssistant,
		Content:    "partial answer",
		Thinking:   "partial thought",
		CreatedAt:  "2026-11-02T10:00:00Z",
		Status:     protocol.StatusStreaming,
		ModelID:    "model",
		ProviderID: "provider",
	}
	identity := streamingIdentity(message)
	if identity.Content != "" || identity.Thinking != "" {
		t.Fatalf("identity = %#v, want empty growing content", identity)
	}
	if identity.ID != "row-1" || identity.CreatedAt != message.CreatedAt ||
		identity.Status != protocol.StatusStreaming ||
		identity.ModelID != "model" || identity.ProviderID != "provider" {
		t.Fatalf("identity lost framing fields: %#v", identity)
	}
}
