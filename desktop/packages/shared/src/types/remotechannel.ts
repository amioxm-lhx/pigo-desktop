/**
 * The remote-control channel (Feishu today) as the desktop shell sees it.
 *
 * The channel runs as a separate `pigo im serve` process; this app only stores
 * its settings, supervises the process and shows what it printed. Everything
 * here is therefore about configuration and liveness, not about messages —
 * those never pass through the renderer.
 */

/** Which session store the channel drives. */
export type RemoteChannelHost = "desktop" | "cli";

/**
 * Which IM platform this page configures.
 *
 * The two differ in more than credentials: Feishu is configured by typing an app
 * id and secret, WeChat is configured by scanning a QR code once. The page shows
 * one form at a time and keeps both sets of fields.
 */
export type RemoteChannelId = "feishu" | "wechat";

/**
 * What an IM message is allowed to do to this machine. `allow` means a phone
 * message can run a shell command unasked, so the page says so out loud.
 */
export type RemoteChannelToolPolicy = "confirm" | "allow" | "deny";

/** Where the resolved `pigo` binary came from. */
export type RemoteChannelBinarySource = "settings" | "app" | "path";

export type RemoteChannelSettings = {
  /** Which channel the form below configures. */
  channel: RemoteChannelId;
  /** Master switch, stored so a restart re-opens the channel (not auto-started). */
  enabled: boolean;
  appId: string;
  /** The secret itself never reaches the renderer — only whether it is stored. */
  hasSecret: boolean;
  /** open_id or email of the humans allowed to control this machine. */
  allowedUsers: string[];
  allowGroups: boolean;
  /** WeChat sender ids allowed to control this machine. */
  wechatAllowedUsers: string[];
  /** WeChat API root override; empty uses the host remembered at login. */
  wechatBaseUrl: string;
  sessionHost: RemoteChannelHost;
  toolPolicy: RemoteChannelToolPolicy;
  cwd: string;
  binaryPath: string;
};

/** Settings as written back. `appSecret` empty means "leave the stored one". */
export type RemoteChannelPatch = Omit<RemoteChannelSettings, "hasSecret"> & {
  appSecret?: string;
};

export type RemoteChannelExit = {
  /** Null when the process was killed or its status was unavailable. */
  code: number | null;
  /** Local clock label of the last exit. */
  at: string;
};

/**
 * WeChat pairing state. The bot token is not a settings field — it only exists as
 * the result of a scan — so "configured" means this, not a filled-in box.
 */
export type RemoteChannelWechat = {
  paired: boolean;
  /** The WeChat account that completed the scan (its ilink user id). */
  account: string;
  botId: string;
  /** Where the token, the message cursor and the per-peer tokens live. */
  statePath: string;
};

/**
 * A QR pairing in progress. `qrPng` is a data URL because the shell reads the
 * image the Go login wrote: a terminal block-art QR inside a webview is a gamble
 * on font metrics, an image is not.
 */
export type RemoteChannelLogin = {
  running: boolean;
  startedAt: string | null;
  qrUrl: string | null;
  qrPng: string | null;
  /** True once the child has printed the "enter the digits" prompt. */
  needsCode: boolean;
};

export type RemoteChannelState = {
  settings: RemoteChannelSettings;
  running: boolean;
  pid: number | null;
  startedAt: string | null;
  exit: RemoteChannelExit | null;
  /** Absolute path of the binary that would run, or null when none was found. */
  binary: string | null;
  binarySource: RemoteChannelBinarySource | null;
  /** Whether this app's control endpoint is listening right now. */
  endpoint: boolean;
  /** Generated `config.toml` handed to the child on each start. */
  configPath: string;
  wechat: RemoteChannelWechat;
  login: RemoteChannelLogin;
  /** Tail of the child's stdout+stderr, oldest first. */
  logs: string[];
};

export type RemoteChannelPickResult = {
  canceled: boolean;
  path?: string;
};
