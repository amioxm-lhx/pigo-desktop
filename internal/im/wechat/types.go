package wechat

import (
	"encoding/json"
	"strconv"
	"strings"
)

// Wire types for the iLink bot protocol, the transport behind WeChat's ClawBot.
//
// The protocol is JSON over HTTPS and mirrors an internal protobuf: field names
// are snake_case, enums are integers, and byte fields would be base64. Only the
// text subset is modelled here — media goes through a pre-signed CDN upload plus
// AES-ECB decryption that this adapter does not implement, and a partially
// decoded media path would be worse than an honest "unsupported".
//
// Reference implementation: @tencent-weixin/openclaw-weixin 2.4.10.

// MessageItemType classifies one entry of a message's item_list.
type MessageItemType int

const (
	ItemNone          MessageItemType = 0
	ItemText          MessageItemType = 1
	ItemImage         MessageItemType = 2
	ItemVoice         MessageItemType = 3
	ItemFile          MessageItemType = 4
	ItemVideo         MessageItemType = 5
	ItemToolCallStart MessageItemType = 11
	ItemToolCallDone  MessageItemType = 12
)

// MessageType distinguishes a human message from the bot's own echo.
type MessageType int

const (
	MessageTypeNone MessageType = 0
	MessageTypeUser MessageType = 1
	MessageTypeBot  MessageType = 2
)

// MessageState is the delivery state of an outbound message.
type MessageState int

const (
	// StateGenerating marks a message the bot is still filling in.
	StateGenerating MessageState = 1
	// StateFinish marks a complete message. Everything this adapter sends is
	// final: the channel has no patch endpoint, so a "generating" message would
	// never be updated.
	StateFinish MessageState = 2
)

// TypingStatus is the body enum of sendtyping.
type TypingStatus int

const (
	TypingStart  TypingStatus = 1
	TypingCancel TypingStatus = 2
)

// flexID is a message identifier that arrives either as a JSON string or as a
// bare uint64.
//
// This matters: the server emits 64-bit ids unquoted, and encoding them into a
// float64 (what Go's default decoding does with `any`) silently rounds them —
// the upstream JavaScript client goes as far as rewriting the payload to quote
// these keys before parsing. Decoding straight into a string keeps every digit.
type flexID string

func (f *flexID) UnmarshalJSON(b []byte) error {
	raw := strings.TrimSpace(string(b))
	if raw == "" || raw == "null" {
		*f = ""
		return nil
	}
	if raw[0] == '"' {
		var s string
		if err := json.Unmarshal(b, &s); err != nil {
			return err
		}
		*f = flexID(s)
		return nil
	}
	*f = flexID(raw)
	return nil
}

func (f flexID) String() string { return string(f) }

// String renders an id for logs and binding keys, tolerating an empty value.
func (f flexID) empty() bool { return strings.TrimSpace(string(f)) == "" }

// baseInfo is attached to every request body. The server uses it for telemetry
// and compatibility gating.
type baseInfo struct {
	ChannelVersion string `json:"channel_version"`
	BotAgent       string `json:"bot_agent"`
}

// TextItem is a plain text payload.
type TextItem struct {
	Text string `json:"text"`
}

// VoiceItem carries the server-side speech-to-text result. When present the
// text is used as the prompt: the user spoke, and refusing a voice message on a
// phone-first channel would be a poor trade.
type VoiceItem struct {
	Text  string          `json:"text,omitempty"`
	Media json.RawMessage `json:"media,omitempty"`
}

// MediaItem marks any non-text payload this adapter cannot decode. It is only
// used to recognise that something was sent, so the reply can say what arrived
// instead of staying silent.
type MediaItem struct {
	Media json.RawMessage `json:"media,omitempty"`
}

// Item is one entry of item_list.
type Item struct {
	Type      MessageItemType `json:"type"`
	MsgID     flexID          `json:"msg_id,omitempty"`
	TextItem  *TextItem       `json:"text_item,omitempty"`
	VoiceItem *VoiceItem      `json:"voice_item,omitempty"`
	ImageItem *MediaItem      `json:"image_item,omitempty"`
	FileItem  *MediaItem      `json:"file_item,omitempty"`
	VideoItem *MediaItem      `json:"video_item,omitempty"`

	// The tool-call fields are only ever written, never read: they let the
	// channel show "running bash" without spending a chat message on it.
	CreateTimeMS int64               `json:"create_time_ms,omitempty"`
	IsCompleted  bool                `json:"is_completed,omitempty"`
	ToolStart    *ToolCallStartItem  `json:"tool_call_start_item,omitempty"`
	ToolResult   *ToolCallResultItem `json:"tool_call_result_item,omitempty"`
	RefMsg       map[string]any      `json:"ref_msg,omitempty"`
}

// ToolCallStartItem announces one tool call.
type ToolCallStartItem struct {
	ToolName   string `json:"tool_name,omitempty"`
	ToolCallID string `json:"tool_call_id,omitempty"`
}

// Typing call results carry a status word, not a boolean.
type ToolCallResultItem struct {
	ToolName   string `json:"tool_name,omitempty"`
	ToolCallID string `json:"tool_call_id,omitempty"`
	Status     string `json:"status,omitempty"`
}

// Message is one inbound (or echoed outbound) message from getupdates.
type Message struct {
	FromUserID   string       `json:"from_user_id"`
	ToUserID     string       `json:"to_user_id"`
	ClientID     string       `json:"client_id"`
	MessageID    flexID       `json:"message_id"`
	MessageType  MessageType  `json:"message_type"`
	MessageState MessageState `json:"message_state"`
	ContextToken string       `json:"context_token"`
	CreateTimeMS int64        `json:"create_time_ms"`
	ItemList     []Item       `json:"item_list"`
	RunID        string       `json:"run_id"`
}

// UpdatesResp is the getupdates response. Ret/Errcode are both checked because
// the server has used either field for the same failure across versions.
type UpdatesResp struct {
	Ret                  int       `json:"ret"`
	Errcode              int       `json:"errcode"`
	Errmsg               string    `json:"errmsg"`
	Msgs                 []Message `json:"msgs"`
	GetUpdatesBuf        string    `json:"get_updates_buf"`
	LongPollingTimeoutMS int       `json:"longpolling_timeout_ms"`
}

// outMessage is the msg object of a sendmessage request.
type outMessage struct {
	FromUserID   string       `json:"from_user_id"`
	ToUserID     string       `json:"to_user_id"`
	ClientID     string       `json:"client_id"`
	MessageType  MessageType  `json:"message_type"`
	MessageState MessageState `json:"message_state"`
	ItemList     []Item       `json:"item_list,omitempty"`
	ContextToken string       `json:"context_token,omitempty"`
	RunID        string       `json:"run_id,omitempty"`
}

type sendMessageReq struct {
	Msg      outMessage `json:"msg"`
	BaseInfo baseInfo   `json:"base_info"`
}

type sendMessageResp struct {
	Ret       int    `json:"ret"`
	Errcode   int    `json:"errcode"`
	Errmsg    string `json:"errmsg"`
	MessageID flexID `json:"message_id"`
}

type getUpdatesReq struct {
	GetUpdatesBuf string   `json:"get_updates_buf"`
	BaseInfo      baseInfo `json:"base_info"`
}

type sendTypingReq struct {
	IlinkUserID  string       `json:"ilink_user_id"`
	TypingTicket string       `json:"typing_ticket,omitempty"`
	Status       TypingStatus `json:"status"`
	BaseInfo     baseInfo     `json:"base_info"`
}

type getConfigReq struct {
	IlinkUserID  string   `json:"ilink_user_id"`
	ContextToken string   `json:"context_token,omitempty"`
	BaseInfo     baseInfo `json:"base_info"`
}

type getConfigResp struct {
	Ret          int    `json:"ret"`
	Errcode      int    `json:"errcode"`
	Errmsg       string `json:"errmsg"`
	TypingTicket string `json:"typing_ticket"`
}

// qrCodeResp is get_bot_qrcode's answer: a short opaque qrcode token plus the
// URL that token encodes.
type qrCodeResp struct {
	Ret              int    `json:"ret"`
	Errcode          int    `json:"errcode"`
	Errmsg           string `json:"errmsg"`
	QRCode           string `json:"qrcode"`
	QRCodeImgContent string `json:"qrcode_img_content"`
}

// qrStatusResp is one long-poll answer from get_qrcode_status.
type qrStatusResp struct {
	Status       string `json:"status"`
	BotToken     string `json:"bot_token"`
	IlinkBotID   string `json:"ilink_bot_id"`
	IlinkUserID  string `json:"ilink_user_id"`
	BaseURL      string `json:"baseurl"`
	RedirectHost string `json:"redirect_host"`
}

// QR status values. Anything not listed is treated as "keep polling", which is
// the safe default: an unknown status must not be mistaken for a login.
const (
	qrWait            = "wait"
	qrScanned         = "scaned"
	qrNeedVerifyCode  = "need_verifycode"
	qrVerifyBlocked   = "verify_code_blocked"
	qrExpired         = "expired"
	qrScannedRedirect = "scaned_but_redirect"
	qrBindedRedirect  = "binded_redirect"
	qrConfirmed       = "confirmed"
)

// errStaleToken is the server's way of saying the bot token was rotated out
// (another WeChat login replaced this one). The upstream client pauses an hour
// instead of hammering the endpoint, and so does this one.
const errStaleToken = -14

// idString renders a flexible id for a struct key without the JSON quirks.
func idString(f flexID) string { return string(f) }

// itoa is a tiny helper so call sites do not need strconv imported twice.
func itoa(n int) string { return strconv.Itoa(n) }
