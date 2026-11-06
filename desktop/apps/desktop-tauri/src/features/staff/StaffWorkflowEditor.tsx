/**
 * Workflow editor: a visual node graph, a parameter list, and the YAML the
 * host would compile.
 *
 * Validation is always the host's answer -- staff.workflow.validate -- because
 * the reachability and cycle rules live there. This file only keeps the
 * pointer from drawing something the host would obviously reject.
 */
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  STAFF_NODE_KINDS,
  type StaffEmployee,
  type StaffGraphValidation,
  type StaffNodeKind,
  type StaffWorkflow,
  type StaffWorkflowStatus,
  type StaffWorkflowVersion,
} from "@pi-desktop/shared";
import { api } from "../../lib/api";
import { useAppStore } from "../../stores/app-store";
import { Badge, Button, Field, Input, Select } from "../../components/ui";
import {
  IconCheck,
  IconPlus,
  IconTrash,
  IconTriangleAlert,
  IconWorkflow,
} from "../../components/icons";
import { StaffWorkflowCanvas } from "./StaffWorkflowCanvas";
import {
  REQUIRED_CONFIG,
  autoLayout,
  configOf,
  configList,
  configText,
  createNode,
  fromGraph,
  toGraph,
  toYaml,
  type EditorGraph,
  type EditorNode,
} from "./workflow-graph";
import { errorText, nodeKindKey, whenLabel } from "./staff-format";
import { NODE_FIELDS, NodeInspector } from "./StaffWorkflowNodeForm";

type View = "graph" | "config" | "yaml";


export function StaffWorkflowEditor({ employees }: { employees: StaffEmployee[] }) {
  const { t, i18n } = useTranslation();
  const showToast = useAppStore((s) => s.showToast);
  const [workflows, setWorkflows] = useState<StaffWorkflow[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [versions, setVersions] = useState<StaffWorkflowVersion[]>([]);
  const [version, setVersion] = useState(0);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [status, setStatus] = useState<StaffWorkflowStatus>("draft");
  const [graph, setGraph] = useState<EditorGraph>({ nodes: [], edges: [] });
  const [selectedNode, setSelectedNode] = useState<string | null>(null);
  const [linkFrom, setLinkFrom] = useState<string | null>(null);
  const [view, setView] = useState<View>("graph");
  // Metadata edits must not mint a graph version, so the two are tracked apart.
  const [graphDirty, setGraphDirty] = useState(false);
  const [validation, setValidation] = useState<StaffGraphValidation | null>(null);
  const [busy, setBusy] = useState(false);

  const refreshList = async () => {
    try {
      const res = await api.listStaffWorkflows();
      setWorkflows(res.workflows ?? []);
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    }
  };

  useEffect(() => {
    void refreshList();
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setLinkFrom(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const openWorkflow = async (id: string, atVersion?: number) => {
    try {
      const res = await api.getStaffWorkflow(id, atVersion);
      setSelectedId(id);
      setName(res.workflow.name);
      setDescription(res.workflow.description);
      setStatus(res.workflow.status);
      setVersion(res.version);
      setGraph(fromGraph(res.graph));
      setSelectedNode(null);
      setLinkFrom(null);
      setGraphDirty(false);
      setValidation(null);
      const list = await api.listStaffWorkflowVersions(id);
      setVersions(list.versions ?? []);
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    }
  };

  const startNew = () => {
    setSelectedId(null);
    setName("");
    setDescription("");
    setStatus("draft");
    setVersion(0);
    setVersions([]);
    setGraph({ nodes: [], edges: [] });
    setSelectedNode(null);
    setGraphDirty(true);
    setValidation(null);
  };

  const edit = (next: EditorGraph) => {
    setGraph(next);
    setGraphDirty(true);
    setValidation(null);
  };

  const addNode = (kind: StaffNodeKind) => {
    const node = createNode(kind, graph.nodes);
    edit({ ...graph, nodes: [...graph.nodes, node] });
    setSelectedNode(node.id);
    setView("graph");
  };

  const removeNode = (id: string) => {
    edit({
      nodes: graph.nodes.filter((node) => node.id !== id),
      edges: graph.edges.filter((edge) => edge.from !== id && edge.to !== id),
    });
    if (selectedNode === id) setSelectedNode(null);
  };

  const patchNode = (id: string, patch: Partial<EditorNode>) => {
    edit({
      ...graph,
      nodes: graph.nodes.map((node) => (node.id === id ? { ...node, ...patch } : node)),
    });
  };

  const setConfig = (id: string, key: string, value: string | string[]) => {
    const node = graph.nodes.find((item) => item.id === id);
    if (!node) return;
    const next = { ...configOf(node) };
    if (Array.isArray(value)) {
      // An empty list is the same statement as an empty text field: unset, so
      // the saved graph does not carry a parameter nobody chose.
      if (value.length === 0) delete next[key];
      else next[key] = value;
      patchNode(id, { config: next });
      return;
    }
    if (value === "") delete next[key];
    else if (NODE_FIELDS[node.kind].find((field) => field.key === key)?.type === "number") {
      next[key] = Number(value);
    } else next[key] = value;
    patchNode(id, { config: next });
  };

  const validate = async () => {
    try {
      const result = await api.validateStaffWorkflow(toGraph(graph));
      setValidation(result);
    } catch (error) {
      setValidation({ valid: false, error: errorText(error) });
    }
  };

  const save = async () => {
    setBusy(true);
    try {
      if (selectedId === null) {
        const res = await api.createStaffWorkflow({
          name: name.trim(),
          description: description.trim(),
          status,
          graph: toGraph(graph),
        });
        await refreshList();
        await openWorkflow(res.workflow.id);
      } else {
        await api.updateStaffWorkflow({
          id: selectedId,
          name: name.trim(),
          description: description.trim(),
          status,
          ...(graphDirty ? { graph: toGraph(graph) } : {}),
        });
        await refreshList();
        await openWorkflow(selectedId);
      }
      showToast(t("staff.workflow.saved"));
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    } finally {
      setBusy(false);
    }
  };

  const removeWorkflow = async () => {
    if (selectedId === null) return;
    if (!window.confirm(t("staff.workflow.deleteConfirm"))) return;
    try {
      await api.deleteStaffWorkflow(selectedId);
      startNew();
      await refreshList();
    } catch (error) {
      showToast(errorText(error), { variant: "error" });
    }
  };

  const kindLabel = (kind: string) => t(nodeKindKey(kind));
  const active = graph.nodes.find((node) => node.id === selectedNode) ?? null;
  const nodeSubtitle = (node: EditorNode) => {
    const required = REQUIRED_CONFIG[node.kind] ?? [];
    const parts = required.map((key) => configText(node, key)).filter(Boolean);
    if (parts.length === 0 && node.kind === "probe") {
      const count = configList(node, "sources").length;
      if (count > 0) return t("staff.config.sourceCount", { count });
    }
    return parts.join(" · ") || node.kind;
  };

  return (
    <div className="staff-body">
      <div className="staff-list">
        <button
          type="button"
          className={"staff-list-item" + (selectedId === null ? " on" : "")}
          onClick={startNew}
        >
          <IconPlus size={13} aria-hidden />
          <span className="staff-list-main">{t("staff.workflow.new")}</span>
        </button>
        {workflows.length === 0 ? (
          <div className="staff-hint px-2 py-1.5">{t("staff.workflow.empty")}</div>
        ) : null}
        {workflows.map((item) => (
          <button
            key={item.id}
            type="button"
            className={"staff-list-item" + (selectedId === item.id ? " on" : "")}
            onClick={() => void openWorkflow(item.id)}
          >
            <IconWorkflow size={13} aria-hidden />
            <span className="staff-list-main">
              <span>{item.name}</span>
              <span className="staff-list-sub">
                {item.status + " · v" + item.latestVersion}
                {item.nodeCount == null ? "" : " · " + item.nodeCount}
              </span>
            </span>
          </button>
        ))}
        {selectedId === null || versions.length === 0 ? null : (
          <div className="mt-2 border-t border-border-subtle pt-2">
            <div className="staff-hint px-2 pb-1">{t("staff.workflow.versions")}</div>
            {versions.map((item) => (
              <button
                key={item.id}
                type="button"
                className={"staff-list-item" + (version === item.version ? " on" : "")}
                onClick={() => void openWorkflow(item.workflowId, item.version)}
              >
                <span className="staff-list-main">
                  <span>{"v" + item.version}</span>
                  <span className="staff-list-sub">{whenLabel(item.createdAt, i18n.language)}</span>
                </span>
                <Badge tone="neutral">{String(item.nodeCount)}</Badge>
              </button>
            ))}
          </div>
        )}
      </div>

      <div className="staff-form">
        <div className="staff-grid">
          <Field label={t("staff.workflow.name")}>
            <Input
              value={name}
              placeholder={t("staff.workflow.namePlaceholder")}
              onChange={(event) => {
                setName(event.target.value);
                        }}
            />
          </Field>
          <Field label={t("staff.workflow.status")}>
            <Select
              value={status}
              onChange={(event) => {
                setStatus(event.target.value as StaffWorkflowStatus);
                        }}
            >
              <option value="draft">{t("staff.workflow.statusDraft")}</option>
              <option value="active">{t("staff.workflow.statusActive")}</option>
              <option value="archived">{t("staff.workflow.statusArchived")}</option>
            </Select>
          </Field>
        </div>
        <Field label={t("staff.workflow.description")}>
          <Input
            value={description}
            onChange={(event) => {
              setDescription(event.target.value);
                    }}
          />
        </Field>

        <div className="staff-toolbar">
          <div className="staff-tabs">
            {(["graph", "config", "yaml"] as View[]).map((value) => (
              <button
                key={value}
                type="button"
                className={"staff-tab" + (view === value ? " on" : "")}
                onClick={() => setView(value)}
              >
                {value === "graph"
                  ? t("staff.workflow.viewGraph")
                  : value === "config"
                    ? t("staff.workflow.viewConfig")
                    : t("staff.workflow.viewYaml")}
              </button>
            ))}
          </div>
          <span className="staff-spacer" />
          <Button variant="ghost" onClick={() => void validate()}>
            {t("staff.workflow.validate")}
          </Button>
          <Button
            variant="ghost"
            onClick={() => edit({ ...graph, nodes: autoLayout(graph.nodes, graph.edges) })}
          >
            {t("staff.workflow.autoLayout")}
          </Button>
          <Button
            variant="primary"
            disabled={busy || !name.trim()}
            onClick={() => void save()}
          >
            {selectedId === null ? t("staff.workflow.create") : t("staff.workflow.save")}
          </Button>
          {selectedId === null ? null : (
            <Button variant="ghost" onClick={() => void removeWorkflow()}>
              <IconTrash size={13} aria-hidden />
            </Button>
          )}
        </div>

        {validation === null ? null : (
          <div className={validation.valid ? "staff-ok" : "staff-error"}>
            {validation.valid ? (
              <span>
                <IconCheck size={12} aria-hidden /> {t("staff.workflow.valid")}
                {validation.nodeCount == null
                  ? ""
                  : " · " + t("staff.workflow.nodeCount") + " " + String(validation.nodeCount)}
                {validation.steps == null
                  ? ""
                  : " · " + t("staff.workflow.stepCount") + " " + String(validation.steps)}
              </span>
            ) : (
              <span>
                <IconTriangleAlert size={12} aria-hidden /> {t("staff.workflow.invalid")}{" "}
                {validation.error ?? ""}
              </span>
            )}
          </div>
        )}

        {view === "graph" ? (
          <div className="space-y-3">
            <div className="staff-palette">
              {STAFF_NODE_KINDS.map((kind) => (
                <button
                  key={kind}
                  type="button"
                  className="staff-chip"
                  onClick={() => addNode(kind)}
                >
                  {kindLabel(kind)}
                </button>
              ))}
            </div>
            <StaffWorkflowCanvas
              graph={graph}
              selectedId={selectedNode}
              linkFrom={linkFrom}
              onChange={edit}
              onSelect={setSelectedNode}
              onStartLink={setLinkFrom}
              onDeleteEdge={(from, to) =>
                edit({ ...graph, edges: graph.edges.filter((edge) => !(edge.from === from && edge.to === to)) })
              }
              nodeSubtitle={nodeSubtitle}
              kindLabel={kindLabel}
              hint={t("staff.workflow.canvasHint")}
              linkingLabel={t("staff.workflow.linkHint")}
              fitLabel={t("staff.workflow.fitToWindow")}
              documentKey={(selectedId ?? "new") + ":" + String(version)}
            />
            {active === null ? (
              <div className="staff-hint">{t("staff.workflow.noSelection")}</div>
            ) : (
              <NodeInspector
                node={active}
                employees={employees}
                onPatch={(patch) => patchNode(active.id, patch)}
                onConfig={(key, value) => setConfig(active.id, key, value)}
                onRemove={() => removeNode(active.id)}
                t={t}
              />
            )}
          </div>
        ) : null}

        {view === "config" ? (
          <div className="space-y-3">
            {graph.nodes.length === 0 ? (
              <div className="staff-hint">{t("staff.workflow.noSelection")}</div>
            ) : (
              graph.nodes.map((node) => (
                <NodeInspector
                  key={node.id}
                  node={node}
                  employees={employees}
                  onPatch={(patch) => patchNode(node.id, patch)}
                  onConfig={(key, value) => setConfig(node.id, key, value)}
                  onRemove={() => removeNode(node.id)}
                  t={t}
                />
              ))
            )}
          </div>
        ) : null}

        {view === "yaml" ? (
          <pre className="staff-yaml">{toYaml(graph)}</pre>
        ) : null}

        {selectedId === null ? (
          <div className="staff-hint">{t("staff.workflow.firstVersion")}</div>
        ) : null}
      </div>
    </div>
  );
}

