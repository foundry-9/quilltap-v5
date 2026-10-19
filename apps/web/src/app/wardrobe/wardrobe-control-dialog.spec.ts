import type { DebugElement } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { coreStreamStub } from '../core/core-client.testing';
import type { CoreRequest, WardrobeItemDto } from '../core/core-contract';
import type { EquippedSlots } from './equipped-slots';
import {
  WardrobeControlDialog,
  WardrobeControlDialogInner,
  WardrobeTabView,
} from './wardrobe-control-dialog';
import { WardrobeDialogService } from './wardrobe-dialog.service';
import { ToastService } from '../ui/toast.service';
import { WardrobeBrowser } from './wardrobe-browser';
import type { WardrobeItemActions } from './wardrobe-item-actions';
import { WardrobeItemOverlays } from './wardrobe-item-overlays';
import type { WardrobeListData } from './wardrobe-list-data';

type AnyRequest = CoreRequest & Record<string, unknown>;

function dto(partial: Partial<WardrobeItemDto> & { id: string }): WardrobeItemDto {
  return {
    characterId: 'c1',
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

const SHIRT = dto({ id: 'shirt', title: 'Shirt', isDefault: true });
const HAT = dto({ id: 'hat', title: 'Hat', types: ['accessories'] });
const WAVES = dto({ id: 'waves', title: 'Marcel Waves', types: ['hair'] });

const WORN: EquippedSlots = { top: ['shirt'], bottom: [], footwear: [], accessories: [], hair: [] };

/** The default route: two characters, c1's wardrobe, a worn outfit for c1. */
function defaultRoute(req: AnyRequest): Record<string, unknown> | Error {
  switch (req.type as string) {
    case 'characterList':
      return {
        characters: [
          { id: 'c2', name: 'Zed', defaultImage: null, defaultImageId: null },
          { id: 'c1', name: 'Aria', defaultImage: null, defaultImageId: null },
        ],
      };
    case 'imageProfileList':
      return {
        profiles: [
          { id: 'ip1', name: 'Painterly', provider: 'p', modelName: 'm', isDefault: true },
        ],
      };
    case 'projectList':
      return { projects: [{ id: 'p1', name: 'The Estate' }] };
    case 'groupList':
      return { groups: [{ id: 'g1', name: 'The Regiment' }] };
    case 'chatGet':
      return { chat: { projectId: null } };
    case 'characterWardrobeList':
      return { wardrobeItems: (req as { characterId?: string }).characterId === 'c1' ? [SHIRT, HAT] : [] };
    case 'wardrobeList':
      return { wardrobeItems: [] };
    case 'chatOutfitGet':
      return { equippedOutfit: { c1: WORN } };
    case 'chatEquip':
      return { equippedSlots: WORN };
    // P4.D121: the dressing-instructions section reads its container's file on
    // every open. Answering here keeps unrelated beats from tripping its
    // fail-soft warn arm.
    case 'characterWardrobeInstructionsGet':
    case 'groupWardrobeInstructionsGet':
    case 'projectWardrobeInstructionsGet':
    case 'wardrobeInstructionsGet':
      return { instructions: null };
    // P4.D285: the browser reads its container's folder tree on every open.
    case 'wardrobeFoldersList':
      return { folders: [] };
    default:
      return new Error(`unexpected ${req.type}`);
  }
}

interface Handle {
  fixture: ComponentFixture<WardrobeControlDialogInner>;
  component: WardrobeControlDialogInner;
  seen: AnyRequest[];
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 12): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await Promise.resolve();
    fixture.detectChanges();
  }
}

async function renderInner(
  chatId: string | null,
  route: (req: AnyRequest) => Record<string, unknown> | Error = defaultRoute,
): Promise<Handle> {
  const seen: AnyRequest[] = [];
  const core = {
    // RealtimeService (the `characters` hook, P4.D261) reads the stream.
    ...coreStreamStub(),
    dispatchData: vi.fn(async (req: CoreRequest) => {
      const r = req as AnyRequest;
      seen.push(r);
      const out = route(r);
      if (out instanceof Error) throw out;
      return out;
    }),
  } as unknown as CoreClient;

  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [WardrobeControlDialogInner],
    // The editor's Picture section (edit mode) reads through TanStack (P4.D261).
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: core }],
  });
  const fixture = TestBed.createComponent(WardrobeControlDialogInner);
  roots.set(fixture.componentInstance, fixture.debugElement);
  fixture.componentRef.setInput('initialCharacterId', 'c1');
  fixture.componentRef.setInput('chatId', chatId);
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, component: fixture.componentInstance, seen };
}

/** Each rendered inner's fixture, so the adapter below can reach its children. */
const roots = new WeakMap<WardrobeControlDialogInner, DebugElement>();

/** The dialog's own surface after the Phase-F split (P4.D285). */
interface InnerSurface {
  rightTab: { (): string; set(v: string): void };
  liveStagedByChar: () => Record<string, EquippedSlots>;
  fittingSlots: { (): EquippedSlots; set(v: EquippedSlots): void };
  rowEquip: (item: WardrobeItemDto) => void;
  rowAddToSlot: (item: WardrobeItemDto, slot: string) => void;
  wearFitting: () => Promise<void>;
  requestClose: () => void;
  useFittingActions: () => boolean;
  selectedContainer: {
    (): { scope: string; id: string | null } | null;
    set(v: { scope: string; id: string | null } | null): void;
  };
  selectedCharacterId: () => string | null;
  isCharacterScope: () => boolean;
  selectedContainerLabel: () => string;
  pickContainer: (value: string) => void;
  showArchived: { (): boolean; set(v: boolean): void };
  data: WardrobeListData;
  actions: WardrobeItemActions;
  liveHandlers: { onAddToSlot(s: string, id: string): void; onRemoveFromSlot(s: string, id: string): void; onClearSlot(s: string): void };
  fittingHandlers: { onAddToSlot(s: string, id: string): void; onClearSlot(s: string): void };
}

/** The browser's filter state (it moved out of the dialog with the left column). */
interface BrowserSurface {
  titleFilter: { (): string; set(v: string): void };
  slotFilter: { set(v: string): void };
  showShared: { (): boolean; set(v: boolean): void };
  filteredItems: () => WardrobeItemDto[];
}

function browserOf(component: WardrobeControlDialogInner): BrowserSurface {
  return roots.get(component)!.query(By.directive(WardrobeBrowser))
    .componentInstance as unknown as BrowserSurface;
}

/**
 * Reach the protected surface for state-level assertions. **Re-pinned at the
 * Phase-F split (P4.D285):** the spec's cases keep their pre-split vocabulary
 * and this adapter maps each name onto where v4 `9ecebf986` / `4a38222e6`
 * moved it — the filters onto the browser, the row handlers onto the actions
 * store (`handleDuplicate` → `actions.duplicate`, …), the lists onto the
 * list-data store, the Live / Builder handlers onto the shared composer
 * gestures, the transfer exclusion onto the overlays. The pairing is the
 * port's evidence that each behaviour survived the move.
 */
function inner(component: WardrobeControlDialogInner) {
  const c = component as unknown as InnerSurface;
  const overlays = () =>
    roots.get(component)!.query(By.directive(WardrobeItemOverlays))
      .componentInstance as unknown as { transferExclude(): { scope: string; id: string | null } | null };
  return {
    rightTab: c.rightTab,
    liveStagedByChar: c.liveStagedByChar,
    fittingSlots: c.fittingSlots,
    get titleFilter() {
      return browserOf(component).titleFilter;
    },
    /**
     * v4 pre-split `handleEquipItem` → the REAL `rowEquip` on the Live tab
     * (v4 `:326` routes a row to Live staging when the Live tab is showing).
     */
    handleEquipItem: (item: WardrobeItemDto) => {
      c.rightTab.set('live');
      c.rowEquip(item);
    },
    /** v4 pre-split `handleAddToSlot` → the REAL `rowAddToSlot` on the Live tab. */
    handleAddToSlot: (item: WardrobeItemDto, slot: string) => {
      c.rightTab.set('live');
      c.rowAddToSlot(item, slot);
    },
    handleSlotAdd: (slot: string, itemId: string) => c.liveHandlers.onAddToSlot(slot, itemId),
    handleSlotRemove: (slot: string, itemId: string) => c.liveHandlers.onRemoveFromSlot(slot, itemId),
    handleSlotClear: (slot: string) => c.liveHandlers.onClearSlot(slot),
    rowEquip: (item: WardrobeItemDto) => c.rowEquip(item),
    fittingAdd: (slot: string, itemId: string) => c.fittingHandlers.onAddToSlot(slot, itemId),
    fittingClear: (slot: string) => c.fittingHandlers.onClearSlot(slot),
    wearFitting: () => c.wearFitting(),
    fittingResetToDefaults: () => (component as unknown as { fittingResetToDefaults(): void }).fittingResetToDefaults(),
    fittingResetToWorn: () => (component as unknown as { fittingResetToWorn(): void }).fittingResetToWorn(),
    fittingClearAll: () => (component as unknown as { fittingClearAll(): void }).fittingClearAll(),
    requestClose: () => c.requestClose(),
    useFittingActions: c.useFittingActions,
    selectedContainer: c.selectedContainer,
    selectedCharacterId: c.selectedCharacterId,
    isCharacterScope: c.isCharacterScope,
    listItems: () => c.data.listItems(),
    resolutionPool: () => c.data.resolutionPool(),
    canManageItem: (item: WardrobeItemDto) => c.data.canManage(item),
    selectedContainerLabel: c.selectedContainerLabel,
    transferExcludeDestination: () => overlays().transferExclude(),
    pickContainer: (value: string) => c.pickContainer(value),
    handleDuplicate: (item: WardrobeItemDto) => c.actions.duplicate(item),
    handleToggleDefault: (item: WardrobeItemDto) => c.actions.toggleDefault(item),
    handleToggleArchived: (item: WardrobeItemDto) => c.actions.toggleArchived(item),
    handleDelete: (item: WardrobeItemDto) => c.actions.remove(item),
    showArchived: c.showArchived,
    get showShared() {
      return browserOf(component).showShared;
    },
    filteredItems: () => browserOf(component).filteredItems(),
    transferringItem: Object.assign(() => c.actions.transfer()?.item ?? null, {
      set: (v: WardrobeItemDto | null) => (v ? c.actions.move(v) : c.actions.closeTransfer()),
    }),
  };
}

const equips = (seen: AnyRequest[]): AnyRequest[] =>
  seen.filter((r) => (r.type as string) === 'chatEquip');

describe('WardrobeControlDialogInner — chat-aware behavior', () => {
  afterEach(() => vi.restoreAllMocks());

  it('offers a slot filter per slot plus All, Hair included (v4 SLOT_FILTERS, P4.D88)', async () => {
    const { fixture, component } = await renderInner(null, (req) =>
      (req.type as string) === 'characterWardrobeList'
        ? { wardrobeItems: (req as { characterId?: string }).characterId === 'c1' ? [SHIRT, HAT, WAVES] : [] }
        : defaultRoute(req),
    );
    const chips = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('button.qt-button-sm'),
    ).map((b) => b.textContent?.trim());
    for (const label of ['All', 'Top', 'Bottom', 'Footwear', 'Accessories', 'Hair']) {
      expect(chips).toContain(label);
    }

    // …and the Hair chip narrows the wardrobe LIST to hairdos (v4
    // `filteredItems`). Scope to the rows: the Builder's slot rows carry the
    // default-outfit shirt regardless of the filter.
    browserOf(component).slotFilter.set('hair');
    fixture.detectChanges();
    const rows = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('qt-wardrobe-item-row'),
    ).map((r) => r.textContent ?? '');
    expect(rows.some((t) => t.includes('Marcel Waves'))).toBe(true);
    expect(rows.some((t) => t.includes('Shirt'))).toBe(false);
    expect(rows.some((t) => t.includes('Hat'))).toBe(false);
  });

  it('rightTab defaults to live in chat and builder out of chat (v4 :203-204)', async () => {
    const inChat = await renderInner('chat-1');
    expect(inner(inChat.component).rightTab()).toBe('live');
    const outOfChat = await renderInner(null);
    expect(inner(outOfChat.component).rightTab()).toBe('builder');
  });

  it('the four Live staging handlers are pure state — they fire NO route (v4 :476,:484,:499,:514)', async () => {
    const { component, seen } = await renderInner('chat-1');
    const c = inner(component);
    const baseline = equips(seen).length;

    c.handleEquipItem(HAT);
    c.handleAddToSlot(HAT, 'accessories');
    c.handleSlotAdd('top', 'hat');
    c.handleSlotRemove('top', 'shirt');
    c.handleSlotClear('accessories');

    expect(equips(seen).length).toBe(baseline);
    expect(baseline).toBe(0);
    // …and the staged map DID change (seeded from worn, then mutated).
    expect(c.liveStagedByChar()['c1'].top).toEqual([]);
  });

  it('Done fires ONE set_all per dirty character and none for clean ones (v4 :648-664)', async () => {
    const { component, fixture, seen } = await renderInner('chat-1');
    const c = inner(component);
    const closedSpy = vi.fn();
    component.closed.subscribe(closedSpy);

    // Stage a real change for c1 (baseline is the worn snapshot).
    c.handleEquipItem(HAT);
    c.requestClose();
    await settle(fixture);

    const flushed = equips(seen);
    expect(flushed).toEqual([
      {
        type: 'chatEquip',
        chatId: 'chat-1',
        characterId: 'c1',
        mode: 'set_all',
        slots: {
          top: ['shirt'],
          bottom: [],
          footwear: [],
          accessories: ['hat'],
          hair: [],
        },
      },
    ]);
    expect(closedSpy).toHaveBeenCalledTimes(1);
  });

  it('Done with nothing dirty fires no equip at all (v4 :659 early return)', async () => {
    const { component, fixture, seen } = await renderInner('chat-1');
    const closedSpy = vi.fn();
    component.closed.subscribe(closedSpy);
    inner(component).requestClose();
    await settle(fixture);
    expect(equips(seen)).toEqual([]);
    expect(closedSpy).toHaveBeenCalledTimes(1);
  });

  it('a failed flush keeps the dialog open (v4 requestClose only closes when ok)', async () => {
    const { component, fixture, seen } = await renderInner('chat-1', (req) =>
      (req.type as string) === 'chatEquip' ? new Error('boom') : defaultRoute(req),
    );
    const c = inner(component);
    const closedSpy = vi.fn();
    component.closed.subscribe(closedSpy);
    c.handleEquipItem(HAT);
    c.requestClose();
    await settle(fixture);
    expect(equips(seen).length).toBe(1);
    expect(closedSpy).not.toHaveBeenCalled();
  });

  it('Try on fires set_all with the fitting slots then closes (v4 :700-722)', async () => {
    const { component, fixture, seen } = await renderInner('chat-1');
    const c = inner(component);
    const closedSpy = vi.fn();
    component.closed.subscribe(closedSpy);
    c.rightTab.set('builder');
    c.fittingClear('top');
    await c.wearFitting();
    await settle(fixture);
    const wear = equips(seen);
    expect(wear).toEqual([
      {
        type: 'chatEquip',
        chatId: 'chat-1',
        characterId: 'c1',
        mode: 'set_all',
        slots: { top: [], bottom: [], footwear: [], accessories: [], hair: [] },
      },
    ]);
    expect(closedSpy).toHaveBeenCalledTimes(1);
  });

  it('OUT OF CHAT: not one equip call, EVER (v4 :730 + :200)', async () => {
    const { component, fixture, seen } = await renderInner(null);
    const c = inner(component);
    // useFittingActions is forced true out of chat (v4 :730).
    expect(c.useFittingActions()).toBe(true);
    // Row equip, fitting mutations, staging handlers (no-ops out of chat),
    // wearFitting (guard-bails without a chat), and Done — none may equip.
    c.rowEquip(HAT);
    c.fittingAdd('top', 'shirt');
    c.fittingClear('top');
    c.handleEquipItem(HAT);
    c.handleSlotAdd('top', 'hat');
    await c.wearFitting();
    c.requestClose();
    await settle(fixture);
    expect(equips(seen)).toEqual([]);
    // The dispatch log holds ONLY reads (no mutation verb of any kind).
    expect(
      seen.every((r) =>
        [
          'characterList',
          'imageProfileList',
          'projectList',
          'groupList',
          'characterWardrobeList',
          'wardrobeList',
          // P4.D121: the dressing-instructions section reads on open. A READ,
          // and the point of this assertion is that no MUTATION fires.
          'characterWardrobeInstructionsGet',
          // P4.D285: the browser reads its container's folder tree. A READ.
          'wardrobeFoldersList',
        ].includes(r.type as string),
      ),
    ).toBe(true);
  });

  it('out of chat the fitting room seeds from the character defaults (v4 :316-319)', async () => {
    const { component } = await renderInner(null);
    expect(inner(component).fittingSlots().top).toEqual(['shirt']);
  });

  it('in chat the fitting room seeds from the worn snapshot (v4 :316-317)', async () => {
    const { component } = await renderInner('chat-1');
    expect(inner(component).fittingSlots()).toEqual(WORN);
  });
});

// ---------------------------------------------------------------------------
// The container browser (v4 `d7263f39` — the top selector lists every place a
// wardrobe item can live, and a shared container is edited in place)
// ---------------------------------------------------------------------------

const GENERAL_ITEM = dto({ id: 'gen', title: 'Domino Mask', characterId: null });
const PROJECT_ITEM = dto({ id: 'proj', title: 'Estate Livery', characterId: null });

/** A route whose General and project tiers carry their own distinct items. */
function containerRoute(req: AnyRequest): Record<string, unknown> | Error {
  switch (req.type as string) {
    case 'wardrobeList':
      return { wardrobeItems: [GENERAL_ITEM] };
    case 'projectWardrobeList':
      return { wardrobeItems: [PROJECT_ITEM] };
    default:
      return defaultRoute(req);
  }
}

describe('the wardrobe container selector (v4 :1061-1135)', () => {
  afterEach(() => vi.restoreAllMocks());

  it('lists characters, General, projects and groups in v4\'s optgroup order', async () => {
    const { fixture } = await renderInner(null, containerRoute);
    const select = (fixture.nativeElement as HTMLElement).querySelector(
      '#wardrobe-container-select',
    ) as HTMLSelectElement;
    expect([...select.querySelectorAll('optgroup')].map((g) => g.label)).toEqual([
      'Characters',
      'General',
      'Projects',
      'Groups',
    ]);
    expect([...select.querySelectorAll('option')].map((o) => o.value)).toEqual([
      'character:c1',
      'character:c2',
      'general:',
      'project:p1',
      'group:g1',
    ]);
    // The label is "Wardrobe:", not "Character:" (v4 :1064-1066).
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('Wardrobe:');
  });

  it('browsing General lists ONLY that container and drops the character column', async () => {
    const { fixture, component } = await renderInner(null, containerRoute);
    const c = inner(component);
    c.pickContainer('general:');
    await settle(fixture);
    expect(c.isCharacterScope()).toBe(false);
    expect(c.selectedCharacterId()).toBeNull();
    expect(c.listItems().map((i) => i.id)).toEqual(['gen']);
    expect(c.selectedContainerLabel()).toBe('Quilltap General');
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Browsing a shared wardrobe');
    // v4 keeps the right-hand outfit column behind `selectedCharacterId`
    // (`:1260`), so it is gone here — there is nobody to dress.
    expect(text).not.toContain('Outfit Builder');
  });

  it('a shared container grants the full kebab; the character view does not (v4 :442-450)', async () => {
    const { fixture, component } = await renderInner(null, containerRoute);
    const c = inner(component);
    // Character view: a shared archetype merged in is NOT manageable.
    expect(c.canManageItem(SHIRT)).toBe(true);
    expect(c.canManageItem(GENERAL_ITEM)).toBe(false);
    // Browsing General: its own item IS manageable, a stranger is not.
    c.pickContainer('general:');
    await settle(fixture);
    expect(c.canManageItem(GENERAL_ITEM)).toBe(true);
    expect(c.canManageItem(PROJECT_ITEM)).toBe(false);
  });

  it('the project container resolves its label and pool from the catalogue', async () => {
    const { fixture, component } = await renderInner(null, containerRoute);
    const c = inner(component);
    c.pickContainer('project:p1');
    await settle(fixture);
    expect(c.selectedContainerLabel()).toBe('The Estate');
    expect(c.listItems().map((i) => i.id)).toEqual(['proj']);
    // The resolution pool folds in General so a borrowed component still
    // renders (v4 `use-wardrobe-container-items.ts:14-17`).
    expect(c.resolutionPool().map((i) => i.id)).toEqual(['proj', 'gen']);
  });

  it('a mangled option value parks on the placeholder rather than guessing (v4 :1077)', async () => {
    const { fixture, component } = await renderInner(null, containerRoute);
    const c = inner(component);
    c.pickContainer('bogus:x');
    await settle(fixture);
    expect(c.selectedContainer()).toBeNull();
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      'Select a wardrobe to browse.',
    );
  });

  it('Duplicate in a shared container POSTs into THAT container (v4 :526-534)', async () => {
    const { fixture, component, seen } = await renderInner(null, containerRoute);
    const c = inner(component);
    c.pickContainer('project:p1');
    await settle(fixture);
    await c.handleDuplicate(PROJECT_ITEM);
    await settle(fixture);
    const create = seen.filter((r) => (r.type as string).endsWith('WardrobeCreate'));
    expect(create.at(-1)).toMatchObject({ type: 'projectWardrobeCreate', projectId: 'p1' });
  });

  it('the star and Delete in a shared container target THAT container (v4 :495-518)', async () => {
    const { fixture, component, seen } = await renderInner(null, containerRoute);
    const c = inner(component);
    c.pickContainer('project:p1');
    await settle(fixture);
    await c.handleToggleDefault(PROJECT_ITEM);
    expect(seen.at(-1)).toMatchObject({ type: 'projectWardrobeUpdate', projectId: 'p1' });
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    await c.handleDelete(PROJECT_ITEM);
    expect(
      seen.filter((r) => (r.type as string) === 'projectWardrobeDelete').at(-1),
    ).toMatchObject({ projectId: 'p1', itemId: 'proj' });
  });

  it("the transfer dialog hides the item's known home (v4 :1483-1491)", async () => {
    const { fixture, component } = await renderInner(null, containerRoute);
    const c = inner(component);
    // Character view: only a character-OWNED item's own vault is excluded; a
    // merged shared item's home tier isn't tracked, so nothing is.
    c.transferringItem.set(SHIRT);
    expect(c.transferExcludeDestination()).toEqual({ scope: 'character', id: 'c1' });
    c.transferringItem.set(GENERAL_ITEM);
    expect(c.transferExcludeDestination()).toBeNull();
    // Shared container: the container itself.
    c.pickContainer('project:p1');
    await settle(fixture);
    expect(c.transferExcludeDestination()).toEqual({ scope: 'project', id: 'p1' });
  });

  it('the character view is unchanged — merged four-tier read, wear buttons, right column', async () => {
    const { fixture, component } = await renderInner(null, containerRoute);
    const c = inner(component);
    expect(c.isCharacterScope()).toBe(true);
    expect(c.selectedCharacterId()).toBe('c1');
    // Personal items plus the merged General archetype (v4's tier merge).
    expect(c.listItems().map((i) => i.id).sort()).toEqual(['gen', 'hat', 'shirt']);
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Outfit Builder');
    expect(text).not.toContain('Browsing a shared wardrobe');
  });
});

describe('the asTab chrome switch (v4 WardrobeShell :128-164 / WardrobeView :105)', () => {
  afterEach(() => vi.restoreAllMocks());

  function makeCore(seen: AnyRequest[]): CoreClient {
    return {
      ...coreStreamStub(),
      dispatchData: vi.fn(async (req: CoreRequest) => {
        const r = req as AnyRequest;
        seen.push(r);
        const out = defaultRoute(r);
        if (out instanceof Error) throw out;
        return out;
      }),
    } as unknown as CoreClient;
  }

  it('asTab renders BARE (no overlay chrome) while the dialog wraps in the modal', async () => {
    // asTab false — the floating modal chrome.
    const modal = await renderInner(null);
    const modalEl = modal.fixture.nativeElement as HTMLElement;
    expect(modalEl.querySelector('.qt-dialog-overlay')).not.toBeNull();
    expect(modalEl.querySelector('.qt-dialog-footer')).not.toBeNull();
    expect(modalEl.querySelector('.qt-wardrobe-tab')).toBeNull();

    // asTab true (the WardrobeView tab) — bare scroll container, no overlay,
    // no title header, no footer.
    const seen: AnyRequest[] = [];
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [WardrobeTabView],
      providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: makeCore(seen) }],
    });
    const fixture = TestBed.createComponent(WardrobeTabView);
    fixture.detectChanges();
    await settle(fixture);
    const tabEl = fixture.nativeElement as HTMLElement;
    expect(tabEl.querySelector('.qt-wardrobe-tab')).not.toBeNull();
    expect(tabEl.querySelector('.qt-dialog-overlay')).toBeNull();
    expect(tabEl.querySelector('.qt-dialog-footer')).toBeNull();
    // The wardrobe body still renders inside the bare container.
    expect(tabEl.querySelector('#wardrobe-container-select')).not.toBeNull();
  });

  it('the tab has NO Live-outfit tab and fires NO equip route (chatId null)', async () => {
    const seen: AnyRequest[] = [];
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [WardrobeTabView],
      providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: makeCore(seen) }],
    });
    const fixture = TestBed.createComponent(WardrobeTabView);
    fixture.componentRef.setInput('characterId', 'c1');
    fixture.detectChanges();
    await settle(fixture);
    const el = fixture.nativeElement as HTMLElement;

    // No "Live outfit" tab button (only Outfit Builder).
    const tabButtons = [...el.querySelectorAll('.qt-tab')].map((b) => b.textContent ?? '');
    expect(tabButtons.some((t) => /Live outfit/.test(t))).toBe(false);
    expect(tabButtons.some((t) => /Outfit Builder/.test(t))).toBe(true);

    // The dispatch log holds ONLY reads — no chatEquip, no chat-scoped verb.
    expect(equips(seen)).toEqual([]);
    expect(
      seen.every((r) =>
        [
          'characterList',
          'imageProfileList',
          'projectList',
          'groupList',
          'characterWardrobeList',
          'wardrobeList',
          // P4.D121: the dressing-instructions section reads on open. A READ,
          // and the point of this assertion is that no MUTATION fires.
          'characterWardrobeInstructionsGet',
          // P4.D285: the browser reads its container's folder tree. A READ.
          'wardrobeFoldersList',
        ].includes(r.type as string),
      ),
    ).toBe(true);
  });

  it('a payload-less tab auto-selects the first character (v4 WardrobeView, no characterId)', async () => {
    const seen: AnyRequest[] = [];
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [WardrobeTabView],
      providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: makeCore(seen) }],
    });
    const fixture = TestBed.createComponent(WardrobeTabView);
    // NO characterId input — the rail opens the tab without a payload.
    fixture.detectChanges();
    await settle(fixture);
    const el = fixture.nativeElement as HTMLElement;
    const select = el.querySelector('#wardrobe-container-select') as HTMLSelectElement;
    // Characters sort by name (Aria < Zed) → the first is Aria (c1), now named
    // as an encoded container (v4 `encodeWardrobeContainer`).
    expect(select.value).toBe('character:c1');
  });
});

describe('WardrobeControlDialog host — the remount key (v4 :92-93)', () => {
  it('a context change REMOUNTS the inner and discards staged state', async () => {
    const seen: AnyRequest[] = [];
    const core = {
      ...coreStreamStub(),
      dispatchData: vi.fn(async (req: CoreRequest) => {
        const r = req as AnyRequest;
        seen.push(r);
        const out = defaultRoute(r);
        if (out instanceof Error) throw out;
        return out;
      }),
    } as unknown as CoreClient;

    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [WardrobeControlDialog],
      providers: [
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: core },
        WardrobeDialogService,
      ],
    });
    const service = TestBed.inject(WardrobeDialogService);
    const fixture = TestBed.createComponent(WardrobeControlDialog);
    fixture.detectChanges();

    // Closed → renders nothing (v4 :89).
    expect(fixture.debugElement.query(By.directive(WardrobeControlDialogInner))).toBeNull();

    service.open({ characterId: 'c1' });
    await settle(fixture);
    const firstDe = fixture.debugElement.query(By.directive(WardrobeControlDialogInner));
    const first = firstDe.componentInstance as WardrobeControlDialogInner;
    roots.set(first, firstDe);
    inner(first).titleFilter.set('sweater');

    // Same key → same instance survives.
    service.open({ characterId: 'c1' });
    await settle(fixture);
    const same = fixture.debugElement.query(By.directive(WardrobeControlDialogInner))
      .componentInstance as WardrobeControlDialogInner;
    expect(same).toBe(first);

    // Different key → a NEW inner with fresh state.
    service.open({ characterId: 'c2' });
    await settle(fixture);
    const secondDe = fixture.debugElement.query(By.directive(WardrobeControlDialogInner));
    const second = secondDe.componentInstance as WardrobeControlDialogInner;
    roots.set(second, secondDe);
    expect(second).not.toBe(first);
    expect(inner(second).titleFilter()).toBe('');
  });
});

/**
 * The dissolution threading (v4 4.8.2 `61574563`): the dialog's wear/add sites
 * hand `itemsById` to the pure math, so a bundle stages as its PARTS and its own
 * id never reaches the staged slots. The pure rule itself is pinned in
 * `dissolve-bundles.spec.ts`; these cases pin that the call sites pass the
 * lookup at all — without it the math silently falls back to storing the bundle
 * whole and every pure test still passes.
 */
describe('WardrobeControlDialogInner — bundles dissolve as they go on', () => {
  afterEach(() => vi.restoreAllMocks());

  const LEAF_TOP = dto({ id: 'leaf-top', title: 'Leaf Top', types: ['top'] });
  const LEAF_SHOE = dto({ id: 'leaf-shoe', title: 'Leaf Shoe', types: ['footwear'] });
  const BUNDLE = dto({
    id: 'bundle',
    title: 'Man in Black',
    types: ['top', 'footwear'],
    componentItemIds: ['leaf-top', 'leaf-shoe'],
  });

  const bundleRoute = (req: AnyRequest): Record<string, unknown> | Error => {
    if ((req.type as string) === 'characterWardrobeList') {
      return {
        wardrobeItems:
          (req as { characterId?: string }).characterId === 'c1'
            ? [LEAF_TOP, LEAF_SHOE, BUNDLE]
            : [],
      };
    }
    if ((req.type as string) === 'chatOutfitGet') {
      return { equippedOutfit: { c1: { top: [], bottom: [], footwear: [], accessories: [], hair: [] } } };
    }
    return defaultRoute(req);
  };

  it('Wear stages the parts, never the bundle id (v4 handleEquipItem :492-497)', async () => {
    const { component } = await renderInner('chat-1', bundleRoute);
    const c = inner(component);
    c.handleEquipItem(BUNDLE);

    const staged = c.liveStagedByChar()['c1'];
    expect(staged.top).toEqual(['leaf-top']);
    expect(staged.footwear).toEqual(['leaf-shoe']);
    expect(JSON.stringify(staged)).not.toContain('bundle');
  });

  it('adding to a slot contributes only the part covering it (v4 handleAddToSlot :499-508)', async () => {
    const { component } = await renderInner('chat-1', bundleRoute);
    const c = inner(component);
    c.handleAddToSlot(BUNDLE, 'footwear');

    const staged = c.liveStagedByChar()['c1'];
    expect(staged.footwear).toEqual(['leaf-shoe']);
    expect(staged.top).toEqual([]);
  });

  it('picking a bundle from a slot row wears its parts (v4 handleSlotAdd :513-522)', async () => {
    const { component } = await renderInner('chat-1', bundleRoute);
    const c = inner(component);
    c.handleSlotAdd('top', 'bundle');

    const staged = c.liveStagedByChar()['c1'];
    expect(staged.top).toEqual(['leaf-top']);
    expect(staged.footwear).toEqual(['leaf-shoe']);
  });

  it('the fitting room dissolves too (v4 fitting wear sites :551 / :761)', async () => {
    const { component } = await renderInner(null, bundleRoute);
    const c = inner(component);
    c.fittingClear('top');
    c.fittingAdd('top', 'bundle');

    expect(c.fittingSlots().top).toEqual(['leaf-top']);
    expect(c.fittingSlots().footwear).toEqual(['leaf-shoe']);
  });
});

/**
 * P4.D121 — the archive surface in the dialog (v4 `d25dacc1`), plus the
 * dressing-instructions section's mount (v4 `b86bb1a5` `:1139`).
 *
 * The load-bearing claim is that the hiding is SERVER-side: v5's own
 * client-side `if (i.archivedAt) return false` is gone, so an archived garment
 * that comes back from a fetch RENDERS. Without the fetch flag AND the deleted
 * filter, one of those two halves would silently mask the other.
 */
describe('WardrobeControlDialogInner — archived garments (v4 d25dacc1)', () => {
  const ARCHIVED_AT = '2026-02-01T00:00:00.000Z';
  const RETIRED = dto({ id: 'retired', title: 'Retired Cloak', archivedAt: ARCHIVED_AT });

  afterEach(() => vi.restoreAllMocks());

  /** Answers every wardrobe list, honouring `includeArchived` the way a server would. */
  function archiveRoute(req: AnyRequest): Record<string, unknown> | Error {
    if ((req.type as string) === 'characterWardrobeList') {
      if ((req as { characterId?: string }).characterId !== 'c1') return { wardrobeItems: [] };
      const include = (req as { includeArchived?: boolean }).includeArchived === true;
      return { wardrobeItems: include ? [SHIRT, HAT, RETIRED] : [SHIRT, HAT] };
    }
    return defaultRoute(req);
  }

  it('every tier read carries includeArchived, and the flip is a NEW fetch', async () => {
    const { fixture, component, seen } = await renderInner(null, archiveRoute);
    const lists = () => seen.filter((r) => (r.type as string) === 'characterWardrobeList');
    expect(lists().length).toBeGreaterThan(0);
    expect(lists().every((r) => r['includeArchived'] === false)).toBe(true);
    expect(seen.filter((r) => (r.type as string) === 'wardrobeList').every((r) => r['includeArchived'] === false)).toBe(true);
    const before = lists().length;

    inner(component).showArchived.set(true);
    fixture.detectChanges();
    await settle(fixture);

    expect(lists().length).toBeGreaterThan(before);
    expect(lists().slice(before).every((r) => r['includeArchived'] === true)).toBe(true);
  });

  it('an archived garment the fetch returned RENDERS — no client-side filter survives', async () => {
    const { fixture, component } = await renderInner(null, archiveRoute);
    expect(inner(component).filteredItems().some((i) => i.id === 'retired')).toBe(false);

    inner(component).showArchived.set(true);
    fixture.detectChanges();
    await settle(fixture);

    expect(inner(component).filteredItems().some((i) => i.id === 'retired')).toBe(true);
    const rows = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('qt-wardrobe-item-row'),
    ).map((r) => r.textContent ?? '');
    expect(rows.some((t) => t.includes('Retired Cloak') && t.includes('archived'))).toBe(true);
  });

  it('the resolution pool ALWAYS asks for archived archetypes (v4 :71-77)', async () => {
    const { component, fixture, seen } = await renderInner(null, archiveRoute);
    inner(component).selectedContainer.set({ scope: 'project', id: 'p1' });
    fixture.detectChanges();
    await settle(fixture);
    const generalReads = seen.filter((r) => (r.type as string) === 'wardrobeList');
    // The container's own list follows the checkbox; the archetype pool does not.
    expect(generalReads.some((r) => r['includeArchived'] === true)).toBe(true);
  });

  it('archiving routes to the item’s own tier and re-reads, with no confirm', async () => {
    const { component, seen } = await renderInner(null, archiveRoute);
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
    await inner(component).handleToggleArchived(HAT);
    const update = seen.find((r) => (r.type as string) === 'characterWardrobeUpdate');
    expect(update).toBeTruthy();
    expect(update!['itemId']).toBe('hat');
    expect(update!['item']).toEqual({ archived: true });
    expect(confirm).not.toHaveBeenCalled();
  });

  it('restoring sends archived:false (v4’s `!item.archivedAt`)', async () => {
    const { component, seen } = await renderInner(null, archiveRoute);
    await inner(component).handleToggleArchived(RETIRED);
    const update = seen.find((r) => (r.type as string) === 'characterWardrobeUpdate');
    expect(update!['item']).toEqual({ archived: false });
  });

  it('mounts the dressing-instructions section for the browsed container (v4 b86bb1a5 :1139)', async () => {
    const { fixture, seen } = await renderInner(null, archiveRoute);
    expect(
      (fixture.nativeElement as HTMLElement).querySelector('qt-wardrobe-instructions-section'),
    ).toBeTruthy();
    expect(
      seen.some((r) => (r.type as string) === 'characterWardrobeInstructionsGet'),
    ).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// "Show shared" (P4.D188; v4 `055cac45a` +
// `__tests__/unit/components/wardrobe/wardrobe-control-dialog.shared-filter.test.tsx`)
// ---------------------------------------------------------------------------

/**
 * The character view lists the merge of the character's own garments with
 * every shared tier above them (group / project / Quilltap General). Those
 * borrowed rows can't be edited from here (they carry v4 `cc80dc89d`'s origin
 * chip when the read tagged them — the `· shared` text is retired), and when
 * you're dressing a character or building an outfit out of their own clothes
 * they're just noise. The toggle is on by default — hiding is opt-in — and it
 * filters after the merge, since ownership isn't something the fetch can ask
 * the server for.
 */

/** A Quilltap General archetype merged in from above (no `origin` here, so no chip). */
const SHARED_WATCH = dto({ id: 'watch', title: 'Apple Watch', types: ['accessories'] });
(SHARED_WATCH as { characterId: string | null }).characterId = null;

/** Alice's own garment — full management, no badge. */
const OWN_LINEN = dto({ id: 'linen', title: 'Linen Shirt' });

/** c1's own tier answers one row; the General tier answers the shared one. */
function sharedRoute(req: AnyRequest): Record<string, unknown> | Error {
  switch (req.type as string) {
    case 'characterWardrobeList':
      return {
        wardrobeItems:
          (req as { characterId?: string }).characterId === 'c1' &&
          (req as { scope?: string }).scope === undefined
            ? [OWN_LINEN]
            : [],
      };
    case 'wardrobeList':
      return { wardrobeItems: [SHARED_WATCH] };
    default:
      return defaultRoute(req);
  }
}

const sharedBox = (fixture: ComponentFixture<unknown>): HTMLInputElement | null => {
  const labels = Array.from(
    (fixture.nativeElement as HTMLElement).querySelectorAll('label'),
  ).filter((l) => (l.textContent ?? '').includes('Show shared'));
  return (labels[0]?.querySelector('input') as HTMLInputElement | undefined) ?? null;
};

const archivedBox = (fixture: ComponentFixture<unknown>): HTMLInputElement => {
  const label = Array.from(
    (fixture.nativeElement as HTMLElement).querySelectorAll('label'),
  ).find((l) => (l.textContent ?? '').includes('Show archived'))!;
  return label.querySelector('input') as HTMLInputElement;
};

const titles = (component: WardrobeControlDialogInner): string[] =>
  inner(component)
    .filteredItems()
    .map((i) => i.title);

describe('WardrobeControlDialogInner — Show shared', () => {
  afterEach(() => vi.restoreAllMocks());

  /*
   * P4.D285 — v4's three cases REWRITTEN in the span
   * (`__tests__/unit/components/wardrobe/wardrobe-control-dialog.shared-filter.
   * test.tsx` at `4a38222e6`), transcribed 1:1: a borrowed wardrobe now sits
   * behind ONE tier row at the closet's root instead of its rows being listed
   * there. (The pre-span pair — "lists shared items by default …" and "leaves
   * the archived toggle alone" over a flat list — is RETIRED by v4's rewrite.)
   */
  const rowTitled = (fixture: ComponentFixture<unknown>, title: string): Element | null =>
    (fixture.nativeElement as HTMLElement).querySelector(`qt-wardrobe-item-row [title="${title}"]`);
  const generalTierRow = (fixture: ComponentFixture<unknown>): HTMLElement | null =>
    Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll<HTMLElement>(
        '[data-testid="wardrobe-tier-row"]',
      ),
    ).find((row) => row.textContent?.includes('General')) ?? null;

  it('shows each shared wardrobe as a tier row by default and drops it when unticked, keeping the character’s own', async () => {
    const { fixture } = await renderInner(null, sharedRoute);

    // Default on: the closet's root lists Alice's own garment, then one tier
    // row per wardrobe she borrows from; entering it shows its items.
    expect(rowTitled(fixture, 'Linen Shirt')).not.toBeNull();
    expect(generalTierRow(fixture)).not.toBeNull();
    expect(rowTitled(fixture, 'Apple Watch')).toBeNull();
    expect(sharedBox(fixture)!.checked).toBe(true);

    generalTierRow(fixture)!.click();
    await settle(fixture);
    expect(rowTitled(fixture, 'Apple Watch')).not.toBeNull();
    expect(rowTitled(fixture, 'Linen Shirt')).toBeNull();

    // Unticking while inside a shared wardrobe goes back to the closet.
    sharedBox(fixture)!.click();
    await settle(fixture);
    expect(rowTitled(fixture, 'Apple Watch')).toBeNull();
    expect(generalTierRow(fixture)).toBeNull();
    expect(rowTitled(fixture, 'Linen Shirt')).not.toBeNull();

    // And back again — hiding is a view filter, not a fetch.
    sharedBox(fixture)!.click();
    await settle(fixture);
    expect(generalTierRow(fixture)).not.toBeNull();
  });

  it('a search looks in the shared wardrobes too, and honours the toggle', async () => {
    const { fixture } = await renderInner(null, sharedRoute);
    expect(rowTitled(fixture, 'Linen Shirt')).not.toBeNull();

    const search = (fixture.nativeElement as HTMLElement).querySelector<HTMLInputElement>(
      'input[aria-label="Search wardrobe by title"]',
    )!;
    search.value = 'watch';
    search.dispatchEvent(new Event('input'));
    await settle(fixture);
    expect(rowTitled(fixture, 'Apple Watch')).not.toBeNull();
    expect((fixture.nativeElement as HTMLElement).textContent).toContain('Searching everywhere');

    sharedBox(fixture)!.click();
    await settle(fixture);
    expect(rowTitled(fixture, 'Apple Watch')).toBeNull();
  });

  it('leaves the archived toggle alone', async () => {
    const { fixture } = await renderInner(null, sharedRoute);

    expect(generalTierRow(fixture)).not.toBeNull();
    expect(archivedBox(fixture).checked).toBe(false);

    sharedBox(fixture)!.click();
    await settle(fixture);
    expect(generalTierRow(fixture)).toBeNull();
    expect(archivedBox(fixture).checked).toBe(false);
  });

  // v4 `:1237-1249` — "Only the character view merges in other tiers".
  it('hides the tickbox when browsing a shared container directly', async () => {
    const { fixture, component } = await renderInner(null, sharedRoute);
    expect(sharedBox(fixture)).not.toBeNull();

    inner(component).selectedContainer.set({ scope: 'general', id: null });
    await settle(fixture);
    expect(inner(component).isCharacterScope()).toBe(false);
    expect(sharedBox(fixture)).toBeNull();
    // The archived toggle is unconditional and stays.
    expect(archivedBox(fixture)).not.toBeNull();
  });

  /**
   * The contrast the signal's doc draws: "Show archived" is a FETCH parameter
   * (flipping it re-reads all four tiers), "Show shared" is not. The reload
   * effect must not track it — if it did, unticking would issue a fresh round
   * of tier reads for a purely client-side hiding.
   */
  it('issues NO fetch when the box flips — the reload effect does not track it', async () => {
    const { fixture, seen } = await renderInner(null, sharedRoute);
    const reads = (): number =>
      seen.filter((r) =>
        ['characterWardrobeList', 'wardrobeList', 'projectWardrobeList'].includes(
          r.type as string,
        ),
      ).length;
    const before = reads();

    sharedBox(fixture)!.click();
    await settle(fixture);
    expect(reads()).toBe(before);

    // The contrast, in the same beat: the archived toggle DOES re-fetch.
    archivedBox(fixture).click();
    await settle(fixture);
    expect(reads()).toBeGreaterThan(before);
  });

  /**
   * Badge and filter cannot drift apart, because both ask `canManageItem`.
   * In CONTAINER scope that predicate is container membership, not
   * `item.characterId` — so a row with a null `characterId` that the browsed
   * container holds is manageable, and a filter written against
   * `item.characterId` directly would wrongly drop it. (The tickbox is hidden
   * in container scope, so the signal is set directly here.)
   */
  it('filters on canManageItem, not on item.characterId', async () => {
    const { fixture, component } = await renderInner(null, (req) =>
      (req.type as string) === 'wardrobeList'
        ? { wardrobeItems: [SHARED_WATCH] }
        : defaultRoute(req),
    );
    inner(component).selectedContainer.set({ scope: 'general', id: null });
    await settle(fixture);

    // The General container holds the watch, whose `characterId` is null.
    expect(inner(component).canManageItem(SHARED_WATCH)).toBe(true);
    expect(SHARED_WATCH.characterId).toBeNull();

    (inner(component) as unknown as { showShared: { set(v: boolean): void } }).showShared.set(
      false,
    );
    await settle(fixture);
    expect(titles(component)).toContain('Apple Watch');
  });
});

/**
 * P4.D261 — v4 `3ee3b1342` `wardrobe-control-dialog.tsx:226-235, 519-520,
 * 1312-1345, 1386-1395` at the pin `f5e953a3f`: the Sort select and the
 * `Never worn` tickbox. Dialog state only (not persisted); `Never worn` is NOT
 * scope-gated (unlike Show shared) and composes with every other filter.
 */
const wornSummary = (count: number, at: string | null) => ({
  wearCount: count,
  firstWornAt: at,
  lastWornAt: at,
  lastWornChatId: null,
});
const WORN_LINEN = dto({ id: 'linen', title: 'Linen Shirt', wear: wornSummary(2, '2026-09-01T00:00:00.000Z') });
const WORN_BOOTS = dto({ id: 'boots', title: 'Boots', types: ['footwear'], wear: wornSummary(1, '2026-10-01T00:00:00.000Z') });
const NEW_APRON = dto({ id: 'apron', title: 'Apron', wear: wornSummary(0, null) });
const SHARED_CAPE = dto({ id: 'cape', title: 'Cape' });
(SHARED_CAPE as { characterId: string | null }).characterId = null;

function wearRoute(req: AnyRequest): Record<string, unknown> | Error {
  switch (req.type as string) {
    case 'characterWardrobeList':
      return {
        wardrobeItems:
          (req as { characterId?: string }).characterId === 'c1' &&
          (req as { scope?: string }).scope === undefined
            ? [WORN_LINEN, WORN_BOOTS, NEW_APRON]
            : [],
      };
    case 'wardrobeList':
      return { wardrobeItems: [SHARED_CAPE] };
    default:
      return defaultRoute(req);
  }
}

const sortSelect = (fixture: ComponentFixture<unknown>): HTMLSelectElement | null =>
  (fixture.nativeElement as HTMLElement).querySelector('select[aria-label="Sort wardrobe"]');

const neverWornBox = (fixture: ComponentFixture<unknown>): HTMLInputElement | null => {
  const label = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('label')).find(
    (l) => (l.textContent ?? '').trim() === 'Never worn',
  );
  return (label?.querySelector('input') as HTMLInputElement | undefined) ?? null;
};

describe('WardrobeControlDialogInner — Sort and Never worn (v4 3ee3b1342)', () => {
  afterEach(() => vi.restoreAllMocks());

  it('offers the four sorts by text, Title selected, inside a labelled Sort control', async () => {
    const { fixture } = await renderInner(null, wearRoute);
    const select = sortSelect(fixture)!;
    expect(select).not.toBeNull();
    expect(select.className).toBe('qt-select qt-select-sm');
    expect([...select.options].map((o) => o.textContent!.trim())).toEqual([
      'Title',
      'Recently worn',
      'Most worn',
      'Newest',
    ]);
    expect(select.value).toBe('title');
    expect(select.closest('label')!.textContent).toContain('Sort');
    // It sits beside the Items/Outfits tablist, in v4's wrapping row.
    const row = select.closest('label')!.parentElement!;
    expect(row.className).toBe('flex flex-wrap items-center justify-between gap-2');
    expect(row.querySelector('[role="tablist"]')).not.toBeNull();
  });

  it('Recently worn puts the never-worn LAST, alphabetically; Title is the default order', async () => {
    const { fixture, component } = await renderInner(null, wearRoute);
    expect(titles(component)).toEqual(['Apron', 'Boots', 'Cape', 'Linen Shirt']);
    const select = sortSelect(fixture)!;
    select.value = 'recently-worn';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(titles(component)).toEqual(['Boots', 'Linen Shirt', 'Apron', 'Cape']);
    select.value = 'most-worn';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(titles(component)).toEqual(['Linen Shirt', 'Boots', 'Apron', 'Cape']);
  });

  it('Never worn keeps only the never-worn and composes with Show shared', async () => {
    const { fixture, component } = await renderInner(null, wearRoute);
    const box = neverWornBox(fixture)!;
    expect(box.checked).toBe(false);
    expect(box.className).toBe('qt-checkbox');
    box.click();
    await settle(fixture);
    // The absent annotation (the shared Cape) reads as never worn.
    expect(titles(component)).toEqual(['Apron', 'Cape']);
    sharedBox(fixture)!.click();
    await settle(fixture);
    expect(titles(component)).toEqual(['Apron']);
  });

  it('is shown in EVERY scope — not gated like Show shared — and fires no fetch', async () => {
    const { fixture, component, seen } = await renderInner(null, wearRoute);
    inner(component).selectedContainer.set({ scope: 'general', id: null });
    await settle(fixture);
    expect(sharedBox(fixture)).toBeNull();
    expect(neverWornBox(fixture)).not.toBeNull();
    expect(sortSelect(fixture)).not.toBeNull();
    const before = seen.length;
    neverWornBox(fixture)!.click();
    sortSelect(fixture)!.value = 'newest';
    sortSelect(fixture)!.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(seen.length).toBe(before);
  });
});

/**
 * P4.D261 — v4 `3ee3b1342` `__tests__/unit/components/wardrobe/wardrobe-
 * control-dialog.worn-bundles.test.tsx` at the pin `f5e953a3f` (×4,
 * transcribed onto the Angular harness — the staged slots read off the signal
 * rather than a stubbed composer), plus the fitting side v4's dialog threads
 * (`wardrobe-control-dialog.tsx:294, 459-468, 813-871, 1005-1054`) and the
 * three Live gestures that can put an outfit on.
 */
describe('WardrobeControlDialogInner — staged outfits travel with set_all (v4 worn-bundles)', () => {
  afterEach(() => vi.restoreAllMocks());

  const A_SHIRT = dto({ id: 'shirt', title: 'Linen Shirt' });
  const A_HAT = dto({ id: 'hat', title: 'Straw Hat', types: ['accessories'] });
  const A_BOOTS = dto({ id: 'boots', title: 'Walking Boots', types: ['footwear'] });
  const RAMBLER = dto({
    id: 'rambler',
    title: 'Country Rambler',
    types: ['accessories', 'footwear'],
    componentItemIds: ['hat', 'boots'],
  });
  const ramblerRoute = (req: AnyRequest): Record<string, unknown> | Error =>
    (req.type as string) === 'characterWardrobeList'
      ? {
          wardrobeItems:
            (req as { characterId?: string }).characterId === 'c1'
              ? [A_SHIRT, A_HAT, A_BOOTS, RAMBLER]
              : [],
        }
      : defaultRoute(req);

  const el = (f: ComponentFixture<unknown>): HTMLElement => f.nativeElement as HTMLElement;
  const showOutfits = async (f: ComponentFixture<unknown>): Promise<void> => {
    [...el(f).querySelectorAll<HTMLButtonElement>('[role="tab"]')]
      .find((b) => b.textContent!.trim() === 'Outfits')!
      .click();
    await settle(f);
  };
  const clickRowButton = async (f: ComponentFixture<unknown>, title: string, label: string) => {
    const row = [...el(f).querySelectorAll<HTMLElement>('.qt-card-interactive')].find((r) =>
      r.querySelector(`[title="${title}"]`),
    )!;
    [...row.querySelectorAll<HTMLButtonElement>('button')]
      .find((b) => b.textContent!.trim() === label)!
      .click();
    await settle(f);
  };
  const clickDone = async (f: ComponentFixture<unknown>): Promise<void> => {
    [...el(f).querySelectorAll<HTMLButtonElement>('button')]
      .find((b) => b.textContent!.trim() === 'Done')!
      .click();
    await settle(f);
  };

  it('sends the bundle id with the Done flush when an outfit is worn from the list', async () => {
    const { fixture, seen } = await renderInner('chat-1', ramblerRoute);
    await showOutfits(fixture);
    await clickRowButton(fixture, 'Country Rambler', 'Wear');
    await clickDone(fixture);
    const flushed = equips(seen);
    expect(flushed).toHaveLength(1);
    expect(flushed[0]).toEqual({
      type: 'chatEquip',
      chatId: 'chat-1',
      characterId: 'c1',
      mode: 'set_all',
      // Leaves only — the bundle never lands in the slots…
      slots: { top: ['shirt'], bottom: [], footwear: ['boots'], accessories: ['hat'], hair: [] },
      // …so its claim rides beside them.
      wornBundleIds: ['rambler'],
    });
  });

  it('sends no wornBundleIds for a plain garment edit', async () => {
    const { fixture, seen } = await renderInner('chat-1', ramblerRoute);
    await clickRowButton(fixture, 'Straw Hat', 'Wear');
    await clickDone(fixture);
    expect(equips(seen)).toHaveLength(1);
    expect('wornBundleIds' in equips(seen)[0]).toBe(false);
  });

  it("sends the bundle id with the Outfit Builder's Try on", async () => {
    const { fixture, component, seen } = await renderInner('chat-1', ramblerRoute);
    inner(component).rightTab.set('builder');
    await settle(fixture);
    await showOutfits(fixture);
    await clickRowButton(fixture, 'Country Rambler', 'Try on');
    el(fixture)
      .querySelector<HTMLButtonElement>(
        '[title="Replace what the character is wearing with this composition"]',
      )!
      .click();
    await settle(fixture);
    expect(equips(seen)).toHaveLength(1);
    expect(equips(seen)[0]).toMatchObject({
      characterId: 'c1',
      mode: 'set_all',
      slots: { top: ['shirt'], footwear: ['boots'], accessories: ['hat'] },
      wornBundleIds: ['rambler'],
    });
  });

  it('the slot-add and slot-row paths (the quick-pick lands on the latter) claim too', async () => {
    const { fixture, component, seen } = await renderInner('chat-1', ramblerRoute);
    const c = inner(component);
    c.handleAddToSlot(RAMBLER, 'footwear');
    c.handleSlotAdd('accessories', 'rambler');
    c.handleEquipItem(RAMBLER);
    c.requestClose();
    await settle(fixture);
    expect(equips(seen)[0]['wornBundleIds']).toEqual(['rambler']);
  });

  it('a committed claim does not ride the next flush', async () => {
    const { fixture, component, seen } = await renderInner('chat-1', ramblerRoute);
    const c = inner(component);
    c.handleEquipItem(RAMBLER);
    await (component as unknown as { flushStagedLiveOutfits(): Promise<boolean> }).flushStagedLiveOutfits();
    c.handleSlotClear('top');
    await (component as unknown as { flushStagedLiveOutfits(): Promise<boolean> }).flushStagedLiveOutfits();
    await settle(fixture);
    const flushed = equips(seen);
    expect(flushed).toHaveLength(2);
    expect(flushed[0]['wornBundleIds']).toEqual(['rambler']);
    expect('wornBundleIds' in flushed[1]).toBe(false);
  });

  it('the Builder: Reset to defaults credits the default bundle; Reset to worn and Clear all drop the claim', async () => {
    const DEFAULT_RAMBLER = { ...RAMBLER, isDefault: true };
    const route = (req: AnyRequest): Record<string, unknown> | Error =>
      (req.type as string) === 'characterWardrobeList'
        ? {
            wardrobeItems:
              (req as { characterId?: string }).characterId === 'c1'
                ? [A_SHIRT, A_HAT, A_BOOTS, DEFAULT_RAMBLER]
                : [],
          }
        : defaultRoute(req);
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);
    const { fixture, component, seen } = await renderInner('chat-1', route);
    const c = inner(component) as ReturnType<typeof inner> & {
      fittingResetToDefaults(): void;
      fittingResetToWorn(): void;
      fittingClearAll(): void;
    };
    c.rightTab.set('builder');
    c.fittingResetToDefaults();
    await c.wearFitting();
    c.fittingResetToWorn();
    c.fittingAdd('top', 'shirt');
    await c.wearFitting();
    c.fittingResetToDefaults();
    c.fittingClearAll();
    await c.wearFitting();
    await settle(fixture);
    const bodies = equips(seen);
    expect(bodies).toHaveLength(3);
    expect(bodies[0]['wornBundleIds']).toEqual(['rambler']);
    expect('wornBundleIds' in bodies[1]).toBe(false);
    expect('wornBundleIds' in bodies[2]).toBe(false);
    expect(confirmSpy).toHaveBeenCalled();
  });

  it('in chat the Builder seeds from the worn snapshot with NO claim', async () => {
    const { fixture, component, seen } = await renderInner('chat-1', ramblerRoute);
    inner(component).rightTab.set('builder');
    await inner(component).wearFitting();
    await settle(fixture);
    expect('wornBundleIds' in equips(seen)[0]).toBe(false);
  });
});

/**
 * P4.D261 — v4 `7c8572869` `wardrobe-control-dialog.tsx:275, 606-636, 1525-
 * 1545` at the pin `f5e953a3f`: the row's Generate image → the designated
 * profile (no override), the item's HOME container in the character view
 * (`homeContainerForItem`) or the browsed container otherwise; v4's two
 * success toasts and its error fallback; the list reloads; a SET of
 * generating ids (an item can appear as itself and as a component).
 */
describe('WardrobeControlDialogInner — Generate image (v4 7c8572869)', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  const stubGenerate = (status: number, body: unknown): ReturnType<typeof vi.fn> => {
    const fn = vi.fn(async () => ({ ok: status < 300, status, json: async () => body }));
    vi.stubGlobal('fetch', fn);
    return fn;
  };
  const toasts = (): Array<{ type: string; message: string }> =>
    TestBed.inject(ToastService)
      .toasts()
      .map((t) => ({ type: t.type, message: t.message }));
  // Re-pinned (P4.D285): the handler and its in-flight set moved to the actions store.
  type Gen = { handleGenerateImage(i: WardrobeItemDto): Promise<void>; generatingImageIds(): ReadonlySet<string> };
  const gen = (component: WardrobeControlDialogInner): Gen => {
    const actions = (component as unknown as { actions: WardrobeItemActions }).actions;
    return { handleGenerateImage: (i) => actions.generateImage(i), generatingImageIds: () => actions.generatingImageIds() };
  };

  it('the rows opt in, and a character garment is drawn in its own vault', async () => {
    const fetchFn = stubGenerate(201, { rerouted: false, current: 'f1' });
    const { fixture, component, seen } = await renderInner(null);
    const row = fixture.debugElement.queryAll(By.css('qt-wardrobe-item-row'))[0];
    expect(row.componentInstance.canGenerateImage()).toBe(true);
    const before = seen.filter((r) => (r.type as string) === 'characterWardrobeList').length;
    await gen(component).handleGenerateImage(HAT);
    await settle(fixture);
    expect(fetchFn.mock.calls[0][0]).toBe('/api/v1/wardrobe/hat/images?scope=character&id=c1&action=generate');
    expect((fetchFn.mock.calls[0][1] as RequestInit).body).toBe('{}');
    expect(toasts()).toContainEqual({ type: 'success', message: 'A portrait of "Hat" is hung' });
    expect(seen.filter((r) => (r.type as string) === 'characterWardrobeList').length).toBeGreaterThan(before);
    expect(gen(component).generatingImageIds().size).toBe(0);
  });

  it('a shared garment in the character view goes to General; a browsed container is its own home', async () => {
    const fetchFn = stubGenerate(201, { rerouted: true, current: 'f1' });
    const { fixture, component } = await renderInner(null);
    const cape = dto({ id: 'cape', title: 'Cape' });
    (cape as { characterId: string | null }).characterId = null;
    await gen(component).handleGenerateImage(cape);
    expect(fetchFn.mock.calls[0][0]).toBe('/api/v1/wardrobe/cape/images?scope=general&action=generate');
    expect(toasts()).toContainEqual({
      type: 'success',
      message: 'A portrait of "Cape" is hung — drawn at the uncensored desk',
    });
    inner(component).selectedContainer.set({ scope: 'project', id: 'p1' });
    await settle(fixture);
    await gen(component).handleGenerateImage(cape);
    expect(fetchFn.mock.calls[1][0]).toBe('/api/v1/wardrobe/cape/images?scope=project&id=p1&action=generate');
  });

  it('a failure toasts the route’s sentence, or v4’s fallback', async () => {
    stubGenerate(400, { error: 'No image profiles are configured' });
    const { component } = await renderInner(null);
    await gen(component).handleGenerateImage(HAT);
    expect(toasts()).toContainEqual({ type: 'error', message: 'No image profiles are configured' });
    vi.stubGlobal('fetch', vi.fn(async () => {
      throw 'offline';
    }));
    await gen(component).handleGenerateImage(HAT);
    expect(toasts()).toContainEqual({ type: 'error', message: 'Failed to generate a picture' });
  });

  it('marks the item generating while the draw is in flight, and refuses a second press', async () => {
    let release!: () => void;
    const fetchFn = vi.fn(
      () =>
        new Promise((resolve) => {
          release = () => resolve({ ok: true, status: 201, json: async () => ({ rerouted: false }) });
        }),
    );
    vi.stubGlobal('fetch', fetchFn);
    const { fixture, component } = await renderInner(null);
    const g = gen(component);
    const first = g.handleGenerateImage(HAT);
    expect(g.generatingImageIds().has('hat')).toBe(true);
    await g.handleGenerateImage(HAT);
    expect(fetchFn).toHaveBeenCalledTimes(1);
    release();
    await first;
    await settle(fixture);
    expect(g.generatingImageIds().has('hat')).toBe(false);
  });
});

/** P4.D261 — v4 `7c8572869` `wardrobe-control-dialog.tsx:1720` `onImageChanged={() => void reloadActiveItems()}`. */
describe('WardrobeControlDialogInner — the editor’s picture change reloads the list', () => {
  it('re-reads the list when the editor reports a picture change', async () => {
    const { fixture, component, seen } = await renderInner(null);
    inner(component);
    (component as unknown as { actions: WardrobeItemActions }).actions.openEditor(HAT);
    await settle(fixture);
    const editor = fixture.debugElement.query((d) => d.name === 'qt-wardrobe-item-editor');
    expect(editor).not.toBeNull();
    const before = seen.filter((r) => (r.type as string) === 'characterWardrobeList').length;
    (editor.componentInstance as { imageChanged: { emit(): void } }).imageChanged.emit();
    await settle(fixture);
    expect(seen.filter((r) => (r.type as string) === 'characterWardrobeList').length).toBeGreaterThan(before);
  });
});

/**
 * P4.D261 Tier 2 (R-H) — v4 `b3f937076` `lib/realtime/job-topics.ts`: a
 * finished wardrobe-picture job publishes `characters` (id = the payload's
 * `characterId`) + `mountPoints`; v4's wardrobe views watch both. v5's list is
 * signal-held (no TanStack key the topic map could reach), so the dialog
 * subscribes itself through `RealtimeService.onTopic('characters', …,
 * selectedCharacterId)`.
 */
describe('WardrobeControlDialogInner — the `characters` realtime hook (R-H)', () => {
  const hint = (id?: string) => ({ v: 1, topic: 'characters', at: 1, ...(id ? { id } : {}) });

  it('a hint for the selected character re-reads the list; one for another character does not', async () => {
    const { fixture, seen } = await renderInner(null);
    const stream = TestBed.inject(CoreClient) as unknown as { frames: { next(f: unknown): void } };
    const reads = (): number =>
      seen.filter((r) => (r.type as string) === 'characterWardrobeList').length;

    const before = reads();
    stream.frames.next(hint('c2'));
    await settle(fixture);
    expect(reads()).toBe(before);

    stream.frames.next(hint('c1'));
    await settle(fixture);
    expect(reads()).toBeGreaterThan(before);
  });

  it('a collection-wide `characters` hint (no id) re-reads too — it says nothing about which rows', async () => {
    const { fixture, seen } = await renderInner(null);
    const stream = TestBed.inject(CoreClient) as unknown as { frames: { next(f: unknown): void } };
    const before = seen.filter((r) => (r.type as string) === 'characterWardrobeList').length;
    stream.frames.next(hint());
    await settle(fixture);
    expect(seen.filter((r) => (r.type as string) === 'characterWardrobeList').length).toBeGreaterThan(
      before,
    );
  });

  it('another topic is ignored', async () => {
    const { fixture, seen } = await renderInner(null);
    const stream = TestBed.inject(CoreClient) as unknown as { frames: { next(f: unknown): void } };
    const before = seen.length;
    stream.frames.next({ v: 1, topic: 'projects', at: 1, id: 'c1' });
    await settle(fixture);
    expect(seen.length).toBe(before);
  });
});

/**
 * P4.D277 — v4 `01a83539d` `wardrobe-control-dialog.tsx`: `viewingImage`
 * (`:263-267`), `handleOpenImage` (`:613-629` — a borrowed row through its
 * origin's wardrobe, else the same rule as every other row action; no
 * picture → nothing), `editorOpen` gains `|| viewingImage` (`:1198-1204`),
 * the rows' `onOpenImage` (`:1460`), and the viewer mounted before the inline
 * editor with no note and no prev/next (`:1682-1691`).
 */
describe('WardrobeControlDialogInner — the row picture viewer (v4 01a83539d)', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    TestBed.resetTestingModule();
    document.body.style.overflow = '';
  });

  type Viewing = {
    handleOpenImage(item: WardrobeItemDto): void;
    viewingImage(): { item: WardrobeItemDto; container: { scope: string; id: string | null } } | null;
  };
  // Re-pinned (P4.D285): `handleOpenImage` / `viewingImage` moved to the
  // actions store as `openImage` / `imageView` (v4 `useWardrobeItemActions`).
  const v = (component: WardrobeControlDialogInner): Viewing => {
    const actions = (component as unknown as { actions: WardrobeItemActions }).actions;
    return { handleOpenImage: (i) => actions.openImage(i), viewingImage: () => actions.imageView() };
  };
  const frame = (): HTMLElement | null => document.body.querySelector('qt-full-screen-image-viewer');

  const PICTURED_HAT = dto({ id: 'hat', title: 'Hat', types: ['accessories'], imageFileId: 'file-hat' });
  const pictured = (partial: Partial<WardrobeItemDto> & { id: string }): WardrobeItemDto =>
    dto({ imageFileId: `file-${partial.id}`, ...partial });

  function picturedRoute(req: AnyRequest): Record<string, unknown> | Error {
    if ((req.type as string) === 'characterWardrobeList') {
      return { wardrobeItems: (req as { characterId?: string }).characterId === 'c1' ? [SHIRT, PICTURED_HAT] : [] };
    }
    return defaultRoute(req);
  }

  it("a character-owned row opens through the character's wardrobe", async () => {
    const { fixture, component } = await renderInner(null, picturedRoute);
    v(component).handleOpenImage(PICTURED_HAT);
    await settle(fixture);
    expect(v(component).viewingImage()!.container).toEqual({ scope: 'character', id: 'c1' });
    const dialog = frame()!.querySelector('[role="dialog"]')!;
    expect(dialog.getAttribute('aria-label')).toBe('Picture of Hat');
    expect(frame()!.querySelector('img')!.getAttribute('src')).toBe('/api/v1/files/file-hat');
    // No note and no prev/next from the row (v4 :1682-1691).
    expect(frame()!.querySelector('.qt-bg-overlay-caption .text-xs')).toBeNull();
    expect(frame()!.querySelector('button[title="Next image (Right Arrow)"]')).toBeNull();
    expect(frame()!.querySelector('button[title="Previous image (Left Arrow)"]')).toBeNull();
  });

  it("a borrowed row opens through its origin's wardrobe — in the character view and in a container view", async () => {
    const { fixture, component } = await renderInner(null, picturedRoute);
    const borrowed = pictured({
      id: 'cloak',
      title: 'Estate Cloak',
      characterId: null,
      origin: { scope: 'project', id: 'p1', name: 'The Estate' },
    } as Partial<WardrobeItemDto> & { id: string });
    v(component).handleOpenImage(borrowed);
    expect(v(component).viewingImage()!.container).toEqual({ scope: 'project', id: 'p1' });
    inner(component).selectedContainer.set({ scope: 'group', id: 'g1' });
    await settle(fixture);
    // Re-pinned (P4.D285, v4 `4a38222e6` `addressOf`): a container view
    // addresses every row through the browsed container — origin-first is the
    // CHARACTER view's rule only (`useWardrobeItemActions.ts:134-138`).
    v(component).handleOpenImage(borrowed);
    expect(v(component).viewingImage()!.container).toEqual({ scope: 'group', id: 'g1' });
  });

  it('a General archetype with no origin in the character view opens through General', async () => {
    const { component } = await renderInner(null, picturedRoute);
    const cape = pictured({ id: 'cape', title: 'Cape', characterId: null });
    v(component).handleOpenImage(cape);
    expect(v(component).viewingImage()!.container).toEqual({ scope: 'general', id: null });
  });

  it('in a project container view, a row with no origin opens through the selected container', async () => {
    const { fixture, component } = await renderInner(null, picturedRoute);
    inner(component).selectedContainer.set({ scope: 'project', id: 'p1' });
    await settle(fixture);
    v(component).handleOpenImage(pictured({ id: 'apron', title: 'Apron', characterId: null }));
    expect(v(component).viewingImage()!.container).toEqual({ scope: 'project', id: 'p1' });
  });

  it('a row with no picture opens nothing', async () => {
    const { fixture, component } = await renderInner(null, picturedRoute);
    v(component).handleOpenImage(HAT);
    await settle(fixture);
    expect(v(component).viewingImage()).toBeNull();
    expect(frame()).toBeNull();
  });

  it("the list rows opt in, and a row thumbnail's button opens the viewer", async () => {
    const { fixture, component } = await renderInner(null, picturedRoute);
    const rows = fixture.debugElement.queryAll(By.css('qt-wardrobe-item-row'));
    expect(rows.every((r) => r.componentInstance.canOpenImage() === true)).toBe(true);
    (fixture.nativeElement as HTMLElement)
      .querySelector<HTMLButtonElement>('button[aria-label="View the picture of Hat"]')!
      .click();
    await settle(fixture);
    expect(v(component).viewingImage()!.item.id).toBe('hat');
    expect(frame()).toBeTruthy();
  });

  it('with the viewer open, a backdrop click and an Escape leave the dialog open; ONE Escape closes the viewer only (§J.3)', async () => {
    const { fixture, component } = await renderInner(null, picturedRoute);
    let closes = 0;
    component.closed.subscribe(() => (closes += 1));
    v(component).handleOpenImage(PICTURED_HAT);
    await settle(fixture);
    expect(frame()).toBeTruthy();
    // The dialog's backdrop is gated by editorOpen while the viewer is up.
    (fixture.nativeElement as HTMLElement).querySelector<HTMLElement>('.qt-dialog-overlay')!.click();
    await settle(fixture);
    expect(closes).toBe(0);
    // Listener order: the dialog's host Escape listener (registered first)
    // reads editorOpen() === true BEFORE the viewer's listener clears
    // viewingImage — so one Escape closes the viewer and nothing else.
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await settle(fixture);
    expect(v(component).viewingImage()).toBeNull();
    expect(frame()).toBeNull();
    expect(closes).toBe(0);
  });
});

/**
 * P4.D285 — bug 198 (v4 `e23fb095b`). v4's half (a) stopped a CACHED
 * snapshot from an earlier dialog visit seeding the staging. v5 MEASURED: no
 * such cache exists — the outfit store is built per inner mount and the inner
 * is destroyed on close — so this is the PIN of that invariant (a second open
 * re-reads `chatOutfitGet`; nothing seeds before it lands), plus half (b), the
 * `chats` topic re-read (v4 `topic-map.ts:72-76`).
 */
describe('WardrobeControlDialog — bug 198: the worn snapshot is never stale', () => {
  afterEach(() => vi.restoreAllMocks());

  const outfitReads = (seen: AnyRequest[]): number =>
    seen.filter((r) => (r.type as string) === 'chatOutfitGet').length;

  it('every open of the dialog re-reads the chat’s outfit — nothing crosses a close', async () => {
    const seen: AnyRequest[] = [];
    const core = {
      ...coreStreamStub(),
      dispatchData: vi.fn(async (req: CoreRequest) => {
        const r = req as AnyRequest;
        seen.push(r);
        const out = defaultRoute(r);
        if (out instanceof Error) throw out;
        return out;
      }),
    } as unknown as CoreClient;
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [WardrobeControlDialog],
      providers: [
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: core },
        WardrobeDialogService,
      ],
    });
    const service = TestBed.inject(WardrobeDialogService);
    const fixture = TestBed.createComponent(WardrobeControlDialog);
    fixture.detectChanges();

    service.open({ characterId: 'c1', chatId: 'chat-1' });
    await settle(fixture);
    const afterFirst = outfitReads(seen);
    expect(afterFirst).toBeGreaterThanOrEqual(1);

    service.close();
    await settle(fixture);
    service.open({ characterId: 'c1', chatId: 'chat-1' });
    await settle(fixture);
    expect(outfitReads(seen)).toBeGreaterThan(afterFirst);
  });

  it('the staging does not seed — and the Live tab paints nothing worn — until a read lands', async () => {
    let refuse = true;
    const { fixture, component, seen } = await renderInner('chat-1', (req) =>
      (req.type as string) === 'chatOutfitGet' && refuse
        ? new Error('the snapshot has not arrived')
        : defaultRoute(req),
    );
    // The boot read never landed: nothing seeded, nothing worn painted.
    expect(inner(component).liveStagedByChar()).toEqual({});
    expect(
      (component as unknown as { liveDisplaySlots(): EquippedSlots }).liveDisplaySlots(),
    ).toEqual({ top: [], bottom: [], footwear: [], accessories: [], hair: [] });
    refuse = false;
    await (component as unknown as { outfit: { refreshOutfit(): Promise<unknown> } }).outfit.refreshOutfit();
    await settle(fixture);
    expect(inner(component).liveStagedByChar()['c1']).toEqual(WORN);
    expect(seen.filter((r) => (r.type as string) === 'chatOutfitGet').length).toBe(2);
  });

  it('a `chats` hint for THIS chat re-reads the worn snapshot; another chat’s does not', async () => {
    const { fixture, seen } = await renderInner('chat-1');
    const stream = TestBed.inject(CoreClient) as unknown as { frames: { next(f: unknown): void } };
    const before = outfitReads(seen);
    stream.frames.next({ v: 1, topic: 'chats', at: 1, id: 'chat-2' });
    await settle(fixture);
    expect(outfitReads(seen)).toBe(before);
    stream.frames.next({ v: 1, topic: 'chats', at: 1, id: 'chat-1' });
    await settle(fixture);
    expect(outfitReads(seen)).toBe(before + 1);
  });

  it('out of chat a `chats` hint reads nothing', async () => {
    const { fixture, seen } = await renderInner(null);
    const stream = TestBed.inject(CoreClient) as unknown as { frames: { next(f: unknown): void } };
    stream.frames.next({ v: 1, topic: 'chats', at: 1 });
    await settle(fixture);
    expect(outfitReads(seen)).toBe(0);
  });

  it('a re-read never re-seeds the staging (Bug 61’s contract holds)', async () => {
    const { fixture, component } = await renderInner('chat-1');
    inner(component).handleSlotClear('top');
    expect(inner(component).liveStagedByChar()['c1'].top).toEqual([]);
    const stream = TestBed.inject(CoreClient) as unknown as { frames: { next(f: unknown): void } };
    stream.frames.next({ v: 1, topic: 'chats', at: 1, id: 'chat-1' });
    await settle(fixture);
    expect(inner(component).liveStagedByChar()['c1'].top).toEqual([]);
  });
});

/**
 * P4.D285 — bug 229 through the REAL dialog (v4 `97f0d8cb8`; R-O): the
 * Move-to-folder picker's open state is the actions store's, so the dialog's
 * Escape / backdrop gate counts it. v5's vector is the document Escape.
 */
describe('WardrobeControlDialogInner — bug 229: the folder picker counts as an overlay', () => {
  afterEach(() => vi.restoreAllMocks());

  it('with the picker open, a click inside it and ONE Escape leave the dialog open; the next Escape is the dialog’s', async () => {
    const { fixture, component } = await renderInner('chat-1');
    const closed = vi.fn();
    component.closed.subscribe(closed);
    const actions = (component as unknown as { actions: WardrobeItemActions }).actions;
    actions.openFolderPicker([SHIRT]);
    await settle(fixture);
    const el = fixture.nativeElement as HTMLElement;
    const listbox = el.querySelector('[role="listbox"][aria-label="Folders"]')!;
    expect(listbox).not.toBeNull();
    expect((component as unknown as { overlayOpen(): boolean }).overlayOpen()).toBe(true);

    const top = Array.from(listbox.querySelectorAll<HTMLButtonElement>('[role="option"]')).find(
      (o) => o.textContent?.includes('top level'),
    )!;
    top.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    top.click();
    (el.querySelector('.qt-dialog-overlay') as HTMLElement).click();
    await settle(fixture);
    expect(closed).not.toHaveBeenCalled();
    expect(el.querySelector('[role="listbox"][aria-label="Folders"]')).not.toBeNull();

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await settle(fixture);
    expect(el.querySelector('[role="listbox"][aria-label="Folders"]')).toBeNull();
    expect(closed).not.toHaveBeenCalled();

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await settle(fixture);
    expect(closed).toHaveBeenCalledTimes(1);
  });
});

/**
 * P4.D285 — bug 191's client gate (v4 `9ecebf986` `wardrobe-control-dialog.
 * tsx:290-297`, `lib/wardrobe/wearable.ts`): every wear gesture refuses an
 * archived garment with the server's words, and stages nothing.
 */
describe('WardrobeControlDialogInner — an archived garment cannot be put on (bug 191)', () => {
  afterEach(() => vi.restoreAllMocks());

  const RELIC = dto({ id: 'relic', title: 'Moth-eaten Spats', types: ['footwear'], archivedAt: '2026-01-01T00:00:00.000Z' });
  const route = (req: AnyRequest): Record<string, unknown> | Error =>
    (req.type as string) === 'characterWardrobeList' &&
    (req as { characterId?: string }).characterId === 'c1'
      ? { wardrobeItems: [SHIRT, HAT, RELIC] }
      : defaultRoute(req);

  it('Wear / Try on, + to a slot and the composer picker all refuse it, toasting v4’s sentence', async () => {
    const { component } = await renderInner('chat-1', route);
    const toasts = TestBed.inject(ToastService);
    const showError = vi.spyOn(toasts, 'showError').mockImplementation(() => '');
    const c = inner(component);
    const before = JSON.stringify(c.liveStagedByChar());
    const fittingBefore = JSON.stringify(c.fittingSlots());

    c.rightTab.set('live');
    c.rowEquip(RELIC);
    (component as unknown as { rowAddToSlot(i: WardrobeItemDto, s: string): void }).rowAddToSlot(RELIC, 'footwear');
    c.handleSlotAdd('footwear', 'relic');
    c.rightTab.set('builder');
    c.rowEquip(RELIC);
    c.fittingAdd('footwear', 'relic');

    expect(showError).toHaveBeenCalledTimes(5);
    expect(showError).toHaveBeenCalledWith('Item "Moth-eaten Spats" is archived and cannot be worn');
    expect(JSON.stringify(c.liveStagedByChar())).toBe(before);
    expect(JSON.stringify(c.fittingSlots())).toBe(fittingBefore);
  });

  it('taking an archived garment OFF is never refused (archiving undresses nobody)', async () => {
    const { component } = await renderInner('chat-1', route);
    const toasts = TestBed.inject(ToastService);
    const showError = vi.spyOn(toasts, 'showError').mockImplementation(() => '');
    inner(component).handleSlotRemove('top', 'shirt');
    expect(showError).not.toHaveBeenCalled();
    expect(inner(component).liveStagedByChar()['c1'].top).toEqual([]);
  });
});

describe('Wardrobe dialog — the History tab and the Live staging discard (P4.D318, v4 8d8618819)', () => {
  afterEach(() => vi.restoreAllMocks());

  /** The outfit the rollback restores (entry 0 of the timeline). */
  const RESTORED: EquippedSlots = { top: [], bottom: [], footwear: [], accessories: ['hat'], hair: [] };
  const EMPTY_SLOTS = { top: [], bottom: [], footwear: [], accessories: [], hair: [] };
  const TIMELINE = {
    currentIndex: 1,
    entries: [
      {
        index: 0, at: '2026-10-16T10:00:00.000Z', source: 'chat-start', actorCharacterId: null, actorName: null,
        actorToolName: null, changedBy: 'chat opened', isCurrent: false, summary: 'Hat',
        slots: { ...EMPTY_SLOTS, accessories: [{ itemId: 'hat', title: 'Hat', archived: false, missing: false }] },
      },
      {
        index: 1, at: '2026-10-16T11:00:00.000Z', source: 'operator', actorCharacterId: null, actorName: null,
        actorToolName: null, changedBy: 'the operator', isCurrent: true, summary: 'Shirt',
        slots: { ...EMPTY_SLOTS, top: [{ itemId: 'shirt', title: 'Shirt', archived: false, missing: false }] },
      },
    ],
  };

  interface HistoryDialog {
    isLiveDirty(cid: string): boolean;
    discardLive(cid: string, options?: { reseed?: boolean }): void;
    liveDisplaySlots(): EquippedSlots;
    outfit: { refreshOutfit(): Promise<unknown> };
  }
  const surface = (component: WardrobeControlDialogInner) => component as unknown as HistoryDialog;

  /**
   * The default route plus the two history verbs; the worn snapshot is
   * mutable so a rollback lands the restored outfit on the next re-read
   * (the server's one commit). `hold` keeps the rollback open until released.
   */
  function historyRoute(state: { worn: EquippedSlots; hold?: Promise<void> }) {
    return (req: AnyRequest): Record<string, unknown> | Error => {
      switch (req.type as string) {
        case 'chatOutfitGet':
          return { equippedOutfit: { c1: state.worn } };
        case 'chatWardrobeHistory':
          return TIMELINE;
        case 'chatWardrobeRollback': {
          const answer = async () => {
            if (state.hold) await state.hold;
            state.worn = RESTORED;
            return { equippedSlots: RESTORED, previousIndex: 1, restoredIndex: req['index'], skipped: [] };
          };
          return answer() as unknown as Record<string, unknown>;
        }
        default:
          return defaultRoute(req);
      }
    };
  }

  async function settleLong(fixture: ComponentFixture<unknown>, rounds = 10): Promise<void> {
    for (let i = 0; i < rounds; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
  }

  const tabButtons = (el: HTMLElement): HTMLButtonElement[] =>
    [...el.querySelectorAll('.qt-tab-group > button.qt-tab')] as HTMLButtonElement[];

  async function openHistory(fixture: ComponentFixture<WardrobeControlDialogInner>): Promise<HTMLElement> {
    const el = fixture.nativeElement as HTMLElement;
    tabButtons(el).find((b) => b.textContent?.trim() === 'History')!.click();
    await settleLong(fixture);
    return el;
  }

  async function wearAgain(fixture: ComponentFixture<WardrobeControlDialogInner>, index: number): Promise<void> {
    const el = fixture.nativeElement as HTMLElement;
    const entry = el.querySelector(`[data-testid="outfit-history-entry-${index}"]`)!;
    ([...entry.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Wear again') as HTMLButtonElement).click();
    await settleLong(fixture);
  }

  const rollbacks = (seen: AnyRequest[]) => seen.filter((r) => (r.type as string) === 'chatWardrobeRollback');

  it('the History tab exists only in chat, after Outfit Builder', async () => {
    const { fixture, component } = await renderInner('chat-1', historyRoute({ worn: WORN }));
    const el = fixture.nativeElement as HTMLElement;
    const labels = tabButtons(el).map((b) => (b.textContent ?? '').replace(/\s+/g, ' ').trim());
    expect(labels).toHaveLength(3);
    expect(labels[0]).toMatch(/^Live outfit/);
    expect(labels[1]).toBe('Outfit Builder');
    expect(labels[2]).toBe('History');
    expect(el.querySelector('qt-outfit-history-tab')).toBeNull();

    await openHistory(fixture);
    expect(inner(component).rightTab()).toBe('history');
    expect(el.querySelector('qt-outfit-history-tab')).not.toBeNull();
    // The Live and Builder bodies are hidden.
    expect(el.querySelector('qt-outfit-composer')).toBeNull();
    expect(el.querySelector('qt-avatar-generation-pane')).toBeNull();

    // Out of chat (the asTab tab's chatId null): no History button at all.
    const outOfChat = await renderInner(null);
    const outLabels = tabButtons(outOfChat.fixture.nativeElement as HTMLElement).map((b) => b.textContent?.trim());
    expect(outLabels).toEqual(['Outfit Builder']);
  });

  it('isDirty: clean when seeded, dirty after a gesture', async () => {
    const { component } = await renderInner('chat-1', historyRoute({ worn: WORN }));
    expect(surface(component).isLiveDirty('c1')).toBe(false);
    inner(component).handleEquipItem(HAT);
    expect(surface(component).isLiveDirty('c1')).toBe(true);
  });

  it('discard drops the staging and re-seeds from the next snapshot', async () => {
    const state = { worn: WORN };
    const { fixture, component } = await renderInner('chat-1', historyRoute(state));
    inner(component).handleEquipItem(HAT);
    surface(component).discardLive('c1');
    await settleLong(fixture);
    expect(surface(component).isLiveDirty('c1')).toBe(false);
    expect(surface(component).liveDisplaySlots()).toEqual(WORN);
    expect(inner(component).liveStagedByChar()['c1']).toBeUndefined();

    state.worn = RESTORED;
    await surface(component).outfit.refreshOutfit();
    await settleLong(fixture);
    expect(surface(component).liveDisplaySlots()).toEqual(RESTORED);
    expect(inner(component).liveStagedByChar()['c1']).toEqual(RESTORED);
    expect(surface(component).isLiveDirty('c1')).toBe(false);
  });

  it('discard with reseed re-seeds at once when no new snapshot arrived', async () => {
    const { fixture, component } = await renderInner('chat-1', historyRoute({ worn: WORN }));
    inner(component).handleEquipItem(HAT);
    surface(component).discardLive('c1');
    await settleLong(fixture);
    expect(inner(component).liveStagedByChar()['c1']).toBeUndefined();
    surface(component).discardLive('c1', { reseed: true });
    await settleLong(fixture);
    // Re-SEEDED (the staging exists again), not merely painting the fallback:
    // only a seeding effect that READS `liveReseedTick` gets here.
    expect(inner(component).liveStagedByChar()['c1']).toEqual(WORN);
    expect(surface(component).liveDisplaySlots()).toEqual(WORN);
    expect(surface(component).isLiveDirty('c1')).toBe(false);
  });

  it('Wear again with staged edits: confirm → discard → rollback → reseed; Done then flushes NOTHING', async () => {
    const state = { worn: WORN };
    const { fixture, component, seen } = await renderInner('chat-1', historyRoute(state));
    const closedSpy = vi.fn();
    component.closed.subscribe(closedSpy);
    inner(component).handleEquipItem(HAT);
    expect(surface(component).isLiveDirty('c1')).toBe(true);
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);

    await openHistory(fixture);
    await wearAgain(fixture, 0);

    expect(confirm).toHaveBeenCalledWith("Discard the staged changes to Aria's outfit and return to outfit 0?");
    expect(rollbacks(seen)).toEqual([
      { type: 'chatWardrobeRollback', chatId: 'chat-1', characterId: 'c1', index: 0 },
    ]);
    inner(component).rightTab.set('live');
    await settleLong(fixture);
    expect(surface(component).liveDisplaySlots()).toEqual(RESTORED);
    expect(inner(component).liveStagedByChar()['c1']).toEqual(RESTORED);
    expect(surface(component).isLiveDirty('c1')).toBe(false);

    inner(component).requestClose();
    await settleLong(fixture);
    expect(equips(seen)).toEqual([]);
    expect(closedSpy).toHaveBeenCalledTimes(1);
  });

  it("after a rollback the Live tab shows the restored outfit (R-L: RULED fix in v5 — a recorded divergence from v4)", async () => {
    // The CLEAN path: nothing staged, so Wear again asks nothing. v4 at the pin
    // also skips the discard there, and the settle's `{ reseed: true }` is a
    // no-op for a character already seeded — so v4's Live tab keeps the
    // PRE-rollback staging and a later gesture + Done set_all's over the
    // rollback (measured, P4.D318 Tier 1 item 1). The human's ruling
    // (2026-10-19): v5 discards unconditionally, confirming only when dirty.
    const state = { worn: WORN };
    const { fixture, component, seen } = await renderInner('chat-1', historyRoute(state));
    const confirm = vi.spyOn(window, 'confirm');
    await openHistory(fixture);
    await wearAgain(fixture, 0);
    expect(confirm).not.toHaveBeenCalled();
    expect(rollbacks(seen)).toHaveLength(1);

    inner(component).rightTab.set('live');
    await settleLong(fixture);
    expect(surface(component).liveDisplaySlots()).toEqual(RESTORED);
    expect(inner(component).liveStagedByChar()['c1']).toEqual(RESTORED);
    expect(surface(component).isLiveDirty('c1')).toBe(false);

    // A later gesture stages onto the RESTORED outfit, so Done keeps the rollback.
    inner(component).handleSlotAdd('top', 'shirt');
    inner(component).requestClose();
    await settleLong(fixture);
    expect(equips(seen)).toHaveLength(1);
    expect((equips(seen)[0] as { slots: EquippedSlots }).slots).toEqual({
      top: ['shirt'],
      bottom: [],
      footwear: [],
      accessories: ['hat'],
      hair: [],
    });
  });

  it('a confirm the operator declines changes nothing', async () => {
    const { fixture, component, seen } = await renderInner('chat-1', historyRoute({ worn: WORN }));
    inner(component).handleEquipItem(HAT);
    const staged = inner(component).liveStagedByChar()['c1'];
    vi.spyOn(window, 'confirm').mockReturnValue(false);
    await openHistory(fixture);
    await wearAgain(fixture, 0);
    expect(rollbacks(seen)).toEqual([]);
    expect(inner(component).liveStagedByChar()['c1']).toEqual(staged);
    expect(surface(component).isLiveDirty('c1')).toBe(true);

    inner(component).requestClose();
    await settleLong(fixture);
    expect(equips(seen)).toHaveLength(1);
    expect((equips(seen)[0] as { slots: EquippedSlots }).slots.accessories).toEqual(['hat']);
  });

  it('a character switch mid-rollback re-seeds the character rolled back, never the newly selected one', async () => {
    let release!: () => void;
    const state: { worn: EquippedSlots; hold?: Promise<void> } = {
      worn: WORN,
      hold: new Promise<void>((r) => (release = r)),
    };
    const { fixture, component, seen } = await renderInner('chat-1', historyRoute(state));
    inner(component).handleEquipItem(HAT);
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    await openHistory(fixture);
    await wearAgain(fixture, 0);
    expect(rollbacks(seen)).toEqual([
      { type: 'chatWardrobeRollback', chatId: 'chat-1', characterId: 'c1', index: 0 },
    ]);
    // c1's staging was discarded before the rollback.
    expect(inner(component).liveStagedByChar()['c1']).toBeUndefined();

    // Switch to Zed and stage something there while the rollback is held.
    inner(component).selectedContainer.set({ scope: 'character', id: 'c2' });
    await settleLong(fixture);
    inner(component).rightTab.set('live');
    await settleLong(fixture);
    const zedBefore = inner(component).liveStagedByChar()['c2'];
    expect(zedBefore).toBeDefined();
    inner(component).handleSlotAdd('hair', 'waves');
    const zedStaged = inner(component).liveStagedByChar()['c2'];

    release();
    await settleLong(fixture, 16);
    // Zed's staging is untouched by c1's settle.
    expect(inner(component).liveStagedByChar()['c2']).toEqual(zedStaged);

    // Back on Aria: she seeds from the RESTORED snapshot, clean.
    inner(component).selectedContainer.set({ scope: 'character', id: 'c1' });
    await settleLong(fixture);
    expect(inner(component).liveStagedByChar()['c1']).toEqual(RESTORED);
    expect(surface(component).isLiveDirty('c1')).toBe(false);
  });
});
