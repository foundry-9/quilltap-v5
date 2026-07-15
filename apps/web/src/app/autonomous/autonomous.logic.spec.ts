import { describe, expect, it } from 'vitest';

import type { AutonomousRoomStatusDto, AutonomousRoomSummary } from '../core/core-contract';
import {
  INITIAL_AUTONOMOUS_STATE,
  buildAutonomousCreatePatch,
  buildLabel,
  filterBadgeRooms,
  filterManagementRooms,
  formStateToUpdatePatch,
  formatTime,
  formatTokens,
  selectBudgetReadout,
  statusToFormState,
  summarizeBudget,
  timeRemainingMs,
  tryCronNextRun,
  type BadgeRoom,
  type NewChatAutonomousState,
} from './autonomous.logic';

function summary(over: Partial<AutonomousRoomSummary> = {}): AutonomousRoomSummary {
  return {
    id: 'r1',
    title: 'Room One',
    projectId: null,
    projectName: null,
    participants: [],
    runState: 'idle',
    runStateMessage: null,
    currentRunId: null,
    runStartedAt: null,
    runEndedAt: null,
    runPausedAccumMs: 0,
    runTurnsConsumed: 0,
    runTokensConsumed: 0,
    scheduleCron: null,
    scheduleNextRunAt: null,
    scheduleLastRunAt: null,
    scheduleFreshnessWindowMs: null,
    budgetMaxTurns: null,
    budgetMaxTokens: null,
    budgetMaxWallClockMs: null,
    budgetEstimatedSpendCapUSD: null,
    runDestructiveToolsAllowed: 0,
    runVisibility: null,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...over,
  };
}

function status(over: Partial<AutonomousRoomStatusDto> = {}): AutonomousRoomStatusDto {
  return {
    chatId: 'c1',
    chatType: 'autonomous',
    runState: 'idle',
    currentRunId: null,
    runStateMessage: null,
    runStartedAt: null,
    runEndedAt: null,
    runPausedAccumMs: 0,
    runTurnsConsumed: 0,
    runTokensConsumed: 0,
    scheduleCron: null,
    scheduleNextRunAt: null,
    scheduleLastRunAt: null,
    scheduleFreshnessWindowMs: null,
    budgetMaxTurns: null,
    budgetMaxTokens: null,
    budgetMaxWallClockMs: null,
    budgetEstimatedSpendCapUSD: null,
    budgetExcludeCacheHits: 1,
    runDestructiveToolsAllowed: 0,
    runVisibility: null,
    ...over,
  };
}

function badge(over: Partial<BadgeRoom> = {}): BadgeRoom {
  return {
    runState: 'running',
    runStartedAt: null,
    runEndedAt: null,
    runPausedAccumMs: 0,
    runTurnsConsumed: 0,
    runTokensConsumed: 0,
    budgetMaxTurns: null,
    budgetMaxTokens: null,
    budgetMaxWallClockMs: null,
    ...over,
  };
}

describe('summarizeBudget', () => {
  it('shows bare counts with no caps', () => {
    expect(summarizeBudget(summary({ runTurnsConsumed: 3, runTokensConsumed: 12345 }))).toBe(
      '3 turns · 12,345 tokens',
    );
  });

  it('shows used/cap for turn and token caps', () => {
    const s = summary({
      runTurnsConsumed: 4,
      budgetMaxTurns: 10,
      runTokensConsumed: 2000,
      budgetMaxTokens: 1000000,
    });
    expect(summarizeBudget(s)).toBe('4/10 turns · 2,000/1,000,000 tokens');
  });

  it('appends a wall-clock leg, excluding paused time', () => {
    const nowMs = Date.parse('2026-01-01T00:10:00.000Z'); // 10 min later
    const s = summary({
      runStartedAt: '2026-01-01T00:00:00.000Z',
      runPausedAccumMs: 2 * 60 * 1000, // 2 min paused
      budgetMaxWallClockMs: 30 * 60 * 1000, // 30 min cap
    });
    // elapsed = 10min - 2min paused = 8 min
    expect(summarizeBudget(s, nowMs)).toContain('8/30 min');
  });
});

describe('statusToFormState (ms → human units)', () => {
  it('converts ms to hours/minutes and normalizes the flags', () => {
    const s = status({
      scheduleCron: '0 4 * * *',
      scheduleFreshnessWindowMs: 6 * 60 * 60 * 1000, // 6 h
      budgetMaxTurns: 20,
      budgetMaxTokens: 500000,
      budgetMaxWallClockMs: 45 * 60 * 1000, // 45 min
      budgetEstimatedSpendCapUSD: 2.5,
      budgetExcludeCacheHits: 0,
      runDestructiveToolsAllowed: 1,
      runVisibility: 'open',
    });
    expect(statusToFormState(s)).toEqual<NewChatAutonomousState>({
      enabled: true,
      scheduleCron: '0 4 * * *',
      scheduleFreshnessHours: 6,
      budgetMaxTurns: 20,
      budgetMaxTokens: 500000,
      budgetMaxWallClockMinutes: 45,
      budgetEstimatedSpendCapUSD: 2.5,
      runVisibility: 'open',
      runDestructiveToolsAllowed: true,
      budgetExcludeCacheHits: false,
    });
  });

  it('defaults budgetExcludeCacheHits to true when the column is null', () => {
    expect(
      statusToFormState(status({ budgetExcludeCacheHits: null as unknown as number }))
        .budgetExcludeCacheHits,
    ).toBe(true);
  });

  it('leaves nullable ms fields as null (no conversion)', () => {
    const f = statusToFormState(status());
    expect(f.scheduleFreshnessHours).toBeNull();
    expect(f.budgetMaxWallClockMinutes).toBeNull();
    expect(f.scheduleCron).toBe('');
  });
});

describe('formStateToUpdatePatch (human units → ms)', () => {
  const base: NewChatAutonomousState = {
    ...INITIAL_AUTONOMOUS_STATE,
    scheduleCron: ' 0 4 * * * ',
    scheduleFreshnessHours: 12,
    budgetMaxTurns: 10,
    budgetMaxWallClockMinutes: 30,
    runVisibility: 'owner_only',
    runDestructiveToolsAllowed: true,
    budgetExcludeCacheHits: false,
  };

  it('trims the title and cron, and converts hours/minutes to ms', () => {
    const p = formStateToUpdatePatch('  My Room ', base);
    expect(p.title).toBe('My Room');
    expect(p.scheduleCron).toBe('0 4 * * *');
    expect(p.scheduleFreshnessWindowMs).toBe(12 * 60 * 60 * 1000);
    expect(p.budgetMaxWallClockMs).toBe(30 * 60 * 1000);
    expect(p.budgetMaxTurns).toBe(10);
    expect(p.runVisibility).toBe('owner_only');
    expect(p.runDestructiveToolsAllowed).toBe(true);
    expect(p.budgetExcludeCacheHits).toBe(false);
  });

  it('clears (null) a cap left blank or non-positive', () => {
    const p = formStateToUpdatePatch('t', {
      ...base,
      scheduleFreshnessHours: null,
      budgetMaxTurns: null,
      budgetMaxTokens: null,
      budgetMaxWallClockMinutes: 0,
      budgetEstimatedSpendCapUSD: null,
    });
    expect(p.scheduleFreshnessWindowMs).toBeNull();
    expect(p.budgetMaxTurns).toBeNull();
    expect(p.budgetMaxTokens).toBeNull();
    expect(p.budgetMaxWallClockMs).toBeNull();
    expect(p.budgetEstimatedSpendCapUSD).toBeNull();
  });

  it('null runVisibility passes through (inherit / clear)', () => {
    expect(formStateToUpdatePatch('t', { ...base, runVisibility: null }).runVisibility).toBeNull();
  });
});

describe('buildAutonomousCreatePatch', () => {
  it('sends chatType and only-when-positive caps; budgetExcludeCacheHits always', () => {
    const patch = buildAutonomousCreatePatch({
      ...INITIAL_AUTONOMOUS_STATE,
      enabled: true,
      scheduleCron: '  0 4 * * *  ',
      scheduleFreshnessHours: 12,
      budgetMaxTurns: 10,
      budgetMaxTokens: 0, // skipped (not > 0)
      budgetMaxWallClockMinutes: 30,
      budgetEstimatedSpendCapUSD: 1.5,
      runVisibility: 'open',
      runDestructiveToolsAllowed: true,
      budgetExcludeCacheHits: false,
    });
    expect(patch).toEqual({
      chatType: 'autonomous',
      scheduleCron: '0 4 * * *',
      scheduleFreshnessWindowMs: 12 * 60 * 60 * 1000,
      budgetMaxTurns: 10,
      budgetMaxWallClockMs: 30 * 60 * 1000,
      budgetEstimatedSpendCapUSD: 1.5,
      runVisibility: 'open',
      runDestructiveToolsAllowed: true,
      budgetExcludeCacheHits: false,
    });
    expect('budgetMaxTokens' in patch).toBe(false);
  });

  it('omits an empty cron, absent visibility, and unchecked destructive; keeps budgetExcludeCacheHits', () => {
    const patch = buildAutonomousCreatePatch({ ...INITIAL_AUTONOMOUS_STATE, enabled: true });
    expect(patch).toEqual({ chatType: 'autonomous', budgetExcludeCacheHits: true });
  });
});

describe('selectBudgetReadout (priority tokens → turns → time)', () => {
  it('prefers tokens when a token cap is set', () => {
    const r = selectBudgetReadout(
      badge({ budgetMaxTokens: 2048, runTokensConsumed: 1024, budgetMaxTurns: 10 }),
      0,
    );
    expect(r).toEqual({ kind: 'tokens', label: '1K', used: 1024, total: 2048 });
  });

  it('falls back to turns when only a turn cap is set', () => {
    const r = selectBudgetReadout(badge({ budgetMaxTurns: 10, runTurnsConsumed: 3 }), 0);
    expect(r).toEqual({ kind: 'messages', label: '7', used: 3, total: 10 });
  });

  it('uses time last, counting down while running', () => {
    const start = '2026-01-01T00:00:00.000Z';
    const nowMs = Date.parse('2026-01-01T00:01:00.000Z'); // 1 min in
    const r = selectBudgetReadout(
      badge({ budgetMaxWallClockMs: 5 * 60 * 1000, runStartedAt: start, runState: 'running' }),
      nowMs,
    );
    expect(r?.kind).toBe('time');
    if (r?.kind === 'time') expect(r.label).toBe('4:00'); // 4 min remaining
  });

  it('returns null with no budget', () => {
    expect(selectBudgetReadout(badge(), 0)).toBeNull();
  });
});

describe('formatTokens / formatTime / timeRemainingMs', () => {
  it('formats tokens on a 1024 base', () => {
    expect(formatTokens(512)).toBe('512');
    expect(formatTokens(1536)).toBe('1.5K');
    expect(formatTokens(1024 * 1024)).toBe('1M');
  });

  it('formats time as m:ss', () => {
    expect(formatTime(65_000)).toBe('1:05');
    expect(formatTime(-5)).toBe('0:00');
  });

  it('excludes accumulated paused time from the remaining clock', () => {
    const start = '2026-01-01T00:00:00.000Z';
    const nowMs = Date.parse('2026-01-01T00:10:00.000Z');
    const remaining = timeRemainingMs(
      badge({
        budgetMaxWallClockMs: 30 * 60 * 1000,
        runStartedAt: start,
        runState: 'running',
        runPausedAccumMs: 5 * 60 * 1000,
      }),
      nowMs,
    );
    // elapsed = 10 - 5 paused = 5 min; remaining = 25 min
    expect(remaining).toBe(25 * 60 * 1000);
  });
});

describe('buildLabel / filters / cron shape', () => {
  it('abbreviates title and project', () => {
    expect(buildLabel(badge({ title: 'The Long Night', projectName: 'Winter Saga' }))).toBe(
      'WS:TLN',
    );
    expect(buildLabel(badge({ title: 'Solo' }))).toBe('S');
  });

  it('management filter keeps cron rooms in any state; badge filter does not', () => {
    const rooms = [
      summary({ id: 'a', runState: 'stopped', scheduleCron: '0 4 * * *' }),
      summary({ id: 'b', runState: 'running' }),
      summary({ id: 'c', runState: 'error' }),
    ];
    expect(filterManagementRooms(rooms).map((r) => r.id)).toEqual(['a', 'b']);
    expect(filterBadgeRooms(rooms).map((r) => r.id)).toEqual(['b']);
  });

  describe('tryCronNextRun (v4 AutonomousRoomCard L30-50)', () => {
    it('previews nothing for a blank expression (manual-only)', () => {
      expect(tryCronNextRun('')).toEqual({ ok: true, next: null });
      expect(tryCronNextRun('   ')).toEqual({ ok: true, next: null });
    });

    it('returns the next fire time for a valid cron, strictly in the future', () => {
      const before = Date.now();
      const result = tryCronNextRun('0 4 * * *');
      expect(result.ok).toBe(true);
      const next = (result as { ok: true; next: Date | null }).next;
      expect(next).toBeInstanceOf(Date);
      expect(next!.getTime()).toBeGreaterThan(before);
      // The daily 4am slot is always within 24h of now.
      expect(next!.getTime() - before).toBeLessThanOrEqual(24 * 60 * 60 * 1000);
      expect(next!.getHours()).toBe(4);
      expect(next!.getMinutes()).toBe(0);
    });

    it('reports croner\'s own message for an unparseable expression', () => {
      const result = tryCronNextRun('nonsense');
      expect(result.ok).toBe(false);
      expect((result as { ok: false; error: string }).error).toBeTruthy();
    });

    it('rejects a four-field expression (croner wants five or six)', () => {
      expect(tryCronNextRun('0 4 * *').ok).toBe(false);
    });

    it('parses a cron that can never fire and previews no next run', () => {
      // Feb 30th parses but never comes around.
      expect(tryCronNextRun('0 0 30 2 *')).toEqual({ ok: true, next: null });
    });
  });
});
