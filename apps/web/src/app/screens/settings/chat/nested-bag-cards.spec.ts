import type { ComponentFixture } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { cardStub, checkboxes, mountCard, settingsRow, settle, toggle } from './chat-settings.spec-harness';
import { MemoryCascadeSettings } from './memory-cascade-settings';
import { ThinkingDisplaySettings } from './thinking-display-settings';
import { TokenDisplaySettings } from './token-display-settings';

/**
 * The three nested-bag Chat-tab cards (P4.6an unit 4). The load-bearing
 * assertion in each is that the PUT carries the WHOLE merged bag — v4 spread-
 * merges client-side and the server replaces the column wholesale, so a partial
 * nested patch would silently drop the sibling keys.
 */

function selects(fixture: ComponentFixture<unknown>): HTMLSelectElement[] {
  return Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('select'));
}

async function choose(
  fixture: ComponentFixture<unknown>,
  select: HTMLSelectElement,
  value: string,
): Promise<void> {
  select.value = value;
  select.dispatchEvent(new Event('change'));
  await settle(fixture);
}

describe('TokenDisplaySettings', () => {
  it('renders v4\'s four options, all off by default when unset', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(TokenDisplaySettings, stub);
    const boxes = checkboxes(fixture);
    expect(boxes).toHaveLength(4);
    expect(boxes.map((b) => b.checked)).toEqual([false, false, false, false]);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'Show Token Count on Messages',
    );
  });

  it('renders the persisted bag', async () => {
    const stub = cardStub(
      settingsRow({
        tokenDisplaySettings: {
          showPerMessageTokens: true,
          showPerMessageCost: false,
          showChatTotals: true,
          showSystemEvents: false,
        },
      }),
    );
    const fixture = await mountCard(TokenDisplaySettings, stub);
    expect(checkboxes(fixture).map((b) => b.checked)).toEqual([true, false, true, false]);
  });

  it('PUTs the whole bag with one key replaced, preserving its siblings', async () => {
    const stub = cardStub(
      settingsRow({
        tokenDisplaySettings: {
          showPerMessageTokens: true,
          showPerMessageCost: false,
          showChatTotals: true,
          showSystemEvents: false,
        },
      }),
    );
    const fixture = await mountCard(TokenDisplaySettings, stub);
    // Flip the SECOND toggle; the other three must ride along unchanged.
    await toggle(fixture, checkboxes(fixture)[1], true);
    expect(stub.updates).toEqual([
      {
        tokenDisplaySettings: {
          showPerMessageTokens: true,
          showPerMessageCost: true,
          showChatTotals: true,
          showSystemEvents: false,
        },
      },
    ]);
  });

  it('merges over the v4 defaults when the bag is absent', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(TokenDisplaySettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], true);
    expect(stub.updates).toEqual([
      {
        tokenDisplaySettings: {
          showPerMessageTokens: true,
          showPerMessageCost: false,
          showChatTotals: false,
          showSystemEvents: false,
        },
      },
    ]);
  });

  it('surfaces a failed save (dogfood #6)', async () => {
    const stub = cardStub(settingsRow(), true);
    const fixture = await mountCard(TokenDisplaySettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], true);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'Failed to update token display settings',
    );
  });
});

describe('MemoryCascadeSettings', () => {
  it('renders v4\'s defaults when the bag is absent', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(MemoryCascadeSettings, stub);
    const [del, swipe] = selects(fixture);
    expect(del.value).toBe('ASK_EVERY_TIME');
    expect(swipe.value).toBe('DELETE_MEMORIES');
  });

  it('offers all four actions on delete but filters ASK_EVERY_TIME out of swipe', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(MemoryCascadeSettings, stub);
    const [del, swipe] = selects(fixture);
    expect(Array.from(del.options).map((o) => o.value)).toEqual([
      'ASK_EVERY_TIME',
      'DELETE_MEMORIES',
      'KEEP_MEMORIES',
      'REGENERATE_MEMORIES',
    ]);
    expect(Array.from(swipe.options).map((o) => o.value)).toEqual([
      'DELETE_MEMORIES',
      'KEEP_MEMORIES',
      'REGENERATE_MEMORIES',
    ]);
  });

  it('shows the selected action\'s description', async () => {
    const stub = cardStub(
      settingsRow({
        memoryCascadePreferences: {
          onMessageDelete: 'KEEP_MEMORIES',
          onSwipeRegenerate: 'DELETE_MEMORIES',
        },
      }),
    );
    const fixture = await mountCard(MemoryCascadeSettings, stub);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'Keep memories (they will become orphaned)',
    );
  });

  it('PUTs the whole bag with the one key replaced', async () => {
    const stub = cardStub(
      settingsRow({
        memoryCascadePreferences: {
          onMessageDelete: 'ASK_EVERY_TIME',
          onSwipeRegenerate: 'DELETE_MEMORIES',
        },
      }),
    );
    const fixture = await mountCard(MemoryCascadeSettings, stub);
    await choose(fixture, selects(fixture)[1], 'REGENERATE_MEMORIES');
    expect(stub.updates).toEqual([
      {
        memoryCascadePreferences: {
          onMessageDelete: 'ASK_EVERY_TIME',
          onSwipeRegenerate: 'REGENERATE_MEMORIES',
        },
      },
    ]);
  });

  it('displays a stored value that only arrives after first render ([selected]-per-option)', async () => {
    const stub = cardStub(
      settingsRow({
        memoryCascadePreferences: {
          onMessageDelete: 'REGENERATE_MEMORIES',
          onSwipeRegenerate: 'KEEP_MEMORIES',
        },
      }),
    );
    const fixture = await mountCard(MemoryCascadeSettings, stub);
    expect(selects(fixture)[0].value).toBe('REGENERATE_MEMORIES');
    expect(selects(fixture)[1].value).toBe('KEEP_MEMORIES');
  });

  it('surfaces a failed save (dogfood #6)', async () => {
    const stub = cardStub(settingsRow(), true);
    const fixture = await mountCard(MemoryCascadeSettings, stub);
    await choose(fixture, selects(fixture)[0], 'KEEP_MEMORIES');
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'Failed to update memory cascade preferences',
    );
  });
});

describe('ThinkingDisplaySettings', () => {
  it('defaults both toggles on when the bag is absent (v4 ?? true)', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(ThinkingDisplaySettings, stub);
    expect(checkboxes(fixture).map((b) => b.checked)).toEqual([true, true]);
  });

  it('disables "start collapsed" while thinking is hidden', async () => {
    const stub = cardStub(settingsRow({ thinkingDisplay: { defaultVisible: false } }));
    const fixture = await mountCard(ThinkingDisplaySettings, stub);
    expect(checkboxes(fixture)[1].disabled).toBe(true);
  });

  it('PUTs the whole merged bag on the visible toggle', async () => {
    const stub = cardStub(
      settingsRow({ thinkingDisplay: { defaultVisible: true, defaultCollapsed: false } }),
    );
    const fixture = await mountCard(ThinkingDisplaySettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], false);
    expect(stub.updates).toEqual([
      { thinkingDisplay: { defaultVisible: false, defaultCollapsed: false } },
    ]);
  });

  it('fills the merged bag from the defaults when only one key is stored', async () => {
    const stub = cardStub(settingsRow({ thinkingDisplay: { defaultVisible: true } }));
    const fixture = await mountCard(ThinkingDisplaySettings, stub);
    await toggle(fixture, checkboxes(fixture)[1], false);
    expect(stub.updates).toEqual([
      { thinkingDisplay: { defaultVisible: true, defaultCollapsed: false } },
    ]);
  });

  it('surfaces a failed save (dogfood #6)', async () => {
    const stub = cardStub(settingsRow(), true);
    const fixture = await mountCard(ThinkingDisplaySettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], false);
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'Failed to update thinking-display settings',
    );
  });
});
