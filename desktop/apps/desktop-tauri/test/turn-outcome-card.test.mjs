import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { loadStyles } from "./helpers/styles.mjs";
import { readComposerSource } from "./helpers/composer-source.mjs";
import { readStoreSource } from "./helpers/store-source.mjs";
import { readTranscriptSource } from "./helpers/transcript-source.mjs";

const read = (path) => readFile(new URL(path, import.meta.url), "utf8");

const [store, transcript, composer, outcome, styles] = await Promise.all([
  readStoreSource(),
  readTranscriptSource(),
  readComposerSource(),
  read("../src/components/TurnOutcomeCard.tsx"),
  loadStyles(),
]);

test("terminal agent events retain a session-scoped result for the transcript", () => {
  assert.match(store, /latestTurnResults: Record<string, AgentTurnResult>/);
  assert.match(store, /status: event\.type === "error" \? "failed" : "completed"/);
  assert.match(store, /turnId:\s*\n\s*envelope\.turnId \?\?/);
  assert.match(
    store,
    /event\.type === "error" && event\.error\.code === "TURN_ABORTED"[\s\S]*?withoutRecordKey\(\s*state\.latestTurnResults/,
  );
  assert.match(transcript, /<TurnOutcomeCard[\s\S]*?result=\{latestTurnResult\}/);
  assert.doesNotMatch(composer, /<TurnOutcomeCard/);
});

test("outcome card exposes one localized continuation action", () => {
  const sendPrompt = store.slice(
    store.indexOf("sendPrompt: async"),
    store.indexOf("compactContext: async"),
  );
  assert.match(outcome, /data-testid="turn-outcome-card"/);
  assert.match(outcome, /result\.status === "completed"/);
  assert.doesNotMatch(outcome, /resultComplete/);
  assert.match(outcome, /resultNeedsAttention/);
  assert.match(outcome, /resultSteps/);
  assert.equal((outcome.match(/<button/g) ?? []).length, 1);
  assert.match(outcome, /sendPrompt\(t\("chat\.continueUnfinishedTaskPrompt"\)\)/);
  assert.match(outcome, /t\("chat\.resultContinue"\)/);
  assert.doesNotMatch(outcome, /retryLastPrompt/);
  assert.doesNotMatch(outcome, /focusComposer/);
  assert.doesNotMatch(outcome, /t\("chat\.retry"\)/);
  assert.doesNotMatch(outcome, /toolWorkPanelTab/);
  assert.match(sendPrompt, /await api\.prompt\(\{[\s\S]*?sessionId,[\s\S]*?content,/);
  assert.match(sendPrompt, /latestTurnResults: withoutRecordKey/);
  assert.doesNotMatch(sendPrompt, /truncateFromMessageId/);
  assert.match(styles, /\.turn-outcome-card\s*\{/);
  assert.match(styles, /\.turn-outcome-card\.failed\s*\{/);
  assert.doesNotMatch(styles, /\.composer-stack > \.turn-outcome-card\s*\{/);
});

test("outcome card yields to an inline assistant error", () => {
  assert.match(
    outcome,
    /const hasInlineError = tail\.some\(\(message\) => Boolean\(message\.error\)\);/,
  );
  assert.match(outcome, /if \(hasInlineError\) return null;/);
});

test("outcome card speaks for a completed turn that produced no answer", () => {
  // The runtime can settle a turn as completed after its last tool call, with
  // no closing assistant text; the settled activity group then folds to one
  // summary line and the transcript reads as if the reply vanished.
  assert.match(outcome, /let lastToolIndex = -1;/);
  assert.match(outcome, /let lastAnswerIndex = -1;/);
  assert.match(outcome, /if \(message\.role === "tool"\) \{/);
  assert.match(outcome, /const answered = lastAnswerIndex > lastToolIndex;/);
  assert.match(
    outcome,
    /if \(result\.status === "completed" && answered\) return null;/,
  );
});

test("narration before the last tool call is not a closing answer", () => {
  // A real turn lands as: user, "let me explore", 38 tool rows, nothing.
  // Counting *any* assistant text as an answer suppressed the card on exactly
  // the turns it exists for, so only text after the last tool call counts.
  assert.doesNotMatch(outcome, /const answered = tail\.some/);
  assert.match(
    outcome,
    /lastToolIndex = index;[\s\S]*?lastAnswerIndex = index;/,
  );
  const answeredIndex = outcome.indexOf("const answered =");
  const cardGate = outcome.indexOf('result.status === "completed" && answered');
  assert.ok(answeredIndex > 0 && cardGate > answeredIndex);
});
