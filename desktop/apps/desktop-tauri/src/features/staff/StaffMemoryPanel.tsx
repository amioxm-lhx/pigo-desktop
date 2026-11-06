/**
 * Shared memory: what the employees learned, and what the user told them.
 *
 * Writes here are user-authored, so the promotion gate lets them through —
 * that is the whole point of the panel. Retiring a constraint is offered only
 * from this surface for the same reason: it loosens a limit, and only the user
 * may do that.
 */
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
  StaffEmployee,
  StaffMemoryItem,
  StaffMemoryKind,
  StaffMemoryScope,
} from "@pi-desktop/shared";
import { api } from "../../lib/api";
import { useAppStore } from "../../stores/app-store";
import { Badge, Button, Field, Input, Select, Textarea } from "../../components/ui";
import { IconDatabase, IconShield } from "../../components/icons";
import { errorText, whenLabel } from "./staff-format";

const KINDS: StaffMemoryKind[] = ["fact", "preference", "playbook", "constraint", "signal"];

function kindLabel(kind: string, t: (key: string) => string): string {
  const table: Record<string, string> = {
    fact: "staff.config.kindFact",
    preference: "staff.config.kindPreference",
    playbook: "staff.config.kindPlaybook",
    constraint: "staff.config.kindConstraint",
    signal: "staff.config.kindSignal",
  };
  return t(table[kind] ?? "staff.config.kindFact");
}

export function StaffMemoryPanel({ employees }: { employees: StaffEmployee[] }) {
  const { t, i18n } = useTranslation();
  const showToast = useAppStore((s) => s.showToast);
  const [items, setItems] = useState<StaffMemoryItem[]>([]);
  const [scope, setScope] = useState<StaffMemoryScope | "">("");
  const [kind, setKind] = useState<StaffMemoryKind | "">("");
  const [ownerId, setOwnerId] = useState("");
  const [draftScope, setDraftScope] = useState<StaffMemoryScope>("employee");
  const [draftKind, setDraftKind] = useState<StaffMemoryKind>("fact");
  const [draftKey, setDraftKey] = useState("");
  const [draftText, setDraftText] = useState("");
  const [draftSource, setDraftSource] = useState("");
  const [busy, setBusy] = useState(false);

  const load = async () => {
    try {
      const res = await api.queryStaffMemory({
        ...(scope ? { scope } : {}),
        ...(kind ? { kind } : {}),
        ...(scope === "employee" && ownerId ? { ownerId } : {}),
        limit: 200,
      });
      setItems(res.items ?? []);
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    }
  };

  useEffect(() => {
    void load();
  }, [scope, kind, ownerId]);

  const write = async () => {
    if (!draftText.trim()) return;
    setBusy(true);
    try {
      const input: Record<string, unknown> = {
        scope: draftScope,
        kind: draftKind,
        key: draftKey.trim(),
        text: draftText.trim(),
        sourceKind: "user",
        ...(draftSource.trim() ? { sourceRef: draftSource.trim() } : {}),
        ...(draftScope === "employee" && ownerId ? { ownerId } : {}),
      };
      if (draftScope === "shared") await api.publishStaffMemory(input);
      else await api.writeStaffMemory(input);
      setDraftText("");
      setDraftKey("");
      setDraftSource("");
      await load();
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    } finally {
      setBusy(false);
    }
  };

  const retire = async (id: string) => {
    try {
      await api.retireStaffMemory(id);
      await load();
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    }
  };

  return (
    <div className="staff-body single">
      <div className="staff-form">
        <div className="staff-toolbar">
          <Field label={t("staff.memory.scope")}>
            <Select value={scope} onChange={(event) => setScope(event.target.value as StaffMemoryScope | "")}>
              <option value="">{t("staff.memory.scopeAll")}</option>
              <option value="employee">{t("staff.config.scopeEmployee")}</option>
              <option value="shared">{t("staff.config.scopeShared")}</option>
            </Select>
          </Field>
          <Field label={t("staff.memory.kind")}>
            <Select value={kind} onChange={(event) => setKind(event.target.value as StaffMemoryKind | "")}>
              <option value="">{t("staff.memory.kindAll")}</option>
              {KINDS.map((value) => (
                <option key={value} value={value}>
                  {kindLabel(value, t)}
                </option>
              ))}
            </Select>
          </Field>
          <Field label={t("staff.memory.ownerLabel")}>
            <Select value={ownerId} onChange={(event) => setOwnerId(event.target.value)}>
              <option value="">{t("staff.runs.allEmployees")}</option>
              {employees.map((employee) => (
                <option key={employee.id} value={employee.id}>
                  {employee.name}
                </option>
              ))}
            </Select>
          </Field>
        </div>
        <div className="staff-hint">{t("staff.memory.keyHint")}</div>
      </div>

      <div className="staff-form">
        <div className="staff-section-title">{t("staff.memory.write")}</div>
        <div className="staff-grid">
          <Field label={t("staff.memory.scope")}>
            <Select
              value={draftScope}
              onChange={(event) => setDraftScope(event.target.value as StaffMemoryScope)}
            >
              <option value="employee">{t("staff.config.scopeEmployee")}</option>
              <option value="shared">{t("staff.config.scopeShared")}</option>
            </Select>
          </Field>
          <Field label={t("staff.memory.kind")}>
            <Select
              value={draftKind}
              onChange={(event) => setDraftKind(event.target.value as StaffMemoryKind)}
            >
              {KINDS.map((value) => (
                <option key={value} value={value}>
                  {kindLabel(value, t)}
                </option>
              ))}
            </Select>
          </Field>
          <Field label={t("staff.memory.key")}>
            <Input
              value={draftKey}
              placeholder={t("staff.memory.keyPlaceholder")}
              onChange={(event) => setDraftKey(event.target.value)}
            />
          </Field>
          <Field label={t("staff.memory.sourceRef")}>
            <Input value={draftSource} onChange={(event) => setDraftSource(event.target.value)} />
          </Field>
        </div>
        <Field label={t("staff.memory.text")}>
          <Textarea
            rows={3}
            value={draftText}
            placeholder={t("staff.memory.textPlaceholder")}
            onChange={(event) => setDraftText(event.target.value)}
          />
        </Field>
        <div className="staff-hint">
          <IconShield size={12} aria-hidden /> {t("staff.memory.constraintHint")}
        </div>
        <div className="staff-toolbar">
          <Button
            variant="primary"
            disabled={busy || !draftText.trim()}
            onClick={() => void write()}
          >
            {draftScope === "shared" ? t("staff.memory.publish") : t("staff.memory.write")}
          </Button>
          <span className="staff-hint">{t("staff.memory.sourceRefHint")}</span>
        </div>
      </div>

      {items.length === 0 ? (
        <div className="staff-hint">{t("staff.memory.empty")}</div>
      ) : (
        <div className="staff-memory">
          {items.map((item) => (
            <div key={item.id} className="staff-memory-item">
              <div className="staff-memory-head">
                <Badge tone={item.kind === "constraint" ? "warning" : "neutral"}>
                  {kindLabel(item.kind, t)}
                </Badge>
                <span className="staff-tag">
                  {item.scope === "shared"
                    ? t("staff.config.scopeShared")
                    : t("staff.config.scopeEmployee")}
                </span>
                {item.contested ? (
                  <span className="staff-tag warn">{t("staff.memory.contested")}</span>
                ) : null}
                {item.status !== "active" ? (
                  <span className="staff-tag">
                    {item.status === "retired"
                      ? t("staff.memory.retired")
                      : t("staff.memory.superseded")}
                  </span>
                ) : null}
                {item.key ? <span className="staff-memory-meta">{item.key}</span> : null}
                <span className="staff-spacer" />
                <Button variant="ghost" size="sm" onClick={() => void retire(item.id)}>
                  {t("staff.memory.retire")}
                </Button>
              </div>
              <div className="staff-memory-text">{item.text}</div>
              <div className="staff-memory-meta">
                <span>
                  <IconDatabase size={11} aria-hidden /> {item.sourceKind}
                  {item.sourceRef ? " · " + item.sourceRef : ""}
                </span>
                <span>{t("staff.memory.confidence") + " " + item.confidence.toFixed(2)}</span>
                <span>{whenLabel(item.createdAt, i18n.language)}</span>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
