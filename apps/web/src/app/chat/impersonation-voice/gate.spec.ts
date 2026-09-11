import { describe, expect, it } from 'vitest';

import { shouldRehearseImpersonatedLine, type ShouldRehearseArgs } from './gate';

/**
 * The In Their Own Words gate (`shouldRehearseImpersonatedLine`) — the parity
 * spec, transcribed 1:1 from v4's
 * `app/salon/[id]/hooks/__tests__/useImpersonationVoice.test.ts` at `f4ad2c8d1`
 * (its `describe('shouldRehearseImpersonatedLine')` block: twelve `it`s in
 * v4's order, with v4's names and v4's fixture builder).
 *
 * v4's file also carries a `describe('every chat setting the Salon reads is
 * LIVE')` that `readFileSync`s two source files. A vitest spec cannot read
 * source (`spa-spec-cannot-read-source`), and v5's Salon never had the mount-only
 * fetch that scan guards against — those facts are pinned STRUCTURALLY instead,
 * by `screens/salon/salon-settings-live.spec.ts` (bug 134, P4.D181 Tier 1 item 8).
 */

const IMPERSONATED = 'seat-evangeline';

function args(over: Partial<ShouldRehearseArgs> = {}): ShouldRehearseArgs {
  return {
    enabled: true,
    seat: { id: IMPERSONATED, type: 'CHARACTER', controlledBy: 'llm' },
    impersonatingParticipantIds: [IMPERSONATED],
    text: 'I tell him I will take the job.',
    hasAttachmentsOnly: false,
    bypassOnce: false,
    ...over,
  };
}

describe('shouldRehearseImpersonatedLine', () => {
  it('fires for an impersonated character seat with prose', () => {
    expect(shouldRehearseImpersonatedLine(args())).toBe(true);
  });

  describe('rule 1 — the instance setting', () => {
    it('never fires when the setting is off', () => {
      expect(shouldRehearseImpersonatedLine(args({ enabled: false }))).toBe(false);
    });
  });

  describe('rule 2 — the seat', () => {
    it('never fires with no speaking seat at all', () => {
      expect(shouldRehearseImpersonatedLine(args({ seat: null }))).toBe(false);
    });

    it('never fires for a seat that is not in the overlay', () => {
      expect(
        shouldRehearseImpersonatedLine(
          args({ impersonatingParticipantIds: ['seat-someone-else'] }),
        ),
      ).toBe(false);
    });

    it('never fires for the owner persona, even when it is also in the overlay', () => {
      expect(
        shouldRehearseImpersonatedLine(
          args({ seat: { id: IMPERSONATED, type: 'CHARACTER', controlledBy: 'user' } }),
        ),
      ).toBe(false);
    });
  });

  describe('rule 3 — there has to be something to restate', () => {
    it('never fires on empty text', () => {
      expect(shouldRehearseImpersonatedLine(args({ text: '' }))).toBe(false);
    });

    it('never fires on whitespace alone', () => {
      expect(shouldRehearseImpersonatedLine(args({ text: '   \n\t ' }))).toBe(false);
    });

    it('never fires on an attachment-only send', () => {
      expect(shouldRehearseImpersonatedLine(args({ text: '', hasAttachmentsOnly: true }))).toBe(
        false,
      );
    });
  });

  describe('rule 4 — Carina addresses are machinery', () => {
    it('never fires on a public Carina address', () => {
      expect(shouldRehearseImpersonatedLine(args({ text: '@Evangeline: what year is it?' }))).toBe(
        false,
      );
    });

    it('never fires on a whispered Carina address', () => {
      expect(shouldRehearseImpersonatedLine(args({ text: '@Evangeline? what year is it?' }))).toBe(
        false,
      );
    });

    it('still fires on an @name that is not a Carina address', () => {
      expect(
        shouldRehearseImpersonatedLine(args({ text: 'I look at @Evangeline and say nothing.' })),
      ).toBe(true);
    });
  });

  describe('rule 5 — bypass once', () => {
    it('lets a "Send as written" resubmit straight through', () => {
      expect(shouldRehearseImpersonatedLine(args({ bypassOnce: true }))).toBe(false);
    });
  });
});
