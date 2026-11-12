// Package imcmd implements `pigo im`, the remote-control channel command.
//
// It owns the assembly that the rest of pigo does not need to know about:
// resolve credentials from the layered config, build a serve.Service over the
// shared session store, and hand it to a Router so an IM chat can drive a
// session. Nothing here implements agent behaviour — that all comes from
// internal/serve and internal/runtime.
package imcmd

import (
	"bufio"
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"os/signal"
	"path/filepath"
	"sort"
	"strings"
	"time"

	qrcode "github.com/skip2/go-qrcode"
	flag "github.com/spf13/pflag"

	"github.com/smallnest/pigo/internal/agentcore"
	"github.com/smallnest/pigo/internal/cli/config"
	"github.com/smallnest/pigo/internal/cli/headless"
	"github.com/smallnest/pigo/internal/cli/run"
	"github.com/smallnest/pigo/internal/im"
	"github.com/smallnest/pigo/internal/im/desktop"
	"github.com/smallnest/pigo/internal/im/feishu"
	"github.com/smallnest/pigo/internal/im/wechat"
	"github.com/smallnest/pigo/internal/provider"
	"github.com/smallnest/pigo/internal/remotecontrol"
	"github.com/smallnest/pigo/internal/runtime"
	"github.com/smallnest/pigo/internal/serve"
	"github.com/smallnest/pigo/internal/session"
)

// Channel identifiers accepted by --channel. They are the adapters' own ids so a
// log line, a binding key and a flag value always name the same thing.
const (
	channelFeishu = feishu.ID
	channelWechat = wechat.ID
)

// Run dispatches `pigo im <subcommand>` and returns a process exit code.
func Run(ctx context.Context, args []string, out, errOut io.Writer) int {
	if len(args) == 0 {
		usage(errOut)
		return 2
	}
	switch args[0] {
	case "serve":
		return runServe(ctx, args[1:], out, errOut)
	case "login":
		return runLogin(ctx, args[1:], out, errOut)
	case "bindings":
		return runBindings(args[1:], out, errOut)
	case "help", "-h", "--help":
		usage(out)
		return 0
	default:
		fmt.Fprintf(errOut, "pigo im: unknown subcommand %q\n\n", args[0])
		usage(errOut)
		return 2
	}
}

func usage(w io.Writer) {
	fmt.Fprint(w, `usage: pigo im <command>

Commands:
  login      pair a channel that needs it (wechat) by scanning a QR code
  serve      start the IM channel and let it drive pigo sessions
  bindings   list which chat is bound to which session

Serve flags:
  --channel CHANNEL       feishu | wechat (default feishu)
  --config PATH           config.toml to read (default: the user config)
  --cwd PATH              working directory new sessions are bound to
  --model ID              model new sessions start on
  --tool-policy P         confirm | allow | deny (overrides [im] tool_policy)
  --queue-depth N         per-chat backlog bound (overrides config)
  --allow-user ID         identity allowed to control the agent (repeatable)
  --allow-groups          also accept group chats (they must @ the bot)
  --no-tools              disable the built-in file/shell tools
  --session-host HOST     cli (default) | desktop: run sessions here, or drive
                          the ones in a running pigo desktop app

Login flags:
  --channel wechat        channel to pair
  --timeout 8m            how long to wait for the scan
  --state PATH            where the token is written (default: $PIGO_HOME/im/wechat.json)
  --qr-out PATH           write the QR as a PNG there instead of drawing it on the
                          terminal (the desktop app shows that image)
  --base-url URL          API root override (empty uses the one the scan returns)

Configuration lives in config.toml:

  [im]
  enabled = true
  tool_policy = "confirm"

  [im.feishu]
  enabled = true
  app_id = "cli_xxxxxxxx"
  credential = "feishu-app-secret"   # name in .credentials.yaml, not the secret
  allowed_users = ["ou_xxxxxxxxxxxx"]

  [im.wechat]
  enabled = true
  # no secret here: the bot token comes from "pigo im login --channel wechat"
  allowed_users = ["<the id printed by the first inbound message>"]
`)
}

// settings is the resolved, validated configuration for one serve run.
type settings struct {
	// channel is the adapter this run drives: feishu or wechat.
	channel string
	// appID/appSecret are Feishu's credentials. They are empty for WeChat, which
	// authenticates with a token obtained by scanning a QR code.
	appID     string
	appSecret string
	// wechatToken/wechatBaseURL override what the login stored. Both are
	// normally empty: the state file is the source of truth.
	wechatToken   string
	wechatBaseURL string
	// wechatStatePath is where the bot token, the cursor and the per-peer
	// context tokens live.
	wechatStatePath  string
	allowedUsers     []string
	allowGroups      bool
	requireMention   bool
	queueDepth       int
	rejectCooldown   time.Duration
	progressThrottle time.Duration
	approvalTimeout  time.Duration
	toolPolicy       im.ToolPolicy
	// sessionHost is "cli" (sessions live in this process) or "desktop" (they
	// live in a running pigo desktop app, reached over its control endpoint).
	sessionHost   string
	sessionCwd    string
	sessionModel  string
	egressWarning bool
}

func runServe(ctx context.Context, args []string, out, errOut io.Writer) int {
	fs := flag.NewFlagSet("im serve", flag.ContinueOnError)
	fs.SetOutput(errOut)
	cfgPath := fs.String("config", "", "config.toml to read (default: the standard user config)")
	channel := fs.String("channel", "feishu", "channel to start")
	cwd := fs.String("cwd", "", "working directory new sessions are bound to")
	model := fs.String("model", "", "model new sessions start on")
	toolPolicy := fs.String("tool-policy", "", "confirm | allow | deny")
	queueDepth := fs.Int("queue-depth", 0, "per-chat backlog bound")
	allowUsers := fs.StringArray("allow-user", nil, "open_id or email allowed to control the agent (repeatable)")
	allowGroups := fs.Bool("allow-groups", false, "also accept group chats")
	noTools := fs.Bool("no-tools", false, "disable the built-in file/shell tools")
	sessionHost := fs.String("session-host", "", "cli | desktop: where the sessions this channel drives actually live")
	if err := fs.Parse(args); err != nil {
		return 2
	}

	path := *cfgPath
	if path == "" {
		path = config.FileConfigPath()
	}
	fileCfg, err := config.LoadFileConfig(path)
	if err != nil {
		fmt.Fprintf(errOut, "pigo: %v\n", err)
		return 2
	}

	set, err := resolveSettings(*channel, fileCfg.IM, *toolPolicy, *queueDepth, *cwd, *model,
		*allowUsers, *allowGroups, *sessionHost, path)
	if err != nil {
		fmt.Fprintf(errOut, "pigo im: %v\n", err)
		return 2
	}
	if set.channel == channelFeishu {
		if err := set.resolveSecret(fileCfg.IM.Feishu); err != nil {
			fmt.Fprintf(errOut, "pigo im: %v\n", err)
			return 2
		}
	}

	logf := func(format string, a ...any) {
		fmt.Fprintf(errOut, "[im] "+format+"\n", a...)
	}

	// Where the sessions live is a carrier choice, not a channel concern:
	// either this process runs them, or a running desktop app does. Everything
	// downstream (queue, commands, cards) is identical.
	var (
		host    im.SessionHost
		approve *im.ApprovalRouter
		cleanup = func() {}
	)
	if set.sessionHost == im.SessionHostDesktop {
		client, herr := desktop.Discover()
		if herr != nil {
			fmt.Fprintf(errOut, "pigo im: %v\n", herr)
			return 2
		}
		host = client
		// The desktop app owns the tool-confirmation dialog, but a chat driving
		// that session cannot see it. Build the approval router anyway: the
		// Router polls the app's open permission queue (permissions/pending on
		// the control endpoint) and mirrors each question into a chat card.
		// WrapRunConfig is not used in this mode — the run config lives in the
		// app, so there is no BeforeToolCall hook to decorate here.
		approve = im.NewApprovalRouter(set.toolPolicy, set.approvalTimeout, logf)
		logf("sessions come from the desktop app at %s", client.Endpoint)
	} else {
		// The approval router is built first because the service captures its
		// RunConfigFunc at construction: the tool-confirmation hook has to be
		// in place before the first run, so the wrapper is applied while
		// building.
		approve = im.NewApprovalRouter(set.toolPolicy, set.approvalTimeout, logf)

		// The service layer is the only agent-facing dependency: the adapter
		// feeds it prompts, it feeds events back. Building it here (rather than
		// inside im) keeps internal/im free of provider/tool knowledge.
		svc, svcCleanup, serr := buildService(set, *noTools, approve, logf)
		if serr != nil {
			fmt.Fprintf(errOut, "pigo im: %v\n", serr)
			return 2
		}
		host, cleanup = im.NewServeHost(svc), svcCleanup
	}
	defer cleanup()

	adapter, err := buildAdapter(ctx, &set, logf)
	if err != nil {
		fmt.Fprintf(errOut, "pigo im: %v\n", err)
		return 2
	}
	defer adapter.Close()

	store, err := im.NewBindingStore(im.DefaultBindingPath())
	if err != nil {
		fmt.Fprintf(errOut, "pigo im: %v\n", err)
		return 2
	}

	router := im.NewRouter(im.RouterConfig{
		AllowedUsers:     set.allowedUsers,
		AllowGroups:      set.allowGroups,
		RequireMention:   set.requireMention,
		QueueDepth:       set.queueDepth,
		RejectCooldown:   set.rejectCooldown,
		ProgressThrottle: set.progressThrottle,
		SessionCwd:       set.sessionCwd,
		SessionModel:     set.sessionModel,
		Logf:             logf,
	}, host, store, adapter, approve)

	printStartup(out, set, store, errOut)

	sigCtx, stop := signal.NotifyContext(ctx, os.Interrupt)
	defer stop()

	if err := router.Start(sigCtx); err != nil && !errors.Is(err, context.Canceled) {
		fmt.Fprintf(errOut, "pigo im: %v\n", err)
		router.Close()
		router.Wait()
		return 1
	}
	router.Wait()
	fmt.Fprintln(errOut, "[im] stopped")
	return 0
}

// buildAdapter constructs the channel named in settings.
//
// The two channels differ in what "credentials" means: Feishu needs an app id and
// a secret someone typed in, WeChat needs a bot token that only exists after a QR
// scan. Everything after this function — queue, routing, approvals — is
// channel-blind, which is what makes a new channel an adapter and not a fork of
// the router.
func buildAdapter(ctx context.Context, set *settings, logf func(string, ...any)) (im.Adapter, error) {
	switch set.channel {
	case channelWechat:
		adapter, err := wechat.New(wechat.Config{
			Token:           set.wechatToken,
			BaseURL:         set.wechatBaseURL,
			StatePath:       set.wechatStatePath,
			Logf:            logf,
			ApprovalTimeout: set.approvalTimeout,
		})
		if err != nil {
			return nil, err
		}
		logf("wechat: bound to bot=%s user=%s", adapter.BotID(), adapter.UserID())
		return adapter, nil

	case channelFeishu:
		adapter, err := feishu.New(feishu.Config{
			AppID:           set.appID,
			AppSecret:       set.appSecret,
			Logf:            logf,
			ApprovalTimeout: set.approvalTimeout,
		})
		if err != nil {
			return nil, err
		}
		// The allow list is documented as accepting an email, but the message
		// event only carries an open_id, so emails are translated here — once,
		// before the router exists. Doing it at startup is also what makes a bad
		// allow list a visible error instead of a channel that silently drops
		// every message.
		resolved, unresolved, aerr := adapter.ResolveAllowedUsers(ctx, set.allowedUsers)
		if aerr != nil {
			logf("allow list: %v", aerr)
		}
		for _, u := range unresolved {
			logf("allow list entry %q did not resolve to a user; the app needs the 获取用户 user ID "+
				"permission for email entries — otherwise use the ou_… open_id from the "+
				"\"dropped unauthorized\" or \"p2p chat opened\" log lines", u)
		}
		set.allowedUsers = resolved
		return adapter, nil

	default:
		return nil, fmt.Errorf("channel %q is not implemented (supported: %s, %s)",
			set.channel, channelFeishu, channelWechat)
	}
}

// resolveSettings merges the config table with the command-line overrides and
// validates the result. Flags win, matching the rest of pigo's
// flag > config > default chain.
func resolveSettings(channel string, cfg config.IMConfig, policyFlag string, depthFlag int, cwdFlag, modelFlag string,
	usersFlag []string, groupsFlag bool, hostFlag, cfgPath string) (settings, error) {

	channel = strings.ToLower(strings.TrimSpace(channel))
	if channel == "" {
		channel = channelFeishu
	}
	switch channel {
	case channelFeishu, channelWechat:
	default:
		return settings{}, fmt.Errorf("channel %q is not implemented (supported: %s, %s)",
			channel, channelFeishu, channelWechat)
	}

	// Both channel tables carry the same knobs, so they are read through one
	// shape instead of duplicating the validation below per channel.
	var (
		users          []string
		allowGroups    bool
		requireMention *bool
		queueDepth     int
		rejectCooldown string
		progressRaw    string
		model, cwd     string
	)
	fc, wc := cfg.Feishu, cfg.Wechat
	switch channel {
	case channelWechat:
		users = wc.AllowedUsers
		queueDepth = wc.QueueDepth
		rejectCooldown = wc.RejectCooldown
		progressRaw = wc.ProgressThrottle
		model, cwd = wc.Model, wc.Cwd
	default:
		users = fc.AllowedUsers
		allowGroups = fc.AllowGroups
		requireMention = fc.RequireMention
		queueDepth = fc.QueueDepth
		rejectCooldown = fc.RejectCooldown
		progressRaw = fc.ProgressThrottle
		model, cwd = fc.Model, fc.Cwd
	}

	set := settings{
		channel:         channel,
		appID:           fc.AppID,
		appSecret:       fc.AppSecret,
		wechatToken:     wc.BotToken,
		wechatBaseURL:   wc.BaseURL,
		wechatStatePath: wechat.DefaultStatePath(),
		allowGroups:     allowGroups || groupsFlag,
		requireMention:  requireMention == nil || *requireMention,
		queueDepth:      queueDepth,
		toolPolicy:      im.ToolPolicy(strings.TrimSpace(cfg.ToolPolicy)),
		sessionHost:     strings.ToLower(strings.TrimSpace(firstNonEmpty(hostFlag, cfg.SessionHost))),
		sessionCwd:      firstNonEmpty(cwdFlag, cwd, cfg.SessionCwd),
		sessionModel:    firstNonEmpty(modelFlag, model),
		egressWarning:   cfg.EgressWarning == nil || *cfg.EgressWarning,
	}
	// WeChat's bot is a single-account channel with no group surface, so the
	// group gate is not merely off: there is nothing to turn on.
	if channel == channelWechat {
		set.allowGroups = false
		set.requireMention = false
	}

	if len(usersFlag) > 0 {
		set.allowedUsers = normalizeUsers(usersFlag)
	} else {
		set.allowedUsers = normalizeUsers(users)
	}
	if depthFlag > 0 {
		set.queueDepth = depthFlag
	} else if set.queueDepth <= 0 {
		set.queueDepth = im.DefaultQueueDepth
	}
	if policyFlag != "" {
		set.toolPolicy = im.ToolPolicy(strings.TrimSpace(policyFlag))
	}
	switch set.sessionHost {
	case "":
		set.sessionHost = im.SessionHostCLI
	case im.SessionHostCLI, im.SessionHostDesktop:
	default:
		return settings{}, fmt.Errorf("session_host %q must be cli or desktop", set.sessionHost)
	}
	switch set.toolPolicy {
	case "":
		set.toolPolicy = im.ToolPolicyConfirm
	case im.ToolPolicyConfirm, im.ToolPolicyAllow, im.ToolPolicyDeny:
	default:
		return settings{}, fmt.Errorf("tool_policy %q must be confirm, allow or deny", set.toolPolicy)
	}

	var err error
	if set.rejectCooldown, err = durationOr(rejectCooldown, 10*time.Second, "reject_cooldown"); err != nil {
		return settings{}, err
	}
	if set.progressThrottle, err = durationOr(progressRaw, 2*time.Second, "progress_throttle"); err != nil {
		return settings{}, err
	}
	if set.approvalTimeout, err = durationOr(cfg.ApprovalTimeout, feishu.DefaultApprovalTimeout, "approval_timeout"); err != nil {
		return settings{}, err
	}

	// Startup gates. Both of these would otherwise fail silently at the first
	// message, which is the worst moment to learn about a configuration error.
	if !cfg.Enabled && len(usersFlag) == 0 {
		return settings{}, fmt.Errorf("the [im] channel is disabled; set `enabled = true` in [im] in %s (or pass --allow-user to run ad hoc)", displayPath(cfgPath))
	}
	if len(set.allowedUsers) == 0 {
		return settings{}, fmt.Errorf("im.%s.allowed_users is empty: without an allow list the channel would refuse every message. "+
			"Add the id of the account that may control this machine (the channel prints it on the first inbound message)", channel)
	}

	if channel == channelWechat {
		// The token normally lives in the state file the QR login wrote. Checking
		// it here means "not logged in" is a startup error with the exact fix,
		// instead of a channel that starts and then refuses every request.
		if strings.TrimSpace(set.wechatToken) == "" {
			st, lerr := wechat.LoadState(set.wechatStatePath)
			if lerr != nil {
				return settings{}, lerr
			}
			if strings.TrimSpace(st.Token) == "" {
				return settings{}, fmt.Errorf("wechat is not paired yet: run `pigo im login --channel wechat` and scan the QR code "+
					"(the token is written to %s)", displayPath(set.wechatStatePath))
			}
		}
		return set, nil
	}

	if strings.TrimSpace(set.appID) == "" {
		return settings{}, errors.New("im.feishu.app_id is empty; copy it from the app's 凭证与基础信息 page")
	}
	return set, nil
}

// resolveSecret fills appSecret from the credential file when a reference is set.
func (s *settings) resolveSecret(fc config.FeishuConfig) error {
	if strings.TrimSpace(fc.Credential) != "" {
		secret, err := provider.ResolveCredentialReference(provider.CredentialFilePath(), fc.Credential)
		if err != nil {
			return fmt.Errorf("resolve im.feishu.credential %q: %w", fc.Credential, err)
		}
		s.appSecret = secret
	}
	if strings.TrimSpace(s.appSecret) == "" {
		return errors.New("no app secret: set im.feishu.credential (preferred) or im.feishu.app_secret")
	}
	return nil
}

// buildService assembles the serve.Service over the shared session store.
//
// The returned cleanup releases the resources SetupEnv opened (plugins, MCP
// subprocesses, the memory DB). Forgetting to call it would leak child
// processes for the lifetime of the channel.
func buildService(set settings, noTools bool, approve *im.ApprovalRouter, logf func(string, ...any)) (serve.Service, func(), error) {
	noop := func() {}
	store, err := headless.SessionStore()
	if err != nil {
		return nil, noop, err
	}

	env, err := run.SetupEnv(
		set.sessionModel, // model
		"",               // baseURL: provider default
		"",               // protocol: inferred
		"",               // provider: inferred from model
		"",               // apiKey: resolved from env by the provider
		noTools,
		false, // noSkills
		"",    // systemPrompt: default
		nil,   // appendSystemPrompt
		true,  // memEnabled
		run.ToolPolicy{},
		run.MCPSettings{},
	)
	if err != nil {
		return nil, noop, err
	}
	cleanup := func() {
		if env.Plugins != nil {
			_ = env.Plugins.Close()
		}
		if env.MCP != nil {
			_ = env.MCP.Close()
		}
		if env.Memory != nil {
			_ = env.Memory.Close()
		}
	}

	creds := provider.NewCredentialStore(nil)

	base := func(h session.SessionHeader, th agentcore.ThinkingLevel) runtime.RunConfig {
		model := h.Model
		if model == "" {
			model = set.sessionModel
		}
		providerName := h.Provider
		if providerName == "" {
			providerName = env.ProviderName
		}
		cfg := run.NewConfig(model, providerName, th, env.Provider, creds,
			run.ToolRegistry(env.Tools), run.TodoReminders(env.Tools), env.Schedule)
		cfg.SessionID = h.ID
		cfg.MemoryRoot = run.MemoryRootFromTools(env.Tools)
		return cfg
	}
	// Wrapping here (not after construction) is what puts the IM approval hook
	// in the run config the service will use for every prompt.
	svc, err := serve.NewService(store, approve.WrapRunConfig(base))
	if err != nil {
		cleanup()
		return nil, noop, err
	}
	return svc, cleanup, nil
}

// printStartup reports what the channel will and will not allow. This is the
// one place a human reads before walking away from the machine, so it states the
// security posture in plain words rather than echoing config keys.
func printStartup(out io.Writer, set settings, store *im.BindingStore, errOut io.Writer) {
	switch set.channel {
	case channelWechat:
		fmt.Fprintf(out, "channel     wechat (iLink bot, long poll — no public IP needed)\n")
		fmt.Fprintf(out, "state       %s\n", displayPath(set.wechatStatePath))
	default:
		fmt.Fprintf(out, "channel     %s (long connection, no public IP needed)\n", feishu.ID)
		fmt.Fprintf(out, "app_id      %s\n", set.appID)
	}
	fmt.Fprintf(out, "allowed     %s\n", strings.Join(set.allowedUsers, ", "))
	if set.channel == channelFeishu {
		fmt.Fprintf(out, "groups      %s\n", onOff(set.allowGroups))
		if set.allowGroups {
			fmt.Fprintf(out, "mention     %s (group chats must @ the bot)\n", requiredIf(set.requireMention))
		}
	}
	fmt.Fprintf(out, "sessions    %s\n", hostLabel(set.sessionHost))
	fmt.Fprintf(out, "tool policy %s\n", set.toolPolicy)
	fmt.Fprintf(out, "queue       %d per chat, reject reply throttled to once per %s\n", set.queueDepth, set.rejectCooldown)
	fmt.Fprintf(out, "cwd         %s\n", displayPath(set.sessionCwd))
	fmt.Fprintf(out, "bindings    %s\n", displayPath(store.Path()))

	if set.sessionHost == im.SessionHostDesktop {
		fmt.Fprintf(errOut, "[im] note: sessions live in the desktop app; close it and this channel has nothing to drive.\n")
		fmt.Fprintf(errOut, "[im] note: tool confirmations are mirrored into this chat and can also be answered in the app.\n")
	}
	if set.toolPolicy == im.ToolPolicyAllow {
		fmt.Fprintf(errOut, "[im] WARNING: tool_policy=allow means a message in this chat can run shell commands without asking.\n")
	}
	if len(set.allowedUsers) == 0 {
		// An allow list that resolved to nothing looks configured in the
		// settings file and refuses the whole world at runtime. Say so here,
		// where the operator is still looking at the console.
		hint := `Add the sender's ou_… open_id to allowed_users (the "dropped unauthorized" and "p2p chat opened" lines show it).`
		if set.channel == channelWechat {
			hint = `Add the sender's user id to allowed_users (the "chat opened with user=…" line shows it).`
		}
		fmt.Fprintf(errOut, "[im] WARNING: the allow list resolved to nobody, so every message will be refused. %s\n", hint)
	}
	if set.egressWarning {
		fmt.Fprintf(errOut, "[im] note: prompts and answers travel through the platform's servers and are visible to it.\n")
	}
}

// runLogin pairs a channel that needs an interactive handshake.
//
// Only WeChat needs this: it authenticates with a bot token that exists solely as
// the result of a QR scan, so there is nothing to paste into config.toml. The
// token is written to the state file next to the chat bindings, mode 0600.
func runLogin(ctx context.Context, args []string, out, errOut io.Writer) int {
	fs := flag.NewFlagSet("im login", flag.ContinueOnError)
	fs.SetOutput(errOut)
	channel := fs.String("channel", channelWechat, "channel to pair (wechat)")
	baseURL := fs.String("base-url", "", "API root override")
	timeout := fs.Duration("timeout", wechat.DefaultLoginTimeout, "how long to wait for the scan")
	statePath := fs.String("state", "", "where the token is written (default: $PIGO_HOME/im/wechat.json)")
	qrOut := fs.String("qr-out", "", "write the QR code as a PNG here instead of drawing it on the terminal")
	if err := fs.Parse(args); err != nil {
		return 2
	}
	if *channel != channelWechat {
		fmt.Fprintf(errOut, "pigo im login: channel %q does not need a login step (only %s does)\n", *channel, channelWechat)
		return 2
	}

	path := *statePath
	if path == "" {
		path = wechat.DefaultStatePath()
	}
	st, err := wechat.LoadState(path)
	if err != nil {
		fmt.Fprintf(errOut, "pigo im login: %v\n", err)
		return 1
	}

	// Handing the previous token to the server is how a duplicate pairing is
	// detected: it answers "already bound" instead of splitting one account
	// between two clients that each see half the messages.
	var prefer []string
	if strings.TrimSpace(st.Token) != "" {
		prefer = []string{st.Token}
	}

	logf := func(format string, a ...any) {
		fmt.Fprintf(errOut, "[im] "+format+"\n", a...)
	}

	// The verify-code prompt needs a line of stdin. Reading directly (rather than
	// through a shared reader) is fine because login runs no other input.
	stdin := bufio.NewReader(os.Stdin)

	creds, err := wechat.Login(ctx, wechat.LoginOptions{
		BaseURL:      firstNonEmpty(*baseURL, st.BaseURL),
		BotType:      wechat.DefaultBotType,
		Timeout:      *timeout,
		PreferTokens: prefer,
		ShowQR: func(qrURL string) {
			// A GUI cannot rely on block-art line metrics, so it asks for a real
			// image instead and renders it at its own size.
			if target := strings.TrimSpace(*qrOut); target != "" {
				if werr := writeQRPNG(target, qrURL); werr != nil {
					logf("qr image: %v", werr)
				} else {
					fmt.Fprintf(out, "二维码已写入 %s\n", target)
				}
				// The image is what a GUI draws, but the link is still the only
				// way to finish when the phone cannot point at this screen. Print
				// it alone on its line so a caller can lift it out verbatim.
				fmt.Fprintln(out, qrURL)
				return
			}
			if art, rerr := remotecontrol.Render(qrURL); rerr == nil {
				fmt.Fprintln(out, art)
			}
			fmt.Fprintf(out, "若二维码不方便扫描，用手机微信打开这个链接：\n%s\n\n", qrURL)
		},
		AskVerifyCode: func(prompt string) (string, error) {
			// Newline-terminated on purpose: a GUI reads this stream line by line
			// to know when to ask the user for the digits.
			fmt.Fprintln(out, prompt)
			line, rerr := stdin.ReadString('\n')
			if rerr != nil && strings.TrimSpace(line) == "" {
				return "", rerr
			}
			return strings.TrimSpace(line), nil
		},
		Logf: logf,
	})
	if err != nil {
		if errors.Is(err, wechat.ErrLoginAlreadyBound) {
			fmt.Fprintf(errOut, "pigo im login: 这个微信号已经绑定过另一个客户端。\n"+
				"  如果那个客户端还在跑，先把它停掉再重新登录；如果已经不用了，忽略本提示直接\n"+
				"  用现有的 token 启动通道即可（token 在 %s）。\n", displayPath(path))
			return 1
		}
		fmt.Fprintf(errOut, "pigo im login: %v\n", err)
		return 1
	}

	st = wechat.ApplyCredentials(st, creds)
	if err := wechat.SaveState(path, st); err != nil {
		fmt.Fprintf(errOut, "pigo im login: %v\n", err)
		return 1
	}

	fmt.Fprintf(out, "\n绑定完成。\n")
	fmt.Fprintf(out, "  bot      %s\n", creds.BotID)
	fmt.Fprintf(out, "  微信账号 %s\n", creds.UserID)
	fmt.Fprintf(out, "  token    %s\n", displayPath(path))
	fmt.Fprintf(out, "\n下一步：把 %s 写进 config.toml 的 [im.wechat] allowed_users，然后运行\n", creds.UserID)
	fmt.Fprintf(out, "  pigo im serve --channel wechat\n")
	return 0
}

// writeQRPNG renders the pairing URL as an image.
//
// It exists for the desktop app: a QR drawn with block characters assumes a
// specific font, line height and background, all of which a webview is free to
// change, and an unscannable QR is indistinguishable from a failing pairing. A
// PNG has no such assumptions. Written via rename so a reader polling the file
// never sees a half-written image.
func writeQRPNG(path, content string) error {
	png, err := qrcode.Encode(content, qrcode.Medium, 360)
	if err != nil {
		return err
	}
	if dir := filepath.Dir(path); dir != "" {
		if err := os.MkdirAll(dir, 0o700); err != nil {
			return err
		}
	}
	tmp := path + ".tmp"
	if err := os.WriteFile(tmp, png, 0o600); err != nil {
		return err
	}
	return os.Rename(tmp, path)
}

// hostLabel is the human form of the session host setting.
func hostLabel(host string) string {
	if host == im.SessionHostDesktop {
		return "desktop app (same conversations as the app)"
	}
	return "this process (pigo store)"
}

// runBindings prints the chat → session map.
func runBindings(_ []string, out, _ io.Writer) int {
	store, err := im.NewBindingStore(im.DefaultBindingPath())
	if err != nil {
		fmt.Fprintf(out, "read bindings: %v\n", err)
		return 1
	}
	keys := store.Keys()
	if len(keys) == 0 {
		fmt.Fprintln(out, "no bindings yet")
		return 0
	}
	all := store.List()
	rows := make([][4]string, 0, len(keys))
	for _, k := range keys {
		b := all[k]
		rows = append(rows, [4]string{k, b.SessionID, b.Model, b.LastActive.Local().Format("2006-01-02 15:04")})
	}
	sort.SliceStable(rows, func(i, j int) bool { return rows[i][3] > rows[j][3] })
	for _, r := range rows {
		fmt.Fprintf(out, "%s\t%s\t%s\t%s\n", r[0], r[1], r[2], r[3])
	}
	return 0
}

// ---- helpers ----

func durationOr(raw string, def time.Duration, field string) (time.Duration, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return def, nil
	}
	d, err := time.ParseDuration(raw)
	if err != nil {
		return 0, fmt.Errorf("%s %q is not a duration (try \"10m\" or \"30s\"): %w", field, raw, err)
	}
	if d <= 0 {
		return 0, fmt.Errorf("%s must be positive, got %s", field, d)
	}
	return d, nil
}

func normalizeUsers(in []string) []string {
	seen := make(map[string]struct{}, len(in))
	out := make([]string, 0, len(in))
	for _, u := range in {
		u = strings.TrimSpace(u)
		if u == "" {
			continue
		}
		if _, dup := seen[u]; dup {
			continue
		}
		seen[u] = struct{}{}
		out = append(out, u)
	}
	return out
}

func firstNonEmpty(values ...string) string {
	for _, v := range values {
		if strings.TrimSpace(v) != "" {
			return v
		}
	}
	return ""
}

func displayPath(p string) string {
	if strings.TrimSpace(p) == "" {
		return "(process working directory)"
	}
	return p
}

func onOff(v bool) string {
	if v {
		return "allowed (@-mention required)"
	}
	return "direct chats only"
}

func requiredIf(v bool) string {
	if v {
		return "required"
	}
	return "not required"
}
