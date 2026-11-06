import { readStoreSource, readTranscriptSource, readMainSource } from "./helpers/source-contracts.mjs";
import { readSidecarRuntimeSource } from "./helpers/sidecar-source.mjs";
import assert from "node:assert/strict";
import test from "node:test";

test("provider failures stay in the transcript as structured assistant messages", async () => {
  const [runtime, store, main] = await Promise.all([
    readSidecarRuntimeSource(),
    readStoreSource(),
    readMainSource(),
  ]);

  // The Go agent loop stamps a classified failure onto the assistant row it
  // emits; the renderer store and Electron main keep reading it off the wire.
  assert.match(runtime, /row\.Error = &appErr\s*\n\s*row\.IsError = true/);
  assert.match(runtime, /if row\.Status == "error" \|\| row\.IsError \{/);
  assert.match(store, /!event\.message\.error/);
  assert.match(store, /assistantErrorMessage\(event\.error\)/);
  assert.match(main, /failed && empty && !event\.message\.error/);
});

test("assistant error messages expose readable provider details and one Continue action", async () => {
  const transcript = await readTranscriptSource();
  const component = transcript.slice(
    transcript.indexOf("function AssistantErrorMessage"),
    transcript.indexOf("const TOOL_ACTION_KEYS"),
  );

  assert.match(component, /function AssistantErrorMessage/);
  assert.match(component, /aria-expanded=\{open\}/);
  assert.match(component, /error\.message/);
  assert.match(component, /message\.providerId/);
  assert.match(component, /message\.modelId/);
  assert.match(component, /copyErrorDetails/);
  assert.doesNotMatch(component, /retryLastPrompt/);
  assert.doesNotMatch(component, /error\.retriable/);
  assert.doesNotMatch(component, /errors\.action\.retry/);
  assert.match(component, /errors\.action\.continue/);
  assert.match(component, /chat\.continueCurrentTaskPrompt/);
  assert.match(component, /setSettingsTab\("agent"\)/);
});
