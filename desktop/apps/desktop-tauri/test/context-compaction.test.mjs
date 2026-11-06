import {
  readStoreSource,
  readStoreModule,
  readTranscriptSource,
  readMainSource,
  readSharedTypesSource,
} from "./helpers/source-contracts.mjs";
import { readSidecarRuntimeSource } from "./helpers/sidecar-source.mjs";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { loadStyles } from "./helpers/styles.mjs";

const read = (path) => readFile(new URL(path, import.meta.url), "utf8");

const [
  protocol,
  types,
  main,
  api,
  store,
  events,
  runtime,
  commands,
  hostRpc,
  hostPermissions,
  hostSessions,
  hostTranscripts,
  transcript,
  inspector,
  turns,
  styles,
  enLocale,
] = await Promise.all([
  read("../../../packages/shared/src/protocol.ts"),
  readSharedTypesSource(),
  readMainSource(),
  read("../src/lib/api.ts"),
  readStoreSource(),
  readStoreModule("slices/events-slice.ts"),
  readSidecarRuntimeSource(),
  read("../electron/main/builtin-commands.ts"),
  read("../../../crates/host-core/src/rpc/mod.rs"),
  read("../../../crates/host-core/src/permissions.rs"),
  read("../../../crates/host-core/src/sessions.rs"),
  read("../../../crates/host-core/src/transcripts.rs"),
  readTranscriptSource(),
  read("../src/components/ContextUsageInspector.tsx"),
  read("../src/lib/assistant-turns.ts"),
  loadStyles(),
  read("../../../packages/i18n/src/locales/en/index.ts"),
]);

test("context compaction is wired through protocol v11 and the manual IPC path", () => {
  assert.match(protocol, /PROTOCOL_VERSION = 11/);
  assert.match(protocol, /agentCompact:\s*"pi-desktop\/agent\/compact"/);
  assert.match(types, /type ContextCompactionRecord/);
  assert.match(types, /type: "compaction_start"/);
  assert.match(types, /type: "compaction_end"/);
  assert.match(types, /fallback\?: ContextCompactionFallback/);
  assert.match(main, /handle\(IPC\.invoke\.agentCompact/);
  assert.match(
    main,
    /handle\(IPC\.invoke\.agentCompact[\s\S]*activeTurns\.has\(req\.sessionId\)/,
  );
  assert.match(main, /sidecar\.call\("agent\.compact"/);
  assert.match(api, /compact:\s*\(req: AgentCompactRequest\)/);
  assert.match(commands, /slash:\s*"compact"/);
  assert.match(hostRpc, /"session\.appendCompaction"/);
});

test("turn_end remains a per-tool-turn boundary rather than a terminal run state", () => {
  assert.match(
    store,
    /event\.type === "agent_end" \|\|\s*event\.type === "error"/,
  );
  assert.doesNotMatch(
    store,
    /event\.type === "agent_end" \|\|\s*event\.type === "turn_end"/,
  );
  assert.match(store, /case "agent_end":[\s\S]*?set\(\{ isRunning: false \}\)/);
  assert.match(store, /case "turn_end":\s*break/);
});

test("the safe request budget is enforced inline, without the automatic triggers", () => {
  // Ported from the same policy: a checkpoint is priced against the safe request
  // budget before it is installed, and the summary request itself is held to the
  // same budget.
  assert.match(runtime, /func \(s \*Session\) contextBudget\(messages \[\]agent\.AgentMessage\) contextBudget \{/);
  assert.match(runtime, /compacted\.Tokens >= compacted\.HardLimit/);
  assert.match(runtime, /func \(s \*Session\) compactionSummaryWouldExceedBudget\(/);
  assert.match(runtime, /checkpoint truncated: this message crossed the retained context budget/);
  assert.match(runtime, /"CONTEXT_COMPACTION_FAILED", "context compaction failed"/);
  // Not ported yet (PORTING-PI-GO.md, M3 "What remains"): the automatic
  // `threshold` and `overflow` triggers with their `pendingOverflow` latch, the
  // retained-tail recovery family (`fallback: "retained_tail"`), and Codex's
  // parameterless `new_context` tool with its two-tier reminder. An oversized
  // context fails closed as CONTEXT_TOO_LARGE instead.
  assert.doesNotMatch(runtime, /pendingOverflow/);
  // The sidecar header names `new_context` as the one piece of that family it
  // deliberately does not carry.
  assert.match(runtime, /the `new_context` tool, and the compaction settings a prompt/);
  assert.match(runtime, /"CONTEXT_TOO_LARGE"/);
  assert.match(hostPermissions, /"new_context"/);
});

test("every checkpoint is durable, not just the newest one", () => {
  // One transcript row per compaction needs the whole chain to survive a
  // restart, a rewrite, and a fork.
  assert.match(types, /compactions\?: ContextCompactionRecord\[\]/);
  assert.match(hostTranscripts, /pub fn read_compactions\(/);
  assert.match(hostTranscripts, /pub fn write_transcript_with_compactions\(/);
  assert.doesNotMatch(hostTranscripts, /pub fn read_latest_compaction\(/);
  assert.match(hostSessions, /pub compactions: Vec<CompactionRecord>/);
  assert.match(hostSessions, /compaction: compactions\.last\(\)\.cloned\(\)/);
  assert.match(
    hostSessions,
    /\.filter\(\|record\| compaction_valid_for_records\(record, &records\)\)/,
  );
  assert.match(
    hostSessions,
    /\.filter_map\(\|record\| clone_compaction_for_fork\(record, &message_ids, &tool_call_ids\)\)/,
  );
});

test("a checkpoint carries only the active user message past the boundary", () => {
  // The summary covers the whole boundary range. An in-progress turn carries
  // only its latest user message, while a completed turn carries no naked
  // historical user messages into the next task.
  assert.match(runtime, /compactionRetainedUserMessageMaxTokens = 20000/);
  assert.match(runtime, /retentionCompletedTurn = "completed_turn"/);
  assert.match(runtime, /retentionActiveTurn    = "active_turn"/);
  assert.match(runtime, /"retainedTailMode": retentionMode/);
  assert.match(runtime, /func \(s \*Session\) codexShapedPreparation\(/);
  // The split pi's cut point produced is folded back into one contiguous range,
  // and only the active turn keeps a tail.
  assert.match(runtime, /summarize = append\(summarize, preparation\.MessagesToSummarize\.\.\.\)/);
  assert.match(runtime, /summarize = append\(summarize, preparation\.TurnPrefixMessages\.\.\.\)/);
  assert.match(runtime, /summarize = append\(summarize, preparation\.RetainedTail\.\.\.\)/);
  assert.match(runtime, /TurnPrefixMessages:\s+nil,/);
  assert.match(runtime, /IsSplitTurn:\s+false,/);
  assert.match(runtime, /if retentionMode == retentionActiveTurn && latestUser != nil \{/);
  // Newest-first selection with the boundary message truncated, not dropped.
  assert.match(runtime, /func selectRetainedUserMessages\(/);
  assert.match(runtime, /truncateUserMessageForCheckpoint\(message, remaining\)/);
  assert.match(runtime, /for left, right := 0, len\(selected\)-1; left < right; left, right = left\+1, right-1 \{/);
});

test("the no-summary rollover family stays an internal switch", () => {
  // Codex's second path - a fresh context window with no summary request - is
  // only half here: the runtime still reads a `fresh_window` checkpoint when it
  // replays one, but nothing in the port builds one, so the switch is
  // unreachable from settings and i18n (PORTING-PI-GO.md, M3 "What remains").
  assert.match(runtime, /strategy != "fresh_window"/);
  assert.match(runtime, /"strategy":         "summary"/);
  assert.doesNotMatch(runtime, /CONTEXT_ROLLOVER_SUMMARY/);
  assert.doesNotMatch(types, /compactionStrategy/);
  assert.doesNotMatch(enLocale, /compactionStrategy/);
});

test("compaction runs inline at the hard boundary, never ahead of it", () => {
  // Codex has no off-critical-path compaction: the summary is paid for at the
  // turn boundary the user is already waiting on, and only a checkpoint that
  // would not fit the safe budget at all is recovered from.
  assert.match(
    runtime,
    /mustFitSafeBudget := reason == protocol\.CompactionReasonOverflow \|\|\s*\n\s*build\.Budget\.Tokens >= build\.Budget\.HardLimit/,
  );
  // Generation stays separate from installation, so a build that cannot be
  // installed is still a build the caller can report on.
  assert.match(
    runtime,
    /func \(s \*Session\) buildCheckpoint\(ctx context\.Context, retentionMode string\) checkpointBuild \{/,
  );
  assert.match(runtime, /func \(s \*Session\) persistCheckpoint\(/);
});

test("compaction lifecycle keeps the renderer busy until its actual terminal event", () => {
  assert.doesNotMatch(types, /ContextCompactionPhase/);
  assert.match(
    store,
    /event\.type === "compaction_start"\s*\)/,
  );
  assert.match(
    store,
    /event\.type === "compaction_end" && event\.reason === "manual"/,
  );
  assert.match(
    store,
    /case "compaction_start":\s*set\(\{ isRunning: true \}\)/,
  );
  assert.match(
    store,
    /case "compaction_end":[\s\S]*event\.reason === "manual"\) set\(\{ isRunning: false \}\)/,
  );
  assert.match(store, /contextCompaction\.recovered/);
});

test("every compaction announces itself once, on top of the specific toasts", () => {
  const compactionEnd =
    events.match(/case "compaction_end":[\s\S]*?\n        case "agent_end":/)?.[0] ??
    "";
  assert.ok(compactionEnd.length > 0, "compaction_end handler not found");
  // Codex warns after every compaction; ours is unconditional and lands before
  // the three that describe something more specific.
  assert.match(
    compactionEnd,
    /if \(event\.ok\) \{[\s\S]*?get\(\)\.showToast\(i18n\.t\("contextCompaction\.longThreadWarning"\), \{\s*variant: "warning",\s*\}\);/,
  );
  assert.match(
    compactionEnd,
    /if \(event\.fallback\) \{\s*get\(\)\.showToast\(i18n\.t\("contextCompaction\.recovered"\)/,
  );
  assert.match(
    compactionEnd,
    /else if \(event\.reason === "overflow"\) \{\s*get\(\)\.showToast\(\s*i18n\.t\("contextCompaction\.retrying"\)/,
  );
  assert.match(
    compactionEnd,
    /else if \(event\.reason === "manual"\) \{\s*get\(\)\.showToast\(i18n\.t\("contextCompaction\.completed"\)/,
  );
  assert.equal(
    compactionEnd.match(/showToast/g)?.length,
    5,
    "unexpected number of compaction toasts",
  );
  assert.match(enLocale, /longThreadWarning:/);
});

test("a compaction that cannot produce a summary fails instead of installing an empty tail", () => {
  // A retained-tail fallback must not persist an empty tail: with no real
  // summary to carry the boundary, an empty tail restores as an empty context
  // after a runtime rebuild (model switch / restart), and the session would read
  // as if it had just started (#224). The port has no fallback family at all, so
  // a build that cannot produce a summary reports CONTEXT_COMPACTION_FAILED and
  // leaves the previous context in place.
  assert.match(runtime, /func \(s \*Session\) recoverCompactionFailure\(reason string, build checkpointBuild\) bool \{/);
  assert.match(runtime, /failure := &protocol\.CompactionError\{Code: "CONTEXT_COMPACTION_FAILED", Message: message\}/);
  assert.doesNotMatch(runtime, /retained_tail/);
});

test("a fallback notice is stripped without discarding its carried summary", () => {
  // A checkpoint that carried a summary forward ahead of a recovery notice must
  // keep the summary: the next summarization request has to update real history
  // rather than a notice that never was a summary (#224). The port keeps the
  // same extraction helper even though nothing builds such a checkpoint yet.
  assert.match(runtime, /func stripCompactionFallbackNotice\(summary string\) \(string, bool\) \{/);
  assert.match(runtime, /index := strings\.Index\(summary, compactionFallbackMarker\)/);
  assert.match(runtime, /if carried == "" \|\| carried == compactionFallbackNoSummary \{/);
});

test("the transcript shows one row per compaction, the inspector the newest", () => {
  assert.match(types, /type ContextCompactionMark = ContextCompactionStatus & \{/);
  assert.match(types, /mark\?: ContextCompactionMark/);
  // The generation counter and the family both ride inside the opaque details
  // value, so the host persists them without a record schema change.
  assert.match(runtime, /"generation":       s\.nextCheckpointGeneration\(\)/);
  assert.match(runtime, /Mark:\s+contextCompactionMark\(checkpoint\)/);
  // Both the durable records and the live event feed the same per-session list.
  assert.match(store, /sessionCompactions: Record<string, ContextCompactionMark\[\]>/);
  assert.match(store, /rememberSessionCompactions\(id, detail\.session\)/);
  assert.match(store, /event\.type === "compaction_end" && event\.ok && event\.mark/);
  assert.match(store, /withCompactionMark\(/);
  assert.match(store, /withoutRecordKey\(state\.sessionCompactions, id\)/);
  // One transcript row each, anchored on the message the checkpoint covers.
  assert.match(turns, /\{ kind: "compaction"; mark: ContextCompactionMark \}/);
  assert.match(turns, /anchored\.get\(message\.id\)/);
  assert.match(transcript, /function CompactionRow\(\{ mark \}/);
  assert.match(transcript, /chat\.compactionRow/);
  assert.match(transcript, /mark\.summarized/);
  assert.match(transcript, /chat\.compactionRowNoSummary/);
  assert.match(styles, /\.transcript-compaction-row \{/);
  // The inspector keeps its own line, now fed by the newest row.
  assert.match(
    inspector,
    /state\.sessionCompactions\[state\.activeSessionId\]\?\.at\(-1\)/,
  );
  assert.match(inspector, /chat\.usageCompaction/);
  assert.match(enLocale, /usageCompaction:/);
  assert.match(enLocale, /compactionRow:/);
});
