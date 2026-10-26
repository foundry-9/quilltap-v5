/**
 * Oracle case (P4.D335): the interrupts' pure + registry half — v4
 * `2b5e8ee24` ("raise a hand or stop from the composer"), at `322a5488e`.
 *
 * Drives the REAL exports of:
 *   - `lib/chat/turn-control.ts` — the ten `turn-control.test.ts` scenarios as
 *     ordered observation rows, with every `[TurnControl]` line captured by
 *     splicing `logger.transports` (chat ids masked to `<chat>`);
 *   - `lib/chat/turn-manager/yield-signal.ts` — `detectYieldSentinel` over the
 *     ten suite forms + the name / Unicode / double-sentinel edges,
 *     `buildTurnYieldInstruction`, the two constants;
 *   - `lib/chat/turn-manager/skip-signal.ts`'s `matchesSentinelLine` — the
 *     wrapper / bracket / punctuation matrix × both phrases (the shared body);
 *   - `lib/llm/stream-watchdog.ts`'s `withStallWatchdog` `abortSignal` — the
 *     event-driven stop rows (incl. the measured `stop_vs_buffered_chunk`) and
 *     the `[LLMStream] … operator's stop` INFO bag (`elapsedMs` masked).
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/turn-control.ts \
 *     > /tmp/oracle-turn-control.ndjson
 */

process.env.LOG_LEVEL = 'debug';

type LogLine = { level: string; message: string; fields: Record<string, unknown> };
type Row =
  | { kind: 'registry'; id: string; obs: unknown[]; logs: LogLine[] }
  | { kind: 'detect'; id: string; response: string; characterName?: string; aliases?: string[]; out: unknown }
  | { kind: 'note'; id: string; operatorName: string; out: string }
  | { kind: 'constants'; id: string; out: { YIELD_SENTINEL: string; TURN_YIELD_SYSTEM_KIND: string } }
  | { kind: 'sentinelLine'; id: string; line: string; phrase: string; out: boolean }
  | { kind: 'watchdog'; id: string; seen: string[]; logs: LogLine[] };

const rows: Row[] = [];
const MASK = '<chat>';
const captured: LogLine[] = [];
let chatIds: string[] = [];

function mask(v: unknown): unknown {
  if (typeof v === 'string' && chatIds.includes(v)) return MASK;
  return v;
}

async function main() {
  const { logger } = await import('@/lib/logger');
  const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
  internal.transports.splice(0, internal.transports.length, {
    write: (d: unknown) => {
      const data = d as { level: string; message: string; context?: Record<string, unknown> };
      if (!data.message.startsWith('[TurnControl]') && !data.message.startsWith('[LLMStream]')) return;
      const { service: _s, module: _m, environment: _e, ...fields } = (data.context ?? {}) as Record<
        string,
        unknown
      >;
      for (const k of Object.keys(fields)) fields[k] = mask(fields[k]);
      if ('elapsedMs' in fields) fields.elapsedMs = '<ms>';
      captured.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
    },
  });

  const tc = await import('@/lib/chat/turn-control');
  const ys = await import('@/lib/chat/turn-manager/yield-signal');
  const ss = await import('@/lib/chat/turn-manager/skip-signal');
  const wd = await import('@/lib/llm/stream-watchdog');

  const latch = () => {
    let open: () => void = () => {};
    const promise = new Promise<void>((resolve) => {
      open = resolve;
    });
    return { promise, open };
  };

  const scenario = async (id: string, chats: string[], body: (obs: unknown[]) => Promise<void>) => {
    chatIds = chats;
    captured.length = 0;
    const obs: unknown[] = [];
    await body(obs);
    rows.push({ kind: 'registry', id, obs: obs.map(mask), logs: captured.splice(0) });
  };

  // ---- the registry (turn-control.test.ts, 1:1, plus two-streams) -----------
  await scenario('no-handle', [], async (obs) => {
    obs.push(tc.currentTurnStopSignal() === undefined, tc.isCurrentTurnStopped(), tc.takeRaisedHand());
  });

  await scenario('lifetime', ['chat-a'], async (obs) => {
    const gate = latch();
    const run = tc.runWithTurnControl('chat-a', async () => {
      obs.push(tc.hasActiveTurn('chat-a'), tc.currentTurnStopSignal()?.aborted);
      await gate.promise;
      return 'done';
    });
    gate.open();
    obs.push(await run, tc.hasActiveTurn('chat-a'));
  });

  await scenario('throws', ['chat-b'], async (obs) => {
    try {
      await tc.runWithTurnControl('chat-b', async () => {
        throw new Error('boom');
      });
      obs.push('resolved');
    } catch (e) {
      obs.push((e as Error).message);
    }
    obs.push(tc.hasActiveTurn('chat-b'));
  });

  await scenario('hand-once', ['chat-c'], async (obs) => {
    const gate = latch();
    const run = tc.runWithTurnControl('chat-c', async () => {
      await gate.promise;
      obs.push(tc.takeRaisedHand(), tc.takeRaisedHand());
    });
    obs.push(tc.setHandRaised('chat-c', true));
    gate.open();
    await run;
  });

  await scenario('lower', ['chat-d'], async (obs) => {
    const gate = latch();
    const run = tc.runWithTurnControl('chat-d', async () => {
      await gate.promise;
      obs.push(tc.takeRaisedHand());
    });
    obs.push(tc.setHandRaised('chat-d', true), tc.setHandRaised('chat-d', false));
    gate.open();
    await run;
  });

  await scenario('idle', ['chat-idle'], async (obs) => {
    obs.push(tc.setHandRaised('chat-idle', true));
  });

  await scenario('stop-settle', ['chat-e'], async (obs) => {
    const seen: unknown[] = [];
    const run = tc.runWithTurnControl('chat-e', async () => {
      const signal = tc.currentTurnStopSignal()!;
      await new Promise<void>((resolve) => signal.addEventListener('abort', () => resolve(), { once: true }));
      seen.push(signal.aborted, tc.isCurrentTurnStopped(), tc.takeRaisedHand());
    });
    obs.push(tc.setHandRaised('chat-e', true));
    obs.push(await tc.stopActiveTurns('chat-e'));
    obs.push(seen);
    await run;
  });

  await scenario('fresh', ['chat-f'], async (obs) => {
    const first = latch();
    const firstRun = tc.runWithTurnControl('chat-f', async () => {
      await first.promise;
    });
    const stopping = tc.stopActiveTurns('chat-f', { settleTimeoutMs: 1000 });
    let secondStopped: boolean | undefined;
    await tc.runWithTurnControl('chat-f', async () => {
      secondStopped = tc.isCurrentTurnStopped();
    });
    first.open();
    obs.push(await stopping);
    await firstRun;
    obs.push(secondStopped);
  });

  await scenario('give-up', ['chat-g'], async (obs) => {
    const stuck = latch();
    const run = tc.runWithTurnControl('chat-g', async () => {
      await stuck.promise;
    });
    obs.push(await tc.stopActiveTurns('chat-g', { settleTimeoutMs: 20 }));
    stuck.open();
    await run;
  });

  await scenario('noop-stop', ['chat-none'], async (obs) => {
    obs.push(await tc.stopActiveTurns('chat-none'));
  });

  await scenario('two-streams', ['chat-h'], async (obs) => {
    const a = latch();
    const b = latch();
    const runA = tc.runWithTurnControl('chat-h', async () => {
      await a.promise;
      obs.push(tc.takeRaisedHand());
    });
    const runB = tc.runWithTurnControl('chat-h', async () => {
      await b.promise;
      obs.push(tc.takeRaisedHand());
    });
    obs.push(tc.hasActiveTurn('chat-h'), tc.setHandRaised('chat-h', true));
    a.open();
    await runA;
    b.open();
    await runB;
    obs.push(tc.hasActiveTurn('chat-h'));
  });

  // ---- detectYieldSentinel --------------------------------------------------
  const detect = (id: string, response: string, characterName?: string, aliases?: string[]) => {
    rows.push({ kind: 'detect', id, response, characterName, aliases, out: ys.detectYieldSentinel(response, characterName, aliases) });
  };
  detect('bare', '[YIELD]');
  for (const [i, form] of ['[yield]', 'YIELD', '**[YIELD]**', '`[Yield]`', '[YIELD].', '_yield!_', '"[YIELD]"', '  [ YIELD ]  '].entries()) {
    detect(`forgiving-${i}`, form);
  }
  detect('prose-after', '[YIELD]\n*She gestures for you to go on.*');
  detect('last-line', '"After you, my dear."\n\n[YIELD]');
  detect('leading-bracketed', '[YIELD] By all means, speak.');
  detect('leading-bracketed-after-you', '[YIELD] After you.');
  detect('name-prefix', 'Alice: [YIELD]', 'Alice');
  detect('name-prefix-alias', '[Countess] [YIELD]', 'Ada', ['Countess']);
  detect('no-name-guard', '[YIELD]', undefined, []);
  detect('empty-name', '[YIELD]', '');
  detect('prose-yield-word', 'I shall not yield the point so easily.');
  detect('prose-yield-word-head', 'Yield to no one, she said, and meant it.');
  detect('bare-yield-own-line', 'Well then.\nyield');
  detect('ordinary', 'The tea is getting cold.');
  detect('empty', '');
  detect('blank', '   \n  ');
  detect('double-sentinel', '[YIELD]\n[NOTHING TO ADD]');
  detect('skip-then-yield', '[NOTHING TO ADD]\n\n[YIELD]');
  detect('middle-line-not', 'One.\n[YIELD]\nTwo.');
  detect('content-block', '[{"type":"text","text":"[YIELD] Do go on."}]');
  detect('whitespace-tail', '[YIELD]   \n\t ');
  detect('leading-blank-lines', '\n\n[YIELD] Go on.\nMore.');
  detect('unicode-feff-before', '﻿[YIELD] Go on.');
  detect('unicode-nbsp-before', ' [YIELD] Go on.');
  detect('unicode-2028-inside', '[ YIELD] Go on.');
  detect('unicode-0085-before', '\u0085[YIELD] Go on.');
  detect('unicode-180e-before', '᠎[YIELD] Go on.');
  detect('unicode-feff-inside', '[YIELD﻿] Go on.');
  detect('long-s', '[YIEſD] Go on.');
  detect('kelvin', '[YIELDK] Go on.');
  detect('wrapped-leading', '**[YIELD]** Pray continue.');
  detect('punct-leading', '[YIELD]... Pray continue.');
  detect('unbracketed-leading', 'YIELD Pray continue.');

  // ---- buildTurnYieldInstruction + constants ---------------------------------
  for (const [id, name] of [['charlie', 'Charlie'], ['the-user', 'The User'], ['unicode', 'Zoë Ōkami'], ['empty', '']] as const) {
    rows.push({ kind: 'note', id, operatorName: name, out: ys.buildTurnYieldInstruction(name) });
  }
  rows.push({
    kind: 'constants',
    id: 'constants',
    out: { YIELD_SENTINEL: ys.YIELD_SENTINEL, TURN_YIELD_SYSTEM_KIND: ys.TURN_YIELD_SYSTEM_KIND },
  });

  // ---- matchesSentinelLine (the shared body) --------------------------------
  const lines = [
    '[NOTHING TO ADD]', 'nothing to add', '**[Nothing To Add]**', '[nothing to add]!?', '"_[NOTHING TO ADD]_"',
    '[YIELD]', 'yield', '**[yield]**.', '`[Yield]`', '~yield~', '[ yield ]', '[YIELD', 'YIELD]', '[[YIELD]]',
    '[YIELD] now', 'I yield', '', '   ', '*_[YIELD]_*', '[nothing, to add]',
  ];
  for (const [i, line] of lines.entries()) {
    for (const phrase of ['nothing to add', 'yield']) {
      rows.push({ kind: 'sentinelLine', id: `line-${i}-${phrase.replace(/ /g, '-')}`, line, phrase, out: ss.matchesSentinelLine(line, phrase) });
    }
  }

  // ---- withStallWatchdog's abortSignal (event-driven, no clock) -------------
  const slow = { firstChunkTimeoutMs: 5000, idleTimeoutMs: 5000 };
  const logContext = { context: 'streaming.service', userId: 'u1', chatId: 'c1', messageId: 'm1' };
  const wdOpts = (signal: AbortSignal) => ({ ...slow, provider: 'DEEPSEEK', modelName: 'deepseek-v4-flash', logContext, abortSignal: signal });
  async function* silentAfter(values: string[]) {
    for (const v of values) yield v;
    await new Promise(() => {});
  }
  const watchdog = async (id: string, body: (seen: string[]) => Promise<void>) => {
    chatIds = [];
    captured.length = 0;
    const seen: string[] = [];
    await body(seen);
    rows.push({ kind: 'watchdog', id, seen, logs: captured.splice(0) });
  };
  await watchdog('mid-stream', async (seen) => {
    const c = new AbortController();
    const done = (async () => {
      for await (const x of wd.withStallWatchdog(silentAfter(['a', 'b']), wdOpts(c.signal))) seen.push(x);
    })();
    await new Promise((r) => setTimeout(r, 10));
    c.abort();
    await done;
  });
  await watchdog('before-first-chunk', async (seen) => {
    const c = new AbortController();
    const done = (async () => {
      for await (const x of wd.withStallWatchdog(silentAfter([]), wdOpts(c.signal))) seen.push(x);
    })();
    await new Promise((r) => setTimeout(r, 15));
    c.abort();
    await done;
  });
  await watchdog('already-stopped', async (seen) => {
    const c = new AbortController();
    c.abort();
    for await (const x of wd.withStallWatchdog(silentAfter(['a']), wdOpts(c.signal))) seen.push(x);
  });
  await watchdog('stop-vs-buffered-chunk', async (seen) => {
    const c = new AbortController();
    for await (const x of wd.withStallWatchdog(silentAfter(['a', 'b', 'c']), wdOpts(c.signal))) {
      seen.push(x);
      if (x === 'a') c.abort();
    }
  });
  await watchdog('never-fires', async (seen) => {
    const c = new AbortController();
    async function* finite() {
      yield 'a';
      yield 'b';
    }
    for await (const x of wd.withStallWatchdog(finite(), wdOpts(c.signal))) seen.push(x);
  });

  for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
  process.exit(0);
}

main().catch((e) => {
  process.stderr.write(String(e?.stack ?? e) + '\n');
  process.exit(1);
});
