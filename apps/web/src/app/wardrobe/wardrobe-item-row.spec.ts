import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import type { WardrobeItemDto } from '../core/core-contract';
import { WardrobeItemRow } from './wardrobe-item-row';

function dto(partial: Partial<WardrobeItemDto> & { id: string }): WardrobeItemDto {
  return {
    characterId: null,
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

async function render(
  item: WardrobeItemDto,
  canManage?: (i: WardrobeItemDto) => boolean,
  allItems: WardrobeItemDto[] = [],
  opts: {
    canArchive?: boolean;
    canGenerateImage?: boolean;
    generatingImageIds?: ReadonlySet<string>;
    canOpenImage?: boolean;
    canMoveToFolder?: boolean;
    selection?: { selected: boolean; onToggle: () => void };
    folderCrumb?: string | null;
    inChat?: boolean;
  } = {},
): Promise<ComponentFixture<WardrobeItemRow>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [WardrobeItemRow] });
  const fixture = TestBed.createComponent(WardrobeItemRow);
  fixture.componentRef.setInput('item', item);
  fixture.componentRef.setInput('allItems', allItems);
  fixture.componentRef.setInput('inChat', opts.inChat ?? false);
  if (opts.canMoveToFolder) fixture.componentRef.setInput('canMoveToFolder', true);
  if (opts.selection) fixture.componentRef.setInput('selection', opts.selection);
  if (opts.folderCrumb !== undefined) fixture.componentRef.setInput('folderCrumb', opts.folderCrumb);
  if (canManage) fixture.componentRef.setInput('canManage', canManage);
  if (opts.canArchive) fixture.componentRef.setInput('canArchive', true);
  if (opts.canGenerateImage) fixture.componentRef.setInput('canGenerateImage', true);
  if (opts.canOpenImage) fixture.componentRef.setInput('canOpenImage', true);
  if (opts.generatingImageIds) {
    fixture.componentRef.setInput('generatingImageIds', opts.generatingImageIds);
  }
  fixture.detectChanges();
  return fixture;
}

/** Open the kebab and return the menu item labels, in order. */
function kebabLabels(fixture: ComponentFixture<WardrobeItemRow>): string[] {
  const host = fixture.nativeElement as HTMLElement;
  const kebab = Array.from(host.querySelectorAll('button')).find(
    (b) => b.getAttribute('aria-label') === 'More actions',
  ) as HTMLButtonElement;
  kebab.click();
  fixture.detectChanges();
  return Array.from(host.querySelectorAll('[role="menu"] button')).map((b) =>
    (b.textContent ?? '').trim(),
  );
}

/**
 * RE-PINNED by P4.D286: v4 `4a38222e6` renamed the cross-wardrobe pair `Move`
 * / `Copy` to `Move to another wardrobe…` / `Copy to another wardrobe…`
 * (`wardrobe-item-row.tsx:455-480`; v4's own archive / images jsdom cases
 * re-pinned the same way, `wardrobe-item-row-archive.test.tsx:137`, `:148`,
 * `wardrobe-item-images-ui.test.tsx:232`).
 *
 * v4 `wardrobe-item-row.tsx` at `d7263f39`: the `isShared = !item.characterId`
 * rule became the optional `canManage` predicate. Full kebab iff manageable;
 * otherwise Move/Copy only. RE-PINNED by P4.D261: v4 `cc80dc89d` retired the
 * bare `· shared` text for the origin chip (`wardrobe-item-row.tsx:250-256`),
 * which needs an `origin` — these fixtures carry none, so a borrowed row here
 * shows NEITHER (a pre-round server's rows read exactly so).
 */
describe('WardrobeItemRow canManage (v4 :36-40, :83-85, :199, :291, :363)', () => {
  it('falls back to the character-view rule when no predicate is passed (v4 :83-85)', async () => {
    const owned = await render(dto({ id: 'i1', characterId: 'c1' }));
    expect(kebabLabels(owned)).toEqual([
      'Edit',
      '★ Mark as default outfit item',
      'Duplicate',
      'Move to another wardrobe…',
      'Copy to another wardrobe…',
      'Delete',
    ]);
    expect((owned.nativeElement as HTMLElement).textContent).not.toContain('· shared');

    const shared = await render(dto({ id: 'i2', characterId: null }));
    expect(kebabLabels(shared)).toEqual(['Move to another wardrobe…', 'Copy to another wardrobe…']);
    expect((shared.nativeElement as HTMLElement).textContent).not.toContain('· shared');
    expect(
      (shared.nativeElement as HTMLElement).querySelector('.qt-badge-wardrobe-shared'),
    ).toBeNull();
  });

  it('an explicit predicate OVERRIDES the character rule in both directions', async () => {
    // A shared-tier item (characterId null) browsed in its own container is
    // fully manageable — the case the container browser exists for.
    const inContainer = await render(dto({ id: 'i1', characterId: null }), () => true);
    const labels = kebabLabels(inContainer);
    expect(labels).toContain('Edit');
    expect(labels).toContain('Delete');
    expect((inContainer.nativeElement as HTMLElement).textContent).not.toContain('· shared');

    // A character-owned item the predicate refuses keeps Move/Copy only.
    const refused = await render(dto({ id: 'i2', characterId: 'c1' }), () => false);
    expect(kebabLabels(refused)).toEqual(['Move to another wardrobe…', 'Copy to another wardrobe…']);
    expect((refused.nativeElement as HTMLElement).textContent).not.toContain('· shared');
    expect(
      (refused.nativeElement as HTMLElement).querySelector('.qt-badge-wardrobe-shared'),
    ).toBeNull();
  });

  it('the predicate is asked about THIS row, so a mixed list badges per item', async () => {
    const local = dto({ id: 'local', characterId: null });
    const merged = dto({ id: 'merged', characterId: null });
    const inList = (i: WardrobeItemDto): boolean => i.id === 'local';
    expect(kebabLabels(await render(local, inList))).toContain('Edit');
    expect(kebabLabels(await render(merged, inList))).toEqual(['Move to another wardrobe…', 'Copy to another wardrobe…']);
  });

  it('nested composite components inherit the predicate (v4 :399)', async () => {
    const child = dto({
      id: 'child',
      characterId: null,
      title: 'Cravat',
      origin: { scope: 'general', id: null, name: 'Quilltap General' },
    });
    const parent = dto({
      id: 'parent',
      characterId: null,
      title: 'Evening Kit',
      componentItemIds: ['child'],
    });
    const fixture = await render(parent, () => true, [child]);
    const host = fixture.nativeElement as HTMLElement;
    (host.querySelector('button[aria-label="Expand components"]') as HTMLButtonElement).click();
    fixture.detectChanges();
    // The nested row exists and carries NO origin chip although it has an
    // origin — it inherited the predicate rather than falling back to
    // `!characterId`.
    const nested = host.querySelector('qt-wardrobe-item-row') as HTMLElement;
    expect(nested.textContent).toContain('Cravat');
    expect(nested.querySelector('.qt-badge-wardrobe-shared')).toBeNull();
  });
});

/**
 * P4.D121 — the Archive / Restore affordance (v4 `d25dacc1`
 * `__tests__/wardrobe-item-row-archive.test.tsx`, transcribed onto the Angular
 * row). Three rules govern whether it appears, and they are easy to break by
 * accident:
 *
 *  1. It is OPTIONAL — v4's absent `onToggleArchived` prop, here the
 *     `canArchive` gate (default false). The outfit composer, which does the
 *     same job the outfit-selection LLM does, must never offer it.
 *  2. It lives behind `canManage`, alongside Edit and Duplicate: one character
 *     must not be able to retire a coat the whole household shares.
 *  3. The label flips with the item's state, and an archived row is badged.
 */
describe('WardrobeItemRow — the archive affordance (v4 d25dacc1)', () => {
  const ARCHIVED_AT = '2026-02-01T00:00:00.000Z';

  it('offers Archive for an active garment and emits the item', async () => {
    const fixture = await render(dto({ id: 'i1', characterId: 'c1' }), undefined, [], {
      canArchive: true,
    });
    const seen: WardrobeItemDto[] = [];
    fixture.componentInstance.toggleArchived.subscribe((i: WardrobeItemDto) => seen.push(i));
    expect(kebabLabels(fixture)).toEqual([
      'Edit',
      '★ Mark as default outfit item',
      'Duplicate',
      'Archive',
      'Move to another wardrobe…',
      'Copy to another wardrobe…',
      'Delete',
    ]);
    const entry = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('[role="menu"] button'),
    ).find((b) => (b.textContent ?? '').trim() === 'Archive') as HTMLButtonElement;
    entry.click();
    fixture.detectChanges();
    expect(seen.map((i) => i.id)).toEqual(['i1']);
  });

  it('offers "Restore from archive" for an archived garment', async () => {
    const fixture = await render(
      dto({ id: 'i1', characterId: 'c1', archivedAt: ARCHIVED_AT }),
      undefined,
      [],
      { canArchive: true },
    );
    const labels = kebabLabels(fixture);
    expect(labels).toContain('Restore from archive');
    expect(labels).not.toContain('Archive');
  });

  it('badges an archived garment in the row itself, and leaves an active one bare', async () => {
    const archived = await render(dto({ id: 'i1', archivedAt: ARCHIVED_AT }));
    expect((archived.nativeElement as HTMLElement).textContent).toContain('archived');
    const active = await render(dto({ id: 'i2' }));
    expect((active.nativeElement as HTMLElement).textContent).not.toContain('archived');
  });

  it('omits the entry entirely when the surface does not allow archiving', async () => {
    const fixture = await render(dto({ id: 'i1', characterId: 'c1' }));
    const labels = kebabLabels(fixture);
    expect(labels).not.toContain('Archive');
    // The unconditional actions are still there.
    expect(labels).toContain('Move to another wardrobe…');
  });

  it('withholds it from a garment borrowed from another tier, as Edit is withheld', async () => {
    const fixture = await render(dto({ id: 'i1', characterId: null }), () => false, [], {
      canArchive: true,
    });
    const labels = kebabLabels(fixture);
    expect(labels).not.toContain('Archive');
    expect(labels).not.toContain('Edit');
    expect(labels).toContain('Copy to another wardrobe…');
  });
});

/**
 * P4.75 (the residue-host adjudication, item 6): both of this row's stacks are
 * Tailwind `space-y-1` containers (`wardrobe-control-dialog.ts:327` and the
 * composite drill at `wardrobe-item-row.ts:256`), and an Angular custom element
 * with no CSS rule renders `display: inline` — which silently ignores the
 * vertical margin `space-y-*` puts on it. The fix is the host class + the
 * `_surfaces.css` rule; jsdom computes no cascade, so the reachable pin is the
 * class that rule targets (the dogfood-#97 idiom).
 */
describe('WardrobeItemRow — the host carries its own box (P4.75)', () => {
  it('stamps qt-wardrobe-item-row on the host element', async () => {
    const fixture = await render(dto({ id: 'i1', title: 'Silk Blouse' }));
    expect(
      (fixture.nativeElement as HTMLElement).classList.contains('qt-wardrobe-item-row'),
    ).toBe(true);
  });
});

/**
 * P4.D261 — v4 `cc80dc89d` `components/wardrobe/__tests__/wardrobe-item-row-
 * origin.test.tsx` at the pin `f5e953a3f`, transcribed case for case. The
 * chip renders ONLY when the row is not manageable AND `wardrobeOriginLabel`
 * resolves (`wardrobe-item-row.tsx:250-256`), after the slot badges.
 */
describe('WardrobeItemRow — origin chip (v4 wardrobe-item-row-origin.test.tsx)', () => {
  const THORNFIELD = { scope: 'project' as const, id: 'p1', name: 'Thornfield' };
  const flapper = (partial: Partial<WardrobeItemDto> = {}): WardrobeItemDto =>
    dto({
      id: 'item-1',
      characterId: null,
      title: 'Midnight Lightning Flapper Dress',
      types: ['top', 'bottom'],
      ...partial,
    });
  const chipOf = (f: ComponentFixture<WardrobeItemRow>): HTMLElement | null =>
    (f.nativeElement as HTMLElement).querySelector('.qt-badge-wardrobe-shared');

  it('names the wardrobe a borrowed garment came from, as a shared-badge chip', async () => {
    const f = await render(flapper({ origin: THORNFIELD }));
    const chip = chipOf(f)!;
    expect(chip.textContent!.trim()).toBe('Project · Thornfield');
    expect(chip.classList.contains('qt-badge')).toBe(true);
    expect(chip.getAttribute('title')).toBe('Borrowed from Project · Thornfield');
    expect((f.nativeElement as HTMLElement).textContent).not.toContain('· shared');
    // v4 places it AFTER the slot badges.
    const badges = [...(f.nativeElement as HTMLElement).querySelectorAll('.qt-badge')];
    expect(badges.at(-1)).toBe(chip);
  });

  it('spells Quilltap General as a shared wardrobe', async () => {
    const f = await render(flapper({ origin: { scope: 'general', id: null, name: 'Quilltap General' } }));
    expect(chipOf(f)!.textContent!.trim()).toBe('Shared · Quilltap General');
  });

  it('renders no chip for the character’s own garment', async () => {
    const f = await render(
      flapper({ characterId: 'char-1', origin: { scope: 'character', id: 'char-1', name: 'Bertie' } }),
    );
    expect(chipOf(f)).toBeNull();
  });

  it('renders no chip in a project view, where every row is the project’s own', async () => {
    const f = await render(flapper({ origin: THORNFIELD }), () => true);
    expect(chipOf(f)).toBeNull();
  });
});

/**
 * P4.D261 — v4 `3ee3b1342` `wardrobe-wear-ledger-ui.test.tsx` (the row ×3) at
 * the pin `f5e953a3f`; markup `wardrobe-item-row.tsx:257-263` — ALWAYS
 * rendered under the badge row (an absent annotation reads `Never worn`),
 * before `appropriateness`.
 */
describe('WardrobeItemRow — the wear line (v4 wardrobe-wear-ledger-ui.test.tsx)', () => {
  const line = (f: ComponentFixture<WardrobeItemRow>): HTMLElement =>
    (f.nativeElement as HTMLElement).querySelector('[data-testid="wardrobe-wear-line"]')!;
  const daysAgo = (n: number): string => new Date(Date.now() - n * 86_400_000).toISOString();

  it('shows the tally and the relative date of the last wear', async () => {
    const f = await render(
      dto({
        id: 'w',
        characterId: 'c1',
        wear: { wearCount: 4, firstWornAt: daysAgo(30), lastWornAt: daysAgo(3), lastWornChatId: null },
      }),
    );
    expect(line(f).textContent!.trim()).toMatch(/^Worn 4× · last /);
    expect(line(f).classList.contains('qt-text-xs')).toBe(true);
  });

  it('says "once" for a single wear', async () => {
    const f = await render(
      dto({
        id: 'w',
        characterId: 'c1',
        wear: { wearCount: 1, firstWornAt: daysAgo(21), lastWornAt: daysAgo(21), lastWornChatId: null },
      }),
    );
    expect(line(f).textContent!.trim()).toMatch(/^Worn once · last /);
  });

  it('reads an item with no annotation (a pre-round server) as never worn, above appropriateness', async () => {
    const f = await render(dto({ id: 'w', characterId: 'c1', appropriateness: 'formal' }));
    expect(line(f).textContent!.trim()).toBe('Never worn');
    expect(line(f).nextElementSibling!.textContent!.trim()).toBe('formal');
  });
});

/**
 * P4.D261 — v4 `7c8572869` `components/wardrobe/__tests__/wardrobe-item-images-
 * ui.test.tsx` (`WardrobeItemRow — picture` ×5) at the pin `f5e953a3f`; markup
 * `wardrobe-item-row.tsx:217` (the 40 px thumbnail between the expander and
 * the title block) and `:361-373` (the entry directly after Edit). v4 renders
 * the entry when `onGenerateImage` is GIVEN; an Angular output always exists,
 * so the gate is the boolean `canGenerateImage` input (the `canArchive`
 * precedent — recorded).
 */
describe('WardrobeItemRow — picture (v4 wardrobe-item-images-ui.test.tsx)', () => {
  const pictured = (partial: Partial<WardrobeItemDto> = {}): WardrobeItemDto =>
    dto({ id: 'item-1', characterId: 'char-1', title: 'Midnight Lightning Flapper Dress', ...partial });
  const thumbOf = (f: ComponentFixture<WardrobeItemRow>): HTMLImageElement | null =>
    (f.nativeElement as HTMLElement).querySelector('[data-testid="wardrobe-item-thumbnail"]');

  it('shows a 40 px thumbnail when the item has a current picture', async () => {
    const f = await render(pictured({ imageFileId: 'file-9' }));
    const thumb = thumbOf(f)!;
    expect(thumb.getAttribute('src')).toBe('/api/v1/files/file-9?action=thumbnail');
    expect(thumb.getAttribute('width')).toBe('40');
    expect(thumb.getAttribute('height')).toBe('40');
    expect(thumb.getAttribute('alt')).toBe('');
    expect(thumb.getAttribute('loading')).toBe('lazy');
    // Between the expander placeholder and the title block (v4 :217).
    expect(thumb.closest('qt-wardrobe-item-thumbnail')!.nextElementSibling!.textContent).toContain(
      'Midnight Lightning Flapper Dress',
    );
  });

  // P4.D277 — v4 `01a83539d` `wardrobe-item-images-ui.test.tsx:183-189`.
  it('opens the picture full screen from the thumbnail', async () => {
    const item = pictured({ imageFileId: 'file-9' });
    const f = await render(item, undefined, [], { canOpenImage: true });
    const opened: WardrobeItemDto[] = [];
    f.componentInstance.openImage.subscribe((i) => opened.push(i));
    const button = (f.nativeElement as HTMLElement).querySelector<HTMLButtonElement>(
      `button[aria-label="View the picture of ${item.title}"]`,
    )!;
    expect(button.title).toBe('View the picture of Midnight Lightning Flapper Dress');
    button.click();
    expect(opened).toEqual([item]);
  });

  // P4.D277 — v4 `01a83539d` `wardrobe-item-images-ui.test.tsx:191-194`.
  it('leaves the thumbnail a plain image without an open handler', async () => {
    const f = await render(pictured({ imageFileId: 'file-9' }));
    expect(
      (f.nativeElement as HTMLElement).querySelector('[data-testid="wardrobe-item-thumbnail-open"]'),
    ).toBeNull();
    expect(thumbOf(f)).toBeTruthy();
  });

  // P4.D277 — v4 `wardrobe-item-row.tsx:503` passes `onOpenImage` to nested rows.
  it("a nested component row's thumbnail opens ITS item through the parent", async () => {
    const part = dto({ id: 'gloves', characterId: 'char-1', title: 'Opera Gloves', imageFileId: 'file-g' });
    const outfit = pictured({ componentItemIds: ['gloves'] });
    const f = await render(outfit, undefined, [outfit, part], { canOpenImage: true });
    const opened: WardrobeItemDto[] = [];
    f.componentInstance.openImage.subscribe((i) => opened.push(i));
    const host = f.nativeElement as HTMLElement;
    (host.querySelector('button[aria-label="Expand components"]') as HTMLButtonElement).click();
    f.detectChanges();
    host.querySelector<HTMLButtonElement>('button[aria-label="View the picture of Opera Gloves"]')!.click();
    expect(opened).toEqual([part]);
  });

  it('shows no thumbnail when the item has no picture (or a pre-round server sent none)', async () => {
    expect(thumbOf(await render(pictured({ imageFileId: null })))).toBeNull();
    expect(thumbOf(await render(pictured()))).toBeNull();
  });

  it('offers "Generate image" on a manageable row, under Edit', async () => {
    const f = await render(pictured(), undefined, [], { canGenerateImage: true });
    const emitted: WardrobeItemDto[] = [];
    f.componentInstance.generateImage.subscribe((i) => emitted.push(i));
    const labels = kebabLabels(f);
    expect(labels.indexOf('Generate image')).toBe(labels.indexOf('Edit') + 1);
    const entry = [...(f.nativeElement as HTMLElement).querySelectorAll<HTMLButtonElement>('[role="menuitem"]')].find(
      (b) => b.textContent!.trim() === 'Generate image',
    )!;
    entry.click();
    expect(emitted).toEqual([f.componentInstance.item()]);
  });

  it("disables \"Generate image\" while that item's picture is already being drawn", async () => {
    const f = await render(pictured(), undefined, [], {
      canGenerateImage: true,
      generatingImageIds: new Set(['item-1']),
    });
    const emitted: WardrobeItemDto[] = [];
    f.componentInstance.generateImage.subscribe((i) => emitted.push(i));
    kebabLabels(f);
    const entry = [...(f.nativeElement as HTMLElement).querySelectorAll<HTMLButtonElement>('[role="menuitem"]')].find(
      (b) => b.textContent!.trim() === 'Generating image…',
    )!;
    expect(entry.disabled).toBe(true);
    entry.click();
    expect(emitted).toEqual([]);
  });

  it('withholds "Generate image" from a borrowed row', async () => {
    const f = await render(
      pictured({ characterId: null, origin: { scope: 'general', id: null, name: 'Quilltap General' } }),
      undefined,
      [],
      { canGenerateImage: true },
    );
    const labels = kebabLabels(f);
    expect(labels).not.toContain('Generate image');
    expect(labels).toContain('Move to another wardrobe…');
  });

  it('a host that does not opt in offers no entry at all (the boolean gate)', async () => {
    expect(kebabLabels(await render(pictured()))).not.toContain('Generate image');
  });

  it('nested component rows inherit the gate and the generating set', async () => {
    const child = dto({ id: 'child', characterId: 'char-1', title: 'Cravat' });
    const parent = dto({ id: 'parent', characterId: 'char-1', title: 'Kit', componentItemIds: ['child'] });
    const f = await render(parent, undefined, [child], {
      canGenerateImage: true,
      generatingImageIds: new Set(['child']),
    });
    const host = f.nativeElement as HTMLElement;
    (host.querySelector('button[aria-label="Expand components"]') as HTMLButtonElement).click();
    f.detectChanges();
    const nested = host.querySelector('qt-wardrobe-item-row') as HTMLElement;
    (nested.querySelector('button[aria-label="More actions"]') as HTMLButtonElement).click();
    f.detectChanges();
    const labels = [...nested.querySelectorAll('[role="menuitem"]')].map((b) => b.textContent!.trim());
    expect(labels).toContain('Generating image…');
  });
});

/**
 * P4.D286 — v4 `4a38222e6` `wardrobe-item-row.tsx` (read at that pin): the
 * folder-era inputs (`onMoveToFolder`, `selection`, `folderCrumb`), the slot
 * chips as `SlotBadge` labels (`9ecebf986`), and the popovers' outside-press
 * close (`useClickOutside`). v4's browser jsdom file
 * `wardrobe-browser.folders.test.tsx` drives the row from the browser
 * (P4.D285's); these pin the row's half of it.
 */
describe('WardrobeItemRow — folders, select mode, slot labels (v4 4a38222e6)', () => {
  const owned = (): WardrobeItemDto => dto({ id: 'i1', characterId: 'c1', title: 'Greatcoat' });

  it('offers Move to folder… on a manageable row, after Archive and before the cross-wardrobe pair', async () => {
    const fixture = await render(owned(), undefined, [], { canArchive: true, canMoveToFolder: true });
    const seen: string[] = [];
    fixture.componentInstance.moveToFolder.subscribe((i: WardrobeItemDto) => seen.push(i.id));
    expect(kebabLabels(fixture)).toEqual([
      'Edit',
      '★ Mark as default outfit item',
      'Duplicate',
      'Archive',
      'Move to folder…',
      'Move to another wardrobe…',
      'Copy to another wardrobe…',
      'Delete',
    ]);
    const entry = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('[role="menu"] button'),
    ).find((b) => (b.textContent ?? '').trim() === 'Move to folder…') as HTMLButtonElement;
    entry.click();
    fixture.detectChanges();
    expect(seen).toEqual(['i1']);
    expect((fixture.nativeElement as HTMLElement).querySelector('[role="menu"]')).toBeNull();
  });

  it('withholds Move to folder… from a borrowed row and from a host that does not file', async () => {
    const borrowed = await render(dto({ id: 'b1', characterId: null }), () => false, [], {
      canMoveToFolder: true,
    });
    expect(kebabLabels(borrowed)).toEqual(['Move to another wardrobe…', 'Copy to another wardrobe…']);
    const noHost = await render(owned());
    expect(kebabLabels(noHost)).not.toContain('Move to folder…');
  });

  it('in select mode a manageable row leads with a checkbox; its controls go quiet', async () => {
    const toggles: number[] = [];
    const fixture = await render(owned(), undefined, [], {
      inChat: true,
      selection: { selected: true, onToggle: () => toggles.push(1) },
    });
    const host = fixture.nativeElement as HTMLElement;
    const row = host.querySelector('.qt-card-interactive > div')!;
    const box = row.firstElementChild as HTMLInputElement;
    expect(box.tagName).toBe('INPUT');
    expect(box.getAttribute('aria-label')).toBe('Select Greatcoat');
    expect(box.getAttribute('class')).toBe('qt-checkbox mt-1');
    expect(box.checked).toBe(true);
    box.click();
    expect(toggles).toEqual([1]);
    const button = (label: string): HTMLButtonElement =>
      [...host.querySelectorAll('button')].find(
        (b) =>
          b.getAttribute('aria-label') === label ||
          (b.textContent ?? '').trim() === label,
      ) as HTMLButtonElement;
    expect(button('Wear').disabled).toBe(true);
    expect(button('Layer onto top').disabled).toBe(true);
    expect(button('More actions').disabled).toBe(true);
  });

  it('a borrowed row in select mode gets a spacer, never a checkbox', async () => {
    const fixture = await render(dto({ id: 'b1', characterId: null }), () => false, [], {
      selection: { selected: false, onToggle: () => undefined },
    });
    const row = (fixture.nativeElement as HTMLElement).querySelector('.qt-card-interactive > div')!;
    expect(row.querySelector('input[type="checkbox"]')).toBeNull();
    const spacer = row.firstElementChild as HTMLElement;
    expect(spacer.getAttribute('class')).toBe('inline-block w-4');
    expect(spacer.getAttribute('aria-hidden')).toBe('true');
  });

  it('out of select mode there is no checkbox and the controls are live', async () => {
    const fixture = await render(owned(), undefined, [], { inChat: true });
    const host = fixture.nativeElement as HTMLElement;
    expect(host.querySelector('input[type="checkbox"]')).toBeNull();
    const kebab = host.querySelector('button[aria-label="More actions"]') as HTMLButtonElement;
    expect(kebab.disabled).toBe(false);
  });

  it('shows a search hit’s folder crumb between the chips and the wear line', async () => {
    const fixture = await render(owned(), undefined, [], { folderCrumb: 'Formal › Evening' });
    const host = fixture.nativeElement as HTMLElement;
    const crumb = host.querySelector('[data-testid="wardrobe-folder-crumb"]')!;
    expect(crumb.textContent!.trim()).toBe('Formal › Evening');
    expect(crumb.getAttribute('class')).toBe('qt-text-xs qt-text-secondary mt-0.5');
    expect(crumb.nextElementSibling!.getAttribute('data-testid')).toBe('wardrobe-wear-line');
    const none = await render(owned());
    expect(
      (none.nativeElement as HTMLElement).querySelector('[data-testid="wardrobe-folder-crumb"]'),
    ).toBeNull();
  });

  it('nested component rows get no folder entry, no checkbox and no crumb', async () => {
    const child = dto({ id: 'child', characterId: 'c1', title: 'Cravat' });
    const parent = dto({ id: 'parent', characterId: 'c1', title: 'Evening Kit', componentItemIds: ['child'] });
    const fixture = await render(parent, undefined, [child], {
      canMoveToFolder: true,
      folderCrumb: 'Formal',
    });
    const host = fixture.nativeElement as HTMLElement;
    (host.querySelector('button[aria-label="Expand components"]') as HTMLButtonElement).click();
    fixture.detectChanges();
    const nested = host.querySelector('qt-wardrobe-item-row') as HTMLElement;
    expect(nested.querySelector('[data-testid="wardrobe-folder-crumb"]')).toBeNull();
    (nested.querySelector('button[aria-label="More actions"]') as HTMLButtonElement).click();
    fixture.detectChanges();
    const labels = [...nested.querySelectorAll('[role="menu"] button')].map((b) =>
      (b.textContent ?? '').trim(),
    );
    expect(labels).toContain('Edit');
    expect(labels).not.toContain('Move to folder…');

    const selecting = await render(parent, undefined, [child], {
      selection: { selected: false, onToggle: () => undefined },
    });
    const sh = selecting.nativeElement as HTMLElement;
    (sh.querySelector('button[aria-label="Expand components"]') as HTMLButtonElement).click();
    selecting.detectChanges();
    const nestedRow = sh.querySelector('qt-wardrobe-item-row') as HTMLElement;
    expect(nestedRow.querySelector('input[type="checkbox"]')).toBeNull();
  });

  it('prints the slot chips as labels (SlotBadge), in the row and in the slot popover', async () => {
    const fixture = await render(
      dto({ id: 'i1', characterId: 'c1', types: ['top', 'accessories'] }),
      undefined,
      [],
      { inChat: true },
    );
    const host = fixture.nativeElement as HTMLElement;
    const chips = [...host.querySelectorAll('.flex-1 qt-slot-badge span')];
    expect(chips.map((c) => c.textContent)).toEqual(['Top', 'Accessories']);
    expect(chips[0].getAttribute('class')).toBe('qt-badge qt-badge-wardrobe-top');
    (host.querySelector('button[aria-label="Layer onto a slot"]') as HTMLButtonElement).click();
    fixture.detectChanges();
    const popover = [...host.querySelectorAll('.min-w-\\[10rem\\] qt-slot-badge span')];
    expect(popover.map((c) => c.textContent)).toEqual(['Top', 'Accessories']);
  });

  it('closes the kebab on a press outside it, not on one inside (v4 useClickOutside)', async () => {
    const fixture = await render(owned());
    const host = fixture.nativeElement as HTMLElement;
    kebabLabels(fixture);
    const menu = host.querySelector('[role="menu"]')!;
    menu.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    fixture.detectChanges();
    expect(host.querySelector('[role="menu"]')).not.toBeNull();
    document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    fixture.detectChanges();
    expect(host.querySelector('[role="menu"]')).toBeNull();
  });

  it('an Escape with the kebab open closes the kebab and NEVER reaches an enclosing dialog’s document listener (v4 escapeCapture)', async () => {
    // The wardrobe dialog's `(document:keydown.escape)` is registered BEFORE
    // its rows; only a capture-phase listener can stop the press reaching it.
    const enclosing = vi.fn();
    document.addEventListener('keydown', enclosing);
    try {
      const fixture = await render(owned());
      const host = fixture.nativeElement as HTMLElement;
      kebabLabels(fixture);
      expect(host.querySelector('[role="menu"]')).not.toBeNull();
      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
      fixture.detectChanges();
      expect(host.querySelector('[role="menu"]')).toBeNull();
      expect(enclosing).not.toHaveBeenCalled();
      // Closed, the row listens no more: the next Escape is the dialog's.
      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
      expect(enclosing).toHaveBeenCalledTimes(1);
    } finally {
      document.removeEventListener('keydown', enclosing);
    }
  });

  it('closes the slot popover on a press outside it', async () => {
    const fixture = await render(
      dto({ id: 'i1', characterId: 'c1', types: ['top', 'bottom'] }),
      undefined,
      [],
      { inChat: true },
    );
    const host = fixture.nativeElement as HTMLElement;
    (host.querySelector('button[aria-label="Layer onto a slot"]') as HTMLButtonElement).click();
    fixture.detectChanges();
    expect(host.querySelector('.min-w-\\[10rem\\]')).not.toBeNull();
    document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    fixture.detectChanges();
    expect(host.querySelector('.min-w-\\[10rem\\]')).toBeNull();
  });
});
