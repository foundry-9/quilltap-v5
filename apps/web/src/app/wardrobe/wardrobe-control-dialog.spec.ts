import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
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
    providers: [{ provide: CoreClient, useValue: core }],
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
        ['characterList', 'imageProfileList', 'characterWardrobeList', 'wardrobeList'].includes(
          r.type as string,
        ),
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
    expect(tabEl.querySelector('#wardrobe-char-select')).not.toBeNull();
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
        ['characterList', 'imageProfileList', 'characterWardrobeList', 'wardrobeList'].includes(
          r.type as string,
        ),
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
    const select = el.querySelector('#wardrobe-char-select') as HTMLSelectElement;
    // Characters sort by name (Aria < Zed) → the first is Aria (c1).
    expect(select.value).toBe('c1');
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
