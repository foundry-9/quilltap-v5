import { describe, expect, it } from 'vitest';

import type { PascalMeta } from '../core/core-contract';
import {
  getAnnouncementAccentClasses,
  getAnnouncementOutcomeState,
  getSystemKindDisplayLabel,
} from '../chat/system-message-labels';

/**
 * The `pascalMeta` widening is TYPE-ONLY (P4.D307 Tier 2 item 20). v4
 * `362426c8f` adds `invokedBy: 'trigger'` + `triggerPhrase`, and `f0db8cbb8`
 * lets `effects[]` carry a wardrobe record (`target: 'wardrobe.wear'`,
 * `previous` always, a structured `next`). Measured on the SPA: NO reader keys
 * on `invokedBy`, and none reads `effects` at all — the Staff bar's three
 * readers (`chat/system-message-labels.ts`) read `chipLabel` / `toolTitle` /
 * `tool` and `state`. This pins that a trigger-run row with a wardrobe effect
 * goes through all three exactly as a model-run row does.
 */

const triggerRun: PascalMeta = {
  tool: 'spell',
  toolTitle: 'Spell',
  chipLabel: 'Abracadabra',
  definitionTier: 'global',
  definitionMountId: 'm1',
  params: {},
  rollForm: 'range',
  raw: 0.5,
  value: 0.5,
  state: 'success',
  outcomeIndex: 0,
  effects: [
    {
      target: 'wardrobe.wear',
      previous: { top: [], bottom: [], footwear: [], accessories: [], hair: [] },
      next: {
        item: { id: 'i1', title: 'Cloak of Night' },
        slotsAffected: ['top'],
        effect: 'replaced',
        slots: { top: ['i1'], bottom: [], footwear: [], accessories: [], hair: [] },
      },
    },
  ],
  invokedBy: 'trigger',
  triggerPhrase: 'abracadabra',
};

describe('pascalMeta from a trigger-phrase run (type-only widening)', () => {
  const message = {
    systemSender: 'pascal' as const,
    systemKind: 'custom-tool-result',
    content: 'The air shimmers.',
    pascalMeta: triggerRun,
  };

  it('labels, states and accents the row as any other roll', () => {
    expect(getSystemKindDisplayLabel(message)).toBe('Abracadabra');
    expect(getAnnouncementOutcomeState(message)).toBe('success');
    expect(getAnnouncementAccentClasses(message)).toBe('qt-pascal-result qt-pascal-result--success');
  });

  it('is unchanged by the trigger keys — the same row as a model run reads identically', () => {
    const asModel = { ...message, pascalMeta: { ...triggerRun, invokedBy: 'llm' as const, triggerPhrase: undefined } };
    expect(getSystemKindDisplayLabel(asModel)).toBe(getSystemKindDisplayLabel(message));
    expect(getAnnouncementAccentClasses(asModel)).toBe(getAnnouncementAccentClasses(message));
  });
});
