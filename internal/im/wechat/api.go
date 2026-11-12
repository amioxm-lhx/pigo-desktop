package wechat

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/base64"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
	"time"
)

// Protocol constants.
//
// The URL is not configurable in the upstream client, but it is here: the login
// response can hand back a different host (idc redirect), and a self-hosted or
// proxied deployment needs to point somewhere else.
const (
	// DefaultBaseURL is the iLink bot API root.
	DefaultBaseURL = "https://ilinkai.weixin.qq.com"
	// ILinkAppID is the fixed application id of the WeChat bot client. It is
	// not a secret and not per-user; the token identifies the account.
	ILinkAppID = "bot"
	// channelVersion mirrors the upstream plugin version this adapter tracks.
	// It is reported to the server, so it is deliberately the version whose
	// protocol was implemented rather than pigo's own version.
	channelVersion = "2.4.10"
	// botAgent is what the server records as the client. The server's grammar
	// is "name/version", so pigo's identity is expressed in that shape.
	botAgent = "pigo/1.0"
	// appClientVersion is 0x00MMNNPP of channelVersion: major<<16 | minor<<8 |
	// patch. The server rejects a request whose header disagrees with what it
	// expects for this protocol revision, so it is derived, not typed.
	appClientVersion = (2 << 16) | (4 << 8) | 10

	defaultLongPoll      = 35 * time.Second
	defaultAPITimeout    = 15 * time.Second
	defaultConfigTimeout = 10 * time.Second
)

// ErrStaleToken reports that this bot token was replaced by a newer login.
var ErrStaleToken = errors.New("wechat: bot token is stale (another login replaced it)")

// API is the HTTP transport. It is deliberately dependency-free: the whole
// protocol is JSON over HTTPS, so the standard library is the entire stack.
type API struct {
	baseURL string
	token   string
	http    *http.Client
	logf    func(string, ...any)
}

// NewAPI builds a client. token may be empty for the login endpoints, which are
// the only ones that do not authenticate.
func NewAPI(baseURL, token string, logf func(string, ...any)) *API {
	if logf == nil {
		logf = func(string, ...any) {}
	}
	return &API{
		baseURL: normalizeBaseURL(baseURL),
		token:   strings.TrimSpace(token),
		http:    &http.Client{},
		logf:    logf,
	}
}

// SetToken replaces the bot token (after a fresh login).
func (a *API) SetToken(token string) { a.token = strings.TrimSpace(token) }

// SetBaseURL switches hosts (after an idc redirect).
func (a *API) SetBaseURL(base string) { a.baseURL = normalizeBaseURL(base) }

// normalizeBaseURL accepts a bare host, a trailing slash, or a full URL and
// always yields "scheme://host" with no trailing slash.
func normalizeBaseURL(base string) string {
	base = strings.TrimSpace(base)
	if base == "" {
		return DefaultBaseURL
	}
	if !strings.Contains(base, "://") {
		base = "https://" + base
	}
	return strings.TrimRight(base, "/")
}

// requestHeaders builds the header set.
//
// Every request needs the two iLink-* headers; authenticated requests add the
// bearer token. There is no signing, no secret, and no AES handshake: the token
// is the whole of the authentication.
func (a *API) requestHeaders(auth bool) http.Header {
	h := make(http.Header, 6)
	h.Set("Content-Type", "application/json")
	h.Set("iLink-App-Id", ILinkAppID)
	h.Set("iLink-App-ClientVersion", itoa(appClientVersion))
	h.Set("X-WECHAT-UIN", randomUIN())
	if auth {
		h.Set("AuthorizationType", "ilink_bot_token")
		if a.token != "" {
			h.Set("Authorization", "Bearer "+a.token)
		}
	}
	return h
}

// randomUIN is base64(decimal of a random uint32). It is a per-request device
// discriminator, not an identifier of this machine; the server only uses it to
// tell concurrent clients apart.
func randomUIN() string {
	var buf [4]byte
	if _, err := rand.Read(buf[:]); err != nil {
		binary.BigEndian.PutUint32(buf[:], uint32(time.Now().UnixNano()))
	}
	return base64.StdEncoding.EncodeToString([]byte(itoa(int(binary.BigEndian.Uint32(buf[:])))))
}

// post sends a JSON body and decodes the answer into out.
func (a *API) post(ctx context.Context, path string, body any, out any, timeout time.Duration) error {
	return a.do(ctx, http.MethodPost, path, body, out, timeout, true)
}

// get sends an authenticated GET (used by the qrcode status poll, which is a
// GET with query parameters).
func (a *API) get(ctx context.Context, path string, out any, timeout time.Duration, auth bool) error {
	return a.do(ctx, http.MethodGet, path, nil, out, timeout, auth)
}

func (a *API) do(ctx context.Context, method, path string, body any, out any, timeout time.Duration, auth bool) error {
	if timeout <= 0 {
		timeout = defaultAPITimeout
	}
	reqCtx, cancel := context.WithTimeout(ctx, timeout)
	defer cancel()

	var reader io.Reader
	if body != nil {
		raw, err := json.Marshal(body)
		if err != nil {
			return fmt.Errorf("wechat: encode %s request: %w", path, err)
		}
		reader = bytes.NewReader(raw)
	}

	endpoint := a.baseURL + "/" + strings.TrimLeft(path, "/")
	req, err := http.NewRequestWithContext(reqCtx, method, endpoint, reader)
	if err != nil {
		return fmt.Errorf("wechat: build %s request: %w", path, err)
	}
	req.Header = a.requestHeaders(auth)

	resp, err := a.http.Do(req)
	if err != nil {
		return fmt.Errorf("wechat: %s: %w", path, err)
	}
	defer resp.Body.Close()

	raw, err := io.ReadAll(io.LimitReader(resp.Body, 32<<20))
	if err != nil {
		return fmt.Errorf("wechat: read %s response: %w", path, err)
	}
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("wechat: %s http %d: %s", path, resp.StatusCode, truncate(string(raw), 400))
	}
	if out == nil {
		return nil
	}
	if err := json.Unmarshal(raw, out); err != nil {
		return fmt.Errorf("wechat: decode %s response: %w (body: %s)", path, err, truncate(string(raw), 400))
	}
	return nil
}

// Updates long-polls for inbound messages.
//
// A client-side timeout is normal control flow, not an error: the server holds
// the request until there is something to say or roughly 35 seconds pass. This
// returns an empty response with the incoming cursor preserved so the caller can
// simply loop, which is what keeps the retry path free of timers.
func (a *API) Updates(ctx context.Context, buf string, timeout time.Duration) (*UpdatesResp, error) {
	if timeout <= 0 {
		timeout = defaultLongPoll
	}
	var resp UpdatesResp
	err := a.post(ctx, "ilink/bot/getupdates",
		getUpdatesReq{GetUpdatesBuf: buf, BaseInfo: baseInfo{ChannelVersion: channelVersion, BotAgent: botAgent}},
		&resp, timeout)
	if err != nil {
		if ctx.Err() != nil {
			return nil, ctx.Err()
		}
		if isTimeout(err) {
			return &UpdatesResp{GetUpdatesBuf: buf}, nil
		}
		return nil, err
	}
	return &resp, nil
}

// SendMessage delivers one text message and returns the server message id.
func (a *API) SendMessage(ctx context.Context, to, contextToken, text string) (string, error) {
	clientID := newClientID()
	req := sendMessageReq{
		Msg: outMessage{
			ToUserID:     to,
			ClientID:     clientID,
			MessageType:  MessageTypeBot,
			MessageState: StateFinish,
			ItemList:     []Item{{Type: ItemText, TextItem: &TextItem{Text: text}}},
			ContextToken: contextToken,
		},
		BaseInfo: baseInfo{ChannelVersion: channelVersion, BotAgent: botAgent},
	}
	var resp sendMessageResp
	if err := a.post(ctx, "ilink/bot/sendmessage", req, &resp, defaultAPITimeout); err != nil {
		return "", err
	}
	if resp.Ret != 0 || resp.Errcode != 0 {
		return "", fmt.Errorf("wechat: sendmessage ret=%d errcode=%d errmsg=%s",
			resp.Ret, resp.Errcode, resp.Errmsg)
	}
	if id := idString(resp.MessageID); id != "" {
		return id, nil
	}
	// The server does not always echo an id back. The client id is just as good
	// a handle for us, because nothing updates it afterwards anyway.
	return clientID, nil
}

// SendTyping shows or clears the "typing" indicator for a chat.
//
// It is best effort by design: a chat without a typing ticket (the server only
// issues one after a getconfig) simply gets no indicator, and the message flow
// is unaffected.
func (a *API) SendTyping(ctx context.Context, userID, ticket string, status TypingStatus) error {
	if userID == "" || ticket == "" {
		return nil
	}
	return a.post(ctx, "ilink/bot/sendtyping",
		sendTypingReq{IlinkUserID: userID, TypingTicket: ticket, Status: status,
			BaseInfo: baseInfo{ChannelVersion: channelVersion, BotAgent: botAgent}},
		nil, defaultConfigTimeout)
}

// GetConfig fetches per-user bot configuration, notably the typing ticket.
func (a *API) GetConfig(ctx context.Context, userID, contextToken string) (*getConfigResp, error) {
	var resp getConfigResp
	err := a.post(ctx, "ilink/bot/getconfig",
		getConfigReq{IlinkUserID: userID, ContextToken: contextToken,
			BaseInfo: baseInfo{ChannelVersion: channelVersion, BotAgent: botAgent}},
		&resp, defaultConfigTimeout)
	if err != nil {
		return nil, err
	}
	return &resp, nil
}

// NotifyStart and NotifyStop tell the server this client is coming up or going
// away. Neither result is acted on: they are presence hints, and a failed hint
// must not stop the channel.
func (a *API) NotifyStart(ctx context.Context) error {
	return a.post(ctx, "ilink/bot/msg/notifystart",
		struct {
			BaseInfo baseInfo `json:"base_info"`
		}{baseInfo{ChannelVersion: channelVersion, BotAgent: botAgent}}, nil, defaultConfigTimeout)
}

func (a *API) NotifyStop(ctx context.Context) error {
	return a.post(ctx, "ilink/bot/msg/notifystop",
		struct {
			BaseInfo baseInfo `json:"base_info"`
		}{baseInfo{ChannelVersion: channelVersion, BotAgent: botAgent}}, nil, defaultConfigTimeout)
}

// FetchQRCode asks for a fresh login QR code.
//
// localTokens are the bot tokens this machine already holds. The server uses
// them to answer "already bound" instead of issuing a second binding for the
// same WeChat account.
func (a *API) FetchQRCode(ctx context.Context, botType string, localTokens []string) (*qrCodeResp, error) {
	if botType == "" {
		botType = DefaultBotType
	}
	path := "ilink/bot/get_bot_qrcode?bot_type=" + url.QueryEscape(botType)
	body := struct {
		LocalTokenList []string `json:"local_token_list"`
	}{LocalTokenList: localTokens}
	if body.LocalTokenList == nil {
		body.LocalTokenList = []string{}
	}
	var resp qrCodeResp
	if err := a.post(ctx, path, body, &resp, defaultAPITimeout); err != nil {
		return nil, err
	}
	if resp.QRCode == "" {
		return nil, fmt.Errorf("wechat: get_bot_qrcode returned no qrcode (ret=%d errmsg=%s)", resp.Ret, resp.Errmsg)
	}
	return &resp, nil
}

// PollQRStatus long-polls the scan state for one QR code.
//
// This endpoint does not carry a token: the qrcode value itself is the
// credential until the scan confirms, at which point it is exchanged for one.
func (a *API) PollQRStatus(ctx context.Context, qrcode, verifyCode string, timeout time.Duration) (*qrStatusResp, error) {
	if timeout <= 0 {
		timeout = defaultLongPoll
	}
	path := "ilink/bot/get_qrcode_status?qrcode=" + url.QueryEscape(qrcode)
	if verifyCode != "" {
		path += "&verify_code=" + url.QueryEscape(verifyCode)
	}
	var resp qrStatusResp
	err := a.get(ctx, path, &resp, timeout, false)
	if err != nil {
		if ctx.Err() != nil {
			return nil, ctx.Err()
		}
		// A gateway timeout (Cloudflare 524 and friends) is the normal long-poll
		// expiry seen from the outside; treat it as "keep waiting" rather than
		// killing a login the user is about to complete.
		if isTimeout(err) || isGatewayTimeout(err) {
			return &qrStatusResp{Status: qrWait}, nil
		}
		return nil, err
	}
	return &resp, nil
}

// isTimeout reports whether err came from a deadline (ours or the transport's).
func isTimeout(err error) bool {
	if errors.Is(err, context.DeadlineExceeded) {
		return true
	}
	var ne interface{ Timeout() bool }
	return errors.As(err, &ne) && ne.Timeout()
}

// isGatewayTimeout recognises the 5xx that a proxy returns when it gives up on
// a long poll before the origin answers.
func isGatewayTimeout(err error) bool {
	msg := err.Error()
	for _, code := range []string{" http 520:", " http 522:", " http 524:", " http 504:"} {
		if strings.Contains(msg, code) {
			return true
		}
	}
	return false
}

// newClientID mints the idempotency key of one outbound message.
func newClientID() string {
	var buf [16]byte
	if _, err := rand.Read(buf[:]); err != nil {
		binary.BigEndian.PutUint64(buf[:8], uint64(time.Now().UnixNano()))
		binary.BigEndian.PutUint64(buf[8:], uint64(time.Now().UnixNano()>>3))
	}
	return "pigo-wechat-" + base64.RawURLEncoding.EncodeToString(buf[:])
}

// truncate bounds a server response echoed into an error message.
func truncate(s string, n int) string {
	if len(s) <= n {
		return s
	}
	return s[:n] + "…"
}
