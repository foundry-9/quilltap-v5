/**
 * Autonomous-rooms pure logic (v4 `components/new-chat/AutonomousRoomCard.tsx`,
 * `components/tools/autonomous-rooms-card.tsx`,
 * `components/layout/autonomous-room-badges.tsx`, `EditEnclaveModal.tsx`, and the
 * `useNewChat` submit mapping). The load-bearing transforms — the human-unit form
 * state, the ms⇄human conversions, the budget summaries/readouts, the badge label
 * math, the New-Chat create-payload mapping, and the two poll filters — pulled out
 * as pure functions so they carry v4's exact strings/decisions under unit test
 * (tier-4's one differential class). Unit conversion to/from milliseconds lives
 * ONLY here and at each caller's API boundary, never inside a component.
 */

import type {
  AutonomousRoomStatusDto,
  AutonomousRoomSummary,
  AutonomousRoomSettingsPatch,
  ChatCreateRequest,
} from '../core/core-contract';

export const MS_PER_HOUR = 60 * 60 * 1000;
export const MS_PER_MINUTE = 60 * 1000;

/**
 * The autonomous-room creation slice on the New-Chat form (v4
 * `NewChatAutonomousState`, `components/new-chat/types.ts:132-150`). Only consulted
 * when `enabled` is true. Numeric fields are kept in human-friendly units (hours,
 * minutes) and converted to milliseconds at submit / save time.
 */
export interface NewChatAutonomousState {
  enabled: boolean;
  scheduleCron: string;
  scheduleFreshnessHours: number | null;
  budgetMaxTurns: number | null;
  budgetMaxTokens: number | null;
  budgetMaxWallClockMinutes: number | null;
  budgetEstimatedSpendCapUSD: number | null;
  /** Null = inherit the user-default visibility from chat_settings. */
  runVisibility: 'owner_only' | 'household' | 'open' | null;
  runDestructiveToolsAllowed: boolean;
  /**
   * true (default) = the per-run token budget counts only billable cache-miss
   * input + output tokens (prompt-cache hits excluded); false = count every token.
   */
  budgetExcludeCacheHits: boolean;
}

/** The pristine autonomous slice (v4 `INITIAL_STATE.autonomous`). */
export const INITIAL_AUTONOMOUS_STATE: NewChatAutonomousState = {
  enabled: false,
  scheduleCron: '',
  scheduleFreshnessHours: null,
  budgetMaxTurns: null,
  budgetMaxTokens: null,
  budgetMaxWallClockMinutes: null,
  budgetEstimatedSpendCapUSD: null,
  runVisibility: null,
  runDestructiveToolsAllowed: false,
  budgetExcludeCacheHits: true,
};

/** The user-level hints that soften the editor card's placeholders (v4 `settingsHint`). */
export interface AutonomousSettingsHint {
  visibilityDefault?: 'owner_only' | 'household' | 'open';
  destructiveToolPolicy?: 'always_refuse' | 'opt_in_per_room';
  defaultFreshnessHours?: number;
}

// ===========================================================================
// The Edit-Enclave modal ⇄ status conversions (v4 `EditEnclaveModal`)
// ===========================================================================

/**
 * Convert the status snapshot (milliseconds) into the human-unit form state the
 * card edits (v4 `statusToFormState`, `EditEnclaveModal.tsx:54-73`).
 */
export function statusToFormState(s: AutonomousRoomStatusDto): NewChatAutonomousState {
  return {
    enabled: true,
    scheduleCron: s.scheduleCron ?? '',
    scheduleFreshnessHours:
      s.scheduleFreshnessWindowMs == null
        ? null
        : Math.round(s.scheduleFreshnessWindowMs / MS_PER_HOUR),
    budgetMaxTurns: s.budgetMaxTurns ?? null,
    budgetMaxTokens: s.budgetMaxTokens ?? null,
    budgetMaxWallClockMinutes:
      s.budgetMaxWallClockMs == null ? null : Math.round(s.budgetMaxWallClockMs / MS_PER_MINUTE),
    budgetEstimatedSpendCapUSD: s.budgetEstimatedSpendCapUSD ?? null,
    runVisibility: (s.runVisibility as NewChatAutonomousState['runVisibility']) ?? null,
    runDestructiveToolsAllowed: s.runDestructiveToolsAllowed === 1,
    // Default to excluding cache hits when the column is null/absent.
    budgetExcludeCacheHits: (s.budgetExcludeCacheHits ?? 1) === 1,
  };
}

/**
 * Convert the card's human units back into the update-settings patch the API/DB
 * speak (milliseconds) (v4 `EditEnclaveModal.handleSave` body, `:162-179`). A
 * `null` clears a previously-set value; a number sets it. Every field is present
 * (the modal patch), so the caller passes the whole bag.
 */
export function formStateToUpdatePatch(
  title: string,
  auto: NewChatAutonomousState,
): AutonomousRoomSettingsPatch {
  return {
    title: title.trim(),
    scheduleCron: auto.scheduleCron.trim(),
    scheduleFreshnessWindowMs:
      auto.scheduleFreshnessHours != null && auto.scheduleFreshnessHours > 0
        ? auto.scheduleFreshnessHours * MS_PER_HOUR
        : null,
    budgetMaxTurns: auto.budgetMaxTurns ?? null,
    budgetMaxTokens: auto.budgetMaxTokens ?? null,
    budgetMaxWallClockMs:
      auto.budgetMaxWallClockMinutes != null && auto.budgetMaxWallClockMinutes > 0
        ? auto.budgetMaxWallClockMinutes * MS_PER_MINUTE
        : null,
    budgetEstimatedSpendCapUSD: auto.budgetEstimatedSpendCapUSD ?? null,
    runVisibility: auto.runVisibility,
    runDestructiveToolsAllowed: auto.runDestructiveToolsAllowed,
    budgetExcludeCacheHits: auto.budgetExcludeCacheHits,
  };
}

// ===========================================================================
// The New-Chat create payload mapping (v4 `useNewChat.ts:736-765`)
// ===========================================================================

/**
 * The autonomous slice of the chat-create request (v4 `useNewChat.ts:736-765`).
 * `chatType='autonomous'`; the cron is trimmed and sent only if non-empty;
 * freshness hours × 3_600_000; wall-clock minutes × 60_000; each cap is sent only
 * when > 0; `runDestructiveToolsAllowed` only when checked; `budgetExcludeCacheHits`
 * is ALWAYS sent (the API treats an omitted value as "exclude cache hits", and only
 * an explicit `false` opts into counting every token).
 */
export function buildAutonomousCreatePatch(
  auto: NewChatAutonomousState,
): Partial<ChatCreateRequest> {
  const patch: Partial<ChatCreateRequest> = { chatType: 'autonomous' };
  const cron = auto.scheduleCron.trim();
  if (cron.length > 0) patch.scheduleCron = cron;
  if (auto.scheduleFreshnessHours != null && auto.scheduleFreshnessHours > 0) {
    patch.scheduleFreshnessWindowMs = auto.scheduleFreshnessHours * MS_PER_HOUR;
  }
  if (auto.budgetMaxTurns != null && auto.budgetMaxTurns > 0) {
    patch.budgetMaxTurns = auto.budgetMaxTurns;
  }
  if (auto.budgetMaxTokens != null && auto.budgetMaxTokens > 0) {
    patch.budgetMaxTokens = auto.budgetMaxTokens;
  }
  if (auto.budgetMaxWallClockMinutes != null && auto.budgetMaxWallClockMinutes > 0) {
    patch.budgetMaxWallClockMs = auto.budgetMaxWallClockMinutes * MS_PER_MINUTE;
  }
  if (auto.budgetEstimatedSpendCapUSD != null && auto.budgetEstimatedSpendCapUSD > 0) {
    patch.budgetEstimatedSpendCapUSD = auto.budgetEstimatedSpendCapUSD;
  }
  if (auto.runVisibility) {
    patch.runVisibility = auto.runVisibility;
  }
  if (auto.runDestructiveToolsAllowed) {
    patch.runDestructiveToolsAllowed = true;
  }
  // Always send the budget counting mode.
  patch.budgetExcludeCacheHits = auto.budgetExcludeCacheHits;
  return patch;
}

// ===========================================================================
// Cron validation (LIVE next-run preview is a LOUD deferral)
// ===========================================================================

/**
 * A minimal five-field cron shape check (v4 uses `croner` for a live next-run
 * preview; the v5 `enclave::cron` port that would compute the same preview is
 * server-side, so the exact next-run BEFORE save is a LOUD DEFERRAL — see the card
 * template). This guards only the field count so the card can flag obvious typos;
 * the authoritative `scheduleNextRunAt` is computed by the server on save.
 */
export function isCronShapeValid(expr: string): boolean {
  const trimmed = expr.trim();
  if (!trimmed) return true; // blank = manual-only, valid
  return trimmed.split(/\s+/).length === 5;
}

// ===========================================================================
// The management-list budget summary (v4 `autonomous-rooms-card.tsx:57-69`)
// ===========================================================================

function localeInt(n: number): string {
  return n.toLocaleString();
}

/** The one-line budget summary on a management row (v4 `summarizeBudget`). */
export function summarizeBudget(room: AutonomousRoomSummary, nowMs: number = Date.now()): string {
  const parts: string[] = [];
  if (room.budgetMaxTurns != null)
    parts.push(`${room.runTurnsConsumed}/${room.budgetMaxTurns} turns`);
  else parts.push(`${room.runTurnsConsumed} turns`);
  if (room.budgetMaxTokens != null) {
    parts.push(`${localeInt(room.runTokensConsumed)}/${localeInt(room.budgetMaxTokens)} tokens`);
  } else {
    parts.push(`${localeInt(room.runTokensConsumed)} tokens`);
  }
  if (room.budgetMaxWallClockMs != null && room.runStartedAt) {
    // Exclude accumulated paused time so the readout matches the budget check.
    const elapsedMs =
      (room.runEndedAt ? Date.parse(room.runEndedAt) : nowMs) -
      Date.parse(room.runStartedAt) -
      (room.runPausedAccumMs ?? 0);
    parts.push(
      `${Math.round(Math.max(0, elapsedMs) / 60000)}/${Math.round(room.budgetMaxWallClockMs / 60000)} min`,
    );
  }
  return parts.join(' · ');
}

/** The management-list poll filter (v4 `:133-136`): cron rooms always shown, else idle/running/paused. */
export function filterManagementRooms(rooms: AutonomousRoomSummary[]): AutonomousRoomSummary[] {
  return rooms.filter((room) => {
    if (room.scheduleCron) return true;
    return room.runState === 'idle' || room.runState === 'running' || room.runState === 'paused';
  });
}

// ===========================================================================
// The toolbar badges (v4 `autonomous-room-badges.tsx`)
// ===========================================================================

/** The badge poll filter (v4 `:195-197`): idle/running/paused only — narrower than the list's. */
export function filterBadgeRooms(rooms: AutonomousRoomSummary[]): AutonomousRoomSummary[] {
  return rooms.filter(
    (r) => r.runState === 'idle' || r.runState === 'running' || r.runState === 'paused',
  );
}

/** v4 `abbreviate`: first letter of each whitespace-split word. */
export function abbreviate(text: string): string {
  return text
    .split(/\s+/)
    .map((word) => word.charAt(0))
    .filter((c) => c.length > 0)
    .join('');
}

function trimZero(s: string): string {
  return s.replace(/\.0$/, '');
}

/** v4 `formatTokens`: 1024-base K/M/B abbreviation. */
export function formatTokens(n: number): string {
  n = Math.max(0, Math.floor(n));
  const K = 1024;
  const M = K * K;
  const G = M * K;
  if (n < K) return String(n);
  if (n < M) {
    const v = n / K;
    return `${v < 10 ? trimZero(v.toFixed(1)) : Math.round(v)}K`;
  }
  if (n < G) {
    const v = n / M;
    return `${v < 10 ? trimZero(v.toFixed(1)) : Math.round(v)}M`;
  }
  const v = n / G;
  return `${v < 10 ? trimZero(v.toFixed(1)) : Math.round(v)}B`;
}

/** v4 `formatTime`: `m:ss`. */
export function formatTime(ms: number): string {
  const totalSec = Math.max(0, Math.floor(ms / 1000));
  const mm = Math.floor(totalSec / 60);
  const ss = totalSec % 60;
  return `${mm}:${ss.toString().padStart(2, '0')}`;
}

/** A slim room shape the badge math reads (v4 `AutonomousRoom` badge subset). */
export interface BadgeRoom {
  runState: string | null;
  runStartedAt: string | null;
  runEndedAt: string | null;
  runPausedAccumMs: number;
  runTurnsConsumed: number;
  runTokensConsumed: number;
  budgetMaxTurns: number | null;
  budgetMaxTokens: number | null;
  budgetMaxWallClockMs: number | null;
  title?: string;
  projectName?: string | null;
}

/** v4 `timeRemainingMs`: excludes accumulated paused time, matching the budget check. */
export function timeRemainingMs(room: BadgeRoom, nowMs: number): number {
  const max = room.budgetMaxWallClockMs;
  if (!max) return 0;
  if (!room.runStartedAt) return max;
  const start = Date.parse(room.runStartedAt);
  if (Number.isNaN(start)) return max;
  const end =
    room.runState === 'running' ? nowMs : room.runEndedAt ? Date.parse(room.runEndedAt) : nowMs;
  const elapsed = (Number.isNaN(end) ? nowMs : end) - start - (room.runPausedAccumMs ?? 0);
  return Math.max(0, max - Math.max(0, elapsed));
}

export type BudgetReadout =
  | { kind: 'tokens'; label: string; used: number; total: number }
  | { kind: 'messages'; label: string; used: number; total: number }
  | { kind: 'time'; label: string; usedMs: number; totalMs: number }
  | null;

/** v4 `selectBudgetReadout`: priority tokens → turns → time. */
export function selectBudgetReadout(room: BadgeRoom, nowMs: number): BudgetReadout {
  if (room.budgetMaxTokens != null) {
    const total = room.budgetMaxTokens;
    const used = Math.max(0, room.runTokensConsumed ?? 0);
    const remaining = Math.max(0, total - used);
    return { kind: 'tokens', label: formatTokens(remaining), used, total };
  }
  if (room.budgetMaxTurns != null) {
    const total = room.budgetMaxTurns;
    const used = Math.max(0, room.runTurnsConsumed ?? 0);
    const remaining = Math.max(0, total - used);
    return { kind: 'messages', label: String(remaining), used, total };
  }
  if (room.budgetMaxWallClockMs != null) {
    const total = room.budgetMaxWallClockMs;
    const remainingMs = timeRemainingMs(room, nowMs);
    const usedMs = Math.max(0, total - remainingMs);
    return { kind: 'time', label: formatTime(remainingMs), usedMs, totalMs: total };
  }
  return null;
}

/** v4 `buildLabel`: `project:chat` abbreviation. */
export function buildLabel(room: BadgeRoom): string {
  const chat = abbreviate(room.title || 'untitled');
  if (room.projectName) {
    const project = abbreviate(room.projectName);
    if (project) return `${project}:${chat || '?'}`;
  }
  return chat || '?';
}

/** v4 `buildTooltip`: the four-line hover readout. */
export function buildTooltip(room: BadgeRoom, readout: BudgetReadout): string {
  const projectLine = room.projectName ?? '(no project)';
  const titleLine = room.title || '(untitled autonomous room)';
  const status = room.runState ?? 'idle';
  let limitLine = 'no budget set';
  if (readout?.kind === 'tokens') {
    limitLine = `tokens: ${localeInt(readout.used)}/${localeInt(readout.total)}`;
  } else if (readout?.kind === 'messages') {
    limitLine = `messages: ${readout.used}/${readout.total}`;
  } else if (readout?.kind === 'time') {
    limitLine = `time: ${formatTime(readout.usedMs)}/${formatTime(readout.totalMs)}`;
  }
  return `${projectLine}\n${titleLine}\n${limitLine}\nstatus: ${status}`;
}
