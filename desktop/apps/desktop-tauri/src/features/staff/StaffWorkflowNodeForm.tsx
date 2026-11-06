/**
 * The per-kind parameter form, shared by the graph view and the parameter list.
 *
 * A node kind decides its own fields here, so adding a kind in host-core means
 * one entry in this table and nothing else in the editor.
 */
import { Badge, Button, Field, Input, Select, Textarea } from "../../components/ui";
import { IconTrash } from "../../components/icons";
import {
  STAFF_SOURCE_CHANNELS,
  staffSourceChannelKey,
  type StaffEmployee,
  type StaffNodeKind,
} from "@pi-desktop/shared";
import { configList, configText, type EditorNode } from "./workflow-graph";
import { nodeKindKey } from "./staff-format";

type FieldSpec = {
  key: string;
  labelKey: string;
  type: "text" | "number" | "select" | "employee" | "channels" | "list";
  options?: Array<{ value: string; labelKey: string }>;
};

const select = (key: string, labelKey: string, options: Array<[string, string]>): FieldSpec => ({
  key,
  labelKey,
  type: "select",
  options: options.map(([value, optionLabel]) => ({ value, labelKey: optionLabel })),
});

export const NODE_FIELDS: Record<StaffNodeKind, FieldSpec[]> = {
  trigger: [
    select("mode", "staff.config.mode", [
      ["schedule", "staff.config.modeSchedule"],
      ["manual", "staff.config.modeManual"],
      ["event", "staff.config.modeEvent"],
    ]),
  ],
  probe: [
    { key: "sources", labelKey: "staff.config.sources", type: "channels" },
    { key: "channel", labelKey: "staff.config.channel", type: "text" },
  ],
  analyze: [{ key: "focus", labelKey: "staff.config.focus", type: "text" }],
  produce: [{ key: "artifact", labelKey: "staff.config.artifact", type: "text" }],
  agent: [{ key: "limit", labelKey: "staff.config.limit", type: "number" }],
  memory: [
    select("operation", "staff.config.operation", [
      ["read", "staff.config.operationRead"],
      ["write", "staff.config.operationWrite"],
      ["publish", "staff.config.operationPublish"],
      ["retire", "staff.config.operationRetire"],
    ]),
    select("scope", "staff.config.scope", [
      ["employee", "staff.config.scopeEmployee"],
      ["shared", "staff.config.scopeShared"],
    ]),
    select("kind", "staff.config.kind", [
      ["fact", "staff.config.kindFact"],
      ["preference", "staff.config.kindPreference"],
      ["playbook", "staff.config.kindPlaybook"],
      ["constraint", "staff.config.kindConstraint"],
      ["signal", "staff.config.kindSignal"],
    ]),
    { key: "limit", labelKey: "staff.config.limit", type: "number" },
  ],
  condition: [{ key: "expression", labelKey: "staff.config.expression", type: "text" }],
  handoff: [{ key: "employeeId", labelKey: "staff.config.employeeId", type: "employee" }],
  humanGate: [{ key: "reason", labelKey: "staff.config.reason", type: "text" }],
  notify: [{ key: "channel", labelKey: "staff.config.channel", type: "text" }],
  board: [
    select("action", "staff.config.boardAction", [
      ["read", "staff.config.boardActionRead"],
      ["judge", "staff.config.boardActionJudge"],
      ["dispatch", "staff.config.boardActionDispatch"],
      ["report", "staff.config.boardActionReport"],
    ]),
    select("board", "staff.config.board", [
      ["leads", "staff.config.boardLeads"],
      ["contents", "staff.config.boardContents"],
    ]),
    // Who this manager may hand work to, and of what kind. Stored as a list, so
    // the authority is declared in the same graph the host validates.
    { key: "targets", labelKey: "staff.config.boardTargets", type: "list" },
    { key: "verbs", labelKey: "staff.config.boardVerbs", type: "list" },
  ],
};

export function NodeInspector({
  node,
  employees,
  onPatch,
  onConfig,
  onRemove,
  t,
}: {
  node: EditorNode;
  employees: StaffEmployee[];
  onPatch: (patch: Partial<EditorNode>) => void;
  onConfig: (key: string, value: string | string[]) => void;
  onRemove: () => void;
  t: (key: string, options?: Record<string, unknown>) => string;
}) {
  return (
    <div className="staff-form">
      <div className="staff-toolbar">
        <Badge tone="neutral">{t(nodeKindKey(node.kind))}</Badge>
        <span className="staff-mono">{node.id}</span>
        <span className="staff-spacer" />
        <Button variant="ghost" size="sm" onClick={onRemove}>
          <IconTrash size={12} aria-hidden /> {t("staff.workflow.deleteNode")}
        </Button>
      </div>
      <div className="staff-grid">
        <Field label={t("staff.workflow.title")}>
          <Input
            value={node.title ?? ""}
            placeholder={t("staff.workflow.titlePlaceholder")}
            onChange={(event) => onPatch({ title: event.target.value })}
          />
        </Field>
        {(NODE_FIELDS[node.kind] ?? []).map((field) => {
          const value = configText(node, field.key);
          if (field.type === "employee") {
            return (
              <Field key={field.key} label={t(field.labelKey)}>
                <Select value={value} onChange={(event) => onConfig(field.key, event.target.value)}>
                  <option value="" />
                  {employees.map((employee) => (
                    <option key={employee.id} value={employee.id}>
                      {employee.name}
                    </option>
                  ))}
                </Select>
              </Field>
            );
          }
          if (field.type === "list") {
            const chosen = configList(node, field.key);
            return (
              <Field key={field.key} label={t(field.labelKey)}>
                <Input
                  value={chosen.join(", ")}
                  onChange={(event) =>
                    onConfig(
                      field.key,
                      event.target.value
                        .split(/[,，]/)
                        .map((item) => item.trim())
                        .filter(Boolean),
                    )
                  }
                />
              </Field>
            );
          }
          if (field.type === "channels") {
            const chosen = configList(node, field.key);
            return (
              <Field key={field.key} label={t(field.labelKey)}>
                <div className="staff-chips">
                  {STAFF_SOURCE_CHANNELS.map((channel) => {
                    const on = chosen.includes(channel);
                    return (
                      <button
                        key={channel}
                        type="button"
                        className={"staff-chip" + (on ? " on" : "")}
                        aria-pressed={on}
                        onClick={() =>
                          onConfig(
                            field.key,
                            on
                              ? chosen.filter((item) => item !== channel)
                              : [...chosen, channel],
                          )
                        }
                      >
                        {t(staffSourceChannelKey(channel))}
                      </button>
                    );
                  })}
                </div>
              </Field>
            );
          }
          if (field.type === "select") {
            return (
              <Field key={field.key} label={t(field.labelKey)}>
                <Select value={value} onChange={(event) => onConfig(field.key, event.target.value)}>
                  <option value="" />
                  {(field.options ?? []).map((option) => (
                    <option key={option.value} value={option.value}>
                      {t(option.labelKey)}
                    </option>
                  ))}
                </Select>
              </Field>
            );
          }
          return (
            <Field key={field.key} label={t(field.labelKey)}>
              <Input
                type={field.type === "number" ? "number" : "text"}
                value={value}
                onChange={(event) => onConfig(field.key, event.target.value)}
              />
            </Field>
          );
        })}
      </div>
      {node.kind === "agent" ||
      node.kind === "produce" ||
      node.kind === "analyze" ||
      node.kind === "board" ? (
        <Field label={t("staff.workflow.instruction")}>
          <Textarea
            rows={3}
            value={node.instruction ?? ""}
            onChange={(event) => onPatch({ instruction: event.target.value })}
          />
        </Field>
      ) : null}
    </div>
  );
}

