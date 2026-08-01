import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { CoreRequest, WardrobeItemDto } from '../core/core-contract';
import { WardrobeItemEditor } from './wardrobe-item-editor';
import { ToastService } from '../ui/toast.service';

type AnyRequest = CoreRequest & Record<string, unknown>;

function item(over: Partial<WardrobeItemDto> = {}): WardrobeItemDto {
  return {
    id: 'w1',
    characterId: 'char-1',
    title: 'Charcoal Sweater',
    description: null,
    imagePrompt: null,
    types: ['top'],
    componentItemIds: [],
    appropriateness: null,
    isDefault: false,
    replace: false,
    archivedAt: null,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    ...over,
  };
}

interface Handle {
  fixture: ComponentFixture<WardrobeItemEditor>;
  component: WardrobeItemEditor;
  seen: AnyRequest[];
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 4): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await Promise.resolve();
    fixture.detectChanges();
  }
}

async function render(
  inputs: Partial<{
    characterId: string;
    item: WardrobeItemDto | null;
    isShared: boolean;
    projectId: string | null;
    initialComponentItemIds: string[];
    initialMode: 'single' | 'bundle';
  }> = {},
  route: (req: AnyRequest) => Record<string, unknown> | Error = () => ({ wardrobeItems: [] }),
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
    imports: [WardrobeItemEditor],
    providers: [{ provide: CoreClient, useValue: core }],
  });
  const fixture = TestBed.createComponent(WardrobeItemEditor);
  fixture.componentRef.setInput('characterId', inputs.characterId ?? 'char-1');
  if (inputs.item !== undefined) fixture.componentRef.setInput('item', inputs.item);
  if (inputs.isShared !== undefined) fixture.componentRef.setInput('isShared', inputs.isShared);
  if (inputs.projectId !== undefined) fixture.componentRef.setInput('projectId', inputs.projectId);
  if (inputs.initialComponentItemIds !== undefined) {
    fixture.componentRef.setInput('initialComponentItemIds', inputs.initialComponentItemIds);
  }
  if (inputs.initialMode !== undefined) {
    fixture.componentRef.setInput('initialMode', inputs.initialMode);
  }
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, component: fixture.componentInstance, seen };
}

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('WardrobeItemEditor (v4 wardrobe-item-editor.tsx)', () => {
  afterEach(() => vi.restoreAllMocks());

  // -------------------------------------------------------------------------
  // The five tier-routed save paths (v4 :358-369) — the lane's key transcription
  // -------------------------------------------------------------------------
  describe('the save route is chosen BY TIER (v4 :358-369)', () => {
    it('editing a shared item PUTs the global route (:360)', async () => {
      const { component } = await render({
        item: item({ id: 'shared-1', characterId: null }),
        isShared: true,
      });
      expect(component.buildSaveRequest()).toMatchObject({
        type: 'wardrobeUpdate',
        itemId: 'shared-1',
      });
    });

    it('editing an owned item PUTs the character route (:361)', async () => {
      const { component } = await render({ item: item({ id: 'own-1' }) });
      expect(component.buildSaveRequest()).toMatchObject({
        type: 'characterWardrobeUpdate',
        characterId: 'char-1',
        itemId: 'own-1',
      });
    });

    it("createScope 'project' with a projectId POSTs the project route (:363)", async () => {
      const { component } = await render({ projectId: 'p1' });
      (component as unknown as { createScope: { set(v: string): void } }).createScope.set(
        'project',
      );
      expect(component.buildSaveRequest()).toMatchObject({
        type: 'projectWardrobeCreate',
        projectId: 'p1',
      });
    });

    it("createScope 'global' POSTs the global route (:365)", async () => {
      const { component } = await render();
      (component as unknown as { createScope: { set(v: string): void } }).createScope.set(
        'global',
      );
      expect(component.buildSaveRequest()).toMatchObject({ type: 'wardrobeCreate' });
    });

    it('otherwise POSTs the character route (:367)', async () => {
      const { component } = await render();
      expect(component.buildSaveRequest()).toMatchObject({
        type: 'characterWardrobeCreate',
        characterId: 'char-1',
      });
    });

    it("createScope 'project' WITHOUT a projectId falls through to the character route (v4's `&& projectId` guard)", async () => {
      const { component } = await render();
      (component as unknown as { createScope: { set(v: string): void } }).createScope.set(
        'project',
      );
      expect(component.buildSaveRequest()).toMatchObject({ type: 'characterWardrobeCreate' });
    });
  });

  it('the payload sends blanks as null and forces replace=false for leaf items (v4 :342-352)', async () => {
    const { component } = await render();
    const c = component as unknown as {
      title: { set(v: string): void };
      selectedTypes: { set(v: string[]): void };
      replace: { set(v: boolean): void };
    };
    c.title.set('Charcoal Sweater');
    c.selectedTypes.set(['top']);
    c.replace.set(true); // single mode → forced back to false on save
    expect(component.buildSaveRequest()).toMatchObject({
      item: {
        title: 'Charcoal Sweater',
        description: null,
        imagePrompt: null,
        types: ['top'],
        appropriateness: null,
        isDefault: false,
        componentItemIds: [],
        replace: false,
      },
    });
  });

  it('isShared on edit is fixed by the item tier — a shared item without the input still routes global (v4 :57-62)', async () => {
    const { component } = await render({ item: item({ id: 's2', characterId: null }) });
    expect(component.buildSaveRequest()).toMatchObject({ type: 'wardrobeUpdate', itemId: 's2' });
  });

  // -------------------------------------------------------------------------
  // Candidate loading (v4 :139-189)
  // -------------------------------------------------------------------------
  it('loads candidates from the three tiers, de-duped, flagging shared tiers (v4 :144-174)', async () => {
    const { component, seen } = await render(
      { projectId: 'p1' },
      (req) => {
        switch (req.type as string) {
          case 'characterWardrobeList':
            return { wardrobeItems: [item({ id: 'a' })] };
          case 'projectWardrobeList':
            return { wardrobeItems: [item({ id: 'a' }), item({ id: 'b' })] };
          case 'wardrobeList':
            return { wardrobeItems: [item({ id: 'c', characterId: null })] };
          default:
            return new Error(`unexpected ${req.type}`);
        }
      },
    );
    expect(seen.map((r) => r.type)).toEqual([
      'characterWardrobeList',
      'projectWardrobeList',
      'wardrobeList',
    ]);
    const candidates = (
      component as unknown as { candidates: () => Array<{ id: string; isShared: boolean }> }
    ).candidates();
    expect(candidates.map((c) => [c.id, c.isShared])).toEqual([
      ['a', false],
      ['b', true],
      ['c', true],
    ]);
  });

  it('a failing global tier fails soft (pre-unify: lane P4.9f1 not landed)', async () => {
    const { component } = await render({}, (req) =>
      (req.type as string) === 'wardrobeList'
        ? new Error('unknown request type')
        : { wardrobeItems: [item({ id: 'a' })] },
    );
    const candidates = (
      component as unknown as { candidates: () => Array<{ id: string }> }
    ).candidates();
    expect(candidates.map((c) => c.id)).toEqual(['a']);
  });

  // -------------------------------------------------------------------------
  // Types / bundle machinery (v4 :226-257)
  // -------------------------------------------------------------------------
  it('computedTypes mirrors the server union in canonical order (v4 :226-230)', async () => {
    const { component } = await render({}, (req) =>
      (req.type as string) === 'characterWardrobeList'
        ? {
            wardrobeItems: [
              item({ id: 'boots', types: ['footwear'] }),
              item({ id: 'shirt', types: ['top'] }),
            ],
          }
        : { wardrobeItems: [] },
    );
    const c = component as unknown as {
      componentItemIds: { set(v: string[]): void };
      computedTypes: () => string[];
    };
    c.componentItemIds.set(['boots', 'shirt']);
    expect(c.computedTypes()).toEqual(['top', 'footwear']);
  });

  it('a replace composite may designate slots beyond the union; union slots are locked on (v4 :232-257)', async () => {
    const { component } = await render({ initialMode: 'bundle' }, (req) =>
      (req.type as string) === 'characterWardrobeList'
        ? { wardrobeItems: [item({ id: 'shirt', types: ['top'] })] }
        : { wardrobeItems: [] },
    );
    const c = component as unknown as {
      componentItemIds: { set(v: string[]): void };
      bundleDesignatedTypes: { set(v: string[]): void };
      replace: { set(v: boolean): void };
      effectiveTypes: () => string[];
      handleTypeToggle: (t: string) => void;
    };
    c.componentItemIds.set(['shirt']);
    c.bundleDesignatedTypes.set([]);
    // Additive composite: coverage is exactly the union.
    expect(c.effectiveTypes()).toEqual(['top']);
    // Toggling a type in additive bundle mode is a no-op (v4 :246 `if (!replace) return`).
    c.handleTypeToggle('bottom');
    expect(c.effectiveTypes()).toEqual(['top']);
    // Replace composite: designation widens beyond the union…
    c.replace.set(true);
    c.handleTypeToggle('bottom');
    expect(c.effectiveTypes()).toEqual(['top', 'bottom']);
    // …but a union slot cannot be toggled off (v4 :247 `if (computedTypes.includes) return`).
    c.handleTypeToggle('top');
    expect(c.effectiveTypes()).toEqual(['top', 'bottom']);
  });

  // -------------------------------------------------------------------------
  // Mode change (v4 :274-298)
  // -------------------------------------------------------------------------
  it('bundle → single with components prompts; Keep locks the computed types, Reset clears (v4 :285-298)', async () => {
    const { component } = await render({ initialMode: 'bundle' }, (req) =>
      (req.type as string) === 'characterWardrobeList'
        ? { wardrobeItems: [item({ id: 'dress', types: ['top', 'bottom'] })] }
        : { wardrobeItems: [] },
    );
    const c = component as unknown as {
      componentItemIds: { set(v: string[]): void; (): string[] };
      handleModeChange: (m: string) => void;
      handleConfirmKeepTypes: () => void;
      handleConfirmReset: () => void;
      showKeepResetPrompt: () => boolean;
      editorMode: () => string;
      selectedTypes: () => string[];
    };
    c.componentItemIds.set(['dress']);
    c.handleModeChange('single');
    expect(c.showKeepResetPrompt()).toBe(true);
    expect(c.editorMode()).toBe('bundle');
    c.handleConfirmKeepTypes();
    expect(c.editorMode()).toBe('single');
    expect(c.selectedTypes()).toEqual(['top', 'bottom']);
    expect(c.componentItemIds()).toEqual([]);
  });

  it('seeding: components imply bundle mode; initialMode wins (v4 :96-101)', async () => {
    const a = await render({ item: item({ componentItemIds: ['x'] }) });
    expect((a.component as unknown as { editorMode: () => string }).editorMode()).toBe('bundle');
    const b = await render({ initialMode: 'bundle', initialComponentItemIds: [] });
    expect((b.component as unknown as { editorMode: () => string }).editorMode()).toBe('bundle');
  });

  // -------------------------------------------------------------------------
  // Validation gauntlet (v4 :317-335)
  // -------------------------------------------------------------------------
  it('handleSave surfaces the v4 validation messages in order and fires no dispatch', async () => {
    const { component, seen } = await render();
    const c = component as unknown as {
      handleSave: () => Promise<void>;
      saveError: () => string | null;
      title: { set(v: string): void };
      selectedTypes: { set(v: string[]): void };
    };
    const baseline = seen.length;
    await c.handleSave();
    expect(toasts().at(-1)).toEqual({ type: 'error', message: 'Enter a title' });
    c.title.set('Hat');
    await c.handleSave();
    expect(toasts().at(-1)).toEqual({ type: 'error', message: 'Select at least one type' });
    expect(seen.length).toBe(baseline);
  });

  it('a successful save dispatches the routed request and emits saved', async () => {
    const { component, fixture, seen } = await render();
    const savedSpy = vi.fn();
    component.saved.subscribe(savedSpy);
    const c = component as unknown as {
      handleSave: () => Promise<void>;
      title: { set(v: string): void };
      selectedTypes: { set(v: string[]): void };
    };
    c.title.set('Hat');
    c.selectedTypes.set(['accessories']);
    await c.handleSave();
    await settle(fixture);
    expect(seen.at(-1)).toMatchObject({ type: 'characterWardrobeCreate', characterId: 'char-1' });
    expect(savedSpy).toHaveBeenCalledTimes(1);
    // v4 `wardrobe-item-editor.tsx:383` — the create-mode success toast.
    expect(toasts().at(-1)).toEqual({ type: 'success', message: 'Wardrobe item created' });
  });
});
