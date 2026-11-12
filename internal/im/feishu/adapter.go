// Package feishu implements the Feishu (Lark) IM channel for pigo's remote
// control.
//
// It talks to Feishu over the official Go SDK's WebSocket long connection,
// which is what makes this channel work with no public IP, no domain, no
// reverse proxy and no signature/AES plumbing: the SDK owns the outbound socket,
// authenticates once at dial time, and delivers events in the clear. The trade
// off is that Feishu closes the door on a handler that takes longer than three
// seconds, so every callback here only parses and hands off.
//
// Scope: enterprise self-built app (企业自建应用). The long connection mode is not
// available to marketplace apps, so neither is this adapter.
package feishu

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"sync"
	"time"

	lark "github.com/larksuite/oapi-sdk-go/v3"
	larkcore "github.com/larksuite/oapi-sdk-go/v3/core"
	"github.com/larksuite/oapi-sdk-go/v3/event/dispatcher"
	"github.com/larksuite/oapi-sdk-go/v3/event/dispatcher/callback"
	larkcontact "github.com/larksuite/oapi-sdk-go/v3/service/contact/v3"
	larkim "github.com/larksuite/oapi-sdk-go/v3/service/im/v1"
	larkws "github.com/larksuite/oapi-sdk-go/v3/ws"

	"github.com/smallnest/pigo/internal/im"
)

// ID is the channel identifier stored with bindings and shown in logs.
const ID = "feishu"

// receiveIDTypeChatID is Feishu's receive_id_type for a chat. The SDK does not
// export a constant for it, so it is named here rather than repeated as a
// literal at every call site.
const receiveIDTypeChatID = "chat_id"

// DefaultApprovalTimeout is used when a request carries none.
const DefaultApprovalTimeout = 10 * time.Minute

// ErrApprovalTimeout is returned when nobody pressed a button in time. The
// caller treats it as a denial; it is a distinct error so the transcript says
// why rather than implying the human refused.
var ErrApprovalTimeout = errors.New("等待审批超时")

// Config is the adapter's credentials and tuning.
type Config struct {
	// AppID and AppSecret come from the app's 凭证与基础信息 page.
	AppID     string
	AppSecret string
	// Logf receives diagnostics. It must never receive message bodies.
	Logf func(format string, args ...any)
	// ApprovalTimeout bounds how long a confirmation card waits.
	ApprovalTimeout time.Duration
}

// sdkLogger routes the SDK's own diagnostics into the channel log and drops its
// informational chatter. Without it, every connect prints "[Info] [client
// ready]" on stderr, which in a long-running channel looks like an error.
type sdkLogger struct {
	logf func(string, ...any)
}

func (l sdkLogger) Debug(context.Context, ...interface{}) {}
func (l sdkLogger) Info(context.Context, ...interface{})  {}

func (l sdkLogger) Warn(_ context.Context, args ...interface{}) {
	l.logf("feishu sdk: %v", fmt.Sprint(args...))
}

func (l sdkLogger) Error(_ context.Context, args ...interface{}) {
	l.logf("feishu sdk error: %v", fmt.Sprint(args...))
}

// Adapter is the Feishu channel. It implements both im.Adapter and im.Approver:
// the confirmation question is an interactive card, and its buttons come back
// through the same long connection the prompts do.
type Adapter struct {
	cfg Config
	api *lark.Client

	mu      sync.Mutex
	pending map[string]chan im.Decision
	// entered records the p2p chats already reported, so the chat-access push
	// prints its diagnostic line once per chat instead of once per tap.
	entered map[string]bool
}

// New builds the adapter. It validates credentials eagerly so a typo surfaces at
// startup instead of at the first message.
func New(cfg Config) (*Adapter, error) {
	if strings.TrimSpace(cfg.AppID) == "" {
		return nil, errors.New("feishu: app_id is required")
	}
	if strings.TrimSpace(cfg.AppSecret) == "" {
		return nil, errors.New("feishu: app_secret is required")
	}
	if cfg.Logf == nil {
		cfg.Logf = func(string, ...any) {}
	}
	if cfg.ApprovalTimeout <= 0 {
		cfg.ApprovalTimeout = DefaultApprovalTimeout
	}
	return &Adapter{
		cfg:     cfg,
		api:     lark.NewClient(cfg.AppID, cfg.AppSecret, lark.WithLogger(sdkLogger{cfg.Logf})),
		pending: make(map[string]chan im.Decision),
		entered: make(map[string]bool),
	}, nil
}

// ID implements im.Adapter.
func (a *Adapter) ID() string { return ID }

// ResolveAllowedUsers expands email entries in the allow list into the open_ids
// the router matches at message time.
//
// The allow list is documented as accepting "open_id or email", but Feishu puts
// only an open_id on the message event: there is no email on the wire for the
// router to compare against. An allow list written as emails therefore matched
// nothing and every message was dropped as unauthorized — silently, because the
// router stays quiet towards strangers. This translates the emails up front, in
// one batch call, so the documented form actually works.
//
// Entries that cannot be translated come back in unresolved rather than being
// dropped: the batch_get_id call needs the 获取用户 user ID application
// permission, and "your allow list does not point at anyone" is something the
// operator has to be told at startup, not at the first dropped message.
func (a *Adapter) ResolveAllowedUsers(ctx context.Context, users []string) (resolved, unresolved []string, err error) {
	var emails []string
	resolved = make([]string, 0, len(users))
	for _, u := range users {
		u = strings.TrimSpace(u)
		if u == "" {
			continue
		}
		if strings.Contains(u, "@") {
			emails = append(emails, u)
			continue
		}
		resolved = append(resolved, u)
	}
	if len(emails) == 0 {
		return resolved, nil, nil
	}

	// user_id_type=open_id makes the response's UserId field carry the open_id,
	// which is exactly the identity the message event will report.
	req := larkcontact.NewBatchGetIdUserReqBuilder().
		UserIdType("open_id").
		Body(larkcontact.NewBatchGetIdUserReqBodyBuilder().Emails(emails).Build()).
		Build()

	callCtx, cancel := context.WithTimeout(ctx, 15*time.Second)
	defer cancel()
	resp, err := a.api.Contact.V3.User.BatchGetId(callCtx, req)
	if err != nil {
		return resolved, emails, fmt.Errorf("feishu: resolve allow-list emails: %w", err)
	}
	if !resp.Success() {
		return resolved, emails, fmt.Errorf("feishu: resolve allow-list emails: code=%d msg=%s", resp.Code, resp.Msg)
	}

	found := make(map[string]string, len(emails))
	if resp.Data != nil {
		for _, info := range resp.Data.UserList {
			if info == nil || info.Email == nil || info.UserId == nil {
				continue
			}
			found[strings.ToLower(larkcore.StringValue(info.Email))] = larkcore.StringValue(info.UserId)
		}
	}
	for _, e := range emails {
		if id, ok := found[strings.ToLower(e)]; ok && id != "" {
			resolved = append(resolved, id)
			continue
		}
		unresolved = append(unresolved, e)
	}
	return resolved, unresolved, nil
}

// Run opens the long connection and streams inbound messages until ctx ends.
//
// Feishu's long-connection push is cluster mode, not broadcast: if the same app
// is connected from several processes, each event reaches exactly one of them at
// random. Running two instances of this adapter against one app therefore
// silently splits the conversation, which is why the CLI refuses a second
// foreground instance rather than pretending to support it.
func (a *Adapter) Run(ctx context.Context, in chan<- im.Inbound) error {
	handler := dispatcher.NewEventDispatcher("", "")
	// The dispatcher logs its "ready" line from inside the constructor, before
	// this assignment; that single line is unavoidable. Everything it logs
	// afterwards goes through our logger and stays quiet at info level.
	handler.Config.Logger = sdkLogger{a.cfg.Logf}
	handler = handler.
		OnP2MessageReceiveV1(a.onMessage(in)).
		OnP2CardActionTrigger(a.onCardAction).
		// Feishu pushes this every time a user opens the p2p chat with the bot
		// — first message, and every re-entry from the 机器人 tab. With no
		// handler the dispatcher reports "not found handler" and the SDK logs
		// it at error level, which reads like the channel is broken while the
		// channel is fine. Registering it silences that and buys a diagnostic
		// line carrying the open_id the allow list matches on.
		OnP2ChatAccessEventBotP2pChatEnteredV1(a.onP2PChatEntered)

	client := larkws.NewClient(a.cfg.AppID, a.cfg.AppSecret,
		larkws.WithEventHandler(handler),
		larkws.WithLogLevel(larkcore.LogLevelError),
		larkws.WithLogger(sdkLogger{a.cfg.Logf}),
		larkws.WithOnReconnecting(func() { a.cfg.Logf("feishu: long connection lost, reconnecting") }),
		larkws.WithOnReconnected(func() { a.cfg.Logf("feishu: long connection re-established") }),
		larkws.WithOnError(func(err error) { a.cfg.Logf("feishu: connection error: %v", err) }),
	)

	a.cfg.Logf("feishu: connecting (app_id=%s)", a.cfg.AppID)
	err := client.Start(ctx)
	if ctx.Err() != nil {
		return ctx.Err()
	}
	if err != nil {
		return fmt.Errorf("feishu: long connection: %w", err)
	}
	return nil
}

// Close implements im.Adapter. The connection is owned by Start's context, so
// there is nothing else to release.
func (a *Adapter) Close() error {
	a.mu.Lock()
	for token, ch := range a.pending {
		close(ch)
		delete(a.pending, token)
	}
	a.mu.Unlock()
	return nil
}

// Send delivers a card and returns its message id.
//
// Everything is sent as an interactive card, never as plain text, because
// Feishu's PATCH endpoint only updates interactive messages — and the progress
// message has to become the final answer, not sit above it.
func (a *Adapter) Send(ctx context.Context, chat im.ChatRef, msg im.Outbound) (string, error) {
	if chat.ID == "" {
		return "", errors.New("feishu: chat id is empty")
	}
	content, err := buildCard(msg)
	if err != nil {
		return "", err
	}
	req := larkim.NewCreateMessageReqBuilder().
		ReceiveIdType(receiveIDTypeChatID).
		Body(larkim.NewCreateMessageReqBodyBuilder().
			ReceiveId(chat.ID).
			MsgType(larkim.MsgTypeInteractive).
			Content(content).
			Build()).
		Build()

	resp, err := a.api.Im.V1.Message.Create(ctx, req)
	if err != nil {
		return "", fmt.Errorf("feishu: send to %s: %w", chat.ID, err)
	}
	if !resp.Success() {
		return "", fmt.Errorf("feishu: send to %s: code=%d msg=%s", chat.ID, resp.Code, resp.Msg)
	}
	if resp.Data == nil {
		return "", nil
	}
	return larkcore.StringValue(resp.Data.MessageId), nil
}

// Update replaces a card in place.
func (a *Adapter) Update(ctx context.Context, chat im.ChatRef, msgID string, msg im.Outbound) error {
	if msgID == "" {
		return im.ErrUpdateUnsupported
	}
	content, err := buildCard(msg)
	if err != nil {
		return err
	}
	req := larkim.NewPatchMessageReqBuilder().
		MessageId(msgID).
		Body(larkim.NewPatchMessageReqBodyBuilder().Content(content).Build()).
		Build()

	resp, err := a.api.Im.V1.Message.Patch(ctx, req)
	if err != nil {
		return fmt.Errorf("feishu: patch %s in %s: %w", msgID, chat.ID, err)
	}
	if !resp.Success() {
		return fmt.Errorf("feishu: patch %s in %s: code=%d msg=%s", msgID, chat.ID, resp.Code, resp.Msg)
	}
	return nil
}

// Ask implements im.Approver with an interactive card.
//
// It blocks the caller on purpose: the tool hook has to stop the world until a
// human answers, and the answer is what decides whether bash runs.
func (a *Adapter) Ask(ctx context.Context, req im.ApprovalReq) (im.Decision, error) {
	if req.Token == "" {
		return im.Decision{}, errors.New("feishu: approval token is empty")
	}
	timeout := req.Timeout
	if timeout <= 0 {
		timeout = a.cfg.ApprovalTimeout
	}

	ch := make(chan im.Decision, 1)
	a.mu.Lock()
	a.pending[req.Token] = ch
	a.mu.Unlock()
	defer func() {
		a.mu.Lock()
		delete(a.pending, req.Token)
		a.mu.Unlock()
	}()

	waitCtx, cancel := context.WithTimeout(ctx, timeout)
	defer cancel()

	msgID, err := a.Send(waitCtx, req.Chat, im.Outbound{
		Title:   "pigo · 需要确认",
		Text:    im.ApprovalSummaryLines(req),
		Buttons: approvalButtons(req.Token),
		Tone:    im.ToneWarn,
	})
	if err != nil {
		return im.Decision{}, err
	}

	select {
	case d, ok := <-ch:
		if !ok {
			return im.Decision{}, errors.New("feishu: approval channel closed")
		}
		a.settleCard(msgID, req.Chat, d)
		return d, nil
	case <-waitCtx.Done():
		// The poller cancels an ask when the host's queue drops the request —
		// the window answered it, or the run stopped. That is not a human
		// missing a deadline, so do not say "超时已拒绝" on the card.
		if ctx.Err() != nil && !errors.Is(waitCtx.Err(), context.DeadlineExceeded) {
			a.withdrawCard(msgID, req.Chat)
		} else {
			a.timeoutCard(msgID, req.Chat, timeout)
		}
		return im.Decision{}, ErrApprovalTimeout
	}
}

// settleCard rewrites the approval card to show the decision, so the buttons are
// gone and the transcript records who said what.
func (a *Adapter) settleCard(msgID string, chat im.ChatRef, d im.Decision) {
	title, body, tone := "pigo · 已允许", "该操作已获准执行。", im.ToneSuccess
	switch {
	case !d.Approve:
		title, body, tone = "pigo · 已拒绝", "该操作被拒绝，未执行。", im.ToneError
	case d.Always:
		body = "已允许执行，本会话内同类操作不再询问。"
	}
	a.patchDecisionCard(msgID, chat, title, body, tone)
}

func (a *Adapter) timeoutCard(msgID string, chat im.ChatRef, timeout time.Duration) {
	a.patchDecisionCard(msgID, chat,
		"pigo · 审批超时",
		fmt.Sprintf("%s 内没有收到确认，已按拒绝处理。", timeout),
		im.ToneError)
}

// withdrawCard closes a card whose question was settled elsewhere — the desktop
// window answered first, or the run stopped. Unlike a timeout, nobody was late.
func (a *Adapter) withdrawCard(msgID string, chat im.ChatRef) {
	a.patchDecisionCard(msgID, chat,
		"pigo · 已处理",
		"该确认已在电脑端处理或这一轮已结束，无需再操作。",
		im.ToneInfo)
}

func (a *Adapter) patchDecisionCard(msgID string, _ im.ChatRef, title, body, tone string) {
	if msgID == "" {
		return
	}
	content, err := buildDecisionCard(title, body, tone)
	if err != nil {
		return
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	req := larkim.NewPatchMessageReqBuilder().
		MessageId(msgID).
		Body(larkim.NewPatchMessageReqBodyBuilder().Content(content).Build()).
		Build()
	if resp, err := a.api.Im.V1.Message.Patch(ctx, req); err != nil || !resp.Success() {
		a.cfg.Logf("feishu: patch decision card %s: err=%v", msgID, err)
	}
}

// onP2PChatEntered reports who opened a p2p chat with the bot.
//
// It does no routing — the message event carries everything a prompt needs —
// but it is the only place the channel learns a user's open_id without waiting
// for them to send something, and open_id is the identity the allow list is
// matched against. One line per chat per process, so it stays useful without
// turning into a heartbeat.
func (a *Adapter) onP2PChatEntered(_ context.Context, ev *larkim.P2ChatAccessEventBotP2pChatEnteredV1) error {
	if ev == nil || ev.Event == nil {
		return nil
	}
	chatID := larkcore.StringValue(ev.Event.ChatId)
	if chatID == "" {
		return nil
	}
	openID := ""
	if ev.Event.OperatorId != nil {
		openID = larkcore.StringValue(ev.Event.OperatorId.OpenId)
	}

	a.mu.Lock()
	reported := a.entered[chatID]
	a.entered[chatID] = true
	a.mu.Unlock()
	if reported {
		return nil
	}

	if openID == "" {
		a.cfg.Logf("feishu: p2p chat opened chat=%s (no operator id in the event)", chatID)
		return nil
	}
	a.cfg.Logf("feishu: p2p chat opened chat=%s open_id=%s — add this open_id to allowed_users to authorize it", chatID, openID)
	return nil
}

// onMessage converts a Feishu message event into an Inbound and hands it off.
//
// The only work allowed here is parsing: Feishu retries the event if the handler
// has not returned within three seconds, and a retry would replay the message
// (the Router dedupes it, but the latency budget is still blown).
func (a *Adapter) onMessage(in chan<- im.Inbound) func(context.Context, *larkim.P2MessageReceiveV1) error {
	return func(_ context.Context, ev *larkim.P2MessageReceiveV1) error {
		if ev == nil || ev.Event == nil || ev.Event.Message == nil {
			return nil
		}
		msg := ev.Event.Message

		// Our own replies come back as bot messages; routing them would make
		// the bot answer itself.
		if ev.Event.Sender != nil && larkcore.StringValue(ev.Event.Sender.SenderType) == "bot" {
			return nil
		}

		msgType := larkcore.StringValue(msg.MessageType)
		if msgType != "text" {
			// Answer unsupported types explicitly rather than letting our own
			// silent filter look like a broken bot. Off the read goroutine so
			// the network call cannot eat the three-second budget.
			a.replyUnsupported(msg)
			return nil
		}

		text := stripMentionTokens(parseTextContent(larkcore.StringValue(msg.Content)))
		if text == "" {
			return nil
		}

		chatKind := im.ChatP2P
		if larkcore.StringValue(msg.ChatType) == "group" {
			chatKind = im.ChatGroup
		}

		inbound := im.Inbound{
			Adapter:   ID,
			Chat:      im.ChatRef{ID: larkcore.StringValue(msg.ChatId), Kind: chatKind},
			Sender:    senderOf(ev.Event.Sender),
			MsgID:     larkcore.StringValue(msg.MessageId),
			Text:      text,
			Kind:      im.KindText,
			Mentioned: mentionsBot(msg.Mentions),
			Received:  time.Now(),
		}

		select {
		case in <- inbound:
		default:
			// The router is behind. Dropping is the correct failure: blocking
			// here would stall the socket and look like a network problem.
			a.cfg.Logf("feishu: inbound queue full, dropping chat=%s msg=%s", inbound.Chat.ID, inbound.MsgID)
		}
		return nil
	}
}

// onCardAction resolves a pending approval from a button press.
func (a *Adapter) onCardAction(_ context.Context, ev *callback.CardActionTriggerEvent) (*callback.CardActionTriggerResponse, error) {
	if ev == nil || ev.Event == nil || ev.Event.Action == nil {
		return nil, nil
	}
	action, token := "", ""
	if v, ok := ev.Event.Action.Value["action"].(string); ok {
		action = v
	}
	if v, ok := ev.Event.Action.Value["token"].(string); ok {
		token = v
	}
	if token == "" {
		return nil, nil
	}

	a.mu.Lock()
	ch := a.pending[token]
	a.mu.Unlock()
	if ch == nil {
		// A stale card (already answered, or from a previous process). Tell the
		// user instead of silently doing nothing.
		return &callback.CardActionTriggerResponse{
			Toast: &callback.Toast{Type: "info", Content: "这条确认已失效"},
		}, nil
	}

	decision := im.Decision{}
	toast := "已拒绝"
	switch action {
	case "approve":
		decision.Approve = true
		toast = "已允许"
	case "approve_always":
		decision.Approve = true
		decision.Always = true
		toast = "已允许（本会话）"
	case "deny":
		toast = "已拒绝"
	default:
		return nil, nil
	}

	select {
	case ch <- decision:
	default:
		// Buffered size one and consumed once; a second press is a no-op.
	}
	return &callback.CardActionTriggerResponse{
		Toast: &callback.Toast{Type: "success", Content: toast},
	}, nil
}

// replyUnsupported tells the user which message types work. It runs on its own
// goroutine with its own deadline so a slow network cannot delay the callback.
func (a *Adapter) replyUnsupported(msg *larkim.EventMessage) {
	chatID := larkcore.StringValue(msg.ChatId)
	if chatID == "" {
		return
	}
	go func() {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		_, err := a.Send(ctx, im.ChatRef{ID: chatID, Kind: im.ChatP2P}, im.Outbound{
			Title: "pigo · 暂不支持",
			Text:  fmt.Sprintf("这个通道目前只处理文字消息（收到的是 `%s`）。请把内容用文字发一遍。", larkcore.StringValue(msg.MessageType)),
			Tone:  im.ToneInfo,
		})
		if err != nil {
			a.cfg.Logf("feishu: reply unsupported to %s: %v", chatID, err)
		}
	}()
}

// parseTextContent decodes Feishu's text payload: {"text":"..."}.
func parseTextContent(raw string) string {
	if raw == "" {
		return ""
	}
	var payload struct {
		Text string `json:"text"`
	}
	if err := json.Unmarshal([]byte(raw), &payload); err != nil {
		return ""
	}
	return strings.TrimSpace(payload.Text)
}

// stripMentionTokens removes "@_user_N" placeholders that Feishu embeds in the
// text body when the bot or a colleague is mentioned. Leaving them in would feed
// the model noise that looks like a variable name.
func stripMentionTokens(s string) string {
	fields := strings.Fields(s)
	out := fields[:0]
	for _, f := range fields {
		if strings.HasPrefix(f, "@_user_") || f == "@_all" {
			continue
		}
		out = append(out, f)
	}
	return strings.TrimSpace(strings.Join(out, " "))
}

// senderOf extracts the sender identity used for the allow list.
func senderOf(s *larkim.EventSender) im.UserRef {
	if s == nil || s.SenderId == nil {
		return im.UserRef{}
	}
	return im.UserRef{
		ID: larkcore.StringValue(s.SenderId.OpenId),
	}
}

// mentionsBot reports whether the bot was @-mentioned.
func mentionsBot(mentions []*larkim.MentionEvent) bool {
	for _, m := range mentions {
		if m == nil {
			continue
		}
		if larkcore.StringValue(m.MentionedType) == "bot" {
			return true
		}
	}
	return false
}
