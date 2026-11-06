/**
 * Employee roster and editor.
 *
 * Everything an unattended run needs is pinned here: the credential profile it
 * pays with, the provider and model it talks to, the workflow revision it
 * follows, and the schedule that decides when it starts.
 */
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  THINKING_LEVELS,
  type ModelInfo,
  type ProviderPublic,
  type StaffCredentialProfile,
  type StaffEmployee,
  type StaffWorkflow,
} from "@pi-desktop/shared";
import { api } from "../../lib/api";
import { useAppStore } from "../../stores/app-store";
import { Badge, Button, Field, Input, Select } from "../../components/ui";
import { IconBot, IconClock, IconPlay, IconPlus, IconTrash } from "../../components/icons";
import {
  employeeStatusLabel,
  employeeStatusTone,
  errorText,
  scheduleSummary,
  whenLabel,
} from "./staff-format";
import {
  EMPTY_DRAFT,
  employeePayload,
  type Draft,
  draftOf,
} from "./staff-employee-draft";

const NEW_EMPLOYEE = "staff-employee-new";


export function StaffEmployeeEditor({
  employees,
  onChanged,
  refreshToken,
}: {
  employees: StaffEmployee[];
  onChanged: () => void;
  refreshToken: number;
}) {
  const { t, i18n } = useTranslation();
  const showToast = useAppStore((s) => s.showToast);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState<Draft>(EMPTY_DRAFT);
  const [profiles, setProfiles] = useState<StaffCredentialProfile[]>([]);
  const [providers, setProviders] = useState<ProviderPublic[]>([]);
  const [workflows, setWorkflows] = useState<StaffWorkflow[]>([]);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    void (async () => {
      try {
        const [profileRes, providerRes, workflowRes] = await Promise.all([
          api.listStaffProfiles(),
          api.listProviders(),
          api.listStaffWorkflows(),
        ]);
        setProfiles(profileRes.profiles ?? []);
        setProviders(providerRes.providers ?? []);
        setWorkflows(workflowRes.workflows ?? []);
      } catch (error) {
        showToast(errorText(error), { variant: "error" });
      }
    })();
  }, [refreshToken]);

  useEffect(() => {
    if (!draft.providerId) {
      setModels([]);
      return;
    }
    void (async () => {
      try {
        const res = await api.listProviderModels({ providerId: draft.providerId, source: "cache" });
        setModels(res.models ?? []);
      } catch {
        setModels([]);
      }
    })();
  }, [draft.providerId]);

  const select = (employee: StaffEmployee) => {
    setSelectedId(employee.id);
    setDraft(draftOf(employee));
  };

  const startNew = () => {
    setSelectedId(NEW_EMPLOYEE);
    setDraft(EMPTY_DRAFT);
  };

  const set = (patch: Partial<Draft>) => setDraft((current) => ({ ...current, ...patch }));

  const save = async () => {
    setBusy(true);
    try {
      const payload = employeePayload(draft);
      if (selectedId === NEW_EMPLOYEE) {
        const res = await api.createStaffEmployee(payload);
        setSelectedId(res.employee.id);
      } else if (selectedId) {
        await api.updateStaffEmployee({ ...payload, id: selectedId });
      }
      onChanged();
      showToast(t("staff.employees.saved"));
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!selectedId || selectedId === NEW_EMPLOYEE) return;
    if (!window.confirm(t("staff.employees.deleteConfirm"))) return;
    try {
      await api.deleteStaffEmployee(selectedId);
      startNew();
      onChanged();
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    }
  };

  const runNow = async () => {
    if (!selectedId || selectedId === NEW_EMPLOYEE) return;
    try {
      await api.runStaffEmployeeNow(selectedId);
      showToast(t("staff.employees.runStarted"));
      onChanged();
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    }
  };

  const refreshSchedule = async () => {
    if (!selectedId || selectedId === NEW_EMPLOYEE) return;
    try {
      await api.refreshStaffSchedule(selectedId);
      onChanged();
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    }
  };

  const workflow = workflows.find((item) => item.id === draft.workflowId) ?? null;
  const versions: number[] = [];
  if (workflow) {
    for (let index = workflow.latestVersion; index >= 1; index -= 1) versions.push(index);
  }
  const current = employees.find((employee) => employee.id === selectedId) ?? null;

  return (
    <div className="staff-body">
      <div className="staff-list">
        <button
          type="button"
          className={"staff-list-item" + (selectedId === NEW_EMPLOYEE ? " on" : "")}
          onClick={startNew}
        >
          <IconPlus size={13} aria-hidden />
          <span className="staff-list-main">{t("staff.employees.new")}</span>
        </button>
        {employees.length === 0 ? (
          <div className="staff-hint px-2 py-1.5">{t("staff.employees.empty")}</div>
        ) : null}
        {employees.map((employee) => (
          <button
            key={employee.id}
            type="button"
            className={"staff-list-item" + (selectedId === employee.id ? " on" : "")}
            onClick={() => select(employee)}
          >
            <span
              className={"staff-dot " + employeeStatusTone(employee.status)}
              aria-hidden
            />
            <span className="staff-list-main">
              <span>{employee.name}</span>
              <span className="staff-list-sub">{scheduleSummary(employee.schedule ?? {}, t)}</span>
            </span>
            <span className="staff-list-sub">
              {whenLabel(employee.nextRunAt, i18n.language) || t("staff.never")}
            </span>
          </button>
        ))}
      </div>

      <div className="staff-form">
        {selectedId === null ? (
          <div className="staff-hint">{t("staff.employees.pick")}</div>
        ) : (
          <>
            <div className="staff-toolbar">
              <IconBot size={14} aria-hidden />
              <span className="staff-section-title">
                {selectedId === NEW_EMPLOYEE ? t("staff.employees.new") : draft.name}
              </span>
              {current ? (
                <Badge tone={current.status === "running" ? "success" : "neutral"}>
                  {employeeStatusLabel(current.status, t)}
                </Badge>
              ) : null}
              <span className="staff-spacer" />
              {current ? (
                <span className="staff-hint">
                  <IconClock size={12} aria-hidden /> {t("staff.employees.nextRun")}
                  {": "}
                  {whenLabel(current.nextRunAt, i18n.language) || t("staff.employees.scheduleManual")}
                  {" · "}
                  {t("staff.employees.lastRun")}
                  {": "}
                  {whenLabel(current.lastRunAt, i18n.language) || t("staff.never")}
                </span>
              ) : null}
            </div>

            <div className="staff-grid">
              <Field label={t("staff.employees.name")}>
                <Input
                  value={draft.name}
                  placeholder={t("staff.employees.namePlaceholder")}
                  onChange={(event) => set({ name: event.target.value })}
                />
              </Field>
              <Field label={t("staff.employees.role")}>
                <Input
                  value={draft.role}
                  placeholder={t("staff.employees.rolePlaceholder")}
                  onChange={(event) => set({ role: event.target.value })}
                />
              </Field>
              <Field label={t("staff.employees.status")}>
                <Select
                  value={draft.status}
                  onChange={(event) =>
                    set({ status: event.target.value as StaffEmployee["status"] })
                  }
                >
                  <option value="paused">{t("staff.employees.statusPaused")}</option>
                  <option value="running">{t("staff.employees.statusRunning")}</option>
                  <option value="stopped">{t("staff.employees.statusStopped")}</option>
                </Select>
              </Field>
              <Field label={t("staff.employees.credential")}>
                <Select
                  value={draft.credentialProfileId}
                  onChange={(event) => {
                    const profile = profiles.find((item) => item.id === event.target.value);
                    set({
                      credentialProfileId: event.target.value,
                      ...(profile ? { providerId: profile.providerId } : {}),
                    });
                  }}
                >
                  <option value="">{t("staff.employees.credentialNone")}</option>
                  {profiles.map((profile) => (
                    <option key={profile.id} value={profile.id}>
                      {profile.label}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field label={t("staff.employees.provider")}>
                <Select
                  value={draft.providerId}
                  onChange={(event) => set({ providerId: event.target.value, modelId: "" })}
                >
                  <option value="">{t("staff.employees.providerInherit")}</option>
                  {providers.map((provider) => (
                    <option key={provider.id} value={provider.id}>
                      {provider.name}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field label={t("staff.employees.model")}>
                <Select
                  value={draft.modelId}
                  onChange={(event) => set({ modelId: event.target.value })}
                >
                  <option value="">{t("staff.employees.modelInherit")}</option>
                  {models.map((model) => (
                    <option key={model.modelId} value={model.modelId}>
                      {model.displayName ?? model.modelId}
                    </option>
                  ))}
                  {draft.modelId && !models.some((model) => model.modelId === draft.modelId) ? (
                    <option value={draft.modelId}>{draft.modelId}</option>
                  ) : null}
                </Select>
              </Field>
              <Field label={t("staff.employees.thinking")}>
                <Select
                  value={draft.thinkingLevel}
                  onChange={(event) => set({ thinkingLevel: event.target.value })}
                >
                  <option value="">{t("staff.employees.thinkingDefault")}</option>
                  {THINKING_LEVELS.map((level) => (
                    <option key={level} value={level}>
                      {level}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field label={t("staff.employees.workflow")}>
                <Select
                  value={draft.workflowId}
                  onChange={(event) => {
                    const next = workflows.find((item) => item.id === event.target.value);
                    set({
                      workflowId: event.target.value,
                      workflowVersion: next ? String(next.latestVersion) : "",
                    });
                  }}
                >
                  <option value="">{t("staff.employees.workflowNone")}</option>
                  {workflows.map((item) => (
                    <option key={item.id} value={item.id}>
                      {item.name}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field label={t("staff.employees.workflowVersion")}>
                <Select
                  value={draft.workflowVersion}
                  disabled={!workflow}
                  onChange={(event) => set({ workflowVersion: event.target.value })}
                >
                  <option value="" />
                  {versions.map((value) => (
                    <option key={value} value={String(value)}>
                      {"v" + value}
                    </option>
                  ))}
                </Select>
              </Field>
            </div>
            <div className="staff-hint">{t("staff.employees.pinnedHint")}</div>

            <div className="staff-section-title">{t("staff.employees.schedule")}</div>
            <div className="staff-grid">
              <Field label={t("staff.employees.everyMinutes")}>
                <Input
                  type="number"
                  min={5}
                  value={draft.everyMinutes}
                  onChange={(event) => set({ everyMinutes: event.target.value })}
                />
              </Field>
              <Field label={t("staff.employees.jitterMinutes")}>
                <Input
                  type="number"
                  min={0}
                  value={draft.jitterMinutes}
                  onChange={(event) => set({ jitterMinutes: event.target.value })}
                />
              </Field>
              <Field label={t("staff.employees.timesOfDay")} hint={t("staff.employees.timesOfDayHint")}>
                <Input
                  value={draft.timesOfDay}
                  placeholder="09:00, 20:00"
                  onChange={(event) => set({ timesOfDay: event.target.value })}
                />
              </Field>
              <Field label={t("staff.employees.utcOffsetMinutes")}>
                <Input
                  type="number"
                  value={draft.utcOffsetMinutes}
                  onChange={(event) => set({ utcOffsetMinutes: event.target.value })}
                />
              </Field>
              <Field label={t("staff.employees.maxRunsPerDay")}>
                <Input
                  type="number"
                  min={1}
                  value={draft.maxRunsPerDay}
                  onChange={(event) => set({ maxRunsPerDay: event.target.value })}
                />
              </Field>
            </div>

            <div className="staff-toolbar">
              <Button
                variant="primary"
                disabled={busy || !draft.name.trim()}
                onClick={() => void save()}
              >
                {selectedId === NEW_EMPLOYEE ? t("staff.employees.create") : t("staff.employees.save")}
              </Button>
              {selectedId === NEW_EMPLOYEE ? null : (
                <>
                  <Button variant="ghost" onClick={() => void runNow()}>
                    <IconPlay size={13} aria-hidden /> {t("staff.employees.runNow")}
                  </Button>
                  <Button variant="ghost" onClick={() => void refreshSchedule()}>
                    <IconClock size={13} aria-hidden /> {t("staff.employees.refreshSchedule")}
                  </Button>
                  <span className="staff-spacer" />
                  <Button variant="ghost" onClick={() => void remove()}>
                    <IconTrash size={13} aria-hidden /> {t("staff.employees.delete")}
                  </Button>
                </>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
