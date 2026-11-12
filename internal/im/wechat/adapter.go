// Package wechat implements the WeChat (ClawBot) IM channel for pigo's remote
// control.
//
// It speaks the iLink bot protocol: plain JSON over HTTPS, no SDK, no public IP,
// no callback URL. Two calls carry the whole channel — getupdates long-polls for
// inbound messages, sendmessage posts replies — which makes this the least
// infrastructure-heavy of pigo's channels.
//
// Three properties of the platform shape every decision below:
//
//   - Messages cannot be edited. There is no patch endpoint, so the progress card
//     Feishu keeps updating does not exist here: Send delivers a placeholder, and
//     the answer arrives as its own message. Update reports ErrUpdateUnsupported
//     rather than pretending otherwise.
//   - There are no buttons. Confirmation is a text question answered with a text
//     reply, handled by Ask, and a reply that is not a decision stays an ordinary
//     prompt.
//   - The token belongs to one WeChat account. A second login invalidates the
//     first with errcode -14, which is why the channel pauses instead of retrying
//     against a token that will never work again.
package wechat

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"sync"
	"time"

	"github.com/smallnest/pigo/internal/im"
)

// ID is the channel identifier stored with bindings and shown in logs.
const ID = "wechat"

// DefaultApprovalTimeout bounds how long a confirmation question waits.
const DefaultApprovalTimeout = 10 * time.Minute

// staleTokenPause is how long the channel idles after the server invalidates the
// token. Retrying sooner cannot succeed — the token is gone until someone logs in
// again — and a tight retry loop would burn rate limit for nothing.
const staleTokenPause = time.Hour

// ErrApprovalTimeout reports that nobody answered the confirmation in time. The
// caller treats it as a denial; it is distinct so the transcript says why.
var ErrApprovalTimeout = errors.New("等待审批超时")

// Config is the adapter's credentials and tuning.
type Config struct {
	// Token is the bot token. Empty means "use the token from the state file".
	Token string
	// BaseURL overrides the API root. Empty uses the one remembered at login.
	BaseURL string
	// StatePath is where the token, the getupdates cursor and the per-peer
	// context tokens live. Empty uses DefaultStatePath.
	StatePath string
	// Logf receives diagnostics. It never receives message bodies.
	Logf func(format string, args ...any)
	// ApprovalTimeout bounds one confirmation question.
	ApprovalTimeout time.Duration
	// LongPoll overrides the getupdates timeout (default 35s).
	LongPoll time.Duration
	// Now is the clock (injectable for tests).
	Now func() time.Time
}

// Adapter is the WeChat channel. It implements both im.Adapter and im.Approver.
type Adapter struct {
	cfg  Config
	api  *API
	path string

	mu        sync.Mutex
	state     State
	lastSave  time.Time
	pending   map[string]chan im.Decision
	tickets   map[string]ticketEntry
	announced map[string]bool
	closed    bool
}

// ticketEntry caches a peer's typing ticket. An empty ticket is cached too, so a
// peer the server declines to issue one for is not asked again on every flush.
type ticketEntry struct {
	ticket string
	at     time.Time
}

// New builds the adapter and loads its persisted state.
func New(cfg Config) (*Adapter, error) {
	if cfg.Logf == nil {
		cfg.Logf = func(string, ...any) {}
	}
	if cfg.ApprovalTimeout <= 0 {
		cfg.ApprovalTimeout = DefaultApprovalTimeout
	}
	if cfg.LongPoll <= 0 {
		cfg.LongPoll = defaultLongPoll
	}
	if cfg.Now == nil {
		cfg.Now = time.Now
	}
	path := cfg.StatePath
	if path == "" {
		path = DefaultStatePath()
	}

	st, err := LoadState(path)
	if err != nil {
		return nil, err
	}
	if tok := strings.TrimSpace(cfg.Token); tok != "" && tok != st.Token {
		// An explicit token is a different binding, not a refresh: the cursor and
		// the context tokens belong to the account that produced them.
		st.Token = tok
		st.SyncBuf = ""
		st.ContextTokens = map[string]string{}
		st.BotID = ""
		st.UserID = ""
	}
	if base := strings.TrimSpace(cfg.BaseURL); base != "" {
		st.BaseURL = normalizeBaseURL(base)
	}
	if strings.TrimSpace(st.Token) == "" {
		return nil, errors.New("wechat: 尚未登录。先运行 `pigo im login --channel wechat` 扫码绑定，再启动通道")
	}

	a := &Adapter{
		cfg:       cfg,
		api:       NewAPI(st.BaseURL, st.Token, cfg.Logf),
		path:      path,
		state:     st,
		pending:   make(map[string]chan im.Decision),
		tickets:   make(map[string]ticketEntry),
		announced: make(map[string]bool),
	}
	return a, nil
}

// ID implements im.Adapter.
func (a *Adapter) ID() string { return ID }

// BotID reports the bound bot account, for the startup banner.
func (a *Adapter) BotID() string {
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.state.BotID
}

// UserID reports the WeChat account that completed the login.
func (a *Adapter) UserID() string {
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.state.UserID
}

// Run long-polls for messages until ctx ends.
//
// Unlike Feishu there is no three-second callback deadline: this side chooses
// when to ask for the next batch, so inbound handling is allowed to be ordinary
// blocking work. It still hands off fast — the agent runs on the queue's lane
// goroutines — because a slow loop here delays every chat equally.
func (a *Adapter) Run(ctx context.Context, in chan<- im.Inbound) error {
	startCtx, cancel := context.WithTimeout(ctx, 10*time.Second)
	if err := a.api.NotifyStart(startCtx); err != nil {
		a.cfg.Logf("wechat: presence notify failed (ignored): %v", err)
	}
	cancel()
	defer func() {
		stopCtx, stopCancel := context.WithTimeout(context.Background(), 5*time.Second)
		if err := a.api.NotifyStop(stopCtx); err != nil {
			a.cfg.Logf("wechat: presence stop failed (ignored): %v", err)
		}
		stopCancel()
		a.persist(true)
	}()

	a.cfg.Logf("wechat: polling for messages (bot=%s)", a.BotID())

	longPoll := a.cfg.LongPoll
	failures := 0

	for {
		if err := ctx.Err(); err != nil {
			return err
		}

		resp, err := a.api.Updates(ctx, a.syncBuf(), longPoll)
		if err != nil {
			if ctx.Err() != nil {
				return ctx.Err()
			}
			failures++
			wait := backoff(failures)
			a.cfg.Logf("wechat: getupdates failed (%d/%d): %v; retrying in %s", failures, 3, err, wait)
			if !sleepCtx(ctx, wait) {
				return ctx.Err()
			}
			continue
		}

		if resp.Errcode == errStaleToken || resp.Ret == errStaleToken {
			a.cfg.Logf("wechat: token 已失效（另一处登录替换了它），暂停 %s；之后需要重新执行 `pigo im login --channel wechat`", staleTokenPause)
			if !sleepCtx(ctx, staleTokenPause) {
				return ctx.Err()
			}
			continue
		}
		if resp.Ret != 0 || resp.Errcode != 0 {
			failures++
			wait := backoff(failures)
			a.cfg.Logf("wechat: getupdates ret=%d errcode=%d errmsg=%q (%d/3); retrying in %s",
				resp.Ret, resp.Errcode, resp.Errmsg, failures, wait)
			if !sleepCtx(ctx, wait) {
				return ctx.Err()
			}
			continue
		}
		failures = 0

		// The server can tell us to poll slower or faster than the default.
		if resp.LongPollingTimeoutMS > 0 {
			longPoll = time.Duration(resp.LongPollingTimeoutMS) * time.Millisecond
		}
		if resp.GetUpdatesBuf != "" && resp.GetUpdatesBuf != a.syncBuf() {
			a.setSyncBuf(resp.GetUpdatesBuf)
			a.persist(false)
		}
		for i := range resp.Msgs {
			a.dispatch(ctx, in, resp.Msgs[i])
		}
	}
}

// dispatch converts one wire message into an Inbound and hands it off.
func (a *Adapter) dispatch(ctx context.Context, in chan<- im.Inbound, msg Message) {
	if msg.MessageType == MessageTypeBot {
		// Our own replies come back through the same stream; routing them would
		// make the bot answer itself.
		return
	}
	from := strings.TrimSpace(msg.FromUserID)
	if from == "" {
		return
	}

	// The context token is per message and has to be echoed on the reply, so it
	// is remembered before anything else can drop this message.
	if tok := strings.TrimSpace(msg.ContextToken); tok != "" {
		a.setContextToken(from, tok)
	}

	a.announce(from)

	text, unsupported := messageText(msg)
	if text == "" {
		if unsupported != "" {
			a.replySoon(from, fmt.Sprintf("这个通道目前只处理文字消息（收到的是%s）。把内容用文字发一遍就行。", unsupported))
		}
		return
	}

	// A pending confirmation turns this message into a vote, not a prompt. This
	// is the whole button replacement: the answer never reaches the queue, and
	// anything that is not a decision falls through as an ordinary message.
	if a.resolveApproval(from, text) {
		return
	}

	inbound := im.Inbound{
		Adapter:  ID,
		Chat:     im.ChatRef{ID: from, Kind: im.ChatP2P},
		Sender:   im.UserRef{ID: from},
		MsgID:    messageID(msg),
		Text:     text,
		Kind:     im.KindText,
		Received: a.cfg.Now(),
	}
	if at := time.UnixMilli(msg.CreateTimeMS); msg.CreateTimeMS > 0 {
		inbound.At = at
	}

	select {
	case in <- inbound:
	default:
		// The router is behind. Dropping is the correct failure: blocking here
		// stops the poll loop and delays every other chat.
		a.cfg.Logf("wechat: inbound queue full, dropping chat=%s msg=%s", inbound.Chat.ID, inbound.MsgID)
	}
}

// announce logs a peer the first time it speaks.
//
// It is the WeChat equivalent of Feishu's "p2p chat opened" line: the identifier
// the allow list needs only exists on a real message, so the log has to hand it
// over rather than leave the operator hunting for it.
func (a *Adapter) announce(userID string) {
	a.mu.Lock()
	seen := a.announced[userID]
	a.announced[userID] = true
	a.mu.Unlock()
	if seen {
		return
	}
	a.cfg.Logf("wechat: chat opened with user=%s — add this id to allowed_users to authorize it", userID)
}

// Send delivers one message and returns its id.
//
// Everything is sent as a rendered text bubble; there is no card to build.
func (a *Adapter) Send(ctx context.Context, chat im.ChatRef, msg im.Outbound) (string, error) {
	if strings.TrimSpace(chat.ID) == "" {
		return "", errors.New("wechat: chat id is empty")
	}
	text := renderMessage(msg)
	if text == "" {
		return "", nil
	}
	id, err := a.api.SendMessage(ctx, chat.ID, a.contextToken(chat.ID), text)
	if err != nil {
		return "", err
	}
	return id, nil
}

// Update reports that in-place editing is impossible, which is the truth: the
// platform has no patch endpoint and a second sendmessage creates a second
// bubble.
//
// The Router reacts to this sentinel by delivering only messages marked Final,
// so progress updates are silently skipped and the answer arrives once, in
// full. Before returning, the typing indicator is refreshed (or cleared for a
// final message) so a long run still looks alive.
func (a *Adapter) Update(_ context.Context, chat im.ChatRef, _ string, msg im.Outbound) error {
	if msg.Final {
		a.finishTyping(chat.ID)
	} else {
		a.pokeTyping(chat.ID)
	}
	return im.ErrUpdateUnsupported
}

// Close releases the pending approvals and clears any typing indicator.
func (a *Adapter) Close() error {
	a.mu.Lock()
	a.closed = true
	pendings := make([]chan im.Decision, 0, len(a.pending))
	for chat, ch := range a.pending {
		pendings = append(pendings, ch)
		delete(a.pending, chat)
	}
	chats := make([]string, 0, len(a.tickets))
	for chat := range a.tickets {
		chats = append(chats, chat)
	}
	a.mu.Unlock()

	for _, ch := range pendings {
		close(ch)
	}
	for _, chat := range chats {
		a.clearTyping(chat)
	}
	a.persist(true)
	return nil
}

// Ask implements im.Approver with a text question.
//
// It blocks the caller on purpose: the tool hook has to stop the world until a
// human answers, and the answer decides whether the tool runs.
func (a *Adapter) Ask(ctx context.Context, req im.ApprovalReq) (im.Decision, error) {
	if req.Chat.ID == "" {
		return im.Decision{}, errors.New("wechat: approval chat is empty")
	}
	timeout := req.Timeout
	if timeout <= 0 {
		timeout = a.cfg.ApprovalTimeout
	}

	ch := make(chan im.Decision, 1)
	a.mu.Lock()
	if a.closed {
		a.mu.Unlock()
		return im.Decision{}, im.ErrNoApprover
	}
	a.pending[req.Chat.ID] = ch
	a.mu.Unlock()
	defer func() {
		a.mu.Lock()
		if cur, ok := a.pending[req.Chat.ID]; ok && cur == ch {
			delete(a.pending, req.Chat.ID)
		}
		a.mu.Unlock()
	}()

	if _, err := a.Send(ctx, req.Chat, im.Outbound{Text: renderApproval(req), Tone: im.ToneWarn}); err != nil {
		return im.Decision{}, err
	}

	timer := time.NewTimer(timeout)
	defer timer.Stop()

	select {
	case d, ok := <-ch:
		if !ok {
			return im.Decision{}, errors.New("wechat: approval cancelled (channel closed)")
		}
		a.replySoon(req.Chat.ID, decisionAck(d))
		return d, nil
	case <-timer.C:
		a.replySoon(req.Chat.ID, fmt.Sprintf("这次确认超过 %s 没有回复，已按拒绝处理。", timeout))
		return im.Decision{}, ErrApprovalTimeout
	case <-ctx.Done():
		return im.Decision{}, ctx.Err()
	}
}

// resolveApproval delivers a reply to a waiting Ask. It reports whether the
// text was a decision at all, so a non-decision keeps flowing as a prompt.
func (a *Adapter) resolveApproval(chatID, text string) bool {
	a.mu.Lock()
	ch := a.pending[chatID]
	a.mu.Unlock()
	if ch == nil {
		return false
	}
	decision, ok := parseDecision(text)
	if !ok {
		return false
	}
	select {
	case ch <- decision:
		return true
	default:
		// Already answered (a double tap of "同意"); treat the duplicate as
		// handled so it does not also become a prompt.
		return true
	}
}

// decisionAck is the one-line confirmation after a vote.
func decisionAck(d im.Decision) string {
	switch {
	case !d.Approve:
		return "已拒绝，这个操作没有执行。"
	case d.Always:
		return "已允许，本会话内同类操作不再询问。"
	default:
		return "已允许，本次操作继续执行。"
	}
}

// replySoon sends one short message without blocking the caller.
func (a *Adapter) replySoon(chatID, text string) {
	if strings.TrimSpace(text) == "" {
		return
	}
	go func() {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		if _, err := a.api.SendMessage(ctx, chatID, a.contextToken(chatID), text); err != nil {
			a.cfg.Logf("wechat: send notice to %s: %v", chatID, err)
		}
	}()
}

// ---- typing ----

// pokeTyping shows "typing…" for a chat.
func (a *Adapter) pokeTyping(chatID string) {
	a.sendTyping(chatID, TypingStart)
}

// finishTyping clears it.
func (a *Adapter) finishTyping(chatID string) {
	a.sendTyping(chatID, TypingCancel)
}

// clearTyping is finishTyping without a fresh ticket lookup.
func (a *Adapter) clearTyping(chatID string) {
	a.mu.Lock()
	entry, ok := a.tickets[chatID]
	a.mu.Unlock()
	if !ok || entry.ticket == "" {
		return
	}
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	if err := a.api.SendTyping(ctx, chatID, entry.ticket, TypingCancel); err != nil {
		a.cfg.Logf("wechat: clear typing for %s: %v", chatID, err)
	}
}

func (a *Adapter) sendTyping(chatID string, status TypingStatus) {
	ticket := a.typingTicket(chatID)
	if ticket == "" {
		return
	}
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	if err := a.api.SendTyping(ctx, chatID, ticket, status); err != nil {
		// The indicator is decoration; a failure to show it must never surface as
		// a channel error.
		a.cfg.Logf("wechat: typing %d for %s: %v", status, chatID, err)
	}
}

// typingTicket returns the peer's typing ticket, fetching it once.
//
// A negative result is cached for ten minutes: the server only issues a ticket
// after a getconfig, and hammering it on every progress flush would cost a round
// trip per two seconds for nothing.
func (a *Adapter) typingTicket(chatID string) string {
	a.mu.Lock()
	entry, ok := a.tickets[chatID]
	a.mu.Unlock()
	if ok && (entry.ticket != "" || a.cfg.Now().Sub(entry.at) < 10*time.Minute) {
		return entry.ticket
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	ticket := ""
	if resp, err := a.api.GetConfig(ctx, chatID, a.contextToken(chatID)); err == nil && resp.Ret == 0 {
		ticket = strings.TrimSpace(resp.TypingTicket)
	}

	a.mu.Lock()
	a.tickets[chatID] = ticketEntry{ticket: ticket, at: a.cfg.Now()}
	a.mu.Unlock()
	return ticket
}

// ---- state ----

func (a *Adapter) syncBuf() string {
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.state.SyncBuf
}

func (a *Adapter) setSyncBuf(buf string) {
	a.mu.Lock()
	a.state.SyncBuf = buf
	a.mu.Unlock()
}

func (a *Adapter) contextToken(chatID string) string {
	a.mu.Lock()
	defer a.mu.Unlock()
	return a.state.ContextTokens[chatID]
}

func (a *Adapter) setContextToken(chatID, token string) {
	a.mu.Lock()
	if a.state.ContextTokens == nil {
		a.state.ContextTokens = map[string]string{}
	}
	changed := a.state.ContextTokens[chatID] != token
	a.state.ContextTokens[chatID] = token
	a.mu.Unlock()
	if changed {
		a.persist(false)
	}
}

// persist writes the state, throttled to at most once every two seconds unless
// forced. The context token rotates per message, so an unthrottled write would
// hit the disk on every inbound message.
func (a *Adapter) persist(force bool) {
	if a.path == "" {
		return
	}
	a.mu.Lock()
	if !force && a.cfg.Now().Sub(a.lastSave) < 2*time.Second {
		a.mu.Unlock()
		return
	}
	snapshot := a.state
	snapshot.ContextTokens = make(map[string]string, len(a.state.ContextTokens))
	for k, v := range a.state.ContextTokens {
		snapshot.ContextTokens[k] = v
	}
	a.lastSave = a.cfg.Now()
	a.mu.Unlock()

	if err := SaveState(a.path, snapshot); err != nil {
		a.cfg.Logf("wechat: save state: %v", err)
	}
}

// ---- wire helpers ----

// messageText extracts the prompt from a message's items.
//
// Voice messages are accepted through their server-side transcript: this is a
// phone-first channel, and refusing a voice note that WeChat already transcribed
// would be a poor trade. Media that cannot be read yields a label instead, so the
// reply can name what arrived.
func messageText(msg Message) (text, unsupported string) {
	for _, item := range msg.ItemList {
		switch item.Type {
		case ItemText:
			if item.TextItem != nil {
				if s := strings.TrimSpace(item.TextItem.Text); s != "" {
					return s, ""
				}
			}
		case ItemVoice:
			if item.VoiceItem != nil {
				if s := strings.TrimSpace(item.VoiceItem.Text); s != "" {
					return s, ""
				}
			}
			unsupported = mediaLabel(ItemVoice)
		case ItemImage, ItemFile, ItemVideo:
			if unsupported == "" {
				unsupported = mediaLabel(item.Type)
			}
		}
	}
	return "", unsupported
}

// messageID picks the id used for replay suppression.
func messageID(msg Message) string {
	if id := idString(msg.MessageID); id != "" {
		return id
	}
	for _, item := range msg.ItemList {
		if id := idString(item.MsgID); id != "" {
			return id
		}
	}
	if msg.CreateTimeMS > 0 {
		// No stable id on the wire. A timestamp plus sender is the best available
		// key, and it still suppresses a re-delivery of the same batch.
		return fmt.Sprintf("%s-%d", msg.FromUserID, msg.CreateTimeMS)
	}
	return ""
}

// backoff is the retry delay after n consecutive failures: 2s, then 30s.
func backoff(failures int) time.Duration {
	if failures >= 3 {
		return 30 * time.Second
	}
	return 2 * time.Second
}
