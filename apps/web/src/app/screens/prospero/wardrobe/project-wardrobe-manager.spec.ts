import { ComponentFixture, TestBed } from '@angular/core/testing';
import { signal } from '@angular/core';
import { By } from '@angular/platform-browser';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { WardrobeItemDto } from '../../../core/core-contract';
import { RichEditor } from '../../../editor/rich-editor';
import type { WardrobeMutator, WardrobeResult } from '../wardrobe.api';
import { ProjectWardrobeManager } from './project-wardrobe-manager';

/** The draft form's Description editor (qt-markdown-field's RichEditor handle). */
function descriptionEditor(fixture: ComponentFixture<unknown>): RichEditor {
  return fixture.debugElement.query(By.directive(RichEditor)).componentInstance as RichEditor;
}

function item(over: Partial<WardrobeItemDto> = {}): WardrobeItemDto {
  return {
    id: 'w1',
    title: 'House livery jacket',
    description: 'Deep green wool.',
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

interface MockHandle {
  mutator: WardrobeMutator;
  calls: {
    create: unknown[];
    update: [string, unknown][];
    delete: string[];
    archived: [string, boolean][];
    showArchived: boolean[];
  };
  showArchived: ReturnType<typeof signal<boolean>>;
  items: ReturnType<typeof signal<WardrobeItemDto[]>>;
  results: {
    create: WardrobeResult<{ item: WardrobeItemDto | undefined }>;
    update: WardrobeResult;
    delete: WardrobeResult;
  };
}

function mockMutator(initial: WardrobeItemDto[] = []): MockHandle {
  const items = signal<WardrobeItemDto[]>(initial);
  const loading = signal(false);
  const error = signal<string | null>(null);
  const showArchived = signal(false);
  const calls: MockHandle['calls'] = {
    create: [],
    update: [],
    delete: [],
    archived: [],
    showArchived: [],
  };
  const results: MockHandle['results'] = {
    create: { ok: true, item: item() },
    update: { ok: true },
    delete: { ok: true },
  };
  const mutator: WardrobeMutator = {
    items,
    loading,
    error,
    showArchived,
    setShowArchived: (next) => {
      calls.showArchived.push(next);
      showArchived.set(next);
    },
    refresh: async () => undefined,
    createItem: async (input) => {
      calls.create.push(input);
      return results.create;
    },
    updateItem: async (id, patch) => {
      calls.update.push([id, patch]);
      return results.update;
    },
    deleteItem: async (id) => {
      calls.delete.push(id);
      return results.delete;
    },
    setItemArchived: async (id, archived) => {
      calls.archived.push([id, archived]);
      return results.update;
    },
  };
  return { mutator, calls, items, results, showArchived };
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 4): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await Promise.resolve();
    fixture.detectChanges();
  }
}

async function render(handle: MockHandle): Promise<ComponentFixture<ProjectWardrobeManager>> {
  TestBed.configureTestingModule({ imports: [ProjectWardrobeManager] });
  const fixture = TestBed.createComponent(ProjectWardrobeManager);
  fixture.componentRef.setInput('mutator', handle.mutator);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

function byText(fixture: ComponentFixture<unknown>, label: string): HTMLButtonElement {
  const btn = Array.from(
    (fixture.nativeElement as HTMLElement).querySelectorAll('button'),
  ).find((b) => (b.textContent ?? '').trim() === label);
  if (!btn) throw new Error(`button "${label}" not found`);
  return btn as HTMLButtonElement;
}

function setValue(el: HTMLElement, selector: string, value: string): void {
  const input = el.querySelector(selector) as HTMLInputElement | HTMLTextAreaElement;
  input.value = value;
  input.dispatchEvent(new Event('input'));
}

describe('ProjectWardrobeManager', () => {
  afterEach(() => vi.restoreAllMocks());

  it('shows the empty message with no items', async () => {
    const fixture = await render(mockMutator());
    expect(text(fixture)).toContain(
      "No project wardrobe yet. Add a garment and every character in this project's chats can wear it.",
    );
  });

  it('renders a row with types, Composite/Default/Archived badges, and the cue', async () => {
    const fixture = await render(
      mockMutator([
        item({
          title: 'Estate ensemble',
          types: ['top', 'bottom'],
          componentItemIds: ['a', 'b'],
          isDefault: true,
          archivedAt: '2024-02-01T00:00:00.000Z',
          imagePrompt: 'a tailored green suit',
        }),
      ]),
    );
    const t = text(fixture);
    expect(t).toContain('Estate ensemble');
    expect(t).toContain('top, bottom');
    expect(t).toContain('Composite');
    expect(t).toContain('Default');
    expect(t).toContain('Archived');
    // imagePrompt is preferred over description in the secondary line.
    expect(t).toContain('a tailored green suit');
  });

  it('creates: blank optional strings ride as null, default types = [top]', async () => {
    const handle = mockMutator();
    const fixture = await render(handle);
    byText(fixture, '+ New wardrobe item').click();
    await settle(fixture);

    const el = fixture.nativeElement as HTMLElement;
    setValue(el, 'input[placeholder="e.g. House livery jacket"]', 'Cravat');
    await settle(fixture);
    byText(fixture, 'Create item').click();
    await settle(fixture);

    expect(handle.calls.create).toEqual([
      {
        title: 'Cravat',
        description: null,
        imagePrompt: null,
        types: ['top'],
        appropriateness: null,
        isDefault: false,
        replace: false,
        componentItemIds: [],
      },
    ]);
  });

  it('validates: empty title and empty slot floor', async () => {
    const handle = mockMutator();
    const fixture = await render(handle);
    byText(fixture, '+ New wardrobe item').click();
    await settle(fixture);
    // Empty title.
    byText(fixture, 'Create item').click();
    await settle(fixture);
    expect(text(fixture)).toContain('Title is required.');
    expect(handle.calls.create).toEqual([]);

    // Un-checking the sole default slot is a no-op (floor of one).
    const el = fixture.nativeElement as HTMLElement;
    const topCheckbox = Array.from(el.querySelectorAll<HTMLInputElement>('input[type="checkbox"]'))[0];
    topCheckbox.dispatchEvent(new Event('change'));
    await settle(fixture);
    setValue(el, 'input[placeholder="e.g. House livery jacket"]', 'Sash');
    await settle(fixture);
    byText(fixture, 'Create item').click();
    await settle(fixture);
    // Slot floor held → the create still carries ['top'].
    expect(handle.calls.create).toEqual([
      expect.objectContaining({ title: 'Sash', types: ['top'] }),
    ]);
  });

  it('edits: seeds the draft and updates with the full payload', async () => {
    const handle = mockMutator([item({ id: 'w9', title: 'Waistcoat', types: ['top'] })]);
    const fixture = await render(handle);
    byText(fixture, 'Edit').click();
    await settle(fixture);

    const el = fixture.nativeElement as HTMLElement;
    setValue(el, 'input[placeholder="e.g. House livery jacket"]', 'Waistcoat, brocade');
    await settle(fixture);
    byText(fixture, 'Save changes').click();
    await settle(fixture);

    expect(handle.calls.update).toEqual([
      [
        'w9',
        expect.objectContaining({ title: 'Waistcoat, brocade', types: ['top'] }),
      ],
    ]);
  });

  it('the description markdown field seeds from the item and rides into the update', async () => {
    const handle = mockMutator([
      item({ id: 'w9', title: 'Waistcoat', description: 'Deep *green* wool.' }),
    ]);
    const fixture = await render(handle);
    byText(fixture, 'Edit').click();
    await settle(fixture);

    expect(descriptionEditor(fixture).getMarkdown()).toBe('Deep *green* wool.');

    descriptionEditor(fixture).setMarkdown('Deep **green** wool, brass-buttoned.');
    await settle(fixture);
    byText(fixture, 'Save changes').click();
    await settle(fixture);

    expect(handle.calls.update).toEqual([
      ['w9', expect.objectContaining({ description: 'Deep **green** wool, brass-buttoned.' })],
    ]);
  });

  it('composite picker excludes the item being edited', async () => {
    const handle = mockMutator([
      item({ id: 'w1', title: 'Jacket' }),
      item({ id: 'w2', title: 'Trousers' }),
    ]);
    const fixture = await render(handle);
    // Editing w1 → the picker lists only w2.
    byText(fixture, 'Edit').click();
    await settle(fixture);
    const t = text(fixture);
    expect(t).toContain('Bundle other project items (composite — optional)');
    const pickerLabels = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll('.max-h-40 label'),
    ).map((l) => (l.textContent ?? '').trim());
    expect(pickerLabels).toEqual(['Trousers']);
  });

  /**
   * P4.D121 — the archive surface (v4 `d25dacc1`). INLINE buttons here, not a
   * kebab; a "Show archived" checkbox that flips the mutator's FETCH; and no
   * confirm, because archiving destroys nothing.
   */
  it('archives an active garment and restores an archived one, with v4’s titles', async () => {
    const handle = mockMutator([item({ id: 'w1', title: 'Jacket' })]);
    const fixture = await render(handle);

    expect(text(fixture)).not.toContain('Archived');
    const archive = byText(fixture, 'Archive');
    expect(archive.title).toBe('Hide this garment from the wardrobe pickers');
    archive.click();
    await settle(fixture);
    expect(handle.calls.archived).toEqual([['w1', true]]);
    // No confirm dialog was consulted at all.
    expect(vi.mocked(window.confirm).mock?.calls ?? []).toEqual([]);

    handle.items.set([item({ id: 'w1', title: 'Jacket', archivedAt: '2026-02-01T00:00:00.000Z' })]);
    await settle(fixture);
    expect(text(fixture)).toContain('Archived');
    const restore = byText(fixture, 'Restore');
    expect(restore.title).toBe('Restore this garment to the wardrobe pickers');
    restore.click();
    await settle(fixture);
    expect(handle.calls.archived).toEqual([
      ['w1', true],
      ['w1', false],
    ]);
  });

  it('surfaces an archive failure in the action-error banner', async () => {
    const handle = mockMutator([item({ id: 'w1', title: 'Jacket' })]);
    handle.results.update = { ok: false, error: 'Project wardrobe item not found' };
    const fixture = await render(handle);
    byText(fixture, 'Archive').click();
    await settle(fixture);
    expect(text(fixture)).toContain('Project wardrobe item not found');
  });

  it('“Show archived” flips the mutator’s fetch, never a client-side filter', async () => {
    const handle = mockMutator([item({ id: 'w1', title: 'Jacket' })]);
    const fixture = await render(handle);
    const box = (fixture.nativeElement as HTMLElement).querySelector(
      'input[type="checkbox"].qt-checkbox',
    ) as HTMLInputElement;
    expect(box.checked).toBe(false);
    box.checked = true;
    box.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(handle.calls.showArchived).toEqual([true]);
    // The list itself is untouched — the mutator re-fetches, nothing filters here.
    expect(handle.items()).toHaveLength(1);
  });

  it('deletes only after confirm; surfaces failures', async () => {
    const handle = mockMutator([item({ id: 'w1', title: 'Jacket' })]);
    handle.results.delete = { ok: false, error: 'Item is bundled by a composite' };
    const fixture = await render(handle);

    vi.spyOn(window, 'confirm').mockReturnValueOnce(false);
    byText(fixture, 'Delete').click();
    await settle(fixture);
    expect(handle.calls.delete).toEqual([]);

    vi.spyOn(window, 'confirm').mockReturnValue(true);
    byText(fixture, 'Delete').click();
    await settle(fixture);
    expect(handle.calls.delete).toEqual(['w1']);
    expect(text(fixture)).toContain('Item is bundled by a composite');
  });
});
