import assert from "node:assert/strict";
import test from "node:test";
import { shouldLeavePreviewForPageChange } from "../src/lib/work-panel-preview.ts";

test("entering preview from a destination that is not chat sticks", () => {
  // Regression: the guard used to read `maximized && page !== "chat"`, so on
  // every non-chat page the toggle flashed the dock open and snapped it back
  // on the next frame.
  assert.equal(
    shouldLeavePreviewForPageChange({
      maximized: true,
      previewPage: "pulls",
      page: "pulls",
    }),
    false,
  );
  assert.equal(
    shouldLeavePreviewForPageChange({
      maximized: true,
      previewPage: "chat",
      page: "chat",
    }),
    false,
  );
});

test("navigating to another destination ends preview so the page can mount", () => {
  assert.equal(
    shouldLeavePreviewForPageChange({
      maximized: true,
      previewPage: "pulls",
      page: "chat",
    }),
    true,
  );
  assert.equal(
    shouldLeavePreviewForPageChange({
      maximized: true,
      previewPage: "chat",
      page: "plugins",
    }),
    true,
  );
});

test("no page change means no reason to leave preview", () => {
  assert.equal(
    shouldLeavePreviewForPageChange({
      maximized: false,
      previewPage: "pulls",
      page: "pulls",
    }),
    false,
  );
  assert.equal(
    shouldLeavePreviewForPageChange({
      maximized: false,
      previewPage: "chat",
      page: "pulls",
    }),
    false,
  );
});
