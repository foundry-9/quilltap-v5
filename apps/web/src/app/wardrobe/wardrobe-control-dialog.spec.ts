import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { CoreRequest, WardrobeItemDto } from '../core/core-contract';
import type { EquippedSlots } from './equipped-slots';
import {
  WardrobeControlDialog,
  WardrobeControlDialogInner,
  WardrobeTabView,
} from './wardrobe-control-dialog';
import { WardrobeDialogService } from './wardrobe-dialog.service';
import { ToastService } from '../ui/toast.service';

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
  fixture.componentRef.setInput('initialCharacterId', 'c1');
  fixture.componentRef.setInput('chatId', chatId);
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, component: fixture.componentInstance, seen };
}

/** Reach the protected surface for state-level assertions. */
function inner(component: WardrobeControlDialogInner): {
  rightTab: { (): string; set(v: string): void };
  liveStagedByChar: () => Record<string, EquippedSlots>;
  fittingSlots: { (): EquippedSlots; set(v: EquippedSlots): void };
  titleFilter: { (): string; set(v: string): void };
  handleEquipItem: (item: WardrobeItemDto) => void;
  handleAddToSlot: (item: WardrobeItemDto, slot: string) => void;
  handleSlotAdd: (slot: string, itemId: string) => void;
  handleSlotRemove: (slot: string, itemId: string) => void;
  handleSlotClear: (slot: string) => void;
  rowEquip: (item: WardrobeItemDto) => void;
  fittingAdd: (slot: string, itemId: string) => void;
  fittingClear: (slot: string) => void;
  wearFitting: () => Promise<void>;
  requestClose: () => void;
  useFittingActions: () => boolean;
  selectedContainer: {
    (): { scope: string; id: string | null } | null;
    set(v: { scope: string; id: string | null } | null): void;
  };
  selectedCharacterId: () => string | null;
  isCharacterScope: () => boolean;
  listItems: () => WardrobeItemDto[];
  resolutionPool: () => WardrobeItemDto[];
  canManageItem: (item: WardrobeItemDto) => boolean;
  selectedContainerLabel: () => string;
  transferExcludeDestination: () => { scope: string; id: string | null } | null;
  pickContainer: (value: string) => void;
  handleDuplicate: (item: WardrobeItemDto) => Promise<void>;
  handleToggleDefault: (item: WardrobeItemDto) => Promise<void>;
  handleToggleArchived: (item: WardrobeItemDto) => Promise<void>;
  handleDelete: (item: WardrobeItemDto) => Promise<void>;
  showArchived: { (): boolean; set(v: boolean): void };
  showShared: { (): boolean; set(v: boolean): void };
  filteredItems: () => WardrobeItemDto[];
  transferringItem: { (): WardrobeItemDto | null; set(v: WardrobeItemDto | null): void };
} {
  return component as unknown as ReturnType<typeof inner>;
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
    const c = component as unknown as { slotFilter: { set(v: string): void } };
    c.slotFilter.set('hair');
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
      providers: [{ provide: CoreClient, useValue: makeCore(seen) }],
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
      providers: [{ provide: CoreClient, useValue: makeCore(seen) }],
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
        ].includes(r.type as string),
      ),
    ).toBe(true);
  });

  it('a payload-less tab auto-selects the first character (v4 WardrobeView, no characterId)', async () => {
    const seen: AnyRequest[] = [];
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [WardrobeTabView],
      providers: [{ provide: CoreClient, useValue: makeCore(seen) }],
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
      providers: [{ provide: CoreClient, useValue: core }, WardrobeDialogService],
    });
    const service = TestBed.inject(WardrobeDialogService);
    const fixture = TestBed.createComponent(WardrobeControlDialog);
    fixture.detectChanges();

    // Closed → renders nothing (v4 :89).
    expect(fixture.debugElement.query(By.directive(WardrobeControlDialogInner))).toBeNull();

    service.open({ characterId: 'c1' });
    await settle(fixture);
    const first = fixture.debugElement.query(By.directive(WardrobeControlDialogInner))
      .componentInstance as WardrobeControlDialogInner;
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
    const second = fixture.debugElement.query(By.directive(WardrobeControlDialogInner))
      .componentInstance as WardrobeControlDialogInner;
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
 * borrowed rows are badged `· shared` and can't be edited from here, and when
 * you're dressing a character or building an outfit out of their own clothes
 * they're just noise. The toggle is on by default — hiding is opt-in — and it
 * filters after the merge, since ownership isn't something the fetch can ask
 * the server for.
 */

/** A Quilltap General archetype merged in from above — badged `· shared`. */
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

  // v4's first test, 1:1.
  it('lists shared items by default and drops them when unticked, keeping the character’s own', async () => {
    const { fixture, component } = await renderInner(null, sharedRoute);
    expect(titles(component)).toEqual(['Apple Watch', 'Linen Shirt']);
    expect(sharedBox(fixture)!.checked).toBe(true);

    sharedBox(fixture)!.click();
    await settle(fixture);
    expect(titles(component)).toEqual(['Linen Shirt']);

    // And back again — hiding is a view filter, not a fetch.
    sharedBox(fixture)!.click();
    await settle(fixture);
    expect(titles(component)).toEqual(['Apple Watch', 'Linen Shirt']);
  });

  // v4's second test, 1:1.
  it('leaves the archived toggle alone', async () => {
    const { fixture, component } = await renderInner(null, sharedRoute);
    expect(archivedBox(fixture).checked).toBe(false);

    sharedBox(fixture)!.click();
    await settle(fixture);
    expect(titles(component)).toEqual(['Linen Shirt']);
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
  type Gen = { handleGenerateImage(i: WardrobeItemDto): Promise<void>; generatingImageIds(): ReadonlySet<string> };

  it('the rows opt in, and a character garment is drawn in its own vault', async () => {
    const fetchFn = stubGenerate(201, { rerouted: false, current: 'f1' });
    const { fixture, component, seen } = await renderInner(null);
    const row = fixture.debugElement.queryAll(By.css('qt-wardrobe-item-row'))[0];
    expect(row.componentInstance.canGenerateImage()).toBe(true);
    const before = seen.filter((r) => (r.type as string) === 'characterWardrobeList').length;
    await (component as unknown as Gen).handleGenerateImage(HAT);
    await settle(fixture);
    expect(fetchFn.mock.calls[0][0]).toBe('/api/v1/wardrobe/hat/images?scope=character&id=c1&action=generate');
    expect((fetchFn.mock.calls[0][1] as RequestInit).body).toBe('{}');
    expect(toasts()).toContainEqual({ type: 'success', message: 'A portrait of "Hat" is hung' });
    expect(seen.filter((r) => (r.type as string) === 'characterWardrobeList').length).toBeGreaterThan(before);
    expect((component as unknown as Gen).generatingImageIds().size).toBe(0);
  });

  it('a shared garment in the character view goes to General; a browsed container is its own home', async () => {
    const fetchFn = stubGenerate(201, { rerouted: true, current: 'f1' });
    const { fixture, component } = await renderInner(null);
    const cape = dto({ id: 'cape', title: 'Cape' });
    (cape as { characterId: string | null }).characterId = null;
    await (component as unknown as Gen).handleGenerateImage(cape);
    expect(fetchFn.mock.calls[0][0]).toBe('/api/v1/wardrobe/cape/images?scope=general&action=generate');
    expect(toasts()).toContainEqual({
      type: 'success',
      message: 'A portrait of "Cape" is hung — drawn at the uncensored desk',
    });
    inner(component).selectedContainer.set({ scope: 'project', id: 'p1' });
    await settle(fixture);
    await (component as unknown as Gen).handleGenerateImage(cape);
    expect(fetchFn.mock.calls[1][0]).toBe('/api/v1/wardrobe/cape/images?scope=project&id=p1&action=generate');
  });

  it('a failure toasts the route’s sentence, or v4’s fallback', async () => {
    stubGenerate(400, { error: 'No image profiles are configured' });
    const { component } = await renderInner(null);
    await (component as unknown as Gen).handleGenerateImage(HAT);
    expect(toasts()).toContainEqual({ type: 'error', message: 'No image profiles are configured' });
    vi.stubGlobal('fetch', vi.fn(async () => {
      throw 'offline';
    }));
    await (component as unknown as Gen).handleGenerateImage(HAT);
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
    const g = component as unknown as Gen;
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
    (component as unknown as { editingItem: { set(v: WardrobeItemDto): void } }).editingItem.set(HAT);
    await settle(fixture);
    const editor = fixture.debugElement.query((d) => d.name === 'qt-wardrobe-item-editor');
    expect(editor).not.toBeNull();
    const before = seen.filter((r) => (r.type as string) === 'characterWardrobeList').length;
    (editor.componentInstance as { imageChanged: { emit(): void } }).imageChanged.emit();
    await settle(fixture);
    expect(seen.filter((r) => (r.type as string) === 'characterWardrobeList').length).toBeGreaterThan(before);
  });
});
