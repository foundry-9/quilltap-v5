import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { coreStreamStub, type CoreStreamStub } from '../core/core-client.testing';
import { CoreDispatchError, type ScopedEvent } from '../core/core-contract';
import { AlertModalService } from '../ui/alert-modal.service';
import { ToastService } from '../ui/toast.service';
import { WORKSPACE_TAB_VISIBLE } from '../workspace/workspace-contract';
import { BOW, KIT, LAMP, RING, SWORD } from './__fixtures__/inventory-ui.fixture';
import {
  InventoryControlDialog,
  InventoryControlDialogInner,
  InventoryPanel,
  InventoryTabView,
} from './inventory-control-dialog';
import type { InventoryContainer } from './inventory-container';
import { InventoryDialogService } from './inventory-dialog.service';

/**
 * P4.D330 Tier 1 item 19 — the dialog triple, v5-authored over the wardrobe
 * dialog spec's harness (a case-local dispatch route answering C7's verbs):
 * the catalogue select, the fallback container, the shared note, the in-chat
 * grid + pane, `gesture`'s skipped toast and `CAP_EXCEEDED` alert, `reset`,
 * the Escape / backdrop gates, the host's initial container and remount key,
 * the tab view and the panel, and the realtime + tab-activation reloads.
 */

type Req = Record<string, unknown>;
type Route = (req: Req) => unknown;

const LISTS: Route = (req) => {
  switch (req['type']) {
    case 'characterList':
      return { characters: [{ id: 'c-viv', name: 'Vivienne' }, { id: 'c-bert', name: 'Bertie' }] };
    case 'projectList':
      return { projects: [{ id: 'p-2', name: '' }, { id: 'p-1', name: 'The Quest' }] };
    case 'groupList':
      return { groups: [{ id: 'g-1', name: 'The Regulars' }] };
    case 'inventoryList':
      return { items: [SWORD, RING, LAMP, BOW, KIT] };
    case 'inventoryFolders':
      return { folders: [] };
    case 'chatInventoryGet':
      return { inventory: { 'c-viv': { equipped: [RING], kits: { kit: 'some' } } }, projectId: null };
    default:
      return {};
  }
};

interface Handle<T> {
  fixture: ComponentFixture<T>;
  el: HTMLElement;
  seen: Req[];
  stream: CoreStreamStub;
}

async function settle<T>(fixture: ComponentFixture<T>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function configure(imports: unknown[], route: Route, extra: unknown[] = []): { seen: Req[]; stream: CoreStreamStub } {
  const seen: Req[] = [];
  const stream = coreStreamStub();
  TestBed.configureTestingModule({
    imports: imports as never[],
    providers: [
      provideTanStackQuery(new QueryClient()),
      {
        provide: CoreClient,
        useValue: {
          ...stream,
          dispatchData: vi.fn(async (req: Req) => {
            seen.push(req);
            const out = await route(req);
            if (out instanceof Error) throw out;
            return out;
          }),
        },
      },
      ...(extra as never[]),
    ],
  });
  return { seen, stream };
}

async function inner(
  inputs: { chatId?: string | null; initial?: InventoryContainer | null; asTab?: boolean; fixed?: boolean },
  route: Route = LISTS,
): Promise<Handle<InventoryControlDialogInner>> {
  const { seen, stream } = configure([InventoryControlDialogInner], route);
  const fixture = TestBed.createComponent(InventoryControlDialogInner);
  fixture.componentRef.setInput('initialContainer', inputs.initial ?? null);
  fixture.componentRef.setInput('chatId', inputs.chatId ?? null);
  fixture.componentRef.setInput('asTab', inputs.asTab ?? false);
  fixture.componentRef.setInput('fixedContainer', inputs.fixed ?? false);
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, el: fixture.nativeElement as HTMLElement, seen, stream };
}

const select = (el: HTMLElement) => el.querySelector('#inventory-container-select') as HTMLSelectElement | null;
const toasts = () => TestBed.inject(ToastService).toasts().map((t) => t.message);
const hint = (topic: string, id?: string) =>
  (id === undefined ? { v: 1, topic, at: 1 } : { v: 1, topic, id, at: 1 }) as unknown as ScopedEvent;

describe('InventoryControlDialogInner', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    TestBed.resetTestingModule();
  });

  it('the select: the four optgroups from the three LIST reads, sorted, the Untitled fallback', async () => {
    const { el } = await inner({});
    const groups = [...select(el)!.querySelectorAll('optgroup')].map((g) => [
      g.label,
      [...g.querySelectorAll('option')].map((o) => o.textContent!.trim()),
    ]);
    expect(groups).toEqual([
      ['Characters', ['Bertie', 'Vivienne']],
      ['General', ['Quilltap General']],
      ['Projects', ['The Quest', 'Untitled project']],
      ['Groups', ['The Regulars']],
    ]);
    expect(el.querySelector('label[for="inventory-container-select"]')!.textContent).toBe('Inventory:');
  });

  it('falls back to the first character only once the character list settles; General on a characterless instance', async () => {
    let release!: (v: unknown) => void;
    const slow: Route = (req) => (req['type'] === 'characterList' ? new Promise((r) => (release = r)) : LISTS(req));
    const { fixture, el, seen } = await inner({}, slow);
    expect(select(el)!.value).toBe('');
    expect(select(el)!.querySelector('option[disabled]')!.textContent).toBe('Select an inventory');
    expect(seen.filter((r) => r['type'] === 'inventoryList')).toEqual([]);
    release({ characters: [{ id: 'c-viv', name: 'Vivienne' }] });
    await settle(fixture);
    expect(select(el)!.value).toBe('character:c-viv');
    expect(seen.find((r) => r['type'] === 'inventoryList')).toEqual({
      type: 'inventoryList',
      container: { scope: 'character', id: 'c-viv' },
      shared: true,
    });
    TestBed.resetTestingModule();
    const empty = await inner({}, (req) => (req['type'] === 'characterList' ? { characters: [] } : LISTS(req)));
    expect(select(empty.el)!.value).toBe('general:');
  });

  it('the shared note names a library, a project or a group; none for a character', async () => {
    const { fixture, el } = await inner({ initial: { scope: 'general', id: null } });
    const note = () => el.querySelector('p.qt-text-xs.px-1')?.textContent?.replace(/\s+/g, ' ').trim();
    expect(note()).toBe(
      'A shared inventory — every character who can reach this library can carry what is here, and its archetypes can be instantiated by any of them.',
    );
    const s = select(el)!;
    s.value = 'project:p-1';
    s.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(note()).toContain('who can reach this project can carry');
    s.value = 'group:g-1';
    s.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(note()).toContain('who can reach this group can carry');
    s.value = 'character:c-viv';
    s.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(note()).toBeUndefined();
  });

  it('in a chat: the two-column grid, the Carrying-now pane, rows with Equip / Unequip', async () => {
    const { el } = await inner({ chatId: 'chat-1', initial: { scope: 'character', id: 'c-viv' } });
    expect(el.querySelector('.grid')!.getAttribute('class')).toBe('grid gap-4 md:grid-cols-2');
    expect(el.querySelector('[data-testid="inventory-carrying-now"] h3')!.textContent).toBe(
      'Carrying now · Vivienne in this chat',
    );
    expect(el.querySelector('[data-testid="inventory-item-row"] button.qt-button-secondary')!.textContent!.trim()).toBe('Equip');
  });

  it('out of a chat: one column, no pane, no Equip', async () => {
    const { el } = await inner({ initial: { scope: 'character', id: 'c-viv' } });
    expect(el.querySelector('.grid')!.getAttribute('class')).toBe('grid gap-4 ');
    expect(el.querySelector('[data-testid="inventory-carrying-now"]')).toBeNull();
  });

  it('a kit equip with skipped members says so in a SUCCESS toast; a cap breach alerts v4’s sentence (the labelled-button alert)', async () => {
    let refuse = false;
    const route: Route = (req) => {
      if (req['type'] === 'chatInventoryEquip') {
        return refuse
          ? new CoreDispatchError({ kind: 'conflict', message: 'x', code: 'CAP_EXCEEDED', details: { itemName: 'Excalibur', cap: 1 } })
          : { equipped: [], created: [], skipped: [{ id: 'a', kitId: 'kit', reason: 'missing' }, { id: 'b', kitId: 'kit', reason: 'archived' }], alreadyEquipped: [] };
      }
      return LISTS(req);
    };
    const { fixture, el, seen } = await inner({ chatId: 'chat-1', initial: { scope: 'character', id: 'c-viv' } }, route);
    const pane = el.querySelector('[aria-label="Kits"]')!;
    ([...pane.querySelectorAll('button')].find((b) => b.textContent!.trim() === 'Equip') as HTMLButtonElement).click();
    await settle(fixture);
    expect(seen.find((r) => r['type'] === 'chatInventoryEquip')).toEqual({
      type: 'chatInventoryEquip',
      chatId: 'chat-1',
      characterId: 'c-viv',
      ids: ['kit'],
    });
    expect(TestBed.inject(ToastService).toasts().at(-1)).toMatchObject({
      type: 'success',
      message: 'Done — 2 kit members could not be carried and stayed behind',
    });
    refuse = true;
    ([...pane.querySelectorAll('button')].find((b) => b.textContent!.trim() === 'Equip') as HTMLButtonElement).click();
    await settle(fixture);
    const alerts = TestBed.inject(AlertModalService);
    expect(alerts.current()).toMatchObject({
      message: '"Excalibur" can be carried at most 1 at a time — set one down before taking up another.',
      buttons: ['Understood'],
    });
    // While the alert is up the dialog's own Escape stands down.
    let closes = 0;
    fixture.componentInstance.closed.subscribe(() => closes++);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(closes).toBe(0);
    alerts.answer('Understood');
    await settle(fixture);
  });

  it('Reset to defaults asks first, then toasts; a refusal is worded', async () => {
    let fail = false;
    const route: Route = (req) =>
      req['type'] === 'chatInventoryResetDefaults'
        ? fail
          ? new Error('')
          : { equipped: [] }
        : LISTS(req);
    const { fixture, el, seen } = await inner({ chatId: 'chat-1', initial: { scope: 'character', id: 'c-viv' } }, route);
    const confirm = vi.spyOn(window, 'confirm').mockReturnValueOnce(false).mockReturnValue(true);
    const reset = () =>
      ([...el.querySelectorAll('button')].find((b) => b.textContent!.trim() === 'Reset to defaults') as HTMLButtonElement).click();
    reset();
    await settle(fixture);
    expect(confirm).toHaveBeenCalledWith(
      'Put back everything Vivienne carries here and start again from their equipped-by-default objects?',
    );
    expect(seen.filter((r) => r['type'] === 'chatInventoryResetDefaults')).toEqual([]);
    reset();
    await settle(fixture);
    expect(toasts().at(-1)).toBe('Vivienne is carrying their defaults again');
    fail = true;
    reset();
    await settle(fixture);
    expect(toasts().at(-1)).toBe('');
  });

  it('Escape closes the dialog — but not while the Take-up picker, an overlay or a confirmation is up', async () => {
    const { fixture, el } = await inner({ chatId: 'chat-1', initial: { scope: 'character', id: 'c-viv' } });
    let closes = 0;
    fixture.componentInstance.closed.subscribe(() => closes++);
    ([...el.querySelectorAll('button')].find((b) => b.textContent!.trim() === 'Take up…') as HTMLButtonElement).click();
    fixture.detectChanges();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    fixture.detectChanges();
    // The picker took the Escape; the dialog stood down.
    expect(closes).toBe(0);
    expect(el.querySelector('[aria-label="Objects within reach"]')).toBeNull();
    ([...el.querySelectorAll('button')].find((b) => b.textContent!.trim() === '+ New Item') as HTMLButtonElement).click();
    fixture.detectChanges();
    expect(el.querySelector('qt-inventory-item-editor')).not.toBeNull();
    const backdrop = el.querySelector('qt-modal > .qt-dialog-overlay') as HTMLElement;
    backdrop.click();
    expect(closes).toBe(0);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    fixture.detectChanges();
    expect(el.querySelector('qt-inventory-item-editor')).toBeNull();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(closes).toBe(1);
  });

  it('the modal chrome: Inventory, 4xl, Done', async () => {
    const { fixture, el } = await inner({ initial: { scope: 'general', id: null } });
    let closes = 0;
    fixture.componentInstance.closed.subscribe(() => closes++);
    expect(el.querySelector('.qt-dialog-title')!.textContent).toBe('Inventory');
    expect((el.querySelector('.qt-dialog') as HTMLElement).style.maxWidth).toBe('56rem');
    ([...el.querySelectorAll('[qt-modal-footer] button')][0] as HTMLButtonElement).click();
    expect(closes).toBe(1);
  });

  it('an edit opens the editor on the object’s own tier with the container label', async () => {
    const { fixture, el } = await inner({ initial: { scope: 'character', id: 'c-viv' } });
    const row = [...el.querySelectorAll('[data-testid="inventory-item-row"]')].find((r) => r.textContent!.includes('lantern'))!;
    (row.querySelector('[aria-label="More actions"]') as HTMLButtonElement).click();
    fixture.detectChanges();
    ([...el.querySelectorAll('[role="menuitem"]')][0] as HTMLButtonElement).click();
    await settle(fixture);
    expect(el.querySelector('qt-inventory-item-editor .qt-dialog-title')!.textContent).toBe('Edit lantern');
    expect(el.querySelector('qt-inventory-item-editor')!.textContent).toContain("Kept in Vivienne's inventory");
  });

  it('From archetype: a row click SURVIVES change detection through the real parent binding (one archetype read)', async () => {
    // Review finding (BLOCKING, `322a5488e` unification): the parent bound
    // `[destination]="destinationOf(request)"`, a NEW object every change
    // detection, so the instantiate dialog's seed effect re-ran on each click —
    // `chosen` back to null and the archetypes refetched — and no archetype
    // could ever be selected. v4 seeds once (`useState`). This mounts the REAL
    // control dialog and opens the dialog the way an operator does.
    const route: Route = (req) => (req['type'] === 'inventoryArchetypes' ? { items: [BOW] } : LISTS(req));
    const { fixture, el, seen } = await inner({ initial: { scope: 'character', id: 'c-viv' } }, route);
    const open = [...el.querySelectorAll('button')].find((b) => b.textContent!.includes('From archetype'))!;
    open.click();
    await settle(fixture);
    const reads = () => seen.filter((r) => r['type'] === 'inventoryArchetypes').length;
    expect(reads()).toBe(1);
    const option = () => el.querySelector('qt-inventory-instantiate-dialog [role="option"]') as HTMLButtonElement;
    expect(option().getAttribute('aria-selected')).toBe('false');
    option().click();
    await settle(fixture);
    expect(option().getAttribute('aria-selected')).toBe('true');
    const make = [...el.querySelectorAll('qt-inventory-instantiate-dialog button')].find(
      (b) => b.textContent!.trim() === 'Instantiate',
    ) as HTMLButtonElement;
    expect(make.disabled).toBe(false);
    expect(reads()).toBe(1);
  });

  it('an open editor is handed ONE container object across change detection (no refetch per keystroke)', async () => {
    // Review finding (SHOULD-FIX): `editorContainer(...)` / `actions.addressOf(...)`
    // built a new object per change detection, so every keystroke refetched the
    // editor's General list, its pictures and the folder tree.
    const { fixture, el, seen } = await inner({ initial: { scope: 'character', id: 'c-viv' } });
    const row = [...el.querySelectorAll('[data-testid="inventory-item-row"]')].find((r) => r.textContent!.includes('lantern'))!;
    (row.querySelector('[aria-label="More actions"]') as HTMLButtonElement).click();
    fixture.detectChanges();
    ([...el.querySelectorAll('[role="menuitem"]')][0] as HTMLButtonElement).click();
    await settle(fixture);
    const before = seen.length;
    const name = el.querySelector('qt-inventory-item-editor input') as HTMLInputElement;
    for (const ch of 'abc') {
      name.value += ch;
      name.dispatchEvent(new Event('input'));
      await settle(fixture);
    }
    expect(seen.slice(before)).toEqual([]);
  });

  it('the inventory realtime topic reloads the list and the tree; chats (this chat) the carried read', async () => {
    const { fixture, seen, stream } = await inner({ chatId: 'chat-1', initial: { scope: 'character', id: 'c-viv' } });
    const count = (type: string) => seen.filter((r) => r['type'] === type).length;
    const before = [count('inventoryList'), count('inventoryFolders'), count('chatInventoryGet')];
    stream.frames.next(hint('inventory'));
    await settle(fixture);
    expect([count('inventoryList'), count('inventoryFolders')]).toEqual([before[0] + 1, before[1] + 1]);
    stream.frames.next(hint('chats', 'other-chat'));
    await settle(fixture);
    expect(count('chatInventoryGet')).toBe(before[2]);
    stream.frames.next(hint('chats', 'chat-1'));
    await settle(fixture);
    expect(count('chatInventoryGet')).toBe(before[2] + 1);
  });

  it('as a tab: the bare qt-inventory-tab chrome, no Escape close; the panel: no select, a plain column', async () => {
    const tab = await inner({ asTab: true, initial: { scope: 'character', id: 'c-viv' } });
    expect(tab.el.querySelector('.qt-inventory-tab')!.getAttribute('class')).toBe(
      'qt-inventory-tab flex flex-col h-full min-h-0 overflow-y-auto p-4',
    );
    let closes = 0;
    tab.fixture.componentInstance.closed.subscribe(() => closes++);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(closes).toBe(0);
    expect(tab.el.querySelector('.qt-dialog')).toBeNull();
    TestBed.resetTestingModule();
    const panel = await inner({ asTab: true, fixed: true, initial: { scope: 'project', id: 'p-1' } });
    expect(select(panel.el)).toBeNull();
    expect(panel.el.firstElementChild!.getAttribute('class')).toBe('flex flex-col min-h-0');
    expect(panel.seen.find((r) => r['type'] === 'inventoryList')).toEqual({
      type: 'inventoryList',
      container: { scope: 'project', id: 'p-1' },
    });
  });
});

describe('the dialog host, the tab view and the panel', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('the host renders only while open, starting on the context’s character / group / project — never a chat’s project', async () => {
    const { seen } = configure([InventoryControlDialog], LISTS);
    const fixture = TestBed.createComponent(InventoryControlDialog);
    const dialog = TestBed.inject(InventoryDialogService);
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    expect(el.querySelector('qt-inventory-control-dialog-inner')).toBeNull();
    dialog.open({ projectId: 'p-1', chatId: 'chat-1' });
    fixture.detectChanges();
    await settle(fixture);
    // A chat-opened dialog never starts on the project (trap 13): the fallback character.
    expect(select(el)!.value).toBe('character:c-bert');
    dialog.open({ groupId: 'g-1' });
    fixture.detectChanges();
    await settle(fixture);
    expect(select(el)!.value).toBe('group:g-1');
    dialog.open({ projectId: 'p-1' });
    fixture.detectChanges();
    await settle(fixture);
    expect(select(el)!.value).toBe('project:p-1');
    dialog.close();
    fixture.detectChanges();
    expect(el.querySelector('qt-inventory-control-dialog-inner')).toBeNull();
    expect(seen.length).toBeGreaterThan(0);
  });

  it('the tab view starts on its character, else its project, else the fallback; onTabActivated reloads', async () => {
    const visible = signal(true);
    const { seen } = configure([InventoryTabView], LISTS, [{ provide: WORKSPACE_TAB_VISIBLE, useValue: visible }]);
    const fixture = TestBed.createComponent(InventoryTabView);
    fixture.componentRef.setInput('characterId', 'c-viv');
    fixture.detectChanges();
    await settle(fixture);
    const el = fixture.nativeElement as HTMLElement;
    expect(select(el)!.value).toBe('character:c-viv');
    const lists = () => seen.filter((r) => r['type'] === 'inventoryList').length;
    const before = lists();
    visible.set(false);
    fixture.detectChanges();
    visible.set(true);
    fixture.detectChanges();
    await settle(fixture);
    expect(lists()).toBe(before + 1);
  });

  it('the panel pins one tier', async () => {
    @Component({
      imports: [InventoryPanel],
      template: `<qt-inventory-panel [container]="container" projectId="p-1" />`,
    })
    class Card {
      readonly container: InventoryContainer = { scope: 'project', id: 'p-1' };
    }
    configure([Card], LISTS);
    const fixture = TestBed.createComponent(Card);
    fixture.detectChanges();
    await settle(fixture);
    const el = fixture.nativeElement as HTMLElement;
    expect(select(el)).toBeNull();
    expect(el.querySelector('qt-inventory-browser')).not.toBeNull();
  });
});
