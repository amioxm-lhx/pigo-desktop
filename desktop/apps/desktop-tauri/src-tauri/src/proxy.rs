//! Pure proxy channels: renderer payload goes to the backend verbatim.
//! Extracted from the Electron `ipc/` domain files — every entry here was a
//! `handle(IPC.invoke.x, (input) => backend.call("method", input))` one-liner.

/// (invoke channel, backend label, backend method)
pub const PROXY_HOST: &[(&str, &str)] = &[
    // app
    ("pi-desktop/app/health", "app.health"),
    ("pi-desktop/app/getOnboarding", "app.getOnboarding"),
    // notification
    ("pi-desktop/notification/list", "notification.list"),
    ("pi-desktop/notification/markRead", "notification.markRead"),
    (
        "pi-desktop/notification/markAllRead",
        "notification.markAllRead",
    ),
    ("pi-desktop/notification/clear", "notification.clear"),
    // session (pure rows)
    ("pi-desktop/session/rename", "session.rename"),
    ("pi-desktop/session/saveRevision", "session.saveRevision"),
    ("pi-desktop/session/listRevisions", "session.listRevisions"),
    (
        "pi-desktop/session/getScratchPath",
        "session.getScratchPath",
    ),
    (
        "pi-desktop/session/collaboration",
        "session.collaboration.status",
    ),
    // project / groups
    ("pi-desktop/project/list", "projects.list"),
    ("pi-desktop/project-group/list", "project.groups.list"),
    ("pi-desktop/project-group/create", "project.group.create"),
    ("pi-desktop/project-group/rename", "project.group.rename"),
    ("pi-desktop/project-group/update", "project.group.update"),
    (
        "pi-desktop/project-group/memory/get",
        "project.group.memory.get",
    ),
    (
        "pi-desktop/project-group/memory/save",
        "project.group.memory.set",
    ),
    (
        "pi-desktop/project-group/instructions/get",
        "project.group.instructions.get",
    ),
    (
        "pi-desktop/project-group/instructions/save",
        "project.group.instructions.set",
    ),
    ("pi-desktop/project/clear", "workspace.clear"),
    ("pi-desktop/project/memory/get", "project.memory.get"),
    ("pi-desktop/project/memory/save", "project.memory.set"),
    // workspace panel
    ("pi-desktop/workspace/review/rollback", "review.rollback"),
    (
        "pi-desktop/stats/getTokenUsageHistory",
        "stats.getTokenUsageHistory",
    ),
    // scheduled
    ("pi-desktop/scheduled/list", "scheduled.list"),
    ("pi-desktop/scheduled/create", "scheduled.create"),
    ("pi-desktop/scheduled/update", "scheduled.update"),
    ("pi-desktop/scheduled/delete", "scheduled.delete"),
    ("pi-desktop/scheduled/run", "scheduled.run"),
    // secrets
    ("pi-desktop/secrets/set", "secrets.set"),
    ("pi-desktop/secrets/delete", "secrets.delete"),
    ("pi-desktop/secrets/has", "secrets.has"),
    // staff — 44 pure host rows
    ("pi-desktop/staff/presets/list", "staff.presets.list"),
    ("pi-desktop/staff/presets/install", "staff.presets.install"),
    ("pi-desktop/staff/profiles/list", "staff.profiles.list"),
    ("pi-desktop/staff/profiles/create", "staff.profile.create"),
    ("pi-desktop/staff/profiles/update", "staff.profile.update"),
    (
        "pi-desktop/staff/profiles/setSecret",
        "staff.profile.setSecret",
    ),
    ("pi-desktop/staff/profiles/delete", "staff.profile.delete"),
    ("pi-desktop/staff/workflows/list", "staff.workflow.list"),
    ("pi-desktop/staff/workflows/get", "staff.workflow.get"),
    ("pi-desktop/staff/workflows/create", "staff.workflow.create"),
    ("pi-desktop/staff/workflows/update", "staff.workflow.update"),
    (
        "pi-desktop/staff/workflows/versions",
        "staff.workflow.versions",
    ),
    ("pi-desktop/staff/workflows/delete", "staff.workflow.delete"),
    (
        "pi-desktop/staff/workflows/validate",
        "staff.workflow.validate",
    ),
    ("pi-desktop/staff/employees/list", "staff.employee.list"),
    ("pi-desktop/staff/employees/get", "staff.employee.get"),
    ("pi-desktop/staff/employees/create", "staff.employee.create"),
    ("pi-desktop/staff/employees/update", "staff.employee.update"),
    ("pi-desktop/staff/employees/delete", "staff.employee.delete"),
    ("pi-desktop/staff/employees/runs", "staff.employee.runs"),
    (
        "pi-desktop/staff/employees/refreshSchedule",
        "staff.employee.refreshSchedule",
    ),
    ("pi-desktop/staff/memory/query", "staff.memory.query"),
    ("pi-desktop/staff/memory/write", "staff.memory.write"),
    ("pi-desktop/staff/memory/publish", "staff.memory.publish"),
    (
        "pi-desktop/staff/memory/supersede",
        "staff.memory.supersede",
    ),
    ("pi-desktop/staff/memory/retire", "staff.memory.retire"),
    ("pi-desktop/staff/leads/list", "staff.leads.list"),
    ("pi-desktop/staff/leads/save", "staff.lead.save"),
    ("pi-desktop/staff/leads/move", "staff.lead.move"),
    ("pi-desktop/staff/contents/list", "staff.contents.list"),
    ("pi-desktop/staff/contents/save", "staff.content.save"),
    ("pi-desktop/staff/contents/status", "staff.content.status"),
    ("pi-desktop/staff/metrics/list", "staff.metrics.list"),
    ("pi-desktop/staff/metrics/save", "staff.metric.save"),
    ("pi-desktop/staff/report/summary", "staff.report.summary"),
    ("pi-desktop/staff/advice/list", "staff.advice.list"),
    ("pi-desktop/staff/advice/derive", "staff.advice.derive"),
    ("pi-desktop/staff/advice/decide", "staff.advice.decide"),
    ("pi-desktop/staff/directives/list", "staff.directives.list"),
    (
        "pi-desktop/staff/directives/update",
        "staff.directive.update",
    ),
    ("pi-desktop/staff/decisions/list", "staff.decisions.list"),
    ("pi-desktop/staff/data/stats", "staff.data.stats"),
    ("pi-desktop/staff/data/prune", "staff.data.prune"),
    ("pi-desktop/staff/dispatch/scope", "staff.dispatch.scope"),
    // skills
    ("pi-desktop/skill/list", "skills.list"),
    ("pi-desktop/skill/create", "skills.create"),
    ("pi-desktop/skill/update", "skills.update"),
    ("pi-desktop/skill/remove", "skills.remove"),
    ("pi-desktop/skill/setEnabled", "skills.setEnabled"),
    ("pi-desktop/skill/setScope", "skills.setScope"),
    ("pi-desktop/skill/read", "skills.read"),
    // subagents
    ("pi-desktop/subagent/list", "agents.list"),
    ("pi-desktop/subagent/create", "agents.create"),
    ("pi-desktop/subagent/update", "agents.update"),
    ("pi-desktop/subagent/read", "agents.read"),
    ("pi-desktop/subagent/remove", "agents.remove"),
    ("pi-desktop/subagent/setEnabled", "agents.setEnabled"),
    ("pi-desktop/subagent/setScope", "agents.setScope"),
    // market
    ("pi-desktop/market/refresh", "market.refresh"),
    ("pi-desktop/market/search", "market.search"),
    ("pi-desktop/market/getDetail", "market.getDetail"),
    ("pi-desktop/market/install", "market.install"),
    ("pi-desktop/market/checkUpdates", "market.checkUpdates"),
    ("pi-desktop/market/applyUpdates", "market.applyUpdates"),
    // plugins (host registry rows); `plugin/list` is a wrapped channel so the
    // runtime can attach the agent-extension state host-core does not know.
    ("pi-desktop/plugin/setAutoUpdate", "plugins.setAutoUpdate"),
    ("pi-desktop/plugin/enable", "plugins.enable"),
    ("pi-desktop/plugin/disable", "plugins.disable"),
    ("pi-desktop/plugin/uninstall", "plugins.uninstall"),
    ("pi-desktop/plugin/setScope", "plugins.setScope"),
    (
        "pi-desktop/plugin/installFromPath",
        "plugins.installFromPath",
    ),
    (
        "pi-desktop/plugin/installFromPackage",
        "plugins.installFromPackage",
    ),
    // `plugin/loadDev` is a wrapped channel: the shell owns the folder picker.
    // mcp registry
    ("pi-desktop/mcp/list", "mcp.list"),
    ("pi-desktop/mcp/upsert", "mcp.upsert"),
    ("pi-desktop/mcp/remove", "mcp.remove"),
    ("pi-desktop/mcp/setEnabled", "mcp.setEnabled"),
    ("pi-desktop/mcp/setScope", "mcp.setScope"),
    // providers
    ("pi-desktop/providers/create", "providers.create"),
    ("pi-desktop/providers/update", "providers.update"),
    ("pi-desktop/providers/delete", "providers.delete"),
    (
        "pi-desktop/providers/testConnection",
        "providers.testConnection",
    ),
    // agent (sidecar-persisted rows mirrored by host)
    ("pi-desktop/plans/pending", "plans.pending"),
];

/// (invoke channel, sidecar method)
pub const PROXY_SIDECAR: &[(&str, &str)] = &[
    ("pi-desktop/agent/stop", "agent.stop"),
    ("pi-desktop/agent/getStatus", "agent.getStatus"),
    // /rebuild: pure forward, the sidecar owns the live runtime registry.
    ("pi-desktop/agent/rebuild", "agent.rebuild"),
    // /memory: pure forward. Reading the persistent-memory index needs no shell
    // state; the sidecar owns the store path resolution.
    ("pi-desktop/memory/report", "memory.report"),
    ("pi-desktop/agent/askTool/resolve", "asktool.resolve"),
    // `agent.btw` is a wrapped channel (it needs the resolved launch binding);
    // only its cancel is a verbatim forward.
    ("pi-desktop/agent/btwCancel", "agent.btwCancel"),
];

/// Methods the sidecar may ask us to forward to host-core via `host.proxy`
/// (mirrors HOST_PROXY_ALLOWED in agent-sidecar.ts).
pub const HOST_PROXY_ALLOWED: &[&str] = &[
    // Answered by the Tauri shell itself, never forwarded to host-core.
    "provider.resolveAuth",
    "tools.execute",
    "tools.abort",
    "tools.list",
    "session.get",
    "session.list",
    "session.appendMessage",
    "session.appendCompaction",
    "session.replaceMessages",
    "workspace.get",
    "plans.enter",
    "plans.submit",
    "plans.pending",
    "plans.abort",
    "app.health",
    "session.rename",
    "session.create",
    "session.fork",
];
