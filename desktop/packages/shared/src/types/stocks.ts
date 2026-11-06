/**
 * 选股神器 (stock picker): A-share market overview, scoring and next-step
 * reminders computed in the Electron main process on top of the official
 * hithink-finance data service (fuyao.aicubes.cn).
 *
 * The engine only returns structured facts and machine keys; every sentence
 * shown to the user is composed in the renderer through i18n, so the catalogs
 * stay the single source of wording.
 */

/** Market sentiment cycle, following the classic A-share emotion model. */
export type StockSentimentPhase = "freeze" | "ebb" | "warm" | "climax";

export type StockThemeHeat = {
  name: string;
  count: number;
};

export type StockLadderRow = {
  board: number;
  names: string[];
};

export type StockIndexQuote = {
  code: string;
  name: string;
  last: number;
  pct: number;
};

export type StockMarketStats = {
  limitUp: number;
  limitDown: number;
  breakBoard: number;
  maxBoard: number;
  /** Failed limit-ups / (failed + sealed), in percent. */
  breakRatePct: number;
  /** Yesterday's consecutive-board stocks that sealed again today, in percent. */
  promotePct: number;
};

export type StockSentiment = {
  score: number;
  phase: StockSentimentPhase;
  /** Suggested total position cap in percent (0-100). */
  maxPositionPct: number;
};

/** How to enter a position; renderer turns each variant into wording. */
export type StockEntryAdvice =
  | { kind: "auction"; lower: number; upper: number; giveUpBelowPct: number }
  | { kind: "pullback"; lower: number; upper: number }
  | { kind: "now"; anchor: number }
  | { kind: "wait"; anchor: number }
  | { kind: "reclaim"; anchor: number };

/** Data-backed evidence chips rendered on a recommendation card. */
export type StockReason =
  | { key: "boards"; count: number }
  | { key: "seal"; amount: number; unit: "yi" | "wan" }
  | { key: "hotRank"; rank: number; up?: number }
  | { key: "dragonNet"; amount: number; org: boolean }
  | { key: "theme"; name: string }
  | { key: "maBull" }
  | { key: "volPosition"; ratio: number; position: number }
  | { key: "heightMember"; count: number };

export type StockStrategy = "all" | "limitup" | "trend" | "dragon";

export type StockAdviceItem = {
  code: string;
  name: string;
  price: number;
  changePct: number;
  score: number;
  strategyKey: "board" | "trend";
  heat: number;
  money: number;
  momentum: number;
  quality: number;
  entry: StockEntryAdvice;
  stop: number;
  target1: number;
  target2: number;
  /** Suggested single-stock position cap in percent; 0 means watch only. */
  positionPct: number;
  horizonKey: "days1to3" | "days3to8";
  reasons: StockReason[];
  sources: string[];
};

export type StockRecommendResult = {
  generatedAt: string;
  strategy: StockStrategy;
  sentiment: StockSentiment;
  stats: StockMarketStats;
  themes: StockThemeHeat[];
  indices: StockIndexQuote[];
  ladder: StockLadderRow[];
  items: StockAdviceItem[];
};

/** Time-boxed playbook entries; the wording lives in the i18n catalogs. */
export type StockSessionKey =
  | "weekend"
  | "preOpen"
  | "auction"
  | "auctionConfirm"
  | "morning"
  | "sentimentCheck"
  | "midday"
  | "afternoon"
  | "close"
  | "review";

export type StockPlanItem =
  | { kind: "session"; sessionKey: StockSessionKey; urgency: "high" | "normal" | "low" }
  | {
      kind: "stopHit";
      code: string;
      name: string;
      price: number;
      stop: number;
      urgency: "high";
    }
  | {
      kind: "targetHit";
      code: string;
      name: string;
      price: number;
      target: number;
      urgency: "high";
    }
  | {
      kind: "track";
      code: string;
      name: string;
      price: number;
      score: number;
      entry: StockEntryAdvice;
      stop: number;
      urgency: "normal";
    };

export type StockMarketPhaseKey = "preOpen" | "morning" | "midday" | "afternoon" | "closed";

export type StockPlan = {
  /** ISO timestamp of when the plan was computed (Asia/Shanghai clock). */
  now: string;
  sessionKey: StockMarketPhaseKey;
  phase: StockSentimentPhase;
  maxPositionPct: number;
  items: StockPlanItem[];
};

export type StockWatchItem = {
  code: string;
  name: string;
  note?: string;
  stop?: number;
  target?: number;
  addedAt?: string;
};

export type StockWatchRow = StockWatchItem & {
  price: number;
  changePct: number;
};

export type StockApiKeyStatus = {
  configured: boolean;
  /** Where the active key came from: the picker settings file or the env var. */
  source: "settings" | "env" | "none";
  keyMasked?: string;
  applyUrl: string;
  envVar: string;
};

export type StockWatchListResult = {
  items: StockWatchRow[];
};
