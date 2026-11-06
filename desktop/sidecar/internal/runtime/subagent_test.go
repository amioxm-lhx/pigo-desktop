package runtime

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/provider"
	pigoruntime "github.com/smallnest/pigo/internal/runtime"
)

// subagentTestSession builds the minimum runtime state the Task family needs:
// a provider stream (injected), the host catalog for the delegate's tools, and
// the definition catalog.
func subagentTestSession(stream provider.StreamFn, definitions []protocol.SubagentDefinition) *Session {
	host := &fakeHost{}
	session := &Session{
		host:         host,
		logger:       log.New(io.Discard, "", 0),
		sessionID:    "session-1",
		mode:         "agent",
		thinking:     agentcore.ThinkingMedium,
		model:        provider.Model{Provider: "desktop", ID: "test-model", SupportsImages: true},
		streamFn:     stream,
		baseStreamFn: stream,
		hostCatalog: []protocol.ToolDefinition{{
			Name:        "Read",
			Description: "Read a file",
			Parameters:  json.RawMessage(`{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}`),
		}},
		liveTools:         map[string]agentcore.AgentTool{},
		subagents:         definitions,
		subagentProviders: map[string]protocol.ProviderConfig{},
		subagentSem:       make(chan struct{}, MaxSubagentConcurrency),
		delegations:       map[string]*delegationRecord{},
		toolErrors:        map[string]bool{},
		runCfg: pigoruntime.RunConfig{
			LoopConfig: pigoruntime.LoopConfig{
				Model:    "test-model",
				Provider: "desktop",
				Stream:   stream,
				APIKey:   "test-key",
			},
			EventBuffer: 16,
		},
	}
	return session
}

func readOnlyDefinition(name string) protocol.SubagentDefinition {
	return protocol.SubagentDefinition{
		Name:        name,
		Description: "Reads files.",
		Tools:       []string{"Read"},
		Prompt:      "You are a delegate.",
		Source:      "builtin",
	}
}

func startTask(t *testing.T, session *Session, callID, agent, brief string) map[string]any {
	t.Helper()
	result, err := newTaskTool(session).Execute(
		context.Background(),
		callID,
		json.RawMessage(fmt.Sprintf(`{"agent":%q,"task":%q}`, agent, brief)),
		nil,
	)
	if err != nil {
		t.Fatalf("Task Execute: %v", err)
	}
	details, ok := result.Details.(map[string]any)
	if !ok {
		t.Fatalf("Task details = %#v", result.Details)
	}
	if details["status"] != delegationRunning {
		t.Fatalf("Task status = %v", details["status"])
	}
	return details
}

func TestTaskStartsInTheBackgroundAndTaskWaitReturnsTheReport(t *testing.T) {
	stream := &scriptedStream{}
	session := subagentTestSession(stream.Stream, []protocol.SubagentDefinition{readOnlyDefinition("explorer")})
	details := startTask(t, session, "task-call-1", "explorer", "find notes.md")
	delegationID, _ := details["delegationId"].(string)
	if delegationID == "" {
		t.Fatal("Task did not return a delegation id")
	}

	result, err := newTaskWaitTool(session).Execute(
		context.Background(),
		"wait-1",
		json.RawMessage(fmt.Sprintf(`{"delegationIds":[%q]}`, delegationID)),
		nil,
	)
	if err != nil {
		t.Fatalf("TaskWait Execute: %v", err)
	}
	if text := agentcore.ContentToText(result.Content); !strings.Contains(text, "done") {
		t.Fatalf("TaskWait report = %q", text)
	}
	waitDetails, _ := result.Details.(map[string]any)
	delegations, _ := waitDetails["delegations"].([]map[string]any)
	if len(delegations) != 1 || delegations[0]["status"] != "completed" {
		t.Fatalf("TaskWait details = %#v", waitDetails)
	}
	if executed := session.host.(*fakeHost).executedTools(); len(executed) != 1 || executed[0] != "Read" {
		t.Fatalf("child host calls = %#v", executed)
	}
	if session.toolErrors["task-call-1"] {
		t.Fatal("a running Task must not mark the call failed")
	}
}

// usageStream is scriptedStream with token accounting on the final turn.
type usageStream struct {
	mu    sync.Mutex
	calls int
}

func (s *usageStream) Stream(ctx context.Context, _ string, _ provider.LlmContext, _ provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
	s.mu.Lock()
	call := s.calls
	s.calls++
	s.mu.Unlock()
	stream := provider.NewAssistantMessageEventStream(4)
	go func() {
		defer stream.Close()
		now := time.Now().UnixMilli()
		if call == 0 {
			partial := agentcore.AssistantMessage{
				RoleField:  agentcore.RoleAssistant,
				Content:    agentcore.ContentList{agentcore.NewToolCallContent("read-1", "Read", json.RawMessage(`{"path":"notes.md"}`))},
				StopReason: agentcore.StopReasonToolUse,
				Timestamp:  now,
			}
			_ = stream.Emit(ctx, provider.StreamStartEvent{Partial: agentcore.AssistantMessage{RoleField: agentcore.RoleAssistant, Timestamp: now}})
			_ = stream.Emit(ctx, provider.StreamToolCallEvent{Partial: partial})
			_ = stream.Emit(ctx, provider.StreamDoneEvent{Message: partial})
			return
		}
		final := agentcore.AssistantMessage{
			RoleField:  agentcore.RoleAssistant,
			Content:    agentcore.ContentList{agentcore.NewTextContent("done")},
			StopReason: agentcore.StopReasonEndTurn,
			Usage:      &agentcore.Usage{InputTokens: 10, OutputTokens: 5},
			Timestamp:  now,
		}
		_ = stream.Emit(ctx, provider.StreamStartEvent{Partial: agentcore.AssistantMessage{RoleField: agentcore.RoleAssistant, Timestamp: now}})
		_ = stream.Emit(ctx, provider.StreamTextEvent{Partial: final})
		_ = stream.Emit(ctx, provider.StreamDoneEvent{Message: final})
	}()
	return stream, nil
}

func TestTaskUsageAggregatesIntoTheTurn(t *testing.T) {
	session := subagentTestSession((&usageStream{}).Stream, []protocol.SubagentDefinition{readOnlyDefinition("explorer")})
	startTask(t, session, "task-call-1", "explorer", "find notes.md")
	if _, err := newTaskWaitTool(session).Execute(context.Background(), "wait-1", nil, nil); err != nil {
		t.Fatalf("TaskWait Execute: %v", err)
	}
	usage := session.takeSubagentUsage()
	if usage == nil || usage.InputTokens != 10 || usage.OutputTokens != 5 || usage.TotalTokens != 15 {
		t.Fatalf("subagent usage = %#v", usage)
	}
}

func TestTaskListReportsRunningAndSettledDelegations(t *testing.T) {
	stream := &endlessToolStream{}
	session := subagentTestSession(stream.Stream, []protocol.SubagentDefinition{readOnlyDefinition("looper")})
	details := startTask(t, session, "task-call-1", "looper", "loop")
	delegationID, _ := details["delegationId"].(string)

	list, err := newTaskListTool(session).Execute(context.Background(), "list-1", nil, nil)
	if err != nil {
		t.Fatalf("TaskList Execute: %v", err)
	}
	listDetails, _ := list.Details.(map[string]any)
	summaries, _ := listDetails["delegations"].([]map[string]any)
	if len(summaries) != 1 || summaries[0]["delegationId"] != delegationID || summaries[0]["status"] != delegationRunning {
		t.Fatalf("TaskList details = %#v", listDetails)
	}

	stop, err := newTaskStopTool(session).Execute(context.Background(), "stop-1", nil, nil)
	if err != nil {
		t.Fatalf("TaskStop Execute: %v", err)
	}
	stopDetails, _ := stop.Details.(map[string]any)
	stopped, _ := stopDetails["stopped"].([]map[string]any)
	if len(stopped) != 1 || stopped[0]["status"] != delegationStopped {
		t.Fatalf("TaskStop details = %#v", stopDetails)
	}
}

func TestTaskStopEndsAnEndlessDelegateAsStopped(t *testing.T) {
	stream := &endlessToolStream{}
	session := subagentTestSession(stream.Stream, []protocol.SubagentDefinition{readOnlyDefinition("looper")})
	details := startTask(t, session, "task-call-1", "looper", "loop forever")
	delegationID, _ := details["delegationId"].(string)

	// A settled record is already complete; stopping it again must not hang.
	result, err := newTaskStopTool(session).Execute(
		context.Background(),
		"stop-1",
		json.RawMessage(fmt.Sprintf(`{"delegationIds":[%q]}`, delegationID)),
		nil,
	)
	if err != nil {
		t.Fatalf("TaskStop Execute: %v", err)
	}
	if text := agentcore.ContentToText(result.Content); !strings.Contains(text, "Stopped 1 subagent") {
		t.Fatalf("TaskStop text = %q", text)
	}
	record := session.delegation(delegationID)
	if record == nil {
		t.Fatal("stopped delegation disappeared from the registry")
	}
	if snapshot := record.snapshot(); snapshot.status != delegationStopped {
		t.Fatalf("status = %q", snapshot.status)
	}
}

func TestTaskReportsAnUnresolvedModelPinImmediately(t *testing.T) {
	definition := readOnlyDefinition("fixer")
	definition.Model = &protocol.SubagentModelPin{ProviderID: "openrouter", ModelID: "cheap-model"}
	session := subagentTestSession((&scriptedStream{}).Stream, []protocol.SubagentDefinition{definition})

	result, err := newTaskTool(session).Execute(
		context.Background(),
		"call-1",
		json.RawMessage(`{"agent":"fixer","task":"do it"}`),
		nil,
	)
	if err != nil {
		t.Fatalf("Task Execute: %v", err)
	}
	if text := agentcore.ContentToText(result.Content); !strings.Contains(text, "openrouter/cheap-model") {
		t.Fatalf("error text = %q", text)
	}
	if !session.toolErrors["call-1"] {
		t.Fatal("an unresolved pin must mark the Task call failed")
	}
	if len(session.runningDelegations()) != 0 {
		t.Fatal("an unresolved pin must not start a delegation")
	}
}

// endlessToolStream always asks for another tool call, so a bounded delegate
// only stops through its maxTurns cap or TaskStop.
type endlessToolStream struct {
	mu    sync.Mutex
	calls int
}

func (s *endlessToolStream) Stream(ctx context.Context, _ string, _ provider.LlmContext, _ provider.StreamConfig) (*provider.AssistantMessageEventStream, error) {
	s.mu.Lock()
	s.calls++
	id := fmt.Sprintf("child-%d", s.calls)
	s.mu.Unlock()
	stream := provider.NewAssistantMessageEventStream(4)
	go func() {
		defer stream.Close()
		now := time.Now().UnixMilli()
		partial := agentcore.AssistantMessage{
			RoleField:  agentcore.RoleAssistant,
			Content:    agentcore.ContentList{agentcore.NewToolCallContent(id, "Read", json.RawMessage(`{"path":"notes.md"}`))},
			StopReason: agentcore.StopReasonToolUse,
			Timestamp:  now,
		}
		_ = stream.Emit(ctx, provider.StreamStartEvent{Partial: agentcore.AssistantMessage{RoleField: agentcore.RoleAssistant, Timestamp: now}})
		_ = stream.Emit(ctx, provider.StreamToolCallEvent{Partial: partial})
		_ = stream.Emit(ctx, provider.StreamDoneEvent{Message: partial})
	}()
	return stream, nil
}

func TestFollowUpMessagesDeliversAnUnreadReport(t *testing.T) {
	stream := &scriptedStream{}
	session := subagentTestSession(stream.Stream, []protocol.SubagentDefinition{readOnlyDefinition("explorer")})
	startTask(t, session, "task-call-1", "explorer", "find notes.md")

	messages := session.followUpMessages(context.Background())
	if len(messages) != 1 {
		t.Fatalf("follow-up messages = %d, want 1", len(messages))
	}
	user, ok := messages[0].(agentcore.UserMessage)
	if !ok {
		t.Fatalf("follow-up message type = %T", messages[0])
	}
	text := agentcore.ContentToText(user.Content)
	if !strings.Contains(text, delegationResumePrompt) || !strings.Contains(text, "done") {
		t.Fatalf("follow-up text = %q", text)
	}
	records := session.delegationList()
	if len(records) != 1 || !records[0].snapshot().reportDelivered {
		t.Fatalf("reportDelivered = %#v", records)
	}
}

func TestTaskWaitReportsMaxTurnsTruncation(t *testing.T) {
	definition := readOnlyDefinition("looper")
	definition.MaxTurns = intPtr(2)
	session := subagentTestSession((&endlessToolStream{}).Stream, []protocol.SubagentDefinition{definition})
	details := startTask(t, session, "task-call-1", "looper", "loop")
	delegationID, _ := details["delegationId"].(string)

	result, err := newTaskWaitTool(session).Execute(
		context.Background(),
		"wait-1",
		json.RawMessage(fmt.Sprintf(`{"delegationIds":[%q]}`, delegationID)),
		nil,
	)
	if err != nil {
		t.Fatalf("TaskWait Execute: %v", err)
	}
	waitDetails, _ := result.Details.(map[string]any)
	delegations, _ := waitDetails["delegations"].([]map[string]any)
	if len(delegations) != 1 || delegations[0]["status"] != "truncated" {
		t.Fatalf("TaskWait details = %#v", waitDetails)
	}
	if text := agentcore.ContentToText(result.Content); !strings.Contains(text, "maxTurns cap of 2") {
		t.Fatalf("report = %q", text)
	}
}

func TestResolveSubagentToolNamesHonoursInheritAndDenyList(t *testing.T) {
	definition := protocol.SubagentDefinition{Tools: []string{"Bash"}, InheritTools: true}
	resolved := resolveSubagentToolNames(definition, []string{"Read", "Skill", "asktool", "Task", "TaskWait", "Bash"})
	want := []string{"Read", "Skill", "Bash"}
	if strings.Join(resolved, ",") != strings.Join(want, ",") {
		t.Fatalf("resolved = %#v, want %#v", resolved, want)
	}
	definition.InheritTools = false
	if got := resolveSubagentToolNames(definition, []string{"Read"}); len(got) != 1 || got[0] != "Bash" {
		t.Fatalf("declared-only tools = %#v", got)
	}
}

func TestBrowserPreviewForwardsToTheShellAndFoldsTheAlias(t *testing.T) {
	session := subagentTestSession((&scriptedStream{}).Stream, nil)
	tool := &browserPreviewTool{session: session}
	result, err := tool.Execute(
		context.Background(),
		"preview-1",
		json.RawMessage(`{"file_path":"demo/index.html"}`),
		nil,
	)
	if err != nil {
		t.Fatalf("BrowserPreview Execute: %v", err)
	}
	if text := agentcore.ContentToText(result.Content); !strings.Contains(text, "hello from host") {
		t.Fatalf("result = %q", text)
	}
	if executed := session.host.(*fakeHost).executedTools(); len(executed) != 1 || executed[0] != BrowserPreviewToolName {
		t.Fatalf("host calls = %#v", executed)
	}
}

func TestBrowserPreviewPathNormalizesAndRejectsEscapes(t *testing.T) {
	cases := []struct {
		input   string
		want    string
		wantErr bool
	}{
		{"demo/index.html", "demo/index.html", false},
		{"./demo//index.html", "demo/index.html", false},
		{"demo\\index.html", "demo/index.html", false},
		{"  demo/index.html  ", "demo/index.html", false},
		{"notes: draft.html", "notes: draft.html", false},
		{"", "", true},
		{"   ", "", true},
		{".", "", true},
		{"/etc/passwd", "", true},
		{"C:/demo/index.html", "", true},
		{"C:demo/index.html", "", true},
		{"demo/../index.html", "", true},
		{"..", "", true},
	}
	for _, test := range cases {
		got, err := normalizeBrowserPreviewPath(test.input)
		if test.wantErr {
			if err == nil {
				t.Fatalf("normalizeBrowserPreviewPath(%q) = %q, want an error", test.input, got)
			}
			continue
		}
		if err != nil || got != test.want {
			t.Fatalf("normalizeBrowserPreviewPath(%q) = %q, %v; want %q", test.input, got, err, test.want)
		}
	}
}

func TestBrowserPreviewRejectsAnEmptyPath(t *testing.T) {
	session := subagentTestSession((&scriptedStream{}).Stream, nil)
	result, _ := (&browserPreviewTool{session: session}).Execute(context.Background(), "preview-1", nil, nil)
	if text := agentcore.ContentToText(result.Content); !strings.Contains(text, "requires a workspace-relative") {
		t.Fatalf("result = %q", text)
	}
	if !session.toolErrors["preview-1"] {
		t.Fatal("an empty preview path must mark the call failed")
	}
}

func intPtr(value int) *int { return &value }
