/**
 * Parity spec for the client Concierge predicates — transcribed 1:1 from v4's
 * `__tests__/unit/lib/services/dangerous-content/chat-override.test.ts` at
 * `ce2f1dabf` (three states since `4d370a90f`, #75), including its TABLE row
 * for row. (`deriveConciergeModeFromLegacy` / `withConciergeModeFromLegacy`
 * are server-only — the Rust chain lane's; their cases are not client ones.)
 *
 * The recorded-oracle half — v4's REAL module over the whole corpus — is
 * `concierge-state.oracle.spec.ts`.
 */
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
  type ConciergeProvenance,
  type ConciergeState,
} from './concierge-state';

type SetBy = Exclude<ConciergeProvenance, null> | null;

const TABLE: Array<{
  mode: ConciergeState;
  setBy: SetBy;
  provenance: ConciergeProvenance;
  uncensoredRoute: boolean;
  dangerStyling: boolean;
  classifierOnDuty: boolean;
  failOver: boolean;
}> = [
  { mode: 'moderated', setBy: null, provenance: null, uncensoredRoute: false, dangerStyling: false, classifierOnDuty: true, failOver: true },
  // A stray provenance on a Moderated row is never reported.
  { mode: 'moderated', setBy: 'operator', provenance: null, uncensoredRoute: false, dangerStyling: false, classifierOnDuty: true, failOver: true },
  { mode: 'unmoderated', setBy: 'operator', provenance: 'operator', uncensoredRoute: true, dangerStyling: true, classifierOnDuty: false, failOver: true },
  { mode: 'unmoderated', setBy: 'concierge', provenance: 'concierge', uncensoredRoute: true, dangerStyling: true, classifierOnDuty: false, failOver: true },
  { mode: 'locked', setBy: 'operator', provenance: 'operator', uncensoredRoute: false, dangerStyling: false, classifierOnDuty: false, failOver: false },
  // A locked row with no provenance still reads as the operator's.
  { mode: 'locked', setBy: null, provenance: 'operator', uncensoredRoute: false, dangerStyling: false, classifierOnDuty: false, failOver: false },
];

describe('CONCIERGE_STATES', () => {
  it('lists the three states in control order', () => {
    expect(CONCIERGE_STATES).toEqual(['moderated', 'unmoderated', 'locked']);
  });
});

describe('getConciergeState', () => {
  it("returns 'moderated' for a null/undefined chat or a NULL column", () => {
    expect(getConciergeState(null)).toBe('moderated');
    expect(getConciergeState(undefined)).toBe('moderated');
    expect(getConciergeState({})).toBe('moderated');
    expect(getConciergeState({ conciergeMode: null })).toBe('moderated');
  });

  it('ignores the legacy pair entirely', () => {
    const legacy = { conciergeOverride: 'UNCENSORED', isDangerousChat: true } as never;
    expect(getConciergeState(legacy)).toBe('moderated');
    expect(shouldUseUncensoredRoute(legacy)).toBe(false);
    expect(shouldShowDangerStyling(legacy)).toBe(false);
  });

  it('reads a server-derived payload (conciergeState) when the column is absent', () => {
    expect(getConciergeState({ conciergeState: 'locked' })).toBe('locked');
    expect(getConciergeProvenance({ conciergeState: 'unmoderated', conciergeSetBy: 'concierge' })).toBe('concierge');
    expect(getConciergeReason({ conciergeState: 'unmoderated', conciergeReason: 'refusals' })).toBe('refusals');
  });

  it('prefers the column over a derived payload value', () => {
    expect(getConciergeState({ conciergeMode: 'moderated', conciergeState: 'locked' })).toBe('moderated');
  });
});

describe.each(TABLE)('mode=$mode setBy=$setBy', (row) => {
  const chat = {
    conciergeMode: row.mode,
    conciergeModeSetBy: row.setBy,
    conciergeModeReason: row.setBy ? ('manual' as const) : null,
  };

  it('derives the state', () => {
    expect(getConciergeState(chat)).toBe(row.mode);
  });
  it('derives the provenance', () => {
    expect(getConciergeProvenance(chat)).toBe(row.provenance);
  });
  it('answers shouldUseUncensoredRoute and its state-only twin', () => {
    expect(shouldUseUncensoredRoute(chat)).toBe(row.uncensoredRoute);
    expect(conciergeStateUsesUncensoredRoute(row.mode)).toBe(row.uncensoredRoute);
  });
  it('answers shouldShowDangerStyling (provenance never changes the colour)', () => {
    expect(shouldShowDangerStyling(chat)).toBe(row.dangerStyling);
  });
  it('answers isClassifierOnDuty', () => {
    expect(isClassifierOnDuty(chat)).toBe(row.classifierOnDuty);
  });
  it('answers mayFailOver and its state-only twin', () => {
    expect(mayFailOver(chat)).toBe(row.failOver);
    expect(conciergeStateMayFailOver(row.mode)).toBe(row.failOver);
  });
});

describe('getConciergeReason', () => {
  it('is null for Moderated, whatever is stored', () => {
    expect(getConciergeReason({ conciergeMode: 'moderated', conciergeModeReason: 'refusals' })).toBeNull();
  });
  it('returns the stored reason otherwise', () => {
    expect(getConciergeReason({ conciergeMode: 'unmoderated', conciergeModeReason: 'classifier' })).toBe('classifier');
  });
});

describe('mayFailOver', () => {
  it('reads a chatless call as Moderated', () => {
    expect(mayFailOver(null)).toBe(true);
    expect(mayFailOver(undefined)).toBe(true);
  });
});
