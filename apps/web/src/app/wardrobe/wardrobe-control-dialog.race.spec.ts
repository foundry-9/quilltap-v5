/**
 * Bug 61 regression — the Wardrobe dialog's Live tab against a slow outfit read.
 *
 * Ported case-for-case from v4's
 * `__tests__/unit/components/wardrobe/wardrobe-control-dialog.race.test.tsx`
 * (`07d4ccce`). The item list paints from its own dispatch; the worn snapshot
 * arrives after a three-round-trip chain. A Wear click in between used to be
 * overwritten by the first seed and then reported as saved. These tests drive
 * that exact window by holding the `chatOutfitGet` response open across the
 * click.
 *
 * v5 differences from the React harness, both mechanical:
 *  - the staged slots are read off the component's `liveStagedByChar` signal
 *    rather than out of a stubbed OutfitComposer (v4 mocks the composer purely
 *    to serialize them into the DOM);
 *  - "the dialog closed" is the `closed` output emitting, v5's analog of v4's
 *    `live-slots` node disappearing;
 *  - the unresolved confirmation is the native `window.confirm` v5's dialog
 *    uses throughout (the recorded divergence from v4's async
 *    `showConfirmation`), so it is stubbed rather than mocked as a module.
 */

import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { CoreRequest, WardrobeItemDto } from '../core/core-contract';
import type { EquippedSlots } from './equipped-slots';
import { WardrobeControlDialogInner } from './wardrobe-control-dialog';

type AnyRequest = CoreRequest & Record<string, unknown>;

const CHAT_ID = 'chat-1';
const CHARACTER_ID = 'alice';

function dto(partial: Partial<WardrobeItemDto> & { id: string }): WardrobeItemDto {
  return {
    characterId: CHARACTER_ID,
    title: partial.id,
    types: ['top'],
    componentItemIds: [],
    isDefault: false,
    replace: false,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...partial,
  } as WardrobeItemDto;
}

const ITEMS = [
  dto({ id: 'shirt', title: 'Linen Shirt', types: ['top'] }),
  dto({ id: 'hat', title: 'Straw Hat', types: ['accessories'] }),
];

/** The worn snapshot the outfit read eventually publishes. */
const WORN: EquippedSlots = { top: ['shirt'], bottom: [], footwear: [], accessories: [], hair: [] };

interface Harness {
  fixture: ComponentFixture<WardrobeControlDialogInner>;
  component: WardrobeControlDialogInner;
  seen: AnyRequest[];
  /** Resolver for the held-open `chatOutfitGet` response; null once released. */
  releaseOutfit: () => void;
  /** Bodies of every `chatEquip` dispatch the dialog fired. */
  equipCalls: () => AnyRequest[];
  closed: () => number;
  settle: (ticks?: number) => Promise<void>;
}

async function renderRace(deferOutfit: boolean): Promise<Harness> {
  const seen: AnyRequest[] = [];
  let release: (() => void) | null = null;
  const outfitHeld = new Promise<void>((resolve) => {
    release = resolve;
  });

  const core = {
    dispatchData: vi.fn(async (req: CoreRequest) => {
      const r = req as AnyRequest;
      seen.push(r);
      switch (r.type as string) {
        case 'chatOutfitGet':
          if (deferOutfit) await outfitHeld;
          return { equippedOutfit: { [CHARACTER_ID]: WORN } };
        case 'chatEquip':
          return { equippedSlots: WORN };
        case 'characterList':
          return {
            characters: [
              { id: CHARACTER_ID, name: 'Alice', defaultImage: null, defaultImageId: null },
            ],
          };
        case 'imageProfileList':
          return { profiles: [] };
        case 'chatGet':
          return { chat: { projectId: null } };
        case 'characterWardrobeList':
          return { wardrobeItems: ITEMS };
        // The shared-archetype call answers empty so the merge stays
        // deterministic (v4's harness does the same).
        case 'wardrobeList':
          return { wardrobeItems: [] };
        default:
          throw new Error(`unexpected ${r.type}`);
      }
    }),
  } as unknown as CoreClient;

  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [WardrobeControlDialogInner],
    providers: [{ provide: CoreClient, useValue: core }],
  });
  const fixture = TestBed.createComponent(WardrobeControlDialogInner);
  fixture.componentRef.setInput('initialCharacterId', CHARACTER_ID);
  fixture.componentRef.setInput('chatId', CHAT_ID);

  let closeCount = 0;
  fixture.componentInstance.closed.subscribe(() => {
    closeCount += 1;
  });

  const settle = async (ticks = 16): Promise<void> => {
    for (let i = 0; i < ticks; i++) {
      await Promise.resolve();
      fixture.detectChanges();
    }
  };

  fixture.detectChanges();
  await settle();

  return {
    fixture,
    component: fixture.componentInstance,
    seen,
    releaseOutfit: () => release?.(),
    equipCalls: () => seen.filter((r) => (r.type as string) === 'chatEquip'),
    closed: () => closeCount,
    settle,
  };
}

/** The staged Live slots for the character under test. */
function stagedSlots(component: WardrobeControlDialogInner): EquippedSlots {
  const staged = (component as unknown as { liveStagedByChar: () => Record<string, EquippedSlots> })
    .liveStagedByChar();
  return staged[CHARACTER_ID] ?? { top: [], bottom: [], footwear: [], accessories: [] };
}

/**
 * The Wear button on a given item's row (both rows carry one) — located the way
 * v4's `findWearButton` does: the row that owns the title, then its Wear
 * button.
 */
function clickWear(fixture: ComponentFixture<unknown>, itemTitle: string): void {
  const rows = fixture.debugElement.queryAll(By.css('.qt-card-interactive'));
  const row = rows.find((r) =>
    (r.nativeElement as HTMLElement).querySelector(`[title="${itemTitle}"]`),
  );
  if (!row) throw new Error(`no row for ${itemTitle}`);
  const wear = row
    .queryAll(By.css('button'))
    .find((b) => (b.nativeElement as HTMLElement).textContent?.trim() === 'Wear');
  if (!wear) throw new Error(`no Wear button for ${itemTitle}`);
  (wear.nativeElement as HTMLButtonElement).click();
}

/** Done, in the dialog footer. */
function clickDone(fixture: ComponentFixture<unknown>): void {
  const done = fixture.debugElement
    .queryAll(By.css('button'))
    .find((b) => (b.nativeElement as HTMLElement).textContent?.trim() === 'Done');
  if (!done) throw new Error('no Done button');
  (done.nativeElement as HTMLButtonElement).click();
}

describe('WardrobeControlDialogInner — staging before the worn snapshot arrives', () => {
  afterEach(() => vi.restoreAllMocks());

  it('replays a Wear clicked mid-flight onto the snapshot and commits it once', async () => {
    const h = await renderRace(true);

    // The item list paints from its own dispatch, well before the outfit read.
    clickWear(h.fixture, 'Straw Hat');
    await h.settle();

    // Painted against the empty fallback — there is nothing else to paint yet.
    expect(stagedSlots(h.component).accessories).toEqual(['hat']);
    expect(stagedSlots(h.component).top).toEqual([]);

    // Now the snapshot lands. Pre-fix this seed discarded the click.
    h.releaseOutfit();
    await h.settle();

    expect(stagedSlots(h.component).top).toEqual(['shirt']);
    expect(stagedSlots(h.component).accessories).toEqual(['hat']);

    clickDone(h.fixture);
    await h.settle();

    expect(h.equipCalls()).toHaveLength(1);
    expect(h.equipCalls()[0]).toMatchObject({
      characterId: CHARACTER_ID,
      mode: 'set_all',
      slots: { top: ['shirt'], bottom: [], footwear: [], accessories: ['hat'] },
    });
    expect(h.closed()).toBe(1);
  });

  it('sends nothing when the snapshot arrives first and nothing is staged', async () => {
    const h = await renderRace(false);

    expect(stagedSlots(h.component).top).toEqual(['shirt']);

    clickDone(h.fixture);
    await h.settle();

    expect(h.equipCalls()).toHaveLength(0);
    expect(h.closed()).toBe(1);
  });

  it('asks before discarding an edit whose snapshot never arrived, instead of closing as if saved', async () => {
    const h = await renderRace(true);
    const confirm = vi.spyOn(window, 'confirm');

    clickWear(h.fixture, 'Straw Hat');
    await h.settle();
    expect(stagedSlots(h.component).accessories).toEqual(['hat']);

    // Declining keeps the dialog open with the edit intact — the outfit read
    // may still land, and then Done saves normally.
    confirm.mockReturnValueOnce(false);
    clickDone(h.fixture);
    await h.settle();

    expect(confirm).toHaveBeenCalledTimes(1);
    expect(String(confirm.mock.calls[0][0])).toContain('Alice');
    expect(h.equipCalls()).toHaveLength(0);
    expect(h.closed()).toBe(0);
    expect(stagedSlots(h.component).accessories).toEqual(['hat']);

    // Confirming closes, having said plainly that the change is going.
    confirm.mockReturnValueOnce(true);
    clickDone(h.fixture);
    await h.settle();

    expect(h.equipCalls()).toHaveLength(0);
    expect(h.closed()).toBe(1);
  });

  it('puts v4’s sentence to the operator verbatim, naming the character', async () => {
    const h = await renderRace(true);
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);

    clickWear(h.fixture, 'Straw Hat');
    await h.settle();
    clickDone(h.fixture);
    await h.settle();

    expect(confirm).toHaveBeenCalledWith(
      'Word of what Alice is presently wearing never reached us, so your alterations cannot be saved. Close the wardrobe and let them go?',
    );
  });
});
