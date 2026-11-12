package wechat

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"time"
)

// DefaultBotType is the bot_type this channel registers as. The server uses it
// to pick the pairing flow; "3" is what the upstream WeChat plugin sends, and a
// different value pairs into a product this adapter was not written against.
const DefaultBotType = "3"

// DefaultLoginTimeout bounds a whole login. Scanning a QR code with a phone in
// hand takes under a minute; the margin is for finding the phone.
const DefaultLoginTimeout = 8 * time.Minute

// maxQRRefresh is how many times an expired QR code is reissued before the
// login gives up. It mirrors the upstream client: three strikes, because a
// fourth code is a sign the user walked away rather than a sign of a bad code.
const maxQRRefresh = 3

// ErrLoginAlreadyBound reports that this WeChat account is already paired with
// a running client, so issuing another binding would silently split messages
// between two bots.
var ErrLoginAlreadyBound = errors.New("wechat: this account is already connected; stop the other client first")

// Credentials is what a successful scan yields: the bot token plus the account
// identity it belongs to.
type Credentials struct {
	// Token authenticates every subsequent API call. It is the entire
	// credential — there is no secret and no signature.
	Token string
	// BotID identifies this bot binding (ilink_bot_id).
	BotID string
	// UserID identifies the human who scanned (ilink_user_id). It is also the
	// peer id inbound messages carry, and the key context tokens are stored
	// against.
	UserID string
	// BaseURL is the host the server told us to use. It can differ from the
	// default after an idc redirect and must be honoured for later calls.
	BaseURL string
}

// LoginOptions controls the interactive scan.
type LoginOptions struct {
	// BaseURL overrides the API root (defaults to DefaultBaseURL).
	BaseURL string
	// BotType overrides the pairing product (defaults to DefaultBotType).
	BotType string
	// Timeout bounds the whole exchange (defaults to DefaultLoginTimeout).
	Timeout time.Duration
	// PreferTokens are tokens this machine already holds. The server answers
	// "already bound" instead of creating a second binding for the same
	// account, which is the only reliable way to detect a duplicate login.
	PreferTokens []string
	// ShowQR receives each fresh QR URL. It is called again when a code expires
	// and is reissued, so the caller can redraw.
	ShowQR func(qrURL string)
	// AskVerifyCode prompts for the digits WeChat shows on the phone. Some
	// pairings require it; most do not. It may be nil, in which case a pairing
	// that demands a code fails with an explanation.
	AskVerifyCode func(prompt string) (string, error)
	// Logf receives progress lines.
	Logf func(string, ...any)
}

// Login runs the QR pairing and returns the resulting credentials.
//
// The flow is: ask for a QR code, long-poll its status, and handle whatever the
// phone does — scan, expire, or demand a number. Nothing here touches the
// filesystem; persisting the result is the caller's job, which keeps the login
// usable from a test.
func Login(ctx context.Context, opts LoginOptions) (*Credentials, error) {
	logf := opts.Logf
	if logf == nil {
		logf = func(string, ...any) {}
	}
	timeout := opts.Timeout
	if timeout <= 0 {
		timeout = DefaultLoginTimeout
	}
	ctx, cancel := context.WithTimeout(ctx, timeout)
	defer cancel()

	api := NewAPI(opts.BaseURL, "", logf)
	botType := opts.BotType
	if botType == "" {
		botType = DefaultBotType
	}

	qr, err := api.FetchQRCode(ctx, botType, opts.PreferTokens)
	if err != nil {
		return nil, err
	}
	show := func(resp *qrCodeResp) {
		if opts.ShowQR != nil && resp.QRCodeImgContent != "" {
			opts.ShowQR(resp.QRCodeImgContent)
		}
	}
	show(qr)
	logf("wechat: 请用手机微信扫描二维码（或用同账号打开上面的链接）")

	var verifyCode string
	scanned := false
	refreshes := 1

	for ctx.Err() == nil {
		status, err := api.PollQRStatus(ctx, qr.QRCode, verifyCode, defaultLongPoll)
		if err != nil {
			return nil, err
		}

		switch status.Status {
		case qrWait, "":
			// Nothing yet.

		case qrScanned:
			// A pending code that gets re-scanned means the last one was right;
			// keep polling with it rather than asking again.
			verifyCode = ""
			if !scanned {
				logf("wechat: 已扫码，请在手机上确认登录")
				scanned = true
			}

		case qrNeedVerifyCode:
			if opts.AskVerifyCode == nil {
				return nil, errors.New("wechat: 这次登录需要在手机上输入配对数字，但当前环境无法读取输入；请改用 `pigo im login --channel wechat` 在终端完成")
			}
			prompt := "输入手机微信上显示的数字，然后回车："
			if verifyCode != "" {
				prompt = "数字不匹配，请重新输入手机微信上显示的数字："
			}
			code, err := opts.AskVerifyCode(prompt)
			if err != nil {
				return nil, err
			}
			verifyCode = strings.TrimSpace(code)
			// Poll again immediately: waiting a second on a code the phone is
			// already waiting for only makes the pairing feel stuck.
			continue

		case qrVerifyBlocked:
			logf("wechat: 验证码多次错误，换一张二维码重新开始")
			verifyCode = ""
			refreshes++
			if refreshes > maxQRRefresh {
				return nil, errors.New("wechat: 连续输错验证码，登录已停止；稍后再试")
			}
			if err := reissue(ctx, api, botType, opts, show, &qr); err != nil {
				return nil, err
			}
			scanned = false

		case qrExpired:
			refreshes++
			if refreshes > maxQRRefresh {
				return nil, errors.New("wechat: 二维码多次过期，登录已停止；请重新发起")
			}
			logf("wechat: 二维码已过期，正在换一张（%d/%d）", refreshes, maxQRRefresh)
			if err := reissue(ctx, api, botType, opts, show, &qr); err != nil {
				return nil, err
			}
			verifyCode = ""
			scanned = false

		case qrScannedRedirect:
			// The account is served by a different data centre; follow it or
			// every later call lands in the wrong place.
			if status.RedirectHost != "" {
				api.SetBaseURL(status.RedirectHost)
				logf("wechat: 服务端要求切换到 %s，已跟随", status.RedirectHost)
			}

		case qrBindedRedirect:
			return nil, ErrLoginAlreadyBound

		case qrConfirmed:
			if status.BotToken == "" || status.IlinkBotID == "" {
				return nil, errors.New("wechat: 登录已确认，但服务端没有返回 bot token")
			}
			creds := &Credentials{
				Token:   status.BotToken,
				BotID:   status.IlinkBotID,
				UserID:  status.IlinkUserID,
				BaseURL: normalizeBaseURL(status.BaseURL),
			}
			logf("wechat: 登录成功（bot=%s user=%s host=%s）", creds.BotID, creds.UserID, creds.BaseURL)
			return creds, nil

		default:
			// Unknown status: keep polling. Treating it as success would be a
			// security bug; treating it as failure would break on a server that
			// added an informational state.
			logf("wechat: 未知的扫码状态 %q，继续等待", status.Status)
		}

		if !sleepCtx(ctx, time.Second) {
			break
		}
	}

	if errors.Is(ctx.Err(), context.DeadlineExceeded) {
		return nil, fmt.Errorf("wechat: 登录超时（%s 内未完成扫码）", timeout)
	}
	return nil, ctx.Err()
}

// reissue fetches a new QR code and shows it.
func reissue(ctx context.Context, api *API, botType string, opts LoginOptions,
	show func(*qrCodeResp), into **qrCodeResp) error {
	fresh, err := api.FetchQRCode(ctx, botType, opts.PreferTokens)
	if err != nil {
		return fmt.Errorf("wechat: 刷新二维码: %w", err)
	}
	*into = fresh
	show(fresh)
	return nil
}

// sleepCtx waits d, returning false if ctx ended first.
func sleepCtx(ctx context.Context, d time.Duration) bool {
	t := time.NewTimer(d)
	defer t.Stop()
	select {
	case <-ctx.Done():
		return false
	case <-t.C:
		return true
	}
}
