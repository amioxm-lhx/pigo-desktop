import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { RemoteChannelId, RemoteChannelState } from "@pi-desktop/shared";
import { api } from "../../lib/api";
import { useAppStore } from "../../stores/app-store";
import {
  buildPatch,
  channelBlockers,
  channelErrorKey,
  channelStatus,
  draftFromState,
  formatAllowedUsers,
  hasEmailEntry,
  LOG_TAIL,
  missingFromAllowList,
  normalizeAllowedUsers,
  rejectedSenders,
  type ChannelBlocker,
  type ChannelDraft,
} from "../../lib/remote-channel";
import { Button, Input, Select, Textarea, cx } from "../../components/ui";
import { SettingsCard, SettingsRow } from "../../features/settings/primitives";
import { CapabilityToggle } from "./AgentCapabilityLayout";

/** How often the page re-reads the shell's view while it is open. */
const POLL_MS = 2500;

/** A working order for the setup steps; the numbers are the point. */
const FEISHU_STEPS = ["app", "events", "scopes", "openId"] as const;
const WECHAT_STEPS = ["scan", "allow", "start"] as const;

/**
 * Remote-control channels: Feishu and WeChat, which the shell runs as
 * `pigo im serve` and uses to drive this app's own sessions.
 *
 * The page owns no protocol. It edits settings, asks the shell to start or stop
 * the child, and shows the tail of what that child printed — which is where
 * every real diagnosis happens (a rejected app_id, a missing scope, an expired
 * token all show up there and nowhere else).
 *
 * The two channels are configured in genuinely different ways, so the form is
 * per channel rather than shared: Feishu takes an app id and a secret typed by
 * hand, WeChat takes one QR scan and then has no credential field at all. The
 * scan is run as a child of its own — that is the only way the bot token comes
 * into existence.
 */
export function RemoteChannelsPage() {
  const { t } = useTranslation();
  const showToast = useAppStore((state) => state.showToast);
  const [state, setState] = useState<RemoteChannelState | null>(null);
  const [draft, setDraft] = useState<ChannelDraft | null>(null);
  const [secret, setSecret] = useState("");
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState<
    null | "save" | "start" | "stop" | "secret" | "pick" | "login" | "code" | "unpair"
  >(null);
  // Mirrors `dirtyRef` for rendering: a ref alone would not re-enable the
  // Save button when the first keystroke lands.
  const [dirty, setDirty] = useState(false);
  const [error, setError] = useState<{ code?: string; message: string } | null>(null);
  const logRef = useRef<HTMLPreElement | null>(null);
  const codeRef = useRef<HTMLInputElement | null>(null);
  // Polling must never overwrite what the user is typing: only a clean draft
  // follows the shell's copy.
  const dirtyRef = useRef(false);

  const apply = useCallback((next: RemoteChannelState, force = false) => {
    setState(next);
    if (force || !dirtyRef.current) {
      setDraft(draftFromState(next));
      dirtyRef.current = false;
      setDirty(false);
    }
  }, []);

  const fail = useCallback((cause: unknown) => {
    const error = cause as Error & { code?: string };
    setError({ code: error.code, message: error.message });
  }, []);

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      try {
        const next = await api.getRemoteChannel();
        if (!cancelled) apply(next);
      } catch (cause) {
        if (!cancelled) fail(cause);
      }
    };
    void load();
    const timer = window.setInterval(() => void load(), POLL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [apply, fail]);

  // Newest line last is the useful direction; keep the view pinned there so a
  // retry storm is visible without scrolling.
  useEffect(() => {
    const node = logRef.current;
    if (node) node.scrollTop = node.scrollHeight;
  }, [state?.logs.length]);

  const loginRunning = state?.login.running ?? false;
  const needsCode = state?.login.needsCode ?? false;

  // A finished pairing takes the digits box with it; the next scan starts blank.
  useEffect(() => {
    if (!loginRunning) setCode("");
  }, [loginRunning]);

  // The prompt only appears after a scan, so it has to take focus by itself.
  useEffect(() => {
    if (needsCode) codeRef.current?.focus();
  }, [needsCode]);

  const update = <K extends keyof ChannelDraft>(key: K, value: ChannelDraft[K]) => {
    dirtyRef.current = true;
    setDirty(true);
    setDraft((current) => (current ? { ...current, [key]: value } : current));
  };

  const saveOnly = async () => {
    if (!draft || busy) return;
    setBusy("save");
    setError(null);
    try {
      const next = await api.saveRemoteChannel(buildPatch(draft, secret));
      apply(next, true);
      setSecret("");
      showToast(t("settings.remoteChannel.saved"), { variant: "success" });
    } catch (cause) {
      fail(cause);
    } finally {
      setBusy(null);
    }
  };

  const stopChannel = async () => {
    if (busy) return;
    setBusy("stop");
    setError(null);
    try {
      apply(await api.stopRemoteChannel());
      showToast(t("settings.remoteChannel.stopped"), { variant: "success" });
    } catch (cause) {
      fail(cause);
    } finally {
      setBusy(null);
    }
  };

  const clearSecret = async () => {
    if (busy) return;
    setBusy("secret");
    setError(null);
    try {
      apply(await api.clearRemoteChannelSecret(), true);
      setSecret("");
      showToast(t("settings.remoteChannel.appSecretCleared"), { variant: "success" });
    } catch (cause) {
      fail(cause);
    } finally {
      setBusy(null);
    }
  };

  const saveAndStart = async () => {
    if (!draft || busy) return;
    setBusy("start");
    setError(null);
    try {
      // Persist first: start reads the stored settings, so an unsaved edit
      // would silently start the previous configuration.
      const saved = await api.saveRemoteChannel(buildPatch(draft, secret));
      apply(saved, true);
      setSecret("");
      apply(await api.startRemoteChannel());
      showToast(t("settings.remoteChannel.started"), { variant: "success" });
    } catch (cause) {
      fail(cause);
    } finally {
      setBusy(null);
    }
  };

  const pickBinary = async () => {
    if (busy || !draft) return;
    setBusy("pick");
    try {
      const picked = await api.pickRemoteChannelBinary();
      if (!picked.canceled && picked.path) {
        update("binaryPath", picked.path);
      }
    } catch (cause) {
      fail(cause);
    } finally {
      setBusy(null);
    }
  };

  // Pairing does not touch the settings form, so it must not force a draft
  // reset — the user may well have unsaved edits in the card below.
  const startLogin = async () => {
    if (busy) return;
    setBusy("login");
    setError(null);
    try {
      apply(await api.loginRemoteChannel());
    } catch (cause) {
      fail(cause);
    } finally {
      setBusy(null);
    }
  };

  const submitCode = async () => {
    if (busy || code.trim() === "") return;
    setBusy("code");
    setError(null);
    try {
      apply(await api.submitRemoteChannelLoginCode(code.trim()));
      setCode("");
    } catch (cause) {
      fail(cause);
    } finally {
      setBusy(null);
    }
  };

  const cancelLogin = async () => {
    if (busy) return;
    setBusy("unpair");
    setError(null);
    try {
      apply(await api.cancelRemoteChannelLogin());
    } catch (cause) {
      fail(cause);
    } finally {
      setBusy(null);
    }
  };

  // Append to the draft, not to the saved settings: the ids in the log are a
  // suggestion, and the user still gets to look at what they are authorizing
  // before saving and restarting.
  const addAllowedUser = (id: string) => {
    if (!draft) return;
    const key = draft.channel === "wechat" ? "wechatAllowedUsers" : "allowedUsers";
    const users = normalizeAllowedUsers(draft[key]);
    if (users.some((entry) => entry.toLowerCase() === id.toLowerCase())) return;
    update(key, formatAllowedUsers([...users, id]));
  };

  if (!state || !draft) {
    return (
      <div className="im-channel-empty" role="status" aria-live="polite">
        {t("common.loading")}
      </div>
    );
  }

  const channel: RemoteChannelId = draft.channel;
  const isWechat = channel === "wechat";
  const status = channelStatus(state);
  const blockers = channelBlockers(
    draft,
    state.settings.hasSecret,
    Boolean(state.binary),
    state.endpoint,
    state.wechat.paired,
  );
  const binaryMissing = blockers.includes("binary");
  const lines = state.logs.slice(-LOG_TAIL);
  // Senders the channel refused. Only the ones still outside the draft matter:
  // once added, the banner has served its purpose and should get out of the way
  // while the user saves and restarts.
  const allowDraft = isWechat ? draft.wechatAllowedUsers : draft.allowedUsers;
  const refused = missingFromAllowList(
    rejectedSenders(state.logs),
    normalizeAllowedUsers(allowDraft),
  );
  const settingsTitle = isWechat
    ? t("settings.remoteChannel.wechatTitle")
    : t("settings.remoteChannel.feishuTitle");
  const setupTitle = isWechat
    ? t("settings.remoteChannel.setupTitleWechat")
    : t("settings.remoteChannel.setupTitle");
  const setupSteps = isWechat ? WECHAT_STEPS : FEISHU_STEPS;
  const setupPrefix = isWechat ? "settings.remoteChannel.setupWechat" : "settings.remoteChannel.setup";

  const blockerCopy = (blocker: ChannelBlocker): string => {
    switch (blocker) {
      case "appId":
        return t("settings.remoteChannel.blocker.appId");
      case "secret":
        return t("settings.remoteChannel.blocker.secret");
      case "users":
        return t("settings.remoteChannel.blocker.users");
      case "paired":
        return t("settings.remoteChannel.blocker.paired");
      case "binary":
        return t("settings.remoteChannel.blocker.binary");
      case "endpoint":
        return t("settings.remoteChannel.blocker.endpoint");
    }
  };

  const sourceLabel = () => {
    switch (state.binarySource) {
      case "settings":
        return t("settings.remoteChannel.sourceSettings");
      case "app":
        return t("settings.remoteChannel.sourceApp");
      case "path":
        return t("settings.remoteChannel.sourcePath");
      default:
        return t("settings.remoteChannel.sourceNone");
    }
  };

  return (
    <div className="im-channel">
      <p className="im-channel-note im-channel-intro">{t("settings.remoteChannel.description")}</p>

      {/* One form at a time: the two channels share the status, the log and the
          session settings, but nothing about how they authenticate. */}
      <div
        className="settings-segment im-channel-tabs"
        role="radiogroup"
        aria-label={t("settings.remoteChannel.title")}
      >
        {(["feishu", "wechat"] as const).map((id) => (
          <button
            key={id}
            type="button"
            role="radio"
            aria-checked={channel === id}
            className={cx("settings-segment-item", channel === id && "active")}
            onClick={() => update("channel", id)}
          >
            {id === "wechat"
              ? t("settings.remoteChannel.channelWechat")
              : t("settings.remoteChannel.channelFeishu")}
          </button>
        ))}
      </div>

      <SettingsCard title={t("settings.remoteChannel.statusTitle")}>
        <div className="im-channel-status">
          <span
            className={cx(
              "agent-capability-badge is-status",
              status === "running" && "is-ready",
              status === "crashed" && "is-failed",
              status === "stopped" && "is-connecting",
            )}
          >
            <span className="agent-capability-status-dot" aria-hidden="true" />
            {t(`settings.remoteChannel.state.${status}`)}
          </span>
          {state.pid ? (
            <span className="im-channel-meta">
              {t("settings.remoteChannel.pid", { pid: state.pid })}
            </span>
          ) : null}
          {state.startedAt ? (
            <span className="im-channel-meta">
              {t("settings.remoteChannel.since", { time: state.startedAt })}
            </span>
          ) : null}
          <span className="im-channel-actions">
            {state.running ? (
              <Button
                variant="primary"
                disabled={busy !== null}
                onClick={() => void stopChannel()}
              >
                {t("settings.remoteChannel.stop")}
              </Button>
            ) : (
              <Button
                variant="primary"
                disabled={busy !== null}
                onClick={() => void saveAndStart()}
              >
                {busy === "start"
                  ? t("settings.remoteChannel.starting")
                  : t("settings.remoteChannel.start")}
              </Button>
            )}
            <Button
              disabled={busy !== null || !dirty}
              onClick={() => void saveOnly()}
            >
              {t("common.save")}
            </Button>
          </span>
        </div>

        <SettingsRow
          title={t("settings.remoteChannel.endpointLabel")}
          description={t("settings.remoteChannel.endpointHint")}
        >
          <span
            className={cx(
              "agent-capability-badge is-status",
              state.endpoint ? "is-ready" : "is-failed",
            )}
          >
            <span className="agent-capability-status-dot" aria-hidden="true" />
            {state.endpoint
              ? t("settings.remoteChannel.endpointReady")
              : t("settings.remoteChannel.endpointDown")}
          </span>
        </SettingsRow>

        <SettingsRow
          title={t("settings.remoteChannel.binaryLabel")}
          description={
            binaryMissing ? (
              <span className="im-channel-warn">{t("settings.remoteChannel.binaryMissing")}</span>
            ) : (
              t("settings.remoteChannel.binaryHint", { source: sourceLabel() })
            )
          }
        >
          <div className="im-channel-path">
            <code className="im-channel-code">{state.binary ?? "—"}</code>
            <Button disabled={busy !== null} onClick={() => void pickBinary()}>
              {t("settings.remoteChannel.chooseBinary")}
            </Button>
          </div>
        </SettingsRow>

        {state.exit ? (
          <SettingsRow
            title={t("settings.remoteChannel.lastExitLabel")}
            description={t("settings.remoteChannel.lastExitHint")}
          >
            <span className="im-channel-meta">
              {state.exit.code === null
                ? t("settings.remoteChannel.lastExitKilled", { time: state.exit.at })
                : t("settings.remoteChannel.lastExitCode", {
                    code: state.exit.code,
                    time: state.exit.at,
                  })}
            </span>
          </SettingsRow>
        ) : null}

        {error ? (
          <div className="im-channel-error" role="alert">
            <strong>{t(`settings.remoteChannel.error.${channelErrorKey(error.code)}`)}</strong>
            <span>{error.message}</span>
          </div>
        ) : null}

        {blockers.length > 0 ? (
          <ul className="im-channel-blockers">
            {blockers.map((blocker) => (
              <li key={blocker}>{blockerCopy(blocker)}</li>
            ))}
          </ul>
        ) : null}
      </SettingsCard>

      {refused.length > 0 ? (
        <div className="im-channel-invite" role="status">
          <div className="im-channel-invite-text">
            <strong>{t("settings.remoteChannel.rejectedTitle")}</strong>
            <span>{t("settings.remoteChannel.rejectedHint")}</span>
          </div>
          <ul className="im-channel-invite-list">
            {refused.map((id) => (
              <li key={id}>
                <code className="im-channel-code">{id}</code>
                <Button disabled={busy !== null} onClick={() => addAllowedUser(id)}>
                  {t("settings.remoteChannel.rejectedAdd")}
                </Button>
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {isWechat ? (
        <SettingsCard title={t("settings.remoteChannel.wechatPairTitle")}>
          <SettingsRow
            title={t("settings.remoteChannel.wechatTitle")}
            description={
              state.wechat.paired
                ? t("settings.remoteChannel.wechatAccount", {
                    id: state.wechat.account || state.wechat.botId,
                  })
                : t("settings.remoteChannel.wechatPairHint")
            }
          >
            <div className="im-channel-pair">
              <span
                className={cx(
                  "agent-capability-badge is-status",
                  state.wechat.paired ? "is-ready" : "is-failed",
                )}
              >
                <span className="agent-capability-status-dot" aria-hidden="true" />
                {state.wechat.paired
                  ? t("settings.remoteChannel.wechatPaired")
                  : t("settings.remoteChannel.wechatNotPaired")}
              </span>
              <Button
                disabled={busy !== null || loginRunning || state.running}
                onClick={() => void startLogin()}
              >
                {loginRunning
                  ? t("settings.remoteChannel.wechatLoggingIn")
                  : t("settings.remoteChannel.wechatLogin")}
              </Button>
            </div>
          </SettingsRow>

          {/* The QR is the credential. It arrives as a data URL from the shell
              because the Go side renders it as a PNG — block-art in a webview
              depends on font metrics this app does not control. */}
          {loginRunning ? (
            <div className="im-channel-qr">
              {state.login.qrPng ? (
                <img
                  className="im-channel-qr-image"
                  src={state.login.qrPng}
                  alt={t("settings.remoteChannel.wechatQrAlt")}
                  // The Go side draws 360px; half of that is an integer number
                  // of device pixels per module, which keeps the edges crisp
                  // instead of resampling them into grey.
                  width={180}
                  height={180}
                />
              ) : null}
              <p className="im-channel-note im-channel-qr-copy">
                {state.login.qrPng
                  ? t("settings.remoteChannel.wechatQrHint")
                  : t("settings.remoteChannel.wechatLoggingIn")}
              </p>
              {state.login.qrUrl ? (
                <p className="im-channel-note im-channel-qr-link">
                  {t("settings.remoteChannel.wechatQrLink", { url: state.login.qrUrl })}
                </p>
              ) : null}
              {/* Some scans come back needing the digits the phone shows; the
                  shell pipes the answer to the child's stdin. */}
              {needsCode ? (
                <div className="im-channel-qr-code">
                  <Input
                    ref={codeRef}
                    value={code}
                    placeholder={t("settings.remoteChannel.wechatCodePlaceholder")}
                    onChange={(event) => setCode(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") void submitCode();
                    }}
                  />
                  <Button
                    variant="primary"
                    disabled={busy !== null || code.trim() === ""}
                    onClick={() => void submitCode()}
                  >
                    {t("settings.remoteChannel.wechatCodeSubmit")}
                  </Button>
                </div>
              ) : null}
              <div className="im-channel-qr-actions">
                <Button disabled={busy !== null} onClick={() => void cancelLogin()}>
                  {t("settings.remoteChannel.wechatCancel")}
                </Button>
              </div>
            </div>
          ) : null}

          <p className="im-channel-note im-channel-config-path">
            {t("settings.remoteChannel.wechatStatePath", { path: state.wechat.statePath })}
          </p>
        </SettingsCard>
      ) : null}

      <SettingsCard title={settingsTitle}>
        <SettingsRow
          title={t("settings.remoteChannel.autoStart")}
          description={t("settings.remoteChannel.autoStartHint")}
        >
          <CapabilityToggle
            checked={draft.enabled}
            busy={busy !== null}
            label={t("settings.remoteChannel.autoStart")}
            onChange={() => update("enabled", !draft.enabled)}
          />
        </SettingsRow>

        {isWechat ? (
          <>
            <SettingsRow
              title={t("settings.remoteChannel.wechatAllowedUsers")}
              description={t("settings.remoteChannel.wechatAllowedUsersHint")}
            >
              <Textarea
                rows={3}
                value={draft.wechatAllowedUsers}
                placeholder="wxid_xxxxxxxxxxxx"
                onChange={(event) => update("wechatAllowedUsers", event.target.value)}
              />
            </SettingsRow>

            <SettingsRow
              title={t("settings.remoteChannel.wechatBaseUrl")}
              description={t("settings.remoteChannel.wechatBaseUrlHint")}
            >
              <Input
                value={draft.wechatBaseUrl}
                placeholder="https://ilinkai.weixin.qq.com"
                onChange={(event) => update("wechatBaseUrl", event.target.value)}
              />
            </SettingsRow>
          </>
        ) : (
          <>
            <SettingsRow title={t("settings.remoteChannel.appId")} description={t("settings.remoteChannel.appIdHint")}>
              <Input
                value={draft.appId}
                placeholder="cli_xxxxxxxxxxxx"
                onChange={(event) => update("appId", event.target.value)}
              />
            </SettingsRow>

            <SettingsRow
              title={t("settings.remoteChannel.appSecret")}
              description={
                state.settings.hasSecret
                  ? t("settings.remoteChannel.appSecretStored")
                  : t("settings.remoteChannel.appSecretHint")
              }
            >
              <div className="im-channel-secret">
                <Input
                  type="password"
                  value={secret}
                  placeholder={state.settings.hasSecret ? "••••••••" : t("settings.remoteChannel.appSecretPlaceholder")}
                  onChange={(event) => setSecret(event.target.value)}
                />
                {state.settings.hasSecret ? (
                  <Button
                    disabled={busy !== null}
                    onClick={() => void clearSecret()}
                  >
                    {t("settings.remoteChannel.appSecretClear")}
                  </Button>
                ) : null}
              </div>
            </SettingsRow>

            <SettingsRow
              title={t("settings.remoteChannel.allowedUsers")}
              description={
                hasEmailEntry(draft.allowedUsers) ? (
                  <span className="im-channel-warn">{t("settings.remoteChannel.allowedUsersEmailNote")}</span>
                ) : (
                  t("settings.remoteChannel.allowedUsersHint")
                )
              }
            >
              <Textarea
                rows={3}
                value={draft.allowedUsers}
                placeholder="ou_xxxxxxxxxxxxxxxx"
                onChange={(event) => update("allowedUsers", event.target.value)}
              />
            </SettingsRow>
          </>
        )}

        <SettingsRow
          title={t("settings.remoteChannel.sessionHost")}
          description={t("settings.remoteChannel.sessionHostHint")}
        >
          <Select
            value={draft.sessionHost}
            onChange={(event) =>
              update("sessionHost", event.target.value as ChannelDraft["sessionHost"])
            }
          >
            <option value="desktop">{t("settings.remoteChannel.sessionHostDesktop")}</option>
            <option value="cli">{t("settings.remoteChannel.sessionHostCli")}</option>
          </Select>
        </SettingsRow>

        <SettingsRow
          title={t("settings.remoteChannel.toolPolicy")}
          description={
            draft.toolPolicy === "allow" ? (
              <span className="im-channel-warn">{t("settings.remoteChannel.toolPolicyAllowWarn")}</span>
            ) : (
              t("settings.remoteChannel.toolPolicyHint")
            )
          }
        >
          <Select
            value={draft.toolPolicy}
            onChange={(event) =>
              update("toolPolicy", event.target.value as ChannelDraft["toolPolicy"])
            }
          >
            <option value="confirm">{t("settings.remoteChannel.toolPolicyConfirm")}</option>
            <option value="deny">{t("settings.remoteChannel.toolPolicyDeny")}</option>
            <option value="allow">{t("settings.remoteChannel.toolPolicyAllow")}</option>
          </Select>
        </SettingsRow>

        <SettingsRow title={t("settings.remoteChannel.cwd")} description={t("settings.remoteChannel.cwdHint")}>
          <Input
            value={draft.cwd}
            placeholder={t("settings.remoteChannel.cwdPlaceholder")}
            onChange={(event) => update("cwd", event.target.value)}
          />
        </SettingsRow>

        <SettingsRow title={t("settings.remoteChannel.binaryOverride")} description={t("settings.remoteChannel.binaryOverrideHint")}>
          <Input
            value={draft.binaryPath}
            placeholder={t("settings.remoteChannel.binaryOverridePlaceholder")}
            onChange={(event) => update("binaryPath", event.target.value)}
          />
        </SettingsRow>

        <div className="im-channel-save">
          <Button
            variant="primary"
            disabled={busy !== null || !dirty}
            onClick={() => void saveOnly()}
          >
            {t("common.save")}
          </Button>
        </div>
      </SettingsCard>

      <SettingsCard title={setupTitle}>
        <ol className="im-channel-steps">
          {setupSteps.map((step, index) => (
            <li key={step}>
              <span className="im-channel-step-number">{index + 1}</span>
              <span>{t(`${setupPrefix}.${step}`)}</span>
            </li>
          ))}
        </ol>
      </SettingsCard>

      <SettingsCard title={t("settings.remoteChannel.logTitle")}>
        {lines.length === 0 ? (
          <p className="im-channel-note">{t("settings.remoteChannel.logEmpty")}</p>
        ) : (
          <pre className="im-channel-log" ref={logRef}>
            {lines.join("\n")}
          </pre>
        )}
        <p className="im-channel-note im-channel-config-path">
          {t("settings.remoteChannel.configPath", { path: state.configPath })}
        </p>
      </SettingsCard>
    </div>
  );
}
