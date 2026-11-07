// Package protocol names the frozen methods and payload shapes shared with the
// Pi Desktop Electron main process. The wire contract is the one the existing
// Node sidecar and the pi-go sidecar speak; the pigo sidecar must stay
// interchangeable with them.
package protocol

import "encoding/json"

// Inbound methods: Electron main calls the sidecar.
const (
	MethodSidecarConfigure = "sidecar.configure"
	MethodSidecarHealth    = "sidecar.health"

	MethodAgentPrompt         = "agent.prompt"
	MethodAgentSteer          = "agent.steer"
	MethodAgentAbort          = "agent.abort"
	MethodAgentStop           = "agent.stop"
	MethodAgentGetStatus      = "agent.getStatus"
	MethodAgentDisposeSession = "agent.disposeSession"
	// MethodAgentRebuild drops a live runtime and rebuilds its model context
	// from the host transcript without opening a turn. It is the desktop
	// counterpart of the CLI's /rebuild.
	MethodAgentRebuild     = "agent.rebuild"
	MethodAgentExecutePlan = "agent.executeApprovedPlan"
	MethodAgentCompact     = "agent.compact"
	// MethodAgentBtw answers an out-of-band question against a read-only
	// snapshot of the main transcript. It never creates a Session, appends a
	// durable row, or opens a host turn.
	MethodAgentBtw       = "agent.btw"
	MethodAgentBtwCancel = "agent.btwCancel"
	MethodAskToolResolve = "asktool.resolve"
	MethodPromptEnhance  = "prompt.enhance"
	MethodSessionTitle   = "session.summarizeTitle"
	MethodPluginComplete = "plugin.complete"
	// MethodMemoryReport answers /memory: the persistent-memory store state and
	// per-scope entry counts. It never writes the transcript.
	MethodMemoryReport = "memory.report"
	// MethodAgentDream runs one memory-consolidation pass (/dream) with the
	// session's own model. It is the desktop counterpart of `pigo --dream`.
	MethodAgentDream = "agent.dream"
)

// Outbound methods: the sidecar calls Electron main.
const (
	// MethodHostProxy forwards one host method to Electron main. Its params are
	// {"method": string, "params": object}.
	MethodHostProxy = "host.proxy"
	// MethodAgentEvent streams one AgentEventEnvelope to Electron main.
	MethodAgentEvent = "agent.event"
)

// ConfigureParams is the sidecar.configure payload.
type ConfigureParams struct {
	HostBinary   string          `json:"hostBinary,omitempty"`
	DataDir      string          `json:"dataDir,omitempty"`
	NetworkProxy json.RawMessage `json:"networkProxy,omitempty"`
}

// ConfigureResult mirrors the Node sidecar's reply.
type ConfigureResult struct {
	OK   bool   `json:"ok"`
	Mode string `json:"mode"`
}

// HealthResult reports the sidecar's state.
type HealthResult struct {
	OK       bool   `json:"ok"`
	Version  string `json:"version,omitempty"`
	Runtimes int    `json:"runtimes"`
}

// SessionParams is the common {sessionId} request shape.
type SessionParams struct {
	SessionID string `json:"sessionId"`
}

// StopResult is the {requested:boolean} reply agent.stop returns.
type StopResult struct {
	Requested bool `json:"requested"`
}

// BtwParams is the agent.btw payload. Binding is the shell-resolved launch
// descriptor (provider/model/instructions) for the session; it carries no
// turn id because a side question never opens a durable turn. The answer is
// streamed as scoped agent.event notifications, not stored in the transcript.
type BtwParams struct {
	SessionID string       `json:"sessionId"`
	SideID    string       `json:"sideId,omitempty"`
	Question  string       `json:"question"`
	Binding   PromptParams `json:"binding"`
}

// BtwCancelParams is the agent.btwCancel payload.
type BtwCancelParams struct {
	SideID string `json:"sideId"`
}

// PromptAttachment is one file the renderer attached to a turn. Only inlined
// images with Data participate in the model request; file attachments stay
// metadata in the host transcript.
type PromptAttachment struct {
	Path     string `json:"path,omitempty"`
	Name     string `json:"name,omitempty"`
	Kind     string `json:"kind,omitempty"`
	MimeType string `json:"mimeType,omitempty"`
	Size     *int64 `json:"size,omitempty"`
	Data     string `json:"data,omitempty"`
}

// SteerParams is the agent.steer payload.
type SteerParams struct {
	SessionID      string             `json:"sessionId"`
	ExpectedTurnID string             `json:"expectedTurnId"`
	Content        string             `json:"content"`
	Attachments    []PromptAttachment `json:"attachments,omitempty"`
	Message        struct {
		ID        string `json:"id"`
		Role      string `json:"role"`
		Content   string `json:"content"`
		CreatedAt string `json:"createdAt"`
		Steering  bool   `json:"steering"`
	} `json:"message"`
}

// AskToolQuestion is one model-created question shown in the inline card.
type AskToolQuestion struct {
	Question    string   `json:"question"`
	Options     []string `json:"options"`
	MultiSelect bool     `json:"multiSelect,omitempty"`
}

// AskToolRequest is the asktool_request event payload.
type AskToolRequest struct {
	RequestID  string            `json:"requestId"`
	SessionID  string            `json:"sessionId"`
	ToolCallID string            `json:"toolCallId"`
	Questions  []AskToolQuestion `json:"questions"`
}

// AskToolParams is the asktool.resolve payload. Answers keep JSON null for a
// skipped question, so each entry is decoded individually.
type AskToolParams struct {
	SessionID string            `json:"sessionId"`
	RequestID string            `json:"requestId"`
	Answers   []json.RawMessage `json:"answers"`
}

// PlanArtifact is the host-created approval artifact.
type PlanArtifact struct {
	RelativePath string `json:"relativePath"`
	SHA256       string `json:"sha256"`
	SizeBytes    int64  `json:"sizeBytes"`
}

// PlanProposal is the host's durable approval record.
type PlanProposal struct {
	ID                   string        `json:"id"`
	SessionID            string        `json:"sessionId"`
	TurnID               string        `json:"turnId"`
	ToolCallID           string        `json:"toolCallId"`
	Kind                 string        `json:"kind"`
	Title                string        `json:"title"`
	Markdown             string        `json:"markdown"`
	Question             string        `json:"question"`
	Artifact             *PlanArtifact `json:"artifact,omitempty"`
	Version              int64         `json:"version"`
	Status               string        `json:"status"`
	CreatedAt            string        `json:"createdAt"`
	UpdatedAt            string        `json:"updatedAt"`
	ExpiresAt            string        `json:"expiresAt,omitempty"`
	ResolvedAt           string        `json:"resolvedAt,omitempty"`
	Action               string        `json:"action,omitempty"`
	TargetPermissionMode string        `json:"targetPermissionMode,omitempty"`
	ErrorCode            string        `json:"errorCode,omitempty"`
	ExecutionID          string        `json:"executionId,omitempty"`
	ExecutionState       string        `json:"executionState,omitempty"`
	Plan                 string        `json:"plan"`
}

// PlanExecution is the approved descriptor consumed by an execution turn.
type PlanExecution struct {
	ID                   string        `json:"id"`
	ProposalID           string        `json:"proposalId"`
	SessionID            string        `json:"sessionId"`
	Kind                 string        `json:"kind"`
	Plan                 string        `json:"plan"`
	Title                string        `json:"title"`
	Question             string        `json:"question"`
	Artifact             *PlanArtifact `json:"artifact"`
	TargetPermissionMode string        `json:"targetPermissionMode"`
	State                string        `json:"state"`
}

// EnhancePromptParams is the prompt.enhance payload: the provider binding plus
// the draft to rewrite.
type EnhancePromptParams struct {
	PromptParams
	Draft string `json:"draft"`
}

// SummarizeTitleParams is the session.summarizeTitle payload.
type SummarizeTitleParams struct {
	PromptParams
	UserPrompt     string `json:"userPrompt"`
	AssistantReply string `json:"assistantReply,omitempty"`
}

// PluginCompleteParams is the plugin.complete payload: a one-shot completion
// for a plugin tool with an optional system prompt and message history.
type PluginCompleteParams struct {
	PromptParams
	System   string                  `json:"system,omitempty"`
	Messages []PluginCompleteMessage `json:"messages,omitempty"`
}

// PluginCompleteMessage is one user or assistant message in a plugin
// completion request.
type PluginCompleteMessage struct {
	Role    string `json:"role"`
	Content string `json:"content"`
}

// PluginSkill is one catalog entry Electron main resolved from the user's and
// plugins' skill documents. The body is loaded on demand through the Skill tool.
type PluginSkill struct {
	ID          string `json:"id"`
	Name        string `json:"name"`
	Description string `json:"description,omitempty"`
	Path        string `json:"path,omitempty"`
}

// SubagentModelPin is the provider/model a definition pins, resolved by the
// shell into SubagentProviders under this key.
type SubagentModelPin struct {
	ProviderID string `json:"providerId"`
	ModelID    string `json:"modelId"`
}

// SubagentDefinition is one delegate document (builtin or the user's global
// registry). The Markdown body becomes the child's system prompt; tools default
// to read-only and `tools: inherit` opts into the parent's live tool catalog.
type SubagentDefinition struct {
	Name               string            `json:"name"`
	Description        string            `json:"description"`
	Tools              []string          `json:"tools"`
	InheritTools       bool              `json:"inheritTools,omitempty"`
	Model              *SubagentModelPin `json:"model,omitempty"`
	ThinkingLevel      string            `json:"thinkingLevel,omitempty"`
	Permission         string            `json:"permission,omitempty"`
	MaxTurns           *int              `json:"maxTurns,omitempty"`
	MaxTokens          *int              `json:"maxTokens,omitempty"`
	IdleTimeoutSeconds int               `json:"idleTimeoutSeconds,omitempty"`
	MaxDurationSeconds int               `json:"maxDurationSeconds,omitempty"`
	Prompt             string            `json:"prompt"`
	Source             string            `json:"source,omitempty"`
	FilePath           string            `json:"filePath,omitempty"`
}

// ExecuteApprovedPlanParams is the agent.executeApprovedPlan payload: the
// prompt binding plus the approved execution descriptor.
type ExecuteApprovedPlanParams struct {
	PromptParams
	Execution *PlanExecution `json:"execution"`
}

// AgentStatus mirrors AgentStatus in the desktop shared types.
type AgentStatus struct {
	SessionID                string         `json:"sessionId"`
	IsRunning                bool           `json:"isRunning"`
	CurrentTurnID            string         `json:"currentTurnId,omitempty"`
	ModelID                  string         `json:"modelId,omitempty"`
	PendingToolConfirmations int            `json:"pendingToolConfirmations"`
	PlanningState            string         `json:"planningState,omitempty"`
	PendingPlanID            string         `json:"pendingPlanId,omitempty"`
	Activity                 *AgentActivity `json:"activity,omitempty"`
}

// AgentActivity explains a quiet interval in an active turn.
type AgentActivity struct {
	Phase         string               `json:"phase"`
	Since         int64                `json:"since"`
	Reason        string               `json:"reason,omitempty"`
	Attempt       int                  `json:"attempt,omitempty"`
	RetryDelayMs  int64                `json:"retryDelayMs,omitempty"`
	Error         *AgentActivityError  `json:"error,omitempty"`
	SubagentCount int                  `json:"subagentCount,omitempty"`
	Agents        []AgentActivityAgent `json:"agents,omitempty"`
}

// AgentActivityError is the bounded provider diagnostic shown while retrying.
type AgentActivityError struct {
	Code           string `json:"code"`
	Message        string `json:"message"`
	ProviderStatus int    `json:"providerStatus,omitempty"`
}

// AgentActivityAgent is one child agent's coarse activity.
type AgentActivityAgent struct {
	Name         string `json:"name"`
	LastPhase    string `json:"lastPhase,omitempty"`
	LastToolName string `json:"lastToolName,omitempty"`
}

// AgentStatusResult wraps the status, matching the Node sidecar's reply shape.
type AgentStatusResult struct {
	Status AgentStatus `json:"status"`
}

// OKResult is the {ok:true} reply several methods share.
type OKResult struct {
	OK bool `json:"ok"`
}

// RebuildResult reports whether a live runtime was actually replaced. A
// session with no live runtime needs no rebuild: its next prompt builds one
// from the host transcript anyway.
type RebuildResult struct {
	OK      bool `json:"ok"`
	Rebuilt bool `json:"rebuilt"`
}

// MemoryReportParams is the memory.report payload. Root overrides the resolved
// memory root; production leaves it empty so the sidecar applies the CLI's own
// resolution ($PIGO_HOME/memory, else ~/.pigo/memory).
type MemoryReportParams struct {
	Root string `json:"root,omitempty"`
}

// DreamParams is the agent.dream payload: the session's launch binding (so the
// pass consolidates with the model the user is talking to) plus the project
// scope and the dry-run switch.
type DreamParams struct {
	// PromptParams carries the resolved ProviderConfig; only Provider,
	// ThinkingLevel and ProjectPath are read.
	PromptParams
	SessionID string `json:"sessionId"`
	DryRun    bool   `json:"dryRun,omitempty"`
}

// DreamResult is the agent.dream reply. The report keeps the CLI's field names
// (snake_case) so a desktop run and `pigo --dream` produce the same JSON.
type DreamResult struct {
	OK     bool            `json:"ok"`
	Report json.RawMessage `json:"report"`
}

// ProviderConfig mirrors RuntimeProviderConfig in the desktop sidecar. Electron
// main resolves the provider row and ships it with every prompt.
type ProviderConfig struct {
	ID                      string            `json:"id"`
	Name                    string            `json:"name"`
	VendorKey               string            `json:"vendorKey,omitempty"`
	BaseURL                 string            `json:"baseUrl,omitempty"`
	ModelID                 string            `json:"modelId"`
	APIKey                  string            `json:"apiKey"`
	AuthKind                string            `json:"authKind,omitempty"`
	APIStyle                string            `json:"apiStyle,omitempty"`
	SupportsReasoning       bool              `json:"supportsReasoning"`
	SupportedThinkingLevels []string          `json:"supportedThinkingLevels,omitempty"`
	ModelConfig             *ModelConfig      `json:"modelConfig,omitempty"`
	Headers                 map[string]string `json:"headers,omitempty"`
}

// ModelConfig is the models.dev metadata Electron main resolved for the row.
type ModelConfig struct {
	Name             string             `json:"name,omitempty"`
	API              string             `json:"api,omitempty"`
	BaseURL          string             `json:"baseUrl,omitempty"`
	Reasoning        bool               `json:"reasoning,omitempty"`
	ToolCall         *bool              `json:"toolCall,omitempty"`
	ThinkingLevelMap map[string]*string `json:"thinkingLevelMap,omitempty"`
	Input            []string           `json:"input,omitempty"`
	ContextWindow    int                `json:"contextWindow,omitempty"`
	MaxTokens        int                `json:"maxTokens,omitempty"`
	Headers          map[string]string  `json:"headers,omitempty"`
}

// CommandShell is the session's active shell, chosen by Electron main.
type CommandShell struct {
	ID        string `json:"id"`
	Label     string `json:"label,omitempty"`
	Dialect   string `json:"dialect"`
	Available bool   `json:"available"`
}

// PromptParams is the agent.prompt payload.
type PromptParams struct {
	SessionID      string             `json:"sessionId"`
	Mode           string             `json:"mode"`
	TurnID         string             `json:"turnId"`
	ThinkingLevel  string             `json:"thinkingLevel"`
	Provider       ProviderConfig     `json:"provider"`
	CommandShell   CommandShell       `json:"commandShell"`
	Content        string             `json:"content"`
	UserMessageID  string             `json:"userMessageId"`
	Attachments    []PromptAttachment `json:"attachments,omitempty"`
	AttachmentsDir string             `json:"attachmentsDir,omitempty"`
	PluginSkills   []PluginSkill      `json:"pluginSkills,omitempty"`
	// PluginTools are loaded plugins' agent tools. The name is already the
	// forced `plugin_<pluginId>_<tool>` product; host-core gates permission,
	// risk and plan-safety, the sidecar only exposes the schema.
	PluginTools []PluginTool `json:"pluginTools,omitempty"`

	// Subagent catalog and its resolved model pins. SubagentProviders is keyed
	// by "<providerId>/<modelId>"; a pin missing from it is a runtime tool error
	// rather than a fallback to the session model.
	Subagents         []SubagentDefinition      `json:"subagents,omitempty"`
	SubagentProviders map[string]ProviderConfig `json:"subagentProviders,omitempty"`
	SubagentModelKeys []string                  `json:"subagentModelKeys,omitempty"`

	// TruncateFromMessageID is the first transcript row to drop before this
	// prompt (regenerate/edit-resend). TruncateBefore is the deprecated count
	// fallback. RefreshHistory forces a live runtime to reload the transcript;
	// it is a pigo-side extension for hosts that mutate a session externally.
	TruncateFromMessageID string `json:"truncateFromMessageId,omitempty"`
	TruncateBefore        *int   `json:"truncateBefore,omitempty"`
	RefreshHistory        bool   `json:"refreshHistory,omitempty"`

	ProjectPath string `json:"projectPath"`
	// Prompt context resolved by Electron main. The sidecar renders these
	// verbatim; pigo's own environment/AGENTS.md blocks are never injected.
	ProjectInstructions json.RawMessage `json:"projectInstructions"`
	ProjectMemory       string          `json:"projectMemory"`
	ScratchDir          string          `json:"scratchDir"`
}

// ToolDefinition is one entry of the host tool catalog (tools.list).
type ToolDefinition struct {
	Name        string          `json:"name"`
	Description string          `json:"description"`
	Risk        string          `json:"risk,omitempty"`
	Parameters  json.RawMessage `json:"parameters"`
}

// PluginTool is one agent tool contributed by a loaded plugin on this prompt.
type PluginTool struct {
	Name        string          `json:"name"`
	Description string          `json:"description"`
	Risk        string          `json:"risk,omitempty"`
	Parameters  json.RawMessage `json:"parameters"`
	PluginID    string          `json:"pluginId,omitempty"`
	// Action names host-core may admit in Plan/Goal; the desktop runtime
	// enforces the per-action gate (ADR 0211).
	PlanSafeActions []string `json:"planSafeActions,omitempty"`
}

// ToolExecuteParams is the tools.execute request the sidecar proxies.
type ToolExecuteParams struct {
	SessionID  string          `json:"sessionId"`
	TurnID     string          `json:"turnId,omitempty"`
	ToolCallID string          `json:"toolCallId"`
	ToolName   string          `json:"toolName"`
	Args       json.RawMessage `json:"args"`
	Mode       string          `json:"mode"`

	// PermissionScope is a subagent's permission override (ADR 0089). `inherit`
	// and absent both follow the session's effective permission mode.
	PermissionScope string `json:"permissionScope,omitempty"`

	// DeclaredRisk is the manifest-declared risk for a plugin tool; host-core
	// treats a missing declaration as medium. PlanSafeActions lets host-core
	// admit a plugin tool in contract modes (the runtime still gates actions).
	DeclaredRisk    string   `json:"declaredRisk,omitempty"`
	PlanSafeActions []string `json:"planSafeActions,omitempty"`

	ExpectedCommandShellID      string `json:"expectedCommandShellId,omitempty"`
	ExpectedCommandShellDialect string `json:"expectedCommandShellDialect,omitempty"`
	TimeoutMs                   *int64 `json:"timeoutMs,omitempty"`
}

// ToolExecuteResult is the host's answer for one tool call.
type ToolExecuteResult struct {
	OK        bool            `json:"ok"`
	Content   json.RawMessage `json:"content"`
	IsError   bool            `json:"isError,omitempty"`
	ErrorCode string          `json:"errorCode,omitempty"`
	Denied    bool            `json:"denied,omitempty"`
}

// MessageUsage is one provider usage record.
type MessageUsage struct {
	InputTokens  int `json:"inputTokens"`
	OutputTokens int `json:"outputTokens"`
	TotalTokens  int `json:"totalTokens"`
}

// UiMessage mirrors the transcript row the sidecar streams to Electron main.
type UiMessage struct {
	ID        string `json:"id"`
	Role      string `json:"role"`
	Content   string `json:"content"`
	Thinking  string `json:"thinking,omitempty"`
	CreatedAt string `json:"createdAt"`
	Status    string `json:"status,omitempty"`

	ModelID    string `json:"modelId,omitempty"`
	ProviderID string `json:"providerId,omitempty"`

	Usage *MessageUsage `json:"usage,omitempty"`

	Error *AppError `json:"error,omitempty"`

	ToolName   string          `json:"toolName,omitempty"`
	ToolCallID string          `json:"toolCallId,omitempty"`
	ToolStatus string          `json:"toolStatus,omitempty"`
	ToolArgs   json.RawMessage `json:"toolArgs,omitempty"`
	ToolResult json.RawMessage `json:"toolResult,omitempty"`

	// ParentToolCallID and AgentName mark a row a subagent produced (D201,
	// ADR 0062): the `Task` call that spawned the delegate and the definition
	// it ran under. The transcript nests these under that Task card, and the
	// runtime excludes them from the parent's model context.
	ParentToolCallID string `json:"parentToolCallId,omitempty"`
	AgentName        string `json:"agentName,omitempty"`

	Steering *bool `json:"steering,omitempty"`
	IsError  bool  `json:"isError,omitempty"`
}

// Message role and status values.
const (
	RoleUser      = "user"
	RoleAssistant = "assistant"

	StatusStreaming = "streaming"
	StatusComplete  = "complete"
	StatusError     = "error"
	StatusAborted   = "aborted"

	ToolStatusRunning = "running"
	ToolStatusSuccess = "success"
	ToolStatusError   = "error"
	ToolStatusDenied  = "denied"
)

// AppError is the shared application error shape.
type AppError struct {
	Code      string          `json:"code"`
	Message   string          `json:"message"`
	Details   json.RawMessage `json:"details,omitempty"`
	Retriable *bool           `json:"retriable,omitempty"`
	CauseCode string          `json:"causeCode,omitempty"`
}
