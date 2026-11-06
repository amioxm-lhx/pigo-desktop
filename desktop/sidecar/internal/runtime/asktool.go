package runtime

import (
	"context"
	"encoding/json"
	"strings"

	"github.com/smallnest/pigo/desktop/sidecar/internal/jsonrpc"
	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/internal/agentcore"
)

// AskToolName is the model-facing name of the interactive question tool.
const AskToolName = "asktool"

const askToolDescription = "Ask the user one or more questions. Each question has selectable options and the desktop card always provides a custom user-input option; unanswered questions are returned as empty answers."

const askToolInvalidText = AskToolName + " requires one or more questions, each with a question and at least one option."

const maxAskToolQuestions = 20

var askToolSchema = json.RawMessage(`{
  "type": "object",
  "properties": {
    "questions": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "question": {"type": "string"},
          "options": {"type": "array", "items": {"type": "string"}},
          "multiSelect": {"type": "boolean"}
        },
        "required": ["question", "options"]
      }
    }
  },
  "required": ["questions"]
}`)

// askTool is the sidecar-local interactive question tool. A call pauses the
// turn, emits asktool_request, and waits for asktool.resolve from the renderer.
type askToolImpl struct {
	session *Session
}

func newAskTool(session *Session) agentcore.AgentTool {
	return &askToolImpl{session: session}
}

func (t *askToolImpl) Name() string        { return AskToolName }
func (t *askToolImpl) Description() string { return askToolDescription }
func (t *askToolImpl) Schema() json.RawMessage {
	return append(json.RawMessage(nil), askToolSchema...)
}
func (t *askToolImpl) ExecutionMode() agentcore.ToolExecutionMode {
	return agentcore.ToolExecutionSequential
}

func (t *askToolImpl) Execute(ctx context.Context, id string, args json.RawMessage, _ agentcore.ToolUpdateFunc) (agentcore.AgentToolResult, error) {
	var params any
	if err := json.Unmarshal(args, &params); err != nil {
		return t.invalid(id), nil
	}
	questions := normalizeAskQuestions(params)
	if questions == nil {
		return t.invalid(id), nil
	}

	request := protocol.AskToolRequest{
		RequestID:  newID(),
		SessionID:  t.session.sessionID,
		ToolCallID: id,
		Questions:  questions,
	}
	if ctx == nil {
		ctx = context.Background()
	}
	answers := t.session.waitForAskTool(ctx, request)
	return agentcore.AgentToolResult{
		Content: agentcore.ContentList{agentcore.NewTextContent(formatAskToolOutput(questions, answers))},
		Details: askToolDetails{Questions: questions, Answers: answers},
	}, nil
}

func (t *askToolImpl) invalid(toolCallID string) agentcore.AgentToolResult {
	t.session.markToolError(toolCallID)
	return agentcore.AgentToolResult{
		Content: agentcore.ContentList{agentcore.NewTextContent(askToolInvalidText)},
		Details: map[string]any{"errorCode": "ASKTOOL_INVALID_ARGUMENT"},
	}
}

// pendingAsk is one question set waiting for the renderer.
type pendingAsk struct {
	request protocol.AskToolRequest
	resolve func(answers [][]string)
}

type askToolDetails struct {
	Questions []protocol.AskToolQuestion `json:"questions"`
	Answers   [][]string                 `json:"answers"`
}

func (s *Session) waitForAskTool(ctx context.Context, request protocol.AskToolRequest) [][]string {
	delivered := make(chan [][]string, 1)
	s.mu.Lock()
	s.pendingAskTools[request.RequestID] = &pendingAsk{
		request: request,
		resolve: func(answers [][]string) {
			delivered <- answers
		},
	}
	turnID := s.currentTurn
	s.mu.Unlock()

	s.emitEvent(turnID, protocol.AskToolRequestEvent{
		Type:    protocol.TypeAskToolRequest,
		Request: request,
	})

	select {
	case answers := <-delivered:
		return answers
	case <-ctx.Done():
		s.resolveAsk(request.RequestID, skippedAnswers(len(request.Questions)))
		return <-delivered
	}
}

// ResolveAskTool answers one pending asktool request.
func (s *Session) ResolveAskTool(params protocol.AskToolParams) (protocol.OKResult, error) {
	if strings.TrimSpace(params.SessionID) != "" && params.SessionID != s.sessionID {
		return protocol.OKResult{}, jsonrpc.AppError("ASKTOOL_NOT_FOUND", "asktool session mismatch")
	}

	s.mu.Lock()
	pending := s.pendingAskTools[params.RequestID]
	s.mu.Unlock()
	if pending == nil {
		return protocol.OKResult{}, jsonrpc.AppError("ASKTOOL_NOT_FOUND", "asktool request is no longer pending")
	}
	if len(params.Answers) != len(pending.request.Questions) {
		return protocol.OKResult{}, jsonrpc.AppError("ASKTOOL_INVALID_ARGUMENT", "asktool answer count does not match questions")
	}

	answers := make([][]string, 0, len(params.Answers))
	for index, raw := range params.Answers {
		answer, err := decodeAskAnswer(raw, index+1)
		if err != nil {
			return protocol.OKResult{}, err
		}
		if !pending.request.Questions[index].MultiSelect && len(answer) > 1 {
			return protocol.OKResult{}, jsonrpc.AppError("ASKTOOL_INVALID_ARGUMENT", "asktool question accepts one answer")
		}
		answers = append(answers, answer)
	}

	s.resolveAsk(params.RequestID, answers)
	return protocol.OKResult{OK: true}, nil
}

func (s *Session) resolveAsk(requestID string, answers [][]string) bool {
	s.mu.Lock()
	entry := s.pendingAskTools[requestID]
	if entry == nil {
		s.mu.Unlock()
		return false
	}
	delete(s.pendingAskTools, requestID)
	s.mu.Unlock()

	entry.resolve(answers)
	return true
}

// resolvePendingAskTools answers every outstanding request as skipped, which is
// what an aborted or disposed turn owes the renderer.
func (s *Session) resolvePendingAskTools() {
	s.mu.Lock()
	counts := make(map[string]int, len(s.pendingAskTools))
	for requestID, entry := range s.pendingAskTools {
		counts[requestID] = len(entry.request.Questions)
	}
	s.mu.Unlock()

	for requestID, count := range counts {
		s.resolveAsk(requestID, skippedAnswers(count))
	}
}

func decodeAskAnswer(raw json.RawMessage, position int) ([]string, error) {
	trimmed := strings.TrimSpace(string(raw))
	if trimmed == "" || trimmed == "null" {
		return nil, nil
	}
	var values []any
	if err := json.Unmarshal(raw, &values); err != nil {
		return nil, jsonrpc.AppError("ASKTOOL_INVALID_ARGUMENT", "asktool answer must be an array or null")
	}
	answers := make([]string, 0, len(values))
	seen := make(map[string]bool, len(values))
	for _, value := range values {
		text, ok := value.(string)
		if !ok {
			continue
		}
		text = strings.TrimSpace(text)
		if text == "" || seen[text] {
			continue
		}
		seen[text] = true
		answers = append(answers, text)
	}
	return answers, nil
}

func skippedAnswers(count int) [][]string {
	return make([][]string, count)
}

func formatAskToolOutput(questions []protocol.AskToolQuestion, answers [][]string) string {
	lines := make([]string, 0, len(questions))
	for index, question := range questions {
		answer := ""
		if index < len(answers) && answers[index] != nil {
			answer = strings.Join(answers[index], "、")
		}
		lines = append(lines, question.Question+"："+answer)
	}
	return strings.Join(lines, "\n---\n")
}

func normalizeAskQuestions(params any) []protocol.AskToolQuestion {
	record, ok := params.(map[string]any)
	if !ok {
		return nil
	}
	raw, ok := record["questions"].([]any)
	if !ok {
		return nil
	}
	questions := make([]protocol.AskToolQuestion, 0, len(raw))
	for _, entry := range raw {
		item, ok := entry.(map[string]any)
		if !ok {
			return nil
		}
		question, _ := item["question"].(string)
		question = strings.TrimSpace(question)

		options := make([]string, 0)
		if values, ok := item["options"].([]any); ok {
			seen := make(map[string]bool, len(values))
			for _, value := range values {
				option, ok := value.(string)
				if !ok {
					continue
				}
				option = strings.TrimSpace(option)
				if option == "" || seen[option] {
					continue
				}
				seen[option] = true
				options = append(options, option)
			}
		}
		if question == "" || len(options) == 0 {
			return nil
		}
		multiSelect, _ := item["multiSelect"].(bool)
		questions = append(questions, protocol.AskToolQuestion{
			Question:    question,
			Options:     options,
			MultiSelect: multiSelect,
		})
	}
	if len(questions) == 0 || len(questions) > maxAskToolQuestions {
		return nil
	}
	return questions
}
