import { describe, expect, it } from 'vitest';

import {
  CONCIERGE_STATES,
  conciergeStateMayFailOver,
  conciergeStateUsesUncensoredRoute,
  getConciergeProvenance,
  getConciergeReason,
  getConciergeState,
  isClassifierOnDuty,
  mayFailOver,
  shouldShowDangerStyling,
  shouldUseUncensoredRoute,
  type ConciergeChatView,
  type ConciergeState,
} from './concierge-state';
import V4 from './concierge-state.v4.json';

/**
 * The executed-v4 oracle for the client Concierge twin (P4.D229).
 *
 * `concierge-state.v4.json` is emitted by RUNNING v4's real
 * `lib/services/dangerous-content/chat-override.ts` at the round-target pin
 * (`harness/oracle/cases/concierge-chat-override.mjs`) over every mode ×
 * setBy × reason × legacy-pair combination, in both shapes the helpers accept
 * (the stored columns and the derived payload keys) and with the two
 * disagreeing. Every getter's answer is diffed field by field.
 *
 * Regen recipe (the recorder reads through `git show` at the pin):
 *
 * ```bash
 * export PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH
 * QT_V4_PIN=acadcc7cd node ~/source/quilltap-v5/harness/oracle/cases/concierge-chat-override.mjs \
 *   > ~/source/quilltap-v5/apps/web/src/app/chat/concierge-state.v4.json
 * ```
 */

interface Row {
  id: string;
  chat: ConciergeChatView | null | '__undefined__';
  out: {
    state: string;
    provenance: string | null;
    reason: string | null;
    uncensoredRoute: boolean;
    dangerStyling: boolean;
    classifierOnDuty: boolean;
    mayFailOver: boolean;
  };
}

const ROWS = V4.rows as unknown as Row[];

describe('concierge-state agrees with v4 chat-override.ts row for row', () => {
  it('carries the whole recorded corpus, at the round-target pin', () => {
    // A truncated fixture would make the it.each below vacuously green.
    expect(ROWS).toHaveLength(484);
    expect(V4._source.pin).toBe('acadcc7cd');
  });

  it('lists the states in v4 control order', () => {
    expect([...CONCIERGE_STATES]).toEqual(V4.states);
  });

  it.each(V4.stateOnly)('state-only twins — $state', (row) => {
    expect(conciergeStateUsesUncensoredRoute(row.state as ConciergeState)).toBe(row.usesUncensoredRoute);
    expect(conciergeStateMayFailOver(row.state as ConciergeState)).toBe(row.mayFailOver);
  });

  it.each(ROWS.map((r, i) => ({ ...r, n: i })))('#$n $id — $chat', (row) => {
    const chat = row.chat === '__undefined__' ? undefined : row.chat;
    expect({
      state: getConciergeState(chat),
      provenance: getConciergeProvenance(chat),
      reason: getConciergeReason(chat),
      uncensoredRoute: shouldUseUncensoredRoute(chat),
      dangerStyling: shouldShowDangerStyling(chat),
      classifierOnDuty: isClassifierOnDuty(chat),
      mayFailOver: mayFailOver(chat),
    }).toEqual(row.out);
  });
});
