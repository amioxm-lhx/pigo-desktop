/**
 * Pure helpers for the remote-control channel page.
 *
 * Kept out of the component so the rules the page enforces — which fields a
 * start needs, how the allow list is normalised, what "running" means when the
 * child died — are testable without mounting anything.
 */

import type {
  RemoteChannelHost,
  RemoteChannelId,
  RemoteChannelState,
  RemoteChannelToolPolicy,
  RemoteChannelPatch,
} from "@pi-desktop/shared";

/** The page's editable copy: the allow list is a textarea here, a list on disk. */
export type ChannelDraft = {
  channel: RemoteChannelId;
  enabled: boolean;
  appId: string;
  allowedUsers: string;
  allowGroups: boolean;
  wechatAllowedUsers: string;
  wechatBaseUrl: string;
  sessionHost: RemoteChannelHost;
  toolPolicy: RemoteChannelToolPolicy;
  cwd: string;
  binaryPath: string;
};

/** Why a start would fail, in the order the page should show them. */
export type ChannelBlocker =
  | "appId"
  | "secret"
  | "users"
  | "paired"
  | "binary"
  | "endpoint";

export type ChannelStatus = "running" | "stopped" | "crashed";

/**
 * Split the allow list. Commas, semicolons and newlines all separate entries —
 * people paste open_ids from a doc, a sheet or a chat, and each of those brings
 * its own separator.
 */
export function normalizeAllowedUsers(input: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of input.split(/[\s,;，；、]+/)) {
    const value = raw.trim();
    if (!value || seen.has(value)) continue;
    seen.add(value);
    out.push(value);
  }
  return out;
}

export function formatAllowedUsers(users: readonly string[]): string {
  return users.join("\n");
}

export function draftFromState(state: RemoteChannelState): ChannelDraft {
  const settings = state.settings;
  return {
    channel: settings.channel,
    enabled: settings.enabled,
    appId: settings.appId,
    allowedUsers: formatAllowedUsers(settings.allowedUsers),
    allowGroups: settings.allowGroups,
    wechatAllowedUsers: formatAllowedUsers(settings.wechatAllowedUsers),
    wechatBaseUrl: settings.wechatBaseUrl,
    sessionHost: settings.sessionHost,
    toolPolicy: settings.toolPolicy,
    cwd: settings.cwd,
    binaryPath: settings.binaryPath,
  };
}

/**
 * What still has to be true before the channel can start. Every item here is a
 * hard failure at start time in the shell too — the page checks first so the
 * user sees the reason next to the field instead of in a toast.
 *
 * The two channels need genuinely different things, so the list is built per
 * channel rather than unioned: showing "App Secret" on the WeChat tab would be a
 * field that does nothing.
 *
 * `binaryFound` is what the shell resolved from the saved settings; a path the
 * user has typed but not saved yet counts immediately, so the hint does not
 * linger while the form is still dirty.
 */
export function channelBlockers(
  draft: ChannelDraft,
  hasSecret: boolean,
  binaryFound: boolean,
  endpointAlive: boolean,
  wechatPaired = false,
): ChannelBlocker[] {
  const blockers: ChannelBlocker[] = [];
  if (draft.channel === "wechat") {
    // The token only exists after a scan, so "paired" replaces app id + secret.
    if (!wechatPaired) blockers.push("paired");
    if (normalizeAllowedUsers(draft.wechatAllowedUsers).length === 0) blockers.push("users");
  } else {
    if (draft.appId.trim() === "") blockers.push("appId");
    if (!hasSecret) blockers.push("secret");
    if (normalizeAllowedUsers(draft.allowedUsers).length === 0) blockers.push("users");
  }
  if (!binaryFound && draft.binaryPath.trim() === "") blockers.push("binary");
  // Only the desktop carrier needs this app; the cli carrier is self-contained.
  if (draft.sessionHost === "desktop" && !endpointAlive) blockers.push("endpoint");
  return blockers;
}

/**
 * The write-back body. An empty secret box means "keep what is stored": the
 * page never receives the secret, so it cannot echo it back.
 */
export function buildPatch(draft: ChannelDraft, secret: string): RemoteChannelPatch {
  const trimmed = secret.trim();
  return {
    channel: draft.channel,
    enabled: draft.enabled,
    appId: draft.appId.trim(),
    allowedUsers: normalizeAllowedUsers(draft.allowedUsers),
    allowGroups: draft.allowGroups,
    wechatAllowedUsers: normalizeAllowedUsers(draft.wechatAllowedUsers),
    wechatBaseUrl: draft.wechatBaseUrl.trim(),
    sessionHost: draft.sessionHost,
    toolPolicy: draft.toolPolicy,
    cwd: draft.cwd.trim(),
    binaryPath: draft.binaryPath.trim(),
    ...(trimmed ? { appSecret: trimmed } : {}),
  };
}

/**
 * Running is a fact; "crashed" is an interpretation the user needs, because a
 * channel that died on a rejected app_id looks identical to one that was never
 * started unless the exit code is surfaced.
 */
export function channelStatus(state: RemoteChannelState): ChannelStatus {
  if (state.running) return "running";
  if (state.exit && state.exit.code !== null && state.exit.code !== 0) return "crashed";
  return "stopped";
}

/** i18n key suffix for a shell error code, defaulting to a generic sentence. */
export function channelErrorKey(code: string | undefined): string {
  switch (code) {
    case "PIGO_NOT_FOUND":
      return "binaryMissing";
    case "ENDPOINT_UNAVAILABLE":
      return "endpointMissing";
    case "INVALID_ARGUMENT":
      return "invalid";
    // WeChat pairing. A scan cannot start while the channel runs (a new login
    // invalidates the token it is using), and a start cannot happen unpaired.
    case "WECHAT_NOT_PAIRED":
      return "notPaired";
    case "CHANNEL_RUNNING":
      return "channelRunning";
    default:
      return "failed";
  }
}

/**
 * Sender ids the channel refused, mined from its own log.
 *
 * The router answers strangers with silence — a reply would confirm to them
 * that the bot is live — so the log line is the only place a real person who is
 * not yet on the allow list becomes visible. Turning that line into a button is
 * the difference between "it works once you paste an id somewhere" and an
 * invitation the user can act on without knowing what an open_id is.
 *
 * Two sources feed it: the router's refusal, and the chat-access push the
 * channel logs when someone opens a p2p chat with the bot.
 */
export function rejectedSenders(lines: readonly string[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  const push = (id: string) => {
    if (!id || seen.has(id)) return;
    seen.add(id);
    out.push(id);
  };
  for (const line of lines) {
    const dropped = /dropped unauthorized sender="([^"]*)"/.exec(line);
    if (dropped) {
      push(dropped[1]);
      continue;
    }
    const opened = /p2p chat opened .*?open_id=(ou_[A-Za-z0-9]+)/.exec(line);
    if (opened) {
      push(opened[1]);
      continue;
    }
    // WeChat has no open_id: the peer is an ilink user id, printed by the
    // adapter the first time that person speaks.
    const wechat = /wechat: chat opened with user=([^\s—]+)/.exec(line);
    if (wechat) push(wechat[1]);
  }
  return out;
}

/**
 * Which of `candidates` are not on the allow list yet. Comparison ignores case
 * because Feishu ids are lowercase and people paste whatever they copied.
 */
export function missingFromAllowList(candidates: readonly string[], allowList: readonly string[]): string[] {
  const have = new Set(allowList.map((entry) => entry.trim().toLowerCase()));
  return candidates.filter((id) => id !== "" && !have.has(id.toLowerCase()));
}

/** Whether any allow-list entry is an email, which needs a translation step. */
export function hasEmailEntry(input: string): boolean {
  return normalizeAllowedUsers(input).some((entry) => entry.includes("@"));
}

/** How many log lines the collapsed panel shows before scrolling. */
export const LOG_TAIL = 40;
