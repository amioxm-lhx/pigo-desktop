/**
 * Credential profiles: one key + endpoint per account, so two employees can
 * bill two different API keys without either one knowing the other's.
 *
 * The key is write-only. It is read back as a hasSecret flag, never as text,
 * which is why the form clears the field on every save.
 */
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ProviderPublic, StaffCredentialProfile } from "@pi-desktop/shared";
import { api } from "../../lib/api";
import { useAppStore } from "../../stores/app-store";
import { Badge, Button, Field, Input, Select } from "../../components/ui";
import { IconKey, IconPlus, IconTrash } from "../../components/icons";
import { errorText } from "./staff-format";

const NEW_LABEL = "staff-credentials-new";

export function StaffCredentialPanel() {
  const { t } = useTranslation();
  const showToast = useAppStore((s) => s.showToast);
  const [profiles, setProfiles] = useState<StaffCredentialProfile[]>([]);
  const [providers, setProviders] = useState<ProviderPublic[]>([]);
  const [mode, setMode] = useState<string>(NEW_LABEL);
  const [label, setLabel] = useState("");
  const [providerId, setProviderId] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [secretValue, setSecretValue] = useState("");
  const [busy, setBusy] = useState(false);

  const load = async () => {
    try {
      const res = await api.listStaffProfiles();
      setProfiles(res.profiles ?? []);
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    }
  };

  useEffect(() => {
    void load();
    void (async () => {
      try {
        const res = await api.listProviders();
        setProviders(res.providers ?? []);
      } catch {
        setProviders([]);
      }
    })();
  }, []);

  const selectProfile = (profile: StaffCredentialProfile) => {
    setMode(profile.id);
    setLabel(profile.label);
    setProviderId(profile.providerId);
    setBaseUrl(profile.baseUrl ?? "");
    setSecretValue("");
  };

  const startNew = () => {
    setMode(NEW_LABEL);
    setLabel("");
    setProviderId(providers[0]?.id ?? "");
    setBaseUrl("");
    setSecretValue("");
  };

  const save = async () => {
    setBusy(true);
    try {
      if (mode === NEW_LABEL) {
        await api.createStaffProfile({
          label: label.trim() || providerId,
          providerId,
          ...(baseUrl.trim() ? { baseUrl: baseUrl.trim() } : {}),
          ...(secretValue.trim() ? { secretValue: secretValue.trim() } : {}),
        });
      } else {
        await api.updateStaffProfile({
          id: mode,
          label: label.trim(),
          baseUrl: baseUrl.trim(),
        });
        if (secretValue.trim()) {
          await api.setStaffProfileSecret(mode, secretValue.trim());
        }
      }
      setSecretValue("");
      await load();
      showToast(t("staff.credentials.saved"));
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (mode === NEW_LABEL) return;
    if (!window.confirm(t("staff.credentials.deleteConfirm"))) return;
    setBusy(true);
    try {
      await api.deleteStaffProfile(mode);
      startNew();
      await load();
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    } finally {
      setBusy(false);
    }
  };

  // A provider id is a UUID: nothing the owner can read back, and long
  // enough that the row clipped it. Show the provider name instead.
  const providerLabel = (id: string) => providers.find((item) => item.id === id)?.name ?? id;

  return (
    <div className="staff-body">
      <div className="staff-list">
        <button
          type="button"
          className={"staff-list-item" + (mode === NEW_LABEL ? " on" : "")}
          onClick={startNew}
        >
          <IconPlus size={13} aria-hidden />
          <span className="staff-list-main">{t("staff.credentials.new")}</span>
        </button>
        {profiles.length === 0 ? (
          <div className="staff-hint px-2 py-1.5">
            {t("staff.credentials.empty")}
          </div>
        ) : null}
        {profiles.map((profile) => (
          <button
            key={profile.id}
            type="button"
            className={"staff-list-item" + (mode === profile.id ? " on" : "")}
            onClick={() => selectProfile(profile)}
          >
            <IconKey size={13} aria-hidden />
            <span className="staff-list-main">
              <span>{profile.label}</span>
              <span className="staff-list-sub">{providerLabel(profile.providerId)}</span>
            </span>
            <Badge tone={profile.hasSecret ? "success" : "warning"}>
              {profile.hasSecret
                ? t("staff.credentials.hasSecret")
                : t("staff.credentials.noSecret")}
            </Badge>
          </button>
        ))}
      </div>

      <div className="staff-form">
        <div className="staff-section-title">
          {mode === NEW_LABEL ? t("staff.credentials.new") : t("staff.credentials.save")}
        </div>
        <div className="staff-grid">
          <Field label={t("staff.credentials.label")}>
            <Input
              value={label}
              placeholder={t("staff.credentials.labelPlaceholder")}
              onChange={(event) => setLabel(event.target.value)}
            />
          </Field>
          <Field label={t("staff.credentials.providerId")}>
            <Select
              value={providerId}
              disabled={mode !== NEW_LABEL}
              onChange={(event) => setProviderId(event.target.value)}
            >
              {providers.map((provider) => (
                <option key={provider.id} value={provider.id}>
                  {provider.name}
                </option>
              ))}
              {providers.length === 0 ? <option value={providerId}>{providerId}</option> : null}
            </Select>
          </Field>
          <Field label={t("staff.credentials.baseUrl")}>
            <Input
              value={baseUrl}
              placeholder={t("staff.credentials.baseUrlPlaceholder")}
              onChange={(event) => setBaseUrl(event.target.value)}
            />
          </Field>
          <Field
            label={t("staff.credentials.secret")}
            hint={mode === NEW_LABEL ? undefined : t("staff.credentials.secretKeep")}
          >
            <Input
              type="password"
              value={secretValue}
              autoComplete="off"
              onChange={(event) => setSecretValue(event.target.value)}
            />
          </Field>
        </div>
        <div className="staff-hint">{t("staff.credentials.hint")}</div>
        <div className="staff-toolbar">
          <Button variant="primary" disabled={busy || !label.trim() || !providerId} onClick={() => void save()}>
            {mode === NEW_LABEL ? t("staff.credentials.create") : t("staff.credentials.save")}
          </Button>
          {mode === NEW_LABEL ? null : (
            <Button variant="ghost" disabled={busy} onClick={() => void remove()}>
              <IconTrash size={13} aria-hidden />
              {t("staff.credentials.delete")}
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
