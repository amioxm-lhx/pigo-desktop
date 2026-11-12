import assert from "node:assert/strict";
import test from "node:test";
import {
  buildPatch,
  channelBlockers,
  channelErrorKey,
  channelStatus,
  draftFromState,
  hasEmailEntry,
  missingFromAllowList,
  normalizeAllowedUsers,
  rejectedSenders,
} from "../src/lib/remote-channel.ts";

const draft = (overrides = {}) => ({
  channel: "feishu",
  enabled: true,
  appId: "cli_abc",
  allowedUsers: "ou_1",
  allowGroups: false,
  wechatAllowedUsers: "",
  wechatBaseUrl: "",
  sessionHost: "desktop",
  toolPolicy: "confirm",
  cwd: "",
  binaryPath: "",
  ...overrides,
});

const storedSettings = () => ({
  channel: "feishu",
  enabled: true,
  appId: "cli_abc",
  hasSecret: true,
  allowedUsers: ["ou_1", "ou_2"],
  allowGroups: false,
  wechatAllowedUsers: ["wxid_me"],
  wechatBaseUrl: "",
  sessionHost: "desktop",
  toolPolicy: "confirm",
  cwd: "",
  binaryPath: "",
});

const state = (overrides = {}) => ({
  settings: storedSettings(),
  running: false,
  pid: null,
  startedAt: null,
  exit: null,
  binary: null,
  binarySource: null,
  endpoint: true,
  configPath: "/tmp/im-channel.toml",
  wechat: { paired: false, account: "", botId: "", statePath: "/tmp/wechat.json" },
  login: { running: false, startedAt: null, qrUrl: null, qrPng: null, needsCode: false },
  logs: [],
  ...overrides,
});

test("allow list accepts the separators people actually paste", () => {
  // Copying from a doc, a sheet or a chat brings commas, full-width commas,
  // semicolons and newlines — all of them are entry boundaries.
  assert.deepEqual(normalizeAllowedUsers("ou_1, ou_2\nou_3；ou_4，ou_5"), [
    "ou_1",
    "ou_2",
    "ou_3",
    "ou_4",
    "ou_5",
  ]);
  assert.deepEqual(normalizeAllowedUsers("  ou_1  \n\n ou_1 \n"), ["ou_1"]);
  assert.deepEqual(normalizeAllowedUsers("   "), []);
});

test("an empty secret box keeps the stored one", () => {
  const patch = buildPatch(draft(), "");
  assert.equal("appSecret" in patch, false);
  assert.equal(buildPatch(draft(), "  ").appSecret, undefined);
});

test("a typed secret is trimmed and sent", () => {
  assert.equal(buildPatch(draft(), "  s3cret \n").appSecret, "s3cret");
});

test("patch never carries the hasSecret flag back to the shell", () => {
  // hasSecret is a read-only summary; sending it back would be ignored, but
  // depending on that is how a field silently drifts into the write path.
  assert.deepEqual(Object.keys(buildPatch(draft(), "")).sort(), [
    "allowGroups",
    "allowedUsers",
    "appId",
    "binaryPath",
    "channel",
    "cwd",
    "enabled",
    "sessionHost",
    "toolPolicy",
    "wechatAllowedUsers",
    "wechatBaseUrl",
  ]);
});

test("blockers are ordered the way the form is read", () => {
  const blockers = channelBlockers(draft({ appId: "", allowedUsers: "" }), false, false, false);
  assert.deepEqual(blockers, ["appId", "secret", "users", "binary", "endpoint"]);
});

test("the cli carrier does not require this app's control endpoint", () => {
  // session_host=cli runs its own store, so a closed desktop window must not
  // block the channel.
  assert.deepEqual(channelBlockers(draft({ sessionHost: "cli" }), true, true, false), []);
  assert.deepEqual(channelBlockers(draft({ sessionHost: "desktop" }), true, true, false), [
    "endpoint",
  ]);
});

test("a typed binary path counts as found before it is saved", () => {
  assert.deepEqual(
    channelBlockers(draft({ binaryPath: "C:/tools/pigo.exe" }), true, false, true),
    [],
  );
});

test("wechat blocks on pairing instead of an app id and secret", () => {
  // The bot token only exists after a scan, so an unpaired channel has no
  // credential field to complain about — showing App ID here would be a field
  // that does nothing.
  const wechat = draft({ channel: "wechat", appId: "", wechatAllowedUsers: "wxid_me" });
  assert.deepEqual(channelBlockers(wechat, false, true, true, false), ["paired"]);
  assert.deepEqual(channelBlockers(wechat, false, true, true, true), []);
});

test("wechat still needs someone allowed to talk to it", () => {
  const wechat = draft({ channel: "wechat", wechatAllowedUsers: "  " });
  assert.deepEqual(channelBlockers(wechat, false, true, true, true), ["users"]);
  // The Feishu list must not satisfy the WeChat requirement.
  assert.deepEqual(
    channelBlockers(draft({ channel: "wechat", allowedUsers: "ou_1" }), true, true, true, true),
    ["users"],
  );
});

test("the feishu allow list does not leak into wechat's blockers", () => {
  // Switching tabs leaves both sets of fields in the draft; only the selected
  // channel's own list may count.
  const wechat = draft({ channel: "wechat", allowedUsers: "", wechatAllowedUsers: "wxid_me" });
  assert.deepEqual(channelBlockers(wechat, true, true, true, true), []);
});

test("an exit with a non-zero code reads as crashed, not stopped", () => {
  assert.equal(channelStatus(state({ running: true })), "running");
  assert.equal(channelStatus(state()), "stopped");
  assert.equal(channelStatus(state({ exit: { code: 2, at: "10:00:00" } })), "crashed");
  // Killed by the stop button: no code, so it is a clean stop.
  assert.equal(channelStatus(state({ exit: { code: null, at: "10:00:00" } })), "stopped");
});

test("shell error codes map to their own sentence", () => {
  assert.equal(channelErrorKey("PIGO_NOT_FOUND"), "binaryMissing");
  assert.equal(channelErrorKey("ENDPOINT_UNAVAILABLE"), "endpointMissing");
  assert.equal(channelErrorKey("INVALID_ARGUMENT"), "invalid");
  // WeChat pairing: starting unpaired, and scanning while the channel runs.
  assert.equal(channelErrorKey("WECHAT_NOT_PAIRED"), "notPaired");
  assert.equal(channelErrorKey("CHANNEL_RUNNING"), "channelRunning");
  assert.equal(channelErrorKey(undefined), "failed");
  assert.equal(channelErrorKey("SOMETHING_NEW"), "failed");
});

test("draft turns the stored list into one-per-line text and keeps the rest", () => {
  const next = draftFromState(state());
  assert.equal(next.allowedUsers, "ou_1\nou_2");
  assert.equal(next.sessionHost, "desktop");
  assert.equal(next.toolPolicy, "confirm");
  assert.equal(next.appId, "cli_abc");
  assert.equal(next.channel, "feishu");
});

test("draft carries the wechat fields so a tab switch does not drop them", () => {
  const next = draftFromState(
    state({
      settings: {
        ...storedSettings(),
        channel: "wechat",
        wechatAllowedUsers: ["wxid_a", "wxid_b"],
        wechatBaseUrl: "https://ilinkai.weixin.qq.com",
      },
    }),
  );
  assert.equal(next.channel, "wechat");
  assert.equal(next.wechatAllowedUsers, "wxid_a\nwxid_b");
  assert.equal(next.wechatBaseUrl, "https://ilinkai.weixin.qq.com");
});

test("a wechat patch carries the channel and only its own credential fields", () => {
  const patch = buildPatch(
    draft({
      channel: "wechat",
      appId: "  ",
      wechatAllowedUsers: "wxid_a wxid_b",
      wechatBaseUrl: "  https://example.test  ",
    }),
    "",
  );
  assert.equal(patch.channel, "wechat");
  assert.deepEqual(patch.wechatAllowedUsers, ["wxid_a", "wxid_b"]);
  assert.equal(patch.wechatBaseUrl, "https://example.test");
  // Empty secret box still means "keep the stored one" on the WeChat tab.
  assert.equal("appSecret" in patch, false);
});

test("a refused sender is lifted out of the real log line", () => {
  // Verbatim from the channel's own output, timestamp prefix included.
  const lines = [
    '[14:14:45] [im] im: dropped unauthorized sender="ou_60707b5df6bb16e7451bb64a935120c0" chat="oc_f68278474cbd41701c96ebd044b995dc" adapter=feishu (add the sender id to allowed_users to authorize it)',
  ];
  assert.deepEqual(rejectedSenders(lines), ["ou_60707b5df6bb16e7451bb64a935120c0"]);
});

test("opening the bot's chat reveals the id before any message is sent", () => {
  const lines = [
    "[14:12:01] [im] feishu: p2p chat opened chat=oc_f682 chat=oc_f682 open_id=ou_abc123 — add this open_id to allowed_users to authorize it",
  ];
  assert.deepEqual(rejectedSenders(lines), ["ou_abc123"]);
});

test("wechat prints an ilink user id instead of an open_id", () => {
  // Verbatim from the adapter: there is no open_id on this platform, and the
  // em dash after the id must not be swallowed into it.
  const lines = [
    "[09:31:07] [im] wechat: chat opened with user=wxid_9x8y7z — add this id to allowed_users to authorize it",
  ];
  assert.deepEqual(rejectedSenders(lines), ["wxid_9x8y7z"]);
  // Both platforms can appear in one log after a tab switch, and the router's
  // own refusal line is shared by the two adapters.
  assert.deepEqual(
    rejectedSenders([
      '[1] [im] im: dropped unauthorized sender="wxid_other" chat="wxid_other" adapter=wechat',
      "[2] [im] wechat: chat opened with user=wxid_9x8y7z — add this id",
    ]),
    ["wxid_other", "wxid_9x8y7z"],
  );
});

test("rejected senders are deduped and unrelated lines ignored", () => {
  const lines = [
    '[1] [im] im: dropped unauthorized sender="ou_a" chat="oc_1" adapter=feishu',
    '[2] [im] im: dropped unauthorized sender="ou_a" chat="oc_1" adapter=feishu',
    '[3] [im] feishu: connecting (app_id=cli_x)',
    '[4] [im] feishu sdk error: handle message failed',
    '[5] [im] im: dropped unauthorized sender="ou_b" chat="oc_2" adapter=feishu',
  ];
  assert.deepEqual(rejectedSenders(lines), ["ou_a", "ou_b"]);
  // An empty sender (an event without an operator) must not become a row.
  assert.deepEqual(rejectedSenders(['[1] im: dropped unauthorized sender="" chat="oc"']), []);
});

test("ids already on the allow list do not keep asking to be added", () => {
  const refused = ["ou_a", "ou_b"];
  assert.deepEqual(missingFromAllowList(refused, ["ou_a"]), ["ou_b"]);
  // Case-insensitive: Feishu ids are lowercase, pasted ones may not be.
  assert.deepEqual(missingFromAllowList(["ou_a"], ["OU_A"]), []);
  assert.deepEqual(missingFromAllowList([], ["ou_a"]), []);
});

test("email entries are flagged because they need a translation step", () => {
  assert.equal(hasEmailEntry("lhxcc@163.com"), true);
  assert.equal(hasEmailEntry("ou_1\nx@y.com"), true);
  assert.equal(hasEmailEntry("ou_1 ou_2"), false);
  assert.equal(hasEmailEntry(""), false);
});
