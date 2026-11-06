/**
 * The resident staff driver, exercised without Electron.
 *
 * host-core's tests cover the durable half of a schedule. This file covers the
 * half the host cannot: turning a claim into agent turns on a timer, with
 * nobody at the keyboard. Every boundary the driver touches -- host RPCs, the
 * sidecar, the renderer bridge, the clock -- is injected, so the unattended
 * rules are asserted rather than assumed.
 */
import assert from "node:assert/strict";
import test from "node:test";
import { register } from "node:module";
import { IPC } from "@pi-desktop/shared";
import { readMainModuleSync } from "./helpers/main-source.mjs";

register(new URL("./helpers/ts-import-hooks.mjs", import.meta.url));
const { createStaffRuntime } = await import("../electron/main/runtime/staff.ts");
const { memoryOperation } = await import("../electron/main/runtime/staff-memory.ts");

/** Let every queued microtask and immediate run to completion. */
async function flush(rounds = 60) {
  for (let index = 0; index < rounds; index += 1) {
    await new Promise((resolve) => setImmediate(resolve));
  }
}

function step(nodeId, kind, extra = {}) {
  return {
    nodeId,
    kind,
    title: kind + ' ' + nodeId,
    instruction: 'Handle ' + nodeId + '.',
    config: {},
    ...extra,
  };
}

function claimFor(employeeId, steps, sessionId = 'session-' + employeeId, runId = 'run-' + employeeId) {
  return {
    runId,
    employeeId,
    employeeName: employeeId,
    sessionId,
    providerId: 'provider-row',
    modelId: 'model-row',
    workflowId: 'wf-1',
    workflowVersion: 3,
    trigger: 'schedule',
    steps,
    startedAt: new Date(0).toISOString(),
  };
}

function harness(options = {}) {
  const state = {
    calls: [],
    prints: [],
    prompts: [],
    finished: [],
    launches: [],
    logs: [],
    releases: 0,
    inFlightAtPrompt: [],
    queue: [...(options.claims ?? [])],
    activeTurns: new Map(),
    runtime: null,
    siteCalls: [],
  };

  // The site browser is the one dependency whose remote effect is a window on
  // screen, so the tests watch what the driver asks of it rather than trust it.
  const siteBrowser = {
    list: async () => {
      state.siteCalls.push({ call: 'list' });
      return (options.sites ?? []).map((site) => ({ ...site }));
    },
    signIn: async (channel) => {
      state.siteCalls.push({ call: 'signIn', channel });
      const site = (options.sites ?? []).find((entry) => entry.channel === channel);
      if (site) site.signedIn = true;
      return { signedIn: true };
    },
    // The driver asks the browser which pages a sweep would open rather than
    // building them itself: the watchlist -- and the rotation over it -- lives
    // with the site registry, so a test that rebuilt it here would be
    // asserting its own copy of the rule.
    urls: (channel) => {
      state.siteCalls.push({ call: 'urls', channel });
      const site = (options.sites ?? []).find((entry) => entry.channel === channel);
      if (options.sweepUrls) return options.sweepUrls;
      return site?.seedUrl ? [site.seedUrl] : [];
    },
    collect: async (channel, urls) => {
      state.siteCalls.push({ call: 'collect', channel, urls });
      // A sweep returns the list page and, below it, the pages the list
      // linked to. The tests that care about depth say so with sitePages.
      if (options.sitePages) return options.sitePages.map((page) => ({ ...page, channel }));
      return [
        {
          channel,
          url: urls[0],
          title: 'read from the site',
          itemCount: 1,
          items: [{ title: 'thread', url: urls[0] + '#1' }],
        },
      ];
    },
  };
  state.siteBrowser = siteBrowser;

  const host = {
    isAvailable: () => options.hostAvailable !== false,
    call: async (method, params) => {
      state.calls.push({ method, params });
      switch (method) {
        case 'staff.scheduler.claim':
          if (options.claimError) throw options.claimError;
          return {
            claims: state.queue.splice(0, options.batchSize ?? 1),
            skipped: options.skipped ?? [],
          };
        case 'staff.run.binding':
          return {
            employeeId: 'emp-from-binding',
            credentialProfileId: 'profile-1',
            providerId: 'provider-employee',
            modelId: 'model-employee',
            thinkingLevel: 'high',
            baseUrl: 'https://employee.example/v1',
            apiKey: 'sk-employee-secret',
          };
        case 'staff.run.step':
          return { run: { id: params.runId } };
        case 'staff.run.finish':
          state.finished.push(params);
          return { run: { id: params.runId, status: params.status } };
        case 'staff.run.start':
          return options.startFor ? options.startFor(params) : {};
        case 'staff.board.read':
          return options.boardBrief ?? { board: params.board, notes: [] };
        case 'settings.get':
          return { settings: {} };
        case 'session.get':
          return { session: { id: params.id } };
        case 'session.beginTurn':
          return { turnId: 'turn-' + params.sessionId };
        case 'session.appendMessage':
          return { ok: true };
        case 'staff.memory.query':
          return {
            items:
              typeof options.memoryItems === 'function'
                ? options.memoryItems(params)
                : options.memoryItems ?? [],
          };
        case 'staff.memory.write':
        case 'staff.memory.publish':
          if (options.memoryWriteError) throw options.memoryWriteError;
          return { item: { id: 'mem-1' } };
        default:
          return {};
      }
    },
  };

  const sidecar = {
    call: async (method, params) => {
      if (method !== 'agent.prompt') return {};
      state.prompts.push(params);
      state.inFlightAtPrompt.push(state.runtime.status().inFlight.map((f) => f.runId));
      const envelope = (event) => ({
        sessionId: params.sessionId,
        turnId: params.turnId,
        event,
      });
      if ((options.turns ?? {})[params.turnId] === 'error') {
        state.runtime.onAgentEvent(
          envelope({ type: 'error', error: { code: 'PROVIDER_QUOTA_EXCEEDED' } }),
        );
      } else {
        state.runtime.onAgentEvent(
          envelope({
            type: 'message_end',
            message: { content: options.replyText ?? 'STEP RESULT: did the thing' },
          }),
        );
        state.runtime.onAgentEvent(envelope({ type: 'agent_end' }));
      }
      return { accepted: true };
    },
  };

  state.runtime = createStaffRuntime({
    runtimeState: { host, sidecar },
    logger: { app: (...args) => state.logs.push(args) },
    sendToRenderer: (channel, payload) => state.prints.push({ channel, payload }),
    activeTurns: state.activeTurns,
    acquireSessionOperation: async () => () => {
      state.releases += 1;
    },
    resolveAgentRuntimeLaunch: async (sessionId, session, settings, overrides) => {
      state.launches.push({ sessionId, overrides });
      return {
        providerId: overrides.providerId ?? 'provider-fallback',
        modelId: overrides.modelId ?? 'model-fallback',
        sidecarParams: { sessionId },
      };
    },
    isQuitting: () => false,
    getSiteBrowser: () => siteBrowser,
  });
  return state;
}

const prober = (channel) =>
  step('scan', 'probe', { config: { sources: [channel] } });

const zhihuSite = () =>
  ({ channel: 'zhihu', supported: true, signedIn: false, seedUrl: 'https://www.zhihu.com/hot', maxUrls: 1, gate: 'login' });

test("a manual sweep opens the site's sign-in window when there is no session", async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const state = harness({
    sites: [zhihuSite()],
    claims: [
      { ...claimFor('emp-1', [prober('zhihu')]), trigger: 'manual' },
    ],
  });
  state.runtime.install();
  t.mock.timers.tick(10_000);
  await flush();

  assert.deepEqual(
    state.siteCalls.map((call) => call.call),
    ['list', 'signIn', 'list', 'urls', 'collect'],
    'the owner is asked to scan, and the sweep runs once the session exists',
  );
  assert.equal(state.siteCalls[1].channel, 'zhihu');
  // The step is handed the page the site actually rendered.
  assert.match(state.prompts[0].content, /read from the site/);
  assert.doesNotMatch(state.prompts[0].content, /skipped/);
  // And the owner is told why a window just appeared.
  assert.ok(
    state.prints.some(
      (entry) => entry.channel === IPC.event.toast && /扫码登录/.test(entry.payload.message),
    ),
  );
});

test("a scheduled sweep never puts a sign-in window on screen", async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const state = harness({ sites: [zhihuSite()], claims: [claimFor('emp-1', [prober('zhihu')])] });
  state.runtime.install();
  t.mock.timers.tick(10_000);
  await flush();

  assert.deepEqual(state.siteCalls.map((call) => call.call), ['list']);
  assert.ok(
    state.prints.every((entry) => entry.channel !== IPC.event.toast),
    'nobody is at the keyboard at 09:30',
  );
  // The model is told the site was skipped, not that the site was empty.
  assert.match(state.prompts[0].content, /skipped: no signed-in session/);
});

test("a sweep hands the step the answers and comments, not just the headlines", async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const state = harness({
    sites: [zhihuSite()],
    sitePages: [
      {
        url: 'https://www.zhihu.com/hot',
        title: '知乎热榜',
        itemCount: 1,
        items: [{ title: '怎样做一人公司', url: 'https://www.zhihu.com/question/1' }],
      },
      {
        url: 'https://www.zhihu.com/question/1',
        title: '怎样做一人公司?',
        itemCount: 2,
        summary: '12 个回答 / 想找一套能自己跑起来的系统',
        items: [
          {
            kind: 'answer',
            title: '张三',
            url: 'https://www.zhihu.com/question/1',
            meta: '赞同 82 · 2026-09-18',
            excerpt: '我用一套定时任务顶了三个人的活',
          },
          { kind: 'comment', title: '李四', url: '', excerpt: '有联系方式吗' },
        ],
      },
    ],
    claims: [{ ...claimFor('emp-1', [prober('zhihu')]), trigger: 'manual' }],
  });
  state.runtime.install();
  t.mock.timers.tick(10_000);
  await flush();

  const prompt = state.prompts[0].content;
  assert.match(prompt, /正文：12 个回答/);
  assert.match(prompt, /- 回答｜张三/);
  assert.match(prompt, /赞同 82 · 2026-09-18/);
  assert.match(prompt, /我用一套定时任务顶了三个人的活/);
  assert.match(prompt, /- 评论｜李四/);
  assert.match(prompt, /有联系方式吗/);
});

test("a site with no collector stays out of the step", async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const state = harness({
    sites: [{ channel: 'v2ex', supported: false, signedIn: false, seedUrl: '', maxUrls: 1, gate: 'none' }],
    claims: [{ ...claimFor('emp-1', [prober('v2ex')]), trigger: 'manual' }],
  });
  state.runtime.install();
  t.mock.timers.tick(10_000);
  await flush();

  assert.deepEqual(state.siteCalls.map((call) => call.call), ['list']);
  assert.doesNotMatch(state.prompts[0].content, /collected:v2ex/);
});

test("a run started from the console carries its trigger into the sweep", async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  // 立即运行 goes through runNow, which builds its own claim instead of taking
  // one from the scheduler. It forgot `trigger` once, and every sweep it started
  // read as scheduled -- so the console could never raise a sign-in window.
  const state = harness({
    sites: [zhihuSite()],
    startFor: () => ({
      run: {
        id: 'run-manual',
        employeeId: 'emp-1',
        sessionId: 'session-manual',
        providerId: 'provider-row',
        modelId: 'model-row',
        workflowId: 'wf-1',
        workflowVersion: 3,
        trigger: 'manual',
        startedAt: new Date(0).toISOString(),
      },
      steps: [prober('zhihu')],
    }),
  });
  state.runtime.install();
  await state.runtime.runNow('emp-1');
  await flush();

  assert.deepEqual(
    state.siteCalls.map((call) => call.call),
    ['list', 'signIn', 'list', 'urls', 'collect'],
    'the console asked for this sweep, so the sign-in window is allowed',
  );
  assert.match(state.prompts[0].content, /read from the site/);
});

const reasonsOf = (state) =>
  state.prints
    .filter((entry) => entry.channel === IPC.event.staffChanged)
    .map((entry) => entry.payload.reason);

test("a due employee runs its pinned steps and pays with its own key", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const state = harness({
    claims: [
      claimFor('emp-1', [
        step('n1', 'probe', {
          instruction: 'Ask the forum who struggles with data boundaries.',
          config: { channel: 'juejin' },
        }),
      ]),
    ],
  });
  state.runtime.install();
  t.mock.timers.tick(10_000);
  await flush();

  assert.deepEqual(state.finished, [{ runId: 'run-emp-1', status: 'succeeded' }]);
  assert.deepEqual(
    state.calls.filter((call) => call.method === 'staff.run.step').map((call) => call.params),
    [
      {
        runId: 'run-emp-1',
        step: 1,
        // result is the STEP RESULT line; report is the full final reply.
        result: 'did the thing',
        report: 'STEP RESULT: did the thing',
      },
    ],
  );
  assert.equal(state.prompts.length, 1);
  assert.match(state.prompts[0].content, /Ask the forum who struggles/);
  assert.match(state.prompts[0].content, /channel: juejin/);
  assert.match(state.prompts[0].content, /STEP RESULT:/);
  assert.equal(state.prompts[0].mode, 'agent');

  // The employee's own credential reaches the launch; the provider row does not.
  assert.deepEqual(state.launches, [
    {
      sessionId: 'session-emp-1',
      overrides: {
        mode: 'agent',
        providerId: 'provider-employee',
        modelId: 'model-employee',
        thinkingLevel: 'high',
        credential: { apiKey: 'sk-employee-secret', baseUrl: 'https://employee.example/v1' },
      },
    },
  ]);

  // The instruction is persisted before it is dispatched, so the session reads
  // as a real conversation when the owner opens it.
  assert.ok(state.calls.some((call) => call.method === 'session.appendMessage'));
  assert.equal(state.releases, 1);
  assert.deepEqual(state.inFlightAtPrompt, [['run-emp-1']]);
  assert.deepEqual(reasonsOf(state), ['claim', 'run', 'idle']);
  const status = state.runtime.status();
  assert.equal(status.resident, true);
  assert.equal(status.claimedTotal, 1);
  assert.deepEqual(status.inFlight, []);
  assert.equal(status.lastError, undefined);
});

test("a human gate aborts an unattended run instead of waiting for a person", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const state = harness({
    claims: [
      claimFor('emp-1', [
        step('n1', 'produce'),
        step('g1', 'humanGate', { config: { reason: 'approve the campaign' } }),
        step('n3', 'notify'),
      ]),
    ],
  });
  state.runtime.install();
  t.mock.timers.tick(10_000);
  await flush();

  assert.deepEqual(state.finished, [
    { runId: 'run-emp-1', status: 'aborted', errorCode: 'HUMAN_GATE' },
  ]);
  // The step before the gate ran; nothing after it did.
  assert.equal(state.prompts.length, 1);
  assert.deepEqual(
    state.calls.filter((call) => call.method === 'staff.run.step').map((call) => call.params.step),
    [1],
  );
  assert.equal(state.runtime.status().inFlight.length, 0);
});

test("a failed turn fails the run with its code and the loop keeps polling", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const state = harness({
    claims: [
      claimFor('emp-1', [step('n1', 'produce')]),
      claimFor('emp-2', [step('n1', 'produce')]),
    ],
    turns: { 'turn-session-emp-1': 'error' },
  });
  state.runtime.install();
  t.mock.timers.tick(10_000);
  await flush();

  assert.deepEqual(state.finished, [
    { runId: 'run-emp-1', status: 'failed', errorCode: 'PROVIDER_QUOTA_EXCEEDED' },
  ]);
  assert.equal(state.runtime.status().resident, true);

  // The next poll still claims the other employee: one bad run is not a stall.
  t.mock.timers.tick(30_000);
  await flush();
  assert.deepEqual(state.finished[1], { runId: 'run-emp-2', status: 'succeeded' });
  assert.equal(state.runtime.status().claimedTotal, 2);
});

test("a handoff starts the target employee and the chain stops at the cap", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const handoff = (to) => step('h1', 'handoff', { handoffEmployeeId: to });
  const started = [];
  const state = harness({
    claims: [claimFor('emp-1', [handoff('emp-2')])],
    startFor: (params) => {
      started.push(params.employeeId);
      const next = 'emp-' + (Number(params.employeeId.split('-')[1]) + 1);
      return {
        run: {
          id: 'run-' + params.employeeId,
          sessionId: 'session-' + params.employeeId,
          providerId: 'provider-employee',
          modelId: 'model-employee',
          workflowId: 'wf-1',
          workflowVersion: 3,
          startedAt: new Date(0).toISOString(),
        },
        steps: [handoff(next)],
      };
    },
  });
  state.runtime.install();
  t.mock.timers.tick(10_000);
  await flush();

  assert.deepEqual(started, ['emp-2', 'emp-3', 'emp-4']);
  assert.deepEqual(
    state.finished.map((entry) => entry.runId),
    ['run-emp-4', 'run-emp-3', 'run-emp-2', 'run-emp-1'],
  );
  assert.ok(state.finished.every((entry) => entry.status === 'succeeded'));
});

test("memory nodes feed the next prompt and record the step result", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const state = harness({
    claims: [
      claimFor('emp-1', [
        step('m1', 'memory', { memory: { operation: 'query', kind: 'fact' } }),
        step('n2', 'produce', { instruction: 'Write the outreach note.' }),
        step('m3', 'memory', { memory: { operation: 'publish', kind: 'signal' } }),
      ]),
    ],
    memoryItems: [{ kind: 'fact', key: 'pricing', text: 'Solo devs care about data staying local' }],
    replyText: 'STEP RESULT: wrote a two-paragraph note',
  });
  state.runtime.install();
  t.mock.timers.tick(10_000);
  await flush();

  // A query step costs no model turn, and what it read reaches the next prompt.
  assert.deepEqual(
    state.calls.filter((call) => call.method === 'staff.memory.query').map((call) => call.params),
    [{ ownerId: 'emp-1', limit: 10, kind: 'fact' }],
  );
  assert.equal(state.prompts.length, 2);
  assert.match(state.prompts[0].content, /What the team already knows:/);
  assert.match(state.prompts[0].content, /- \[fact\] pricing: Solo devs care/);
  // The read node produced no line of its own, so the first model turn has a
  // team memory to work from and nothing behind it; the node that records the
  // run's signal carries what the produce node established.
  assert.doesNotMatch(state.prompts[0].content, /What this run has already done:/);
  assert.match(state.prompts[1].content, /What this run has already done:/);
  assert.match(state.prompts[1].content, /- \[produce n2\] wrote a two-paragraph note/);

  // The recorded item is the model's own final line, with provenance.
  const published = state.calls.find((call) => call.method === 'staff.memory.publish');
  assert.equal(published.params.text, 'wrote a two-paragraph note');
  assert.equal(published.params.sourceRef, 'session-emp-1#m3');
  assert.equal(published.params.scope, 'shared');
  assert.equal(published.params.kind, 'signal');
  assert.deepEqual(state.finished.map((entry) => entry.status), ['succeeded']);
});

test("two read nodes both reach the prompt that follows them", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const state = harness({
    claims: [
      claimFor('emp-1', [
        step('r1', 'memory', { memory: { operation: 'read', kind: 'playbook' } }),
        step('r2', 'memory', { memory: { operation: 'read', kind: 'signal' } }),
        step('n3', 'produce', { instruction: 'Write the placement.' }),
      ]),
    ],
    memoryItems: (params) =>
      params.kind === 'playbook'
        ? [{ kind: 'playbook', key: 'outreach', text: 'Ask one question, never link.' }]
        : [{ kind: 'signal', key: 'thread', text: 'A thread worth answering' }],
    replyText: 'STEP RESULT: wrote the placement',
  });
  state.runtime.install();
  t.mock.timers.tick(10_000);
  await flush();

  // Two reads are two halves of one briefing. The second must not erase the
  // first: a poster told to read both its playbook and the positions worth
  // posting at would otherwise walk off with half of what it was sent to read.
  assert.deepEqual(
    state.calls.filter((call) => call.method === 'staff.memory.query').map((call) => call.params.kind),
    ['playbook', 'signal'],
  );
  assert.match(state.prompts[0].content, /- \[playbook\] outreach: Ask one question, never link\./);
  assert.match(state.prompts[0].content, /- \[signal\] thread: A thread worth answering/);
});

test("a refused memory write is a logged outcome, not a failed run", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const state = harness({
    claims: [
      claimFor('emp-1', [step('m1', 'memory', { memory: { operation: 'write', kind: 'fact' } })]),
    ],
    memoryWriteError: Object.assign(new Error('duplicate item'), {
      data: { errorCode: 'MEMORY_DUPLICATE' },
    }),
  });
  state.runtime.install();
  t.mock.timers.tick(10_000);
  await flush();

  assert.deepEqual(state.finished, [{ runId: 'run-emp-1', status: 'succeeded' }]);
  assert.ok(
    state.logs.some((args) => String(args[2]).includes('memory write was refused')),
  );
});

test("the loop bounds concurrency, skips a busy session, and survives a host fault", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const state = harness({
    claims: [
      claimFor('emp-1', [step('n1', 'produce')]),
      claimFor('emp-2', [step('n1', 'produce')]),
      claimFor('emp-9', [step('n1', 'produce')], 'session-busy', 'run-emp-9'),
    ],
    batchSize: 3,
    skipped: [{ employeeId: 'emp-8', error: 'no pinned workflow version' }],
  });
  state.activeTurns.set('session-busy', 'turn-user');
  state.runtime.install();
  t.mock.timers.tick(10_000);
  await flush();

  assert.equal(
    state.calls.find((call) => call.method === 'staff.scheduler.claim').params.limit,
    2,
  );
  assert.deepEqual(
    state.finished.map((entry) => entry.runId).sort(),
    ['run-emp-1', 'run-emp-2'],
  );
  assert.ok(state.logs.some((args) => String(args[2]).includes('was skipped')));
  assert.equal(state.inFlightAtPrompt.length, 2);

  // No host yet: the loop waits, and says so, instead of throwing.
  const cold = harness({ hostAvailable: false, claims: [claimFor('emp-1', [step('n1', 'produce')])] });
  cold.runtime.install();
  t.mock.timers.tick(10_000);
  await flush();
  assert.equal(cold.calls.filter((call) => call.method === 'staff.scheduler.claim').length, 0);
  assert.equal(cold.runtime.status().resident, true);

  // A failing claim is reported through lastError, which is what the console shows.
  const broken = harness({ claimError: new Error('host closed') });
  broken.runtime.install();
  t.mock.timers.tick(10_000);
  await flush();
  assert.equal(broken.runtime.status().lastError, 'Error: host closed');
  assert.equal(broken.runtime.status().resident, true);
});

/**
 * The staff channels take the runner object, not the getter that returns it.
 *
 * Passing the getter by value reads as an object at the call site, so the
 * failure lands on the owner's click (`runner.runNow is not a function`)
 * instead of on the build. The typed dependency is what makes that a compile
 * error, and this keeps the wiring itself from drifting back.
 */
test("the staff IPC is wired to the runner, not to the getter", () => {
  const register = readMainModuleSync("ipc/register.ts");
  assert.match(register, /getStaffRunner: \(\) => StaffRunner;/);
  assert.match(register, /getRunner: getStaffRunner,/);
  assert.doesNotMatch(register, /getRunner: \(\) =>/);
});

test('a memory read is answered by the driver, never by a model turn', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  // The seeded 选题员 workflow named this operation "read". A read that falls
  // through to a turn costs money and then records whatever the turn said -- the
  // run of 6e6e5672 published a fragment of its own prose that way.
  const state = harness({
    memoryItems: [{ key: 'signal', kind: 'signal', text: '飞书并入豆包：企业权限怎么继承' }],
    claims: [
      claimFor('emp-1', [
        step('recall', 'memory', {
          memory: { operation: 'read', scope: 'shared', kind: 'signal' },
        }),
        step('mine', 'analyze'),
      ]),
    ],
  });
  state.runtime.install();
  t.mock.timers.tick(10_000);
  await flush();

  assert.equal(
    state.calls.filter((call) => call.method === 'staff.memory.query').length,
    1,
    'the read asks the store',
  );
  assert.deepEqual(
    state.calls
      .filter((call) => call.method === 'staff.memory.write' || call.method === 'staff.memory.publish')
      .map((call) => call.method),
    [],
    'a read never writes what it did not produce',
  );
  assert.equal(state.prompts.length, 1, 'only the analyze node asks the model');
  assert.match(
    state.prompts[0].content,
    /飞书并入豆包：企业权限怎么继承/,
    'what the team already knows reaches the next step',
  );
});

test('a publish node records the step result and nothing else', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const state = harness({
    replyText: 'STEP RESULT: 已沉淀 1 条信号。',
    claims: [
      claimFor('emp-1', [
        step('publish', 'memory', {
          memory: { operation: 'publish', scope: 'shared', kind: 'signal' },
        }),
      ]),
    ],
  });
  state.runtime.install();
  t.mock.timers.tick(10_000);
  await flush();

  const published = state.calls.filter((call) => call.method === 'staff.memory.publish');
  assert.equal(published.length, 1);
  assert.equal(published[0].params.text, '已沉淀 1 条信号。');
});

test('both spellings of a memory read mean the same read', () => {
  assert.equal(memoryOperation({ memory: { operation: 'read' } }), 'read');
  assert.equal(memoryOperation({ memory: { operation: 'query' } }), 'read');
  assert.equal(memoryOperation({}), 'read', 'a node that says nothing is a read');
  assert.equal(memoryOperation({ memory: { operation: 'publish' } }), 'publish');
});

test('a board read is answered by the driver, never by a model turn', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  // A manager decides from the rows the owner can see, so the driver hands it
  // the board itself. Letting a model fetch the numbers would cost money and
  // let the picture the manager decides from drift from the one on screen.
  const managerClaim = {
    ...claimFor('emp-manager', [
      step('read', 'board', { config: { action: 'read', board: 'leads' } }),
      step('judge', 'board', { config: { action: 'judge', board: 'leads' } }),
    ]),
    board: 'leads',
  };
  const state = harness({
    boardBrief: { board: 'leads', notes: ['- 待判线索 3 条', '- 昨天 1 条回复'] },
    claims: [managerClaim],
  });
  state.runtime.install();
  t.mock.timers.tick(10_000);
  await flush();

  const read = state.calls.find((call) => call.method === 'staff.board.read');
  assert.ok(read, 'the read asks the host for the board');
  assert.equal(read.params.board, 'leads');
  assert.equal(read.params.employeeId, 'emp-manager');
  assert.equal(state.prompts.length, 1, 'only the judging node asks the model');
  assert.match(state.prompts[0].content, /待判线索 3 条/, 'the rows reach the judgement');
  assert.deepEqual(
    state.calls.filter((call) => call.method === 'staff.run.step').map((call) => call.params.step),
    [1, 2],
    'the read is still a recorded step of the run',
  );
});

test('an order from a manager reaches the worker that has to carry it out', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  // The worker was hired for its own rounds, and an order rides along with the
  // next one. The prompt states it, so the turn answers the manager.
  const state = harness({
    claims: [
      {
        ...claimFor('emp-writer', [step('draft', 'produce')]),
        directive: {
          id: 'order-1',
          verb: 'produce',
          board: 'contents',
          issuer: '内容主编',
          task: '写一篇讲权限边界的长文',
        },
      },
    ],
  });
  state.runtime.install();
  t.mock.timers.tick(10_000);
  await flush();

  assert.equal(state.prompts.length, 1);
  assert.match(state.prompts[0].content, /今天的临时任务（内容主编 派下来的，类型 produce）/);
  assert.match(state.prompts[0].content, /写一篇讲权限边界的长文/);
});

