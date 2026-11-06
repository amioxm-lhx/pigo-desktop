/**
 * The board helpers that are easy to get wrong and invisible when they are.
 *
 * A column label built as a string, a card that says "3 分钟前" about something
 * that happened yesterday, a draft editor that opens three rows on a paragraph
 * -- none of these throw, they just quietly mislead the person reading them.
 */
import assert from "node:assert/strict";
import test from "node:test";
import { register } from "node:module";

register(new URL("./helpers/ts-import-hooks.mjs", import.meta.url));
const {
  agoLabel,
  boardManagerOf,
  boardNameKey,
  contentStatusKey,
  decidedKey,
  delta,
  draftRows,
  leadStageKey,
  liveOrders,
  settledOrders,
  trendPeak,
} =
  await import("../src/features/boards/board-format.ts");

test("a draft opens tall enough to read, and never taller than the cap", () => {
  assert.equal(draftRows(""), 3);
  assert.equal(draftRows("短句。"), 3);
  // A paragraph with no newlines still counts: this is the case that shipped
  // as three rows on every card. Chinese is counted double, so 29 of these
  // fill a line and 88 of them do not fit in three.
  assert.equal(draftRows("啊".repeat(29)), 3);
  assert.equal(draftRows("啊".repeat(88)), 4);
  assert.equal(draftRows("啊".repeat(174)), 6);
  assert.equal(draftRows("啊".repeat(58 * 40)), 14);
  // Latin text is counted singly.
  assert.equal(draftRows("a".repeat(58)), 3);
  assert.equal(draftRows("a".repeat(232)), 4);
  // Explicit line breaks count too, and the taller of the two wins.
  assert.equal(draftRows("a\nb\nc\nd\ne\nf"), 6);
});

test("a timestamp is described in the units it actually has", () => {
  // The helper reads the real clock, so the fixture has to be relative to it.
  const now = Date.now();
  const t = (key, options) => (options && "count" in options ? `${key}:${options.count}` : key);
  assert.equal(agoLabel(new Date(now - 30_000).toISOString(), t), "boards.ago.minutes");
  assert.equal(agoLabel(new Date(now - 3 * 3_600_000).toISOString(), t), "boards.ago.hours:3");
  assert.equal(agoLabel(new Date(now - 50 * 3_600_000).toISOString(), t), "boards.ago.days:2");
  assert.equal(agoLabel(undefined, t), "");
  assert.equal(agoLabel("not a date", t), "");
});

test("yesterday is compared in words, in both directions", () => {
  const t = (key, options) => (options && "count" in options ? `${key}:${options.count}` : key);
  assert.equal(delta(5, 5, t), "boards.delta.flat");
  assert.equal(delta(8, 5, t), "boards.delta.up:3");
  assert.equal(delta(2, 5, t), "boards.delta.down:3");
});

test("one flat week still draws a bar chart", () => {
  assert.equal(trendPeak([]), 1);
  assert.equal(trendPeak([0, 0, 0]), 1);
  assert.equal(trendPeak([3, 9, 4]), 9);
});

test("the dotted keys are assembled in one place", () => {
  assert.equal(leadStageKey("new"), "boards.stage.new");
  assert.equal(contentStatusKey("draft"), "boards.contentStatus.draft");
  assert.equal(decidedKey("rejected"), "boards.advice.decided.rejected");
});

test("a board names the manager that works it", () => {
  const employees = [
    { id: 'emp-1', layer: 'worker', board: '' },
    { id: 'emp-2', layer: 'manager', board: 'leads' },
    { id: 'emp-3', layer: 'manager', board: 'contents' },
  ];
  assert.equal(boardManagerOf(employees, 'leads').id, 'emp-2');
  assert.equal(boardManagerOf(employees, 'contents').id, 'emp-3');
  // The report and the arsenal are read by the owner and run by nobody, so a
  // page asking who manages them has to get an answer it can render as none.
  assert.equal(boardManagerOf(employees, 'report'), undefined);
});

test("orders still being worked are told apart from the ones that are over", () => {
  const orders = ['queued', 'running', 'done', 'failed'].map((status, index) => ({
    id: 'order-' + index,
    status,
  }));
  assert.deepEqual(liveOrders(orders).map((order) => order.status), ['queued', 'running']);
  assert.deepEqual(settledOrders(orders).map((order) => order.status), ['done', 'failed']);
});

test("a board's own name is the one the sidebar already uses", () => {
  assert.equal(boardNameKey('leads'), 'boards.leads.nav');
  assert.equal(boardNameKey('contents'), 'boards.contents.nav');
});

