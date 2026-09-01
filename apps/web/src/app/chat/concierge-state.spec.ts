/**
 * Parity spec for the client Concierge predicates — transcribed 1:1 from v4's
 * `__tests__/unit/lib/services/dangerous-content/chat-override.test.ts`
 * (v4 `60e3c4a0a`), including its TABLE row for row.
 *
 * The Rust core's `chat_override` module is the differential-proven authority;
 * this file pins that the client twin cannot drift from it.
 */
import {
  getConciergeState,
  isClassifierOnDuty,
  shouldShowDangerStyling,
  shouldUseUncensoredRoute,
  type ConciergeChatView,
  type ConciergeState,
} from './concierge-state';

interface Row {
  chat: ConciergeChatView;
  state: ConciergeState;
  uncensoredRoute: boolean;
  dangerStyling: boolean;
  classifierOnDuty: boolean;
}

// v4's TABLE, verbatim: the full stored-field truth table across both fields,
// with the preserved isDangerousChat label in each operator position (the label
// must not leak into any predicate).
const TABLE: Row[] = [
  { chat: { conciergeOverride: null, isDangerousChat: false }, state: 'monitored', uncensoredRoute: false, dangerStyling: false, classifierOnDuty: true },
  { chat: { conciergeOverride: null, isDangerousChat: null }, state: 'monitored', uncensoredRoute: false, dangerStyling: false, classifierOnDuty: true },
  { chat: { conciergeOverride: null, isDangerousChat: true }, state: 'flagged', uncensoredRoute: true, dangerStyling: true, classifierOnDuty: true },
  { chat: { conciergeOverride: 'OFF', isDangerousChat: false }, state: 'vouched', uncensoredRoute: false, dangerStyling: false, classifierOnDuty: false },
  { chat: { conciergeOverride: 'OFF', isDangerousChat: true }, state: 'vouched', uncensoredRoute: false, dangerStyling: false, classifierOnDuty: false },
  { chat: { conciergeOverride: 'UNCENSORED', isDangerousChat: false }, state: 'uncensored', uncensoredRoute: true, dangerStyling: false, classifierOnDuty: false },
  { chat: { conciergeOverride: 'UNCENSORED', isDangerousChat: true }, state: 'uncensored', uncensoredRoute: true, dangerStyling: false, classifierOnDuty: false },
];

const label = (r: Row) =>
  `${r.state} (override=${String(r.chat.conciergeOverride)}, dangerous=${String(r.chat.isDangerousChat)})`;

describe('getConciergeState', () => {
  it("returns 'monitored' for a null/undefined chat", () => {
    expect(getConciergeState(null)).toBe('monitored');
    expect(getConciergeState(undefined)).toBe('monitored');
  });

  it("returns 'monitored' when both fields are absent", () => {
    expect(getConciergeState({})).toBe('monitored');
  });

  for (const row of TABLE) {
    it(`derives ${label(row)}`, () => {
      expect(getConciergeState(row.chat)).toBe(row.state);
    });
  }
});

describe('shouldUseUncensoredRoute', () => {
  it('returns false for a null/undefined chat', () => {
    expect(shouldUseUncensoredRoute(null)).toBe(false);
    expect(shouldUseUncensoredRoute(undefined)).toBe(false);
  });

  for (const row of TABLE) {
    it(`returns ${row.uncensoredRoute} for ${label(row)}`, () => {
      expect(shouldUseUncensoredRoute(row.chat)).toBe(row.uncensoredRoute);
    });
  }
});

describe('shouldShowDangerStyling', () => {
  it('returns false for a null/undefined chat', () => {
    expect(shouldShowDangerStyling(null)).toBe(false);
    expect(shouldShowDangerStyling(undefined)).toBe(false);
  });

  for (const row of TABLE) {
    it(`returns ${row.dangerStyling} for ${label(row)}`, () => {
      expect(shouldShowDangerStyling(row.chat)).toBe(row.dangerStyling);
    });
  }

  it('paints danger styling only when the Concierge himself flagged the chat', () => {
    // The two predicates diverge exactly on 'uncensored': routed uncensored,
    // never painted as a hazard.
    const uncensored = TABLE.filter((r) => r.state === 'uncensored');
    expect(uncensored.length).toBeGreaterThan(0);
    for (const row of uncensored) {
      expect(shouldUseUncensoredRoute(row.chat)).toBe(true);
      expect(shouldShowDangerStyling(row.chat)).toBe(false);
    }
  });
});

describe('isClassifierOnDuty', () => {
  it('returns true for a null/undefined chat (nothing has taken the classifier off the case)', () => {
    expect(isClassifierOnDuty(null)).toBe(true);
    expect(isClassifierOnDuty(undefined)).toBe(true);
  });

  for (const row of TABLE) {
    it(`returns ${row.classifierOnDuty} for ${label(row)}`, () => {
      expect(isClassifierOnDuty(row.chat)).toBe(row.classifierOnDuty);
    });
  }
});
