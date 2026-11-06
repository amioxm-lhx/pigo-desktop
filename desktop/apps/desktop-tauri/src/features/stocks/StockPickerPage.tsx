import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
  StockAdviceItem,
  StockApiKeyStatus,
  StockEntryAdvice,
  StockPlan,
  StockPlanItem,
  StockReason,
  StockRecommendResult,
  StockStrategy,
  StockWatchRow,
} from "@pi-desktop/shared";
import { api } from "../../lib/api";
import { useAppStore } from "../../stores/app-store";
import { Badge, Button, Input, Panel } from "../../components/ui";
import { IconActivity, IconRefresh } from "../../components/icons";

const STRATEGIES: Array<{ id: StockStrategy; labelKey: string }> = [
  { id: "all", labelKey: "stocks.strategyTab.all" },
  { id: "limitup", labelKey: "stocks.strategyTab.limitup" },
  { id: "trend", labelKey: "stocks.strategyTab.trend" },
  { id: "dragon", labelKey: "stocks.strategyTab.dragon" },
];

const fmt = (value: number, digits = 2) => value.toFixed(digits);

function signedPct(value: number): string {
  return `${value > 0 ? "+" : ""}${fmt(value)}%`;
}

function Pct({ value, className }: { value: number; className?: string }) {
  return (
    <span className={`${value > 0 ? "stock-up" : value < 0 ? "stock-down" : "text-text-secondary"} ${className ?? ""}`}>
      {signedPct(value)}
    </span>
  );
}

function reasonText(reason: StockReason, t: (key: string, params?: Record<string, unknown>) => string): string {
  switch (reason.key) {
    case "boards":
      return t("stocks.reason.boards", { count: reason.count });
    case "seal":
      return reason.unit === "yi"
        ? t("stocks.reason.sealYi", { amount: reason.amount })
        : t("stocks.reason.sealWan", { amount: reason.amount });
    case "hotRank":
      return reason.up
        ? t("stocks.reason.hotRankUp", { rank: reason.rank, up: reason.up })
        : t("stocks.reason.hotRank", { rank: reason.rank });
    case "dragonNet":
      return reason.org
        ? t("stocks.reason.dragonOrg", { amount: reason.amount })
        : t("stocks.reason.dragon", { amount: reason.amount });
    case "theme":
      return t("stocks.reason.theme", { name: reason.name });
    case "maBull":
      return t("stocks.reason.maBull");
    case "volPosition":
      return t("stocks.reason.volPosition", { ratio: reason.ratio, position: reason.position });
    case "heightMember":
      return t("stocks.reason.heightMember", { count: reason.count });
  }
}

function entryText(entry: StockEntryAdvice, t: (key: string, params?: Record<string, unknown>) => string): string {
  switch (entry.kind) {
    case "auction":
      return t("stocks.entry.auction", {
        lower: fmt(entry.lower),
        upper: fmt(entry.upper),
        giveUp: entry.giveUpBelowPct,
      });
    case "pullback":
      return t("stocks.entry.pullback", { lower: fmt(entry.lower), upper: fmt(entry.upper) });
    case "now":
      return t("stocks.entry.now", { anchor: fmt(entry.anchor) });
    case "wait":
      return t("stocks.entry.wait", { anchor: fmt(entry.anchor) });
    case "reclaim":
      return t("stocks.entry.reclaim", { anchor: fmt(entry.anchor) });
  }
}

function AdviceCard({
  item,
  index,
  onWatch,
}: {
  item: StockAdviceItem;
  index: number;
  onWatch: (code: string) => void;
}) {
  const { t } = useTranslation();
  const scoreTone = item.score >= 75 ? "error" : item.score >= 60 ? "warning" : "neutral";
  return (
    <article className="stock-card">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-text-faint">#{index + 1}</span>
        <Badge tone={scoreTone as "error" | "warning" | "neutral"} className="stock-score">
          {item.score}
        </Badge>
        <span className="text-base-plus font-semibold">{item.name}</span>
        <span className="font-mono text-text-faint">{item.code}</span>
        <span className="font-semibold">{fmt(item.price)}</span>
        <Pct value={item.changePct} className="font-semibold" />
        <Badge tone="neutral">{t(`stocks.strategyLabel.${item.strategyKey}`)}</Badge>
        {item.sources.map((source) => (
          <Badge key={source} tone="neutral">
            {t(`stocks.source.${source}`)}
          </Badge>
        ))}
        <Button
          size="sm"
          variant="secondary"
          className="ml-auto"
          onClick={() => onWatch(item.code)}
        >
          {t("stocks.watchAdd")}
        </Button>
      </div>
      <div className="mt-2 flex flex-wrap gap-1.5">
        {item.reasons.map((reason, reasonIndex) => (
          <span key={reasonIndex} className="stock-chip">
            {reasonText(reason, t)}
          </span>
        ))}
      </div>
      <dl className="stock-plan-grid mt-3">
        <div className="stock-plan-row">
          <dt>{t("stocks.planField.entry")}</dt>
          <dd>{entryText(item.entry, t)}</dd>
        </div>
        <div className="stock-plan-row">
          <dt>{t("stocks.planField.stop")}</dt>
          <dd>{t("stocks.stopLine", { price: fmt(item.stop) })}</dd>
        </div>
        <div className="stock-plan-row">
          <dt>{t("stocks.planField.target")}</dt>
          <dd>
            {t("stocks.targetLine", { t1: fmt(item.target1), t2: fmt(item.target2) })}
          </dd>
        </div>
        <div className="stock-plan-row">
          <dt>{t("stocks.planField.position")}</dt>
          <dd>
            {item.positionPct > 0
              ? t("stocks.positionCap", { pct: item.positionPct })
              : t("stocks.positionWatch")}
          </dd>
        </div>
        <div className="stock-plan-row">
          <dt>{t("stocks.planField.horizon")}</dt>
          <dd>{t(`stocks.horizon.${item.horizonKey}`)}</dd>
        </div>
      </dl>
      <div className="mt-2 text-sm-plus text-text-faint">
        {t("stocks.scoreBreakdown", {
          heat: item.heat,
          money: item.money,
          momentum: item.momentum,
          quality: item.quality,
        })}
      </div>
    </article>
  );
}

function PlanRow({
  item,
  index,
  dateKey,
  entryTextOf,
}: {
  item: StockPlanItem;
  index: number;
  dateKey: string;
  entryTextOf: (entry: StockEntryAdvice) => string;
}) {
  const { t } = useTranslation();
  const storageKey = `stocks-plan-check:${dateKey}:${index}`;
  const [checked, setChecked] = useState(() => localStorage.getItem(storageKey) === "1");
  const toggle = () => {
    const next = !checked;
    setChecked(next);
    if (next) localStorage.setItem(storageKey, "1");
    else localStorage.removeItem(storageKey);
  };

  const title =
    item.kind === "session"
      ? t(`stocks.plan.${item.sessionKey}`)
      : item.kind === "stopHit"
        ? t("stocks.planItem.stopHitTitle", { name: item.name })
        : item.kind === "targetHit"
          ? t("stocks.planItem.targetHitTitle", { name: item.name })
          : t("stocks.planItem.trackTitle", { name: item.name });
  const detail =
    item.kind === "session"
      ? t(`stocks.planDetail.${item.sessionKey}`)
      : item.kind === "stopHit"
        ? t("stocks.planItem.stopHit", { name: item.name, price: fmt(item.price), stop: fmt(item.stop) })
        : item.kind === "targetHit"
          ? t("stocks.planItem.targetHit", { name: item.name, price: fmt(item.price), target: fmt(item.target) })
          : t("stocks.planItem.track", {
              name: item.name,
              code: item.code,
              price: fmt(item.price),
              score: item.score,
              entry: entryTextOf(item.entry),
              stop: fmt(item.stop),
            });
  const urgency =
    item.kind === "session" || "urgency" in item
      ? item.urgency
      : ("normal" as const);

  return (
    <label className={`stock-plan-item stock-plan-${urgency}`}>
      <input type="checkbox" checked={checked} onChange={toggle} />
      <span className="min-w-0">
        <strong>{title}</strong>
        <small>{detail}</small>
      </span>
    </label>
  );
}

export function StockPickerPage() {
  const { t } = useTranslation();
  const showToast = useAppStore((s) => s.showToast);
  const [status, setStatus] = useState<StockApiKeyStatus | null>(null);
  const [overview, setOverview] = useState<StockRecommendResult | null>(null);
  const [plan, setPlan] = useState<StockPlan | null>(null);
  const [watch, setWatch] = useState<StockWatchRow[]>([]);
  const [strategy, setStrategy] = useState<StockStrategy>("all");
  const [loading, setLoading] = useState(false);
  const [keyInput, setKeyInput] = useState("");
  const [watchInput, setWatchInput] = useState("");
  const overviewLoadingRef = useRef(false);

  const loadOverview = useCallback(
    async (next: StockStrategy) => {
      if (overviewLoadingRef.current) return;
      overviewLoadingRef.current = true;
      setLoading(true);
      try {
        setOverview(await api.getStocksOverview(next));
      } catch (error) {
        showToast(error instanceof Error ? error.message : String(error), { variant: "error" });
      } finally {
        overviewLoadingRef.current = false;
        setLoading(false);
      }
    },
    [showToast],
  );

  const loadLight = useCallback(async () => {
    try {
      const [planResult, watchResult] = await Promise.allSettled([api.getStocksPlan(), api.listStocksWatchlist()]);
      if (planResult.status === "fulfilled") setPlan(planResult.value);
      if (watchResult.status === "fulfilled") setWatch(watchResult.value.items ?? []);
    } catch {
      // Light refreshes are best effort.
    }
  }, []);

  useEffect(() => {
    void api.getStocksStatus().then((result) => {
      setStatus(result);
      if (result.configured) {
        void loadOverview(strategy);
        void loadLight();
      }
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!status?.configured) return;
    const timer = window.setInterval(() => void loadLight(), 60_000);
    return () => window.clearInterval(timer);
  }, [status?.configured, loadLight]);

  const saveKey = async () => {
    try {
      const next = await api.setStocksApiKey(keyInput.trim());
      setStatus(next);
      setKeyInput("");
      if (next.configured) {
        void loadOverview(strategy);
        void loadLight();
      }
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error), { variant: "error" });
    }
  };

  const addWatch = async (query: string) => {
    const trimmed = query.trim();
    if (!trimmed) return;
    try {
      const result = await api.addStocksWatch({ query: trimmed });
      if (!result.added) {
        showToast(t("stocks.watchDuplicate", { name: result.name }), { variant: "info" });
      }
      setWatchInput("");
      await loadLight();
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error), { variant: "error" });
    }
  };

  const removeWatch = async (code: string) => {
    try {
      await api.removeStocksWatch(code);
      await loadLight();
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error), { variant: "error" });
    }
  };

  const entryTextOf = (entry: StockEntryAdvice) => entryText(entry, t);

  if (status && !status.configured) {
    return (
      <div className="thread-scroll">
        <div className="page-frame">
          <div className="page-header">
            <div>
              <h1 className="page-title">{t("stocks.title")}</h1>
            </div>
          </div>
          <Panel className="page-card stock-setup">
            <div className="page-empty-icon">
              <IconActivity size={20} />
            </div>
            <div className="text-base-plus font-medium">{t("stocks.setup.title")}</div>
            <ol className="stock-setup-steps mt-3 text-md text-text-secondary">
              <li>
                {t("stocks.setup.step1")}
                <a href="https://fuyao.aicubes.cn/" target="_blank" rel="noopener noreferrer" className="ml-1 text-accent">
                  fuyao.aicubes.cn
                </a>
              </li>
              <li>{t("stocks.setup.step2", { envVar: status.envVar })}</li>
              <li>{t("stocks.setup.step3")}</li>
            </ol>
            <div className="mt-4 flex max-w-md gap-2">
              <Input
                value={keyInput}
                placeholder={t("stocks.setup.placeholder")}
                onChange={(event) => setKeyInput(event.target.value)}
              />
              <Button variant="primary" disabled={!keyInput.trim()} onClick={() => void saveKey()}>
                {t("stocks.setup.save")}
              </Button>
            </div>
            <p className="mt-3 text-sm-plus text-text-faint">{t("stocks.setup.envHint", { envVar: status.envVar })}</p>
          </Panel>
        </div>
      </div>
    );
  }

  const sentiment = overview?.sentiment;
  const stats = overview?.stats;

  return (
    <div className="thread-scroll">
      <div className="page-frame">
        <div className="page-header">
          <div>
            <h1 className="page-title">{t("stocks.title")}</h1>
            <p className="text-md text-text-secondary">{t("stocks.subtitle")}</p>
          </div>
          <div className="flex gap-2">
            <Button
              variant="secondary"
              disabled={loading}
              onClick={() => {
                void loadOverview(strategy);
                void loadLight();
              }}
            >
              <span className="flex items-center gap-1">
                <IconRefresh size={13} />
                {loading ? t("stocks.loading") : t("stocks.refresh")}
              </span>
            </Button>
          </div>
        </div>

        <section className="stock-dash">
          <Panel className="stock-dash-card">
            <div className="stock-dash-label">{t("stocks.dash.sentiment")}</div>
            {sentiment ? (
              <>
                <div className={`stock-phase stock-phase-${sentiment.phase}`}>
                  {t(`stocks.phase.${sentiment.phase}`)}
                </div>
                <div className="mt-1 text-sm-plus text-text-faint">
                  {t("stocks.dash.score", { score: sentiment.score })}
                </div>
                <p className="mt-1 text-md text-text-secondary">{t(`stocks.phaseTip.${sentiment.phase}`)}</p>
              </>
            ) : (
              <div className="text-text-faint">—</div>
            )}
          </Panel>
          <Panel className="stock-dash-card">
            <div className="stock-dash-label">{t("stocks.dash.board")}</div>
            {stats ? (
              <>
                <div className="stock-metrics">
                  <span className="stock-up font-semibold">{stats.limitUp}</span>
                  <span className="text-text-faint">{t("stocks.dash.limitUp")}</span>
                  <span className="stock-down font-semibold">{stats.limitDown}</span>
                  <span className="text-text-faint">{t("stocks.dash.limitDown")}</span>
                  <span className="font-semibold">{stats.breakBoard}</span>
                  <span className="text-text-faint">{t("stocks.dash.break")}</span>
                  <span className="font-semibold">{t("stocks.dash.boards", { count: stats.maxBoard })}</span>
                  <span className="font-semibold">{fmt(stats.breakRatePct, 1)}%</span>
                  <span className="text-text-faint">{t("stocks.dash.breakRate")}</span>
                  <span className="font-semibold">{fmt(stats.promotePct, 1)}%</span>
                  <span className="text-text-faint">{t("stocks.dash.promote")}</span>
                </div>
                <div className="mt-2 flex flex-wrap gap-1.5">
                  {(overview?.themes ?? []).map((theme) => (
                    <span key={theme.name} className="stock-chip">
                      {theme.name} · {theme.count}
                    </span>
                  ))}
                </div>
              </>
            ) : (
              <div className="text-text-faint">—</div>
            )}
          </Panel>
          <Panel className="stock-dash-card">
            <div className="stock-dash-label">{t("stocks.dash.indices")}</div>
            <div className="flex flex-col gap-1">
              {(overview?.indices ?? []).map((index) => (
                <div key={index.code} className="flex items-baseline gap-2 text-md">
                  <span className="text-text-secondary">{index.name}</span>
                  <span className="ml-auto font-semibold">{fmt(index.last)}</span>
                  <Pct value={index.pct} />
                </div>
              ))}
              {(overview?.indices ?? []).length === 0 ? <div className="text-text-faint">—</div> : null}
            </div>
          </Panel>
          <Panel className="stock-dash-card">
            <div className="stock-dash-label">{t("stocks.dash.ladder")}</div>
            <div className="flex flex-col gap-1">
              {(overview?.ladder ?? []).map((row) => (
                <div key={row.board} className="flex items-baseline gap-2 text-md">
                  <span className="stock-ladder-board">{t("stocks.dash.boardN", { count: row.board })}</span>
                  <span className="min-w-0 truncate">{row.names.join(" · ")}</span>
                </div>
              ))}
              {(overview?.ladder ?? []).length === 0 ? (
                <div className="text-text-faint">{t("stocks.dash.noLadder")}</div>
              ) : null}
            </div>
          </Panel>
        </section>

        <div className="dest-toolbar mt-4">
          <div className="dest-filters" role="tablist" aria-label={t("stocks.strategyLabel.title")}>
            {STRATEGIES.map((entry) => (
              <button
                key={entry.id}
                type="button"
                role="tab"
                aria-selected={strategy === entry.id}
                className={`dest-filter ${strategy === entry.id ? "active" : ""}`}
                onClick={() => {
                  setStrategy(entry.id);
                  void loadOverview(entry.id);
                }}
              >
                {t(entry.labelKey)}
              </button>
            ))}
          </div>
        </div>

        <div className="mt-3 flex flex-col gap-3">
          {overview?.items.map((item, index) => (
            <AdviceCard key={item.code} item={item} index={index} onWatch={(code) => void addWatch(code)} />
          ))}
          {overview && overview.items.length === 0 ? (
            <Panel className="page-card page-empty">
              <div className="page-empty-icon">
                <IconActivity size={20} />
              </div>
              <div className="text-base-plus font-medium">{t("stocks.empty")}</div>
            </Panel>
          ) : null}
          {!overview && loading ? (
            <Panel className="page-card page-empty">
              <div className="text-md text-text-secondary">{t("stocks.loadingLong")}</div>
            </Panel>
          ) : null}
        </div>

        <section className="stock-cols mt-4">
          <Panel className="stock-dash-card">
            <div className="flex items-baseline justify-between gap-2">
              <div className="stock-dash-label">{t("stocks.planTitle")}</div>
              {plan ? (
                <span className="text-sm-plus text-text-faint">
                  {t(`stocks.marketPhase.${plan.sessionKey}`)} ·{" "}
                  {t("stocks.planPosition", { pct: plan.maxPositionPct })}
                </span>
              ) : null}
            </div>
            <div className="mt-2 flex flex-col gap-2">
              {(plan?.items ?? []).map((item, index) => (
                <PlanRow
                  key={`${item.kind}:${"sessionKey" in item ? item.sessionKey : "code" in item ? item.code : index}:${index}`}
                  item={item}
                  index={index}
                  dateKey={(plan?.now ?? "").slice(0, 10)}
                  entryTextOf={entryTextOf}
                />
              ))}
              {plan && plan.items.length === 0 ? <div className="text-text-faint">—</div> : null}
            </div>
          </Panel>
          <Panel className="stock-dash-card">
            <div className="stock-dash-label">{t("stocks.watchTitle")}</div>
            <div className="mt-2 flex gap-2">
              <Input
                value={watchInput}
                placeholder={t("stocks.watchPlaceholder")}
                onChange={(event) => setWatchInput(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") void addWatch(watchInput);
                }}
              />
              <Button variant="secondary" className="whitespace-nowrap" disabled={!watchInput.trim()} onClick={() => void addWatch(watchInput)}>
                {t("stocks.watchAddAction")}
              </Button>
            </div>
            <div className="mt-3 flex flex-col gap-1.5">
              {watch.map((row) => (
                <div key={row.code} className="stock-watch-row">
                  <span className="font-semibold">{row.name}</span>
                  <span className="font-mono text-sm-plus text-text-faint">{row.code}</span>
                  <span className="ml-auto font-semibold">
                    {row.price > 0 ? fmt(row.price) : "—"}
                  </span>
                  {row.price > 0 ? <Pct value={row.changePct} /> : null}
                  <span className="text-sm-plus text-text-faint">
                    {row.stop ? t("stocks.stopShort", { price: fmt(row.stop) }) : ""}
                    {row.stop && row.target ? " / " : ""}
                    {row.target ? t("stocks.targetShort", { price: fmt(row.target) }) : ""}
                  </span>
                  <button
                    type="button"
                    className="icon-btn"
                    aria-label={t("stocks.watchRemove")}
                    onClick={() => void removeWatch(row.code)}
                  >
                    ✕
                  </button>
                </div>
              ))}
              {watch.length === 0 ? <div className="text-text-faint">{t("stocks.watchEmpty")}</div> : null}
            </div>
          </Panel>
        </section>

        <footer className="mt-4 text-center text-sm-plus text-text-faint">
          {t("stocks.disclaimer")}
          {overview ? ` · ${t("stocks.generatedAt", { time: new Date(overview.generatedAt).toLocaleString() })}` : ""}
        </footer>
      </div>
    </div>
  );
}
