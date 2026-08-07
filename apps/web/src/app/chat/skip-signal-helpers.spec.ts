import { describe, expect, it } from 'vitest';

import { isUserDrivenSeat } from './skip-signal-helpers';

/**
 * `isUserDrivenSeat` is the client mirror of the core `is_user_driven_seat`
 * (`participant_filters.rs`) and v4's `isUserDrivenSeat`
 * (`lib/chat/turn-manager/utils.ts`). The impersonation-overlay arm (v4 Bug 44:
 * `controlledBy` stays `'llm'` while the id sits in
 * `impersonatingParticipantIds`) is the one that healed the missing "type as
 * them" prompt + Skip button on an impersonated seat's paused turn.
 */
describe('isUserDrivenSeat', () => {
  it('is true for a genuine user-controlled seat', () => {
    expect(isUserDrivenSeat('p1', 'user', null)).toBe(true);
    expect(isUserDrivenSeat('p1', 'user', ['p2'])).toBe(true);
  });

  it('is false for a plain LLM seat with no overlay', () => {
    expect(isUserDrivenSeat('p1', 'llm', null)).toBe(false);
    expect(isUserDrivenSeat('p1', 'llm', [])).toBe(false);
    expect(isUserDrivenSeat('p1', 'llm', ['p2'])).toBe(false);
  });

  it('is true for an LLM seat the human is impersonating (the overlay arm)', () => {
    expect(isUserDrivenSeat('p1', 'llm', ['p1'])).toBe(true);
    expect(isUserDrivenSeat('p1', 'llm', ['p2', 'p1'])).toBe(true);
  });

  it('tolerates a nullish controlledBy and a nullish overlay list', () => {
    expect(isUserDrivenSeat('p1', null, null)).toBe(false);
    expect(isUserDrivenSeat('p1', undefined, undefined)).toBe(false);
    expect(isUserDrivenSeat('p1', undefined, ['p1'])).toBe(true);
  });
});
