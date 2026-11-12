package im

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"sync"
	"time"

	"github.com/smallnest/pigo/internal/agentcore"
)

// Defaults for RouterConfig. They are constants rather than zero-value magic so
// that a caller reading RouterConfig can tell "unset" from "chosen".
const (
	// DefaultRejectReply is what a chat sees when its backlog is full. It says
	// what happened and what to do, and nothing else: an IM reply that
	// apologises at length reads worse than one that is blunt.
	DefaultRejectReply = "排队太多，稍后再试。等上一条处理完再发。"

	defaultRejectCooldown   = 10 * time.Second
	defaultProgressThrottle = 2 * time.Second
	defaultProgressMinChars = 48
	defaultRunTimeout       = 30 * time.Minute

	// maxProgressText bounds the narration echoed into a progress card. The
	// card is a status line, not a transcript.
	maxProgressText = 1200
	// maxFinalText bounds the final answer echoed into one message. Beyond this
	// the answer is truncated with a pointer to the session file: silently
	// splitting one answer into six messages is worse than saying it was cut.
	maxFinalText = 12000
	// dedupeWindow is how long a platform message id is remembered. Platforms
	// retry within seconds; a window this long covers retries without growing
	// the map without bound.
	dedupeWindow = 10 * time.Minute
	// progressSteps is how many recent tool steps the card shows.
	progressSteps = 8
)

// RouterConfig is the routing policy for one channel.
type RouterConfig struct {
	// AllowedUsers lists the sender ids (Feishu open_id) or emails allowed to
	// control the agent. An EMPTY list denies everyone: the channel is
	// fail-closed, because "nobody configured an allow list" must not mean
	// "anyone in the tenant may run bash on this machine".
	AllowedUsers []string
	// AllowGroups permits group chats. Group messages additionally require a
	// mention (see RequireMention) and should be paired with ToolPolicyDeny.
	AllowGroups bool
	// RequireMention requires an @-mention in group chats. Default when
	// AllowGroups is set.
	RequireMention bool
	// QueueDepth is the per-chat backlog bound; 0 uses DefaultQueueDepth.
	QueueDepth int
	// RejectReply is the message sent when a chat's backlog is full.
	RejectReply string
	// RejectCooldown throttles that rejection so a burst of ten messages does
	// not produce ten identical replies.
	RejectCooldown time.Duration
	// ProgressThrottle bounds how often the progress card is updated.
	ProgressThrottle time.Duration
	// ProgressMinChars is the smallest text change worth an update.
	ProgressMinChars int
	// SessionCwd is the working directory new sessions are bound to. It is
	// fixed here, not taken from the chat: letting a message choose the
	// directory would hand the whole filesystem to anyone who can post.
	SessionCwd string
	// SessionModel is the model new sessions start on ("" = host default).
	SessionModel string
	// RunTimeout bounds one prompt run. A run that outlives it is aborted so
	// the chat lane cannot be pinned forever.
	RunTimeout time.Duration
	// Now is the clock (injectable for tests).
	Now func() time.Time
	// Logf receives diagnostics. Never log message bodies here.
	Logf func(format string, args ...any)
}

func (c RouterConfig) withDefaults() RouterConfig {
	if c.QueueDepth <= 0 {
		c.QueueDepth = DefaultQueueDepth
	}
	if strings.TrimSpace(c.RejectReply) == "" {
		c.RejectReply = DefaultRejectReply
	}
	if c.RejectCooldown <= 0 {
		c.RejectCooldown = defaultRejectCooldown
	}
	if c.ProgressThrottle <= 0 {
		c.ProgressThrottle = defaultProgressThrottle
	}
	if c.ProgressMinChars <= 0 {
		c.ProgressMinChars = defaultProgressMinChars
	}
	if c.RunTimeout <= 0 {
		c.RunTimeout = defaultRunTimeout
	}
	if c.AllowGroups && !c.RequireMention {
		// Group defaults are opt-in only in the sense that a group must be
		// allow-listed; the mention requirement itself defaults to on because
		// a bot reading every group message is a surprise, not a feature.
		c.RequireMention = true
	}
	if c.Now == nil {
		c.Now = time.Now
	}
	if c.Logf == nil {
		c.Logf = func(string, ...any) {}
	}
	return c
}

// Router binds chats to sessions and drives them. One Router serves one
// Adapter; a second channel is a second Router over the same BindingStore and
// the same serve.Service.
type Router struct {
	cfg     RouterConfig
	host    SessionHost
	store   *BindingStore
	adapter Adapter
	queue   *Queue
	approve *ApprovalRouter

	mu         sync.Mutex
	handles    map[string]*sessionHandle
	lastReject map[string]time.Time
	seen       map[string]time.Time
	seenPruned time.Time
}

// sessionHandle is a live session plus the bookkeeping the status command needs.
type sessionHandle struct {
	sess     AgentSession
	runStart time.Time
}

// NewRouter wires a router. host decides where sessions live (local store or a
// running desktop app); store may be nil (bindings then live in memory only);
// approve may be nil (side-effect tools are then blocked with an explanation
// rather than silently allowed).
func NewRouter(cfg RouterConfig, host SessionHost, store *BindingStore, adapter Adapter, approve *ApprovalRouter) *Router {
	cfg = cfg.withDefaults()
	r := &Router{
		cfg:        cfg,
		host:       host,
		store:      store,
		adapter:    adapter,
		approve:    approve,
		handles:    make(map[string]*sessionHandle),
		lastReject: make(map[string]time.Time),
		seen:       make(map[string]time.Time),
	}
	r.queue = NewQueue(cfg.QueueDepth, r.handleQueued, r.handleRejected)
	r.queue.SetPanicHandler(func(in Inbound, rec any) {
		cfg.Logf("im: handler panic chat=%s msg=%s: %v", in.Chat.ID, in.MsgID, rec)
		r.reply(context.Background(), in.Chat, Outbound{
			Title: "pigo · 出错",
			Text:  "处理这条消息时出错了，已跳过。发 `/status` 看看当前状态。",
			Tone:  ToneError,
		})
	})
	return r
}

// Queue exposes the queue for status reporting.
func (r *Router) Queue() *Queue { return r.queue }

// Start runs the adapter and routes inbound messages until ctx is cancelled or
// the adapter fails. Adapter events are only parsed and handed off here; the
// agent runs on per-chat lane goroutines.
func (r *Router) Start(ctx context.Context) error {
	in := make(chan Inbound, 64)
	errc := make(chan error, 1)
	go func() { errc <- r.adapter.Run(ctx, in) }()

	for {
		select {
		case <-ctx.Done():
			r.Close()
			return ctx.Err()
		case err := <-errc:
			r.Close()
			if err != nil {
				return fmt.Errorf("im: adapter %s: %w", r.adapter.ID(), err)
			}
			return nil
		case msg, ok := <-in:
			if !ok {
				r.Close()
				return nil
			}
			r.accept(msg)
		}
	}
}

// Close stops intake and cancels in-flight runs. It does not wait; use Wait.
func (r *Router) Close() {
	r.queue.Close()
	r.mu.Lock()
	for _, h := range r.handles {
		_ = h.sess.Close()
	}
	r.handles = make(map[string]*sessionHandle)
	r.mu.Unlock()
}

// Wait blocks until every lane worker has drained.
func (r *Router) Wait() { r.queue.Wait() }

// accept applies the gates that must happen before anything is queued:
// authorization, replay suppression, and command interception.
func (r *Router) accept(in Inbound) {
	if in.Received.IsZero() {
		in.Received = r.cfg.Now()
	}
	if in.Adapter == "" {
		in.Adapter = r.adapter.ID()
	}

	if !r.authorized(in) {
		// Deliberately silent towards the sender: replying would confirm to a
		// stranger that the bot is live and listening. The log line names the id
		// to allow-list, because that is the only fix and the operator is the
		// only one who can apply it.
		r.cfg.Logf("im: dropped unauthorized sender=%q chat=%q adapter=%s (add the sender id to allowed_users to authorize it)",
			in.Sender.ID, in.Chat.ID, in.Adapter)
		return
	}
	if r.duplicate(in) {
		r.cfg.Logf("im: dropped replayed msg id=%q chat=%q", in.MsgID, in.Chat.ID)
		return
	}

	cmd, ok := ParseCommand(in.Text)
	if in.Kind == KindAction {
		btnCmd, btnOK := ParseButtonAction(in.Action)
		if !btnOK {
			r.cfg.Logf("im: unknown card action %q chat=%q", in.Action.Name, in.Chat.ID)
			return
		}
		cmd, ok = btnCmd, true
	}
	if ok {
		// Commands bypass the queue. They are cheap (no model call), and /stop
		// in particular must work exactly when the queue is full — that is the
		// moment a user needs it.
		r.execCommand(in, cmd)
		return
	}
	if in.Kind != KindText {
		return
	}
	if strings.TrimSpace(in.Text) == "" {
		return
	}
	r.queue.Submit(in)
}

// authorized applies the allow list and the group gates.
func (r *Router) authorized(in Inbound) bool {
	if in.Chat.Kind == ChatGroup {
		if !r.cfg.AllowGroups {
			return false
		}
		if r.cfg.RequireMention && !in.Mentioned {
			return false
		}
	}
	if len(r.cfg.AllowedUsers) == 0 {
		return false
	}
	for _, allowed := range r.cfg.AllowedUsers {
		allowed = strings.TrimSpace(allowed)
		if allowed == "" {
			continue
		}
		if allowed == in.Sender.ID || allowed == in.Sender.Email {
			return true
		}
	}
	return false
}

// duplicate reports whether this platform message id was already handled.
func (r *Router) duplicate(in Inbound) bool {
	if in.MsgID == "" {
		return false
	}
	now := r.cfg.Now()
	r.mu.Lock()
	defer r.mu.Unlock()
	if now.Sub(r.seenPruned) > dedupeWindow {
		for id, at := range r.seen {
			if now.Sub(at) > dedupeWindow {
				delete(r.seen, id)
			}
		}
		r.seenPruned = now
	}
	if _, ok := r.seen[in.MsgID]; ok {
		return true
	}
	r.seen[in.MsgID] = now
	return false
}

// handleRejected is the Queue's overflow callback: it decides whether the user
// hears about the rejection at all.
//
// The throttle is what keeps the policy humane. Without it, a burst of eight
// messages produces eight identical "queue is full" replies, which reads as a
// malfunction. With it, the first rejection explains the situation and the rest
// are dropped silently — the user already knows.
func (r *Router) handleRejected(in Inbound, backlog int) {
	key := in.Chat.Key()
	now := r.cfg.Now()
	r.mu.Lock()
	if last, ok := r.lastReject[key]; ok && now.Sub(last) < r.cfg.RejectCooldown {
		r.mu.Unlock()
		r.cfg.Logf("im: queue full chat=%s backlog=%d (reply suppressed, cooling down)", in.Chat.ID, backlog)
		return
	}
	r.lastReject[key] = now
	r.mu.Unlock()

	r.cfg.Logf("im: queue full chat=%s backlog=%d (replying %q)", in.Chat.ID, backlog, r.cfg.RejectReply)
	// Asynchronous on purpose: this runs on the adapter's read goroutine, and a
	// blocking network send here would stall every other chat's hand-off. The
	// rejection is advisory, so losing it to a shutdown is acceptable.
	chat := in.Chat
	go r.reply(context.Background(), chat, Outbound{
		Title: "pigo · 忙",
		Text:  r.cfg.RejectReply,
		Tone:  ToneWarn,
	})
}

// handleQueued is the Queue handler: one message, on its chat's lane.
func (r *Router) handleQueued(ctx context.Context, in Inbound) {
	r.runPrompt(ctx, in)
}

// ---- commands ----

func (r *Router) execCommand(in Inbound, cmd Command) {
	ctx := context.Background()
	switch cmd.Name {
	case "help":
		r.reply(ctx, in.Chat, Outbound{Title: "pigo · 帮助", Text: HelpText()})

	case "status":
		r.reply(ctx, in.Chat, Outbound{Title: "pigo · 状态", Text: r.statusText(in.Chat)})

	case "stop":
		r.mu.Lock()
		h := r.handles[in.Chat.Key()]
		r.mu.Unlock()
		if h == nil {
			r.reply(ctx, in.Chat, Outbound{Text: "当前没有在跑的会话。"})
			return
		}
		h.sess.Abort()
		r.reply(ctx, in.Chat, Outbound{Text: "已中止当前这一轮。"})

	case "new":
		r.mu.Lock()
		old := r.handles[in.Chat.Key()]
		delete(r.handles, in.Chat.Key())
		r.mu.Unlock()
		if old != nil {
			if r.approve != nil {
				r.approve.Unregister(old.sess.ID())
			}
			_ = old.sess.Close()
		}
		if err := r.store.Delete(in.Chat.Key()); err != nil {
			r.cfg.Logf("im: delete binding for %s: %v", in.Chat.ID, err)
		}
		r.reply(ctx, in.Chat, Outbound{Text: "已开新会话，直接发消息即可。"})

	case "resume":
		if len(cmd.Args) == 0 {
			r.reply(ctx, in.Chat, Outbound{Text: "用法：`/resume <会话 id>`，用 `/sessions` 查 id。"})
			return
		}
		id := cmd.Args[0]
		sess, err := r.host.OpenSession(ctx, id)
		if err != nil {
			r.reply(ctx, in.Chat, Outbound{Text: fmt.Sprintf("打不开会话 `%s`：%v", id, err), Tone: ToneError})
			return
		}
		r.swapHandle(in.Chat, sess)
		r.reply(ctx, in.Chat, Outbound{Text: fmt.Sprintf("已接回会话 `%s`。", id)})

	case "sessions":
		r.reply(ctx, in.Chat, Outbound{Title: "pigo · 会话", Text: r.sessionsText(ctx, in.Chat)})

	case "model":
		if len(cmd.Args) == 0 {
			r.reply(ctx, in.Chat, Outbound{Text: "用法：`/model <模型 id>`"})
			return
		}
		h, err := r.ensureSession(ctx, in.Chat)
		if err != nil {
			r.reply(ctx, in.Chat, Outbound{Text: "创建会话失败：" + err.Error(), Tone: ToneError})
			return
		}
		if err := h.sess.SetModel(cmd.Args[0]); err != nil {
			r.reply(ctx, in.Chat, Outbound{Text: "切换模型失败：" + err.Error(), Tone: ToneError})
			return
		}
		b, _ := r.store.Get(in.Chat.Key())
		b.Model = cmd.Args[0]
		if err := r.store.Put(in.Chat.Key(), b); err != nil {
			r.cfg.Logf("im: persist model for %s: %v", in.Chat.ID, err)
		}
		r.reply(ctx, in.Chat, Outbound{Text: fmt.Sprintf("已切换到 `%s`。", cmd.Args[0])})

	default:
		r.reply(ctx, in.Chat, Outbound{Text: "未知命令，发 `/help` 看可用命令。"})
	}
}

// statusText renders /status.
func (r *Router) statusText(chat ChatRef) string {
	key := chat.Key()
	r.mu.Lock()
	h := r.handles[key]
	r.mu.Unlock()

	var b strings.Builder
	b.WriteString("**通道** ")
	b.WriteString(r.adapter.ID())
	b.WriteString("\n")

	binding, bound := r.store.Get(key)
	if !bound {
		b.WriteString("**会话** 未绑定，发一条消息就会新建\n")
	} else {
		fmt.Fprintf(&b, "**会话** `%s`\n", binding.SessionID)
		if binding.Model != "" {
			fmt.Fprintf(&b, "**模型** `%s`\n", binding.Model)
		}
		if binding.Cwd != "" {
			fmt.Fprintf(&b, "**目录** `%s`\n", binding.Cwd)
		}
	}
	if h != nil {
		phase := "空闲"
		if h.sess.Busy() {
			phase = "运行中"
			if !h.runStart.IsZero() {
				phase = fmt.Sprintf("运行中 %s", formatDuration(r.cfg.Now().Sub(h.runStart)))
			}
		}
		fmt.Fprintf(&b, "**状态** %s\n", phase)
	}
	fmt.Fprintf(&b, "**排队** %d 条（上限 %d）\n", r.queue.Backlog(key), r.queue.Depth())
	return b.String()
}

// sessionsText renders /sessions.
func (r *Router) sessionsText(ctx context.Context, chat ChatRef) string {
	summaries, err := r.host.ListSessions(ctx)
	if err != nil {
		return "读取会话列表失败：" + err.Error()
	}
	binding, _ := r.store.Get(chat.Key())
	rows := make([]sessionRow, 0, len(summaries))
	for _, s := range summaries {
		label := s.Title
		if label == "" {
			label = shortCwd(s.Cwd)
		}
		rows = append(rows, sessionRow{
			ID:      s.ID,
			Title:   label,
			When:    s.UpdatedAt.Local().Format("01-02 15:04"),
			Current: binding.SessionID == s.ID,
		})
	}
	sortSessionRows(rows)
	return renderSessions(rows, 10)
}

// ---- session lifecycle ----

// ensureSession returns the live handle for a chat, creating or reopening the
// bound session as needed.
func (r *Router) ensureSession(ctx context.Context, chat ChatRef) (*sessionHandle, error) {
	key := chat.Key()
	r.mu.Lock()
	if h := r.handles[key]; h != nil {
		r.mu.Unlock()
		return h, nil
	}
	r.mu.Unlock()

	if binding, ok := r.store.Get(key); ok && binding.SessionID != "" {
		sess, err := r.host.OpenSession(ctx, binding.SessionID)
		if err == nil {
			return r.swapHandle(chat, sess), nil
		}
		// The bound session file is gone or unreadable. Starting fresh is the
		// only recoverable behaviour, but say so rather than silently
		// forgetting the conversation the user thought they were in.
		r.cfg.Logf("im: reopen session %s for chat %s failed, rebinding: %v", binding.SessionID, chat.ID, err)
	}

	cfg := SessionSpec{
		Model: r.cfg.SessionModel,
		Cwd:   r.cfg.SessionCwd,
	}
	if binding, ok := r.store.Get(key); ok {
		if cfg.Cwd == "" {
			cfg.Cwd = binding.Cwd
		}
		if cfg.Model == "" {
			cfg.Model = binding.Model
		}
	}
	sess, err := r.host.CreateSession(ctx, cfg)
	if err != nil {
		return nil, err
	}
	now := r.cfg.Now()
	if err := r.store.Put(key, Binding{
		SessionID:  sess.ID(),
		Cwd:        cfg.Cwd,
		Model:      cfg.Model,
		ChatTitle:  chat.Title,
		CreatedAt:  now,
		LastActive: now,
	}); err != nil {
		r.cfg.Logf("im: persist binding for %s: %v", chat.ID, err)
	}
	return r.swapHandle(chat, sess), nil
}

// swapHandle installs sess as the chat's live session, closing whatever was
// there. It also points the approval router at this chat, since a tool hook
// only knows the session id.
func (r *Router) swapHandle(chat ChatRef, sess AgentSession) *sessionHandle {
	key := chat.Key()
	h := &sessionHandle{sess: sess}

	r.mu.Lock()
	old := r.handles[key]
	r.handles[key] = h
	r.mu.Unlock()

	if old != nil && old.sess.ID() != sess.ID() {
		if r.approve != nil {
			r.approve.Unregister(old.sess.ID())
		}
		_ = old.sess.Close()
	}
	if r.approve != nil {
		if approver, ok := r.adapter.(Approver); ok {
			r.approve.Register(sess.ID(), chat, approver)
		}
	}
	return h
}

// startPermissionPoller wires a host-owned permission queue to this chat.
//
// Only hosts that implement PermissionGate need it. The local (CLI) carrier
// already gates tools through ApprovalRouter.WrapRunConfig, so installing a
// second watcher there would send two cards for one question. The chat and the
// approver come from the binding table, not from the pending request.
func (r *Router) startPermissionPoller(ctx context.Context, sessionID string) {
	if r.approve == nil {
		return
	}
	gate, ok := r.host.(PermissionGate)
	if !ok {
		return
	}
	approver, chat, ok := r.approve.Target(sessionID)
	if !ok {
		return
	}
	p := NewPermissionPoller(PermissionPollerConfig{
		Gate:      gate,
		Approver:  approver,
		Chat:      chat,
		SessionID: sessionID,
		Policy:    r.approve.Policy(),
		Timeout:   r.approve.Timeout(),
		Logf:      r.cfg.Logf,
	})
	go p.Run(ctx)
}

// ---- the run ----

// runPrompt drives one prompt through the session and mirrors it into the chat.
func (r *Router) runPrompt(ctx context.Context, in Inbound) {
	h, err := r.ensureSession(ctx, in.Chat)
	if err != nil {
		r.reply(ctx, in.Chat, Outbound{Title: "pigo · 出错", Text: "创建会话失败：" + err.Error(), Tone: ToneError})
		return
	}

	stream, err := h.sess.Subscribe()
	if err != nil {
		r.reply(ctx, in.Chat, Outbound{Title: "pigo · 出错", Text: "订阅会话失败：" + err.Error(), Tone: ToneError})
		return
	}
	defer stream.Close()

	// A desktop host asks for tool permission in its own window, which this
	// process cannot see. Mirror that queue into the chat for as long as this
	// run is alive; the poller stops with the run, so an idle chat never holds
	// a handle on another session's confirmations.
	pollCtx, stopPoll := context.WithCancel(ctx)
	defer stopPoll()
	r.startPermissionPoller(pollCtx, h.sess.ID())

	started := r.cfg.Now()
	r.mu.Lock()
	h.runStart = started
	r.mu.Unlock()

	// Placeholder first, then prompt: the user gets an acknowledgement inside a
	// second even when the provider takes ten, and the id doubles as the handle
	// the progress card is patched through.
	msgID, err := r.adapter.Send(ctx, in.Chat, Outbound{
		Title: "pigo · 处理中",
		Text:  "收到，正在处理…",
		Tone:  ToneWarn,
	})
	if err != nil {
		r.cfg.Logf("im: send placeholder chat=%s: %v", in.Chat.ID, err)
	}

	if err := h.sess.Prompt(ctx, in.Text); err != nil {
		if errors.Is(err, ErrRunActive) {
			// Another run owns the session (possible after a rebind). Steering
			// is the useful answer: the text joins the current turn instead of
			// being lost.
			if serr := h.sess.Steer(in.Text); serr == nil {
				r.update(ctx, in.Chat, msgID, Outbound{
					Title: "pigo · 已插入",
					Text:  "当前还有一轮在跑，这条已作为补充插入。",
				})
				return
			}
		}
		r.update(ctx, in.Chat, msgID, Outbound{
			Title: "pigo · 出错",
			Text:  "启动失败：" + err.Error(),
			Final: true,
			Tone:  ToneError,
		})
		return
	}

	final := r.consume(ctx, in.Chat, msgID, stream, started)
	r.update(ctx, in.Chat, msgID, final)
	_ = r.store.Touch(in.Chat.Key(), r.cfg.Now())
}

// consume mirrors the run's events into one progress card until the run ends.
func (r *Router) consume(ctx context.Context, chat ChatRef, msgID string, stream EventStream, started time.Time) Outbound {
	runCtx, cancel := context.WithTimeout(ctx, r.cfg.RunTimeout)
	defer cancel()

	var (
		steps     []string
		narration string
		lastFlush = started
		lastSent  int
	)

	flush := func(force bool) {
		now := r.cfg.Now()
		if !force && now.Sub(lastFlush) < r.cfg.ProgressThrottle {
			return
		}
		body := renderProgress(steps, narration, now.Sub(started))
		if !force && len(body)-lastSent < r.cfg.ProgressMinChars {
			return
		}
		lastFlush = now
		lastSent = len(body)
		r.update(runCtx, chat, msgID, Outbound{Title: "pigo · 处理中", Text: body, Tone: ToneWarn})
	}

	for {
		select {
		case <-runCtx.Done():
			return Outbound{
				Title: "pigo · 已中止",
				Text:  fmt.Sprintf("这一轮超过 %s 未完成，已中止。可以发 `/status` 查看，或 `/new` 换一个会话。", formatDuration(r.cfg.RunTimeout)),
				Final: true,
				Tone:  ToneError,
			}
		case ev, ok := <-stream.Events():
			if !ok {
				return r.finish(chat, steps, narration, started)
			}
			switch e := ev.(type) {
			case agentcore.ToolExecutionStartEvent:
				steps = appendStep(steps, "▸ "+e.ToolName)
				flush(false)
			case agentcore.ToolExecutionEndEvent:
				mark := "✓ " + e.ToolName
				if e.IsError {
					mark = "✗ " + e.ToolName
				}
				steps = appendStep(steps, mark)
				flush(false)
			case agentcore.MessageEndEvent:
				if am, ok := e.Message.(agentcore.AssistantMessage); ok {
					if txt := strings.TrimSpace(agentcore.ContentToText(am.Content)); txt != "" {
						narration = truncateRunes(txt, maxProgressText)
						flush(false)
					}
				}
			case agentcore.AgentEndEvent:
				if txt := lastAssistantText(e.Messages); txt != "" {
					narration = txt
				}
				return r.finish(chat, steps, narration, started)
			}
		}
	}
}

// finish builds the terminal message for a run.
func (r *Router) finish(chat ChatRef, steps []string, narration string, started time.Time) Outbound {
	elapsed := formatDuration(r.cfg.Now().Sub(started))
	body := strings.TrimSpace(narration)
	if body == "" {
		if len(steps) == 0 {
			body = "这一轮没有产生文本输出。"
		} else {
			body = "这一轮只执行了工具调用，没有文本输出：\n" + strings.Join(lastN(steps, progressSteps), "\n")
		}
	}
	if len(body) > maxFinalText {
		body = truncateRunes(body, maxFinalText) + "\n\n…（内容过长已截断，完整记录在会话文件里）"
	}
	return Outbound{
		Title:  "pigo · 完成 · " + elapsed,
		Text:   body,
		Final:  true,
		Footer: fmt.Sprintf("%s · %d 步", chat.Title, len(steps)),
		Tone:   ToneSuccess,
	}
}

// update patches a previously sent message, falling back to a fresh message when
// the platform cannot patch. The fallback is why Update returns a sentinel error
// instead of pretending success.
func (r *Router) update(ctx context.Context, chat ChatRef, msgID string, out Outbound) {
	if msgID != "" {
		err := r.adapter.Update(ctx, chat, msgID, out)
		if err == nil {
			return
		}
		if !errors.Is(err, ErrUpdateUnsupported) {
			r.cfg.Logf("im: update message chat=%s id=%s: %v", chat.ID, msgID, err)
			// A failed patch usually means the message is gone; sending a fresh
			// one is still better than dropping the answer.
		}
	}
	if out.Final || msgID == "" {
		r.reply(ctx, chat, out)
	}
}

// reply sends without an update handle.
func (r *Router) reply(ctx context.Context, chat ChatRef, out Outbound) {
	sendCtx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()
	if _, err := r.adapter.Send(sendCtx, chat, out); err != nil {
		r.cfg.Logf("im: send to chat=%s: %v", chat.ID, err)
	}
}

// ---- helpers ----

// lastAssistantText returns the text of the last assistant message among msgs.
func lastAssistantText(msgs []agentcore.AgentMessage) string {
	for i := len(msgs) - 1; i >= 0; i-- {
		am, ok := msgs[i].(agentcore.AssistantMessage)
		if !ok {
			continue
		}
		if txt := strings.TrimSpace(agentcore.ContentToText(am.Content)); txt != "" {
			return txt
		}
	}
	return ""
}

// renderProgress builds the progress card body.
func renderProgress(steps []string, narration string, elapsed time.Duration) string {
	var b strings.Builder
	fmt.Fprintf(&b, "已运行 %s\n", formatDuration(elapsed))
	if len(steps) > 0 {
		b.WriteString("\n")
		b.WriteString(strings.Join(lastN(steps, progressSteps), "\n"))
		b.WriteString("\n")
	}
	if narration != "" {
		b.WriteString("\n")
		b.WriteString(narration)
	}
	return b.String()
}

func appendStep(steps []string, step string) []string {
	steps = append(steps, step)
	if len(steps) > progressSteps*2 {
		steps = steps[len(steps)-progressSteps:]
	}
	return steps
}

func lastN(items []string, n int) []string {
	if n <= 0 || len(items) <= n {
		return items
	}
	return items[len(items)-n:]
}

// formatDuration renders a compact human duration.
func formatDuration(d time.Duration) string {
	if d < time.Second {
		return "0s"
	}
	d = d.Round(time.Second)
	if d < time.Minute {
		return fmt.Sprintf("%ds", int(d.Seconds()))
	}
	if d < time.Hour {
		return fmt.Sprintf("%dm%02ds", int(d.Minutes()), int(d.Seconds())%60)
	}
	return fmt.Sprintf("%dh%02dm", int(d.Hours()), int(d.Minutes())%60)
}

// truncateRunes cuts s to at most n runes, so a multi-byte character is never
// split in half.
func truncateRunes(s string, n int) string {
	if n <= 0 {
		return ""
	}
	r := []rune(s)
	if len(r) <= n {
		return s
	}
	return string(r[:n])
}

// shortCwd returns the last path element of a directory, for compact listings.
func shortCwd(cwd string) string {
	if cwd == "" {
		return ""
	}
	parts := strings.FieldsFunc(cwd, func(r rune) bool { return r == '/' || r == '\\' })
	if len(parts) == 0 {
		return cwd
	}
	return parts[len(parts)-1]
}
