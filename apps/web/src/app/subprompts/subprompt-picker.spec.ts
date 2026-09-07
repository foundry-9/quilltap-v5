import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { coreStreamStub } from '../core/core-client.testing';
import type { SubpromptRecord } from '../core/core-contract';
import { SubpromptPicker } from './subprompt-picker';

/**
 * The subprompt picker — v4 `components/subprompts/SubpromptPicker.tsx` at
 * `2f4254b42`, assertion by assertion against its rendered form.
 *
 * @module subprompts/subprompt-picker.spec
 */

type Req = { type: string; [k: string]: unknown };

function record(over: Partial<SubpromptRecord> = {}): SubpromptRecord {
  return {
    id: 'be-terse',
    path: 'Subprompts/be-terse.md',
    title: 'Be terse',
    content: 'You keep every reply under three sentences.',
    updatedAt: '2026-09-07T12:00:00.000Z',
    ...over,
  };
}

function stub(
  subprompts: SubpromptRecord[],
  seen: Req[] = [],
  created?: SubpromptRecord,
): CoreClient {
  return {
    ...coreStreamStub(),
    dispatchData: vi.fn(async (req: Req) => {
      seen.push(req);
      if (req.type === 'characterSubpromptList') return { subprompts };
      if (req.type === 'characterSubpromptCreate') return { subprompt: created ?? record() };
      return {};
    }),
  } as unknown as CoreClient;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(
  core: CoreClient,
  inputs: Record<string, unknown> = {},
): Promise<ComponentFixture<SubpromptPicker>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [SubpromptPicker],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: core }],
  });
  const fixture = TestBed.createComponent(SubpromptPicker);
  fixture.componentRef.setInput('characterId', 'char-1');
  for (const [k, v] of Object.entries(inputs)) fixture.componentRef.setInput(k, v);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

const text = (f: ComponentFixture<unknown>) => (f.nativeElement as HTMLElement).textContent ?? '';
const disclosure = (f: ComponentFixture<unknown>) =>
  (f.nativeElement as HTMLElement).querySelector('button') as HTMLButtonElement;

describe('SubpromptPicker — the disclosure and its summary', () => {
  it('an empty vault reads `None on file` (v4 :65-70, first arm)', async () => {
    const fixture = await render(stub([]), { defaultOpen: true });
    expect(text(fixture)).toContain('Subprompts · None on file');
  });

  it('a loaded vault counts what is in play (v4 :70, third arm)', async () => {
    const fixture = await render(stub([record(), record({ id: 'verse', title: 'Verse' })]), {
      defaultOpen: true,
      selectedIds: ['verse'],
    });
    expect(text(fixture)).toContain('Subprompts · 1 of 2 in play');
  });

  it('reads `Loading…` only while loading AND empty (v4 :68, second arm)', async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    const core = {
      ...coreStreamStub(),
      dispatchData: vi.fn(async () => {
        await gate;
        return { subprompts: [record()] };
      }),
    } as unknown as CoreClient;
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [SubpromptPicker],
      providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: core }],
    });
    const fixture = TestBed.createComponent(SubpromptPicker);
    fixture.componentRef.setInput('characterId', 'char-1');
    fixture.componentRef.setInput('defaultOpen', true);
    fixture.detectChanges();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(text(fixture)).toContain('Subprompts · Loading…');
    release();
    await settle(fixture);
    expect(text(fixture)).toContain('Subprompts · 0 of 1 in play');
  });

  it('carries v4’s ARIA and title on the disclosure, and flips aria-expanded', async () => {
    const fixture = await render(stub([record()]));
    const button = disclosure(fixture);
    expect(button.getAttribute('title')).toBe('Subprompts in play for this chat');
    expect(button.getAttribute('aria-expanded')).toBe('false');
    expect(button.getAttribute('aria-controls')).toBeTruthy();
    button.click();
    await settle(fixture);
    expect(disclosure(fixture).getAttribute('aria-expanded')).toBe('true');
  });

  it('the two size variants carry v4’s exact class strings', async () => {
    // Pinned at the SOURCE, not on the element: Angular's `[class]` binding
    // sorts and dedups the DOM token list, so a transcribed `className`
    // assertion would compare against a re-ordered string and prove nothing
    // about the bytes this component emits (`angular-class-binding-dedups-and-
    // reorders`). The DOM check below is the wiring half.
    const md = await render(stub([]));
    const mdClass = (md.componentInstance as unknown as { buttonClass(): string }).buttonClass();
    expect(mdClass).toBe(
      'w-full flex items-center justify-between gap-2 rounded-lg border qt-border-default bg-background px-3 py-1.5 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring text-left',
    );
    for (const token of mdClass.split(' ')) {
      expect(disclosure(md).classList.contains(token)).toBe(true);
    }

    const sm = await render(stub([]), { size: 'sm' });
    const smClass = (sm.componentInstance as unknown as { buttonClass(): string }).buttonClass();
    expect(smClass).toBe(
      'qt-select qt-select-sm w-full flex items-center justify-between gap-2 text-left',
    );
    for (const token of smClass.split(' ')) {
      expect(disclosure(sm).classList.contains(token)).toBe(true);
    }
  });

  it('the open panel and the row carry v4’s class strings (source-pinned)', async () => {
    const md = await render(stub([]), { defaultOpen: true });
    const inst = md.componentInstance as unknown as { panelClass(): string; rowClass(): string };
    expect(inst.panelClass()).toBe(
      'mt-1 rounded-lg border qt-border-default qt-bg-card p-2 space-y-1',
    );
    expect(inst.rowClass()).toBe('flex items-start gap-2 px-1 py-0.5 rounded cursor-pointer ');

    const sm = await render(stub([]), { defaultOpen: true, size: 'sm' });
    expect((sm.componentInstance as unknown as { panelClass(): string }).panelClass()).toBe(
      'mt-1 rounded-lg border qt-border-default qt-bg-card p-1.5 space-y-1',
    );

    const off = await render(stub([]), { defaultOpen: true, disabled: true });
    expect((off.componentInstance as unknown as { rowClass(): string }).rowClass()).toBe(
      'flex items-start gap-2 px-1 py-0.5 rounded cursor-pointer opacity-60 cursor-not-allowed',
    );
  });
});

describe('SubpromptPicker — the deferred read (v4 `enabled: open || selectedIds.length > 0`)', () => {
  it('a collapsed picker with nothing ticked reads nothing until it is opened', async () => {
    const seen: Req[] = [];
    const fixture = await render(stub([record()], seen));
    expect(seen.filter((r) => r.type === 'characterSubpromptList')).toEqual([]);
    disclosure(fixture).click();
    await settle(fixture);
    expect(seen.filter((r) => r.type === 'characterSubpromptList').length).toBe(1);
  });

  it('a collapsed picker that ALREADY carries a selection reads immediately', async () => {
    const seen: Req[] = [];
    await render(stub([record()], seen), { selectedIds: ['be-terse'] });
    expect(seen.filter((r) => r.type === 'characterSubpromptList').length).toBe(1);
  });
});

describe('SubpromptPicker — the list', () => {
  it('an empty vault names the character (v4 :99-103)', async () => {
    const fixture = await render(stub([]), { defaultOpen: true, characterName: 'Bertie' });
    expect(text(fixture)).toContain('Bertie has no subprompts yet.');
  });

  it('…and falls back to `This character` when no name is passed', async () => {
    const fixture = await render(stub([]), { defaultOpen: true });
    expect(text(fixture)).toContain('This character has no subprompts yet.');
  });

  it('each row is a labelled checkbox titled with the first 200 characters', async () => {
    const long = 'x'.repeat(300);
    const fixture = await render(stub([record({ content: long })]), { defaultOpen: true });
    const label = fixture.nativeElement.querySelector('label') as HTMLLabelElement;
    expect(label.getAttribute('title')).toBe('x'.repeat(200));
    const box = label.querySelector('input') as HTMLInputElement;
    expect(box.getAttribute('aria-label')).toBe('Subprompt Be terse');
    expect(box.checked).toBe(false);
  });

  it('ticking one emits the FULL next set, appended (v4 `onChange(nextIds)`)', async () => {
    const fixture = await render(stub([record(), record({ id: 'verse', title: 'Verse' })]), {
      defaultOpen: true,
      selectedIds: ['verse'],
    });
    const seen: string[][] = [];
    fixture.componentInstance.selectionChange.subscribe((ids) => seen.push(ids));
    const boxes = fixture.nativeElement.querySelectorAll(
      'input[type=checkbox]',
    ) as NodeListOf<HTMLInputElement>;
    boxes[0].click();
    await settle(fixture);
    expect(seen).toEqual([['verse', 'be-terse']]);
  });

  it('unticking filters it out', async () => {
    const fixture = await render(stub([record()]), {
      defaultOpen: true,
      selectedIds: ['be-terse', 'verse'],
    });
    const seen: string[][] = [];
    fixture.componentInstance.selectionChange.subscribe((ids) => seen.push(ids));
    const box = fixture.nativeElement.querySelector(
      'input[aria-label="Subprompt Be terse"]',
    ) as HTMLInputElement;
    box.click();
    await settle(fixture);
    expect(seen).toEqual([['verse']]);
  });

  it('a toggle to the state it is already in is a NO-OP (v4 :54)', async () => {
    const fixture = await render(stub([record()]), {
      defaultOpen: true,
      selectedIds: ['be-terse'],
    });
    const seen: string[][] = [];
    fixture.componentInstance.selectionChange.subscribe((ids) => seen.push(ids));
    // Drive the handler directly: the DOM cannot express "changed to the value
    // it already had", and that is precisely the branch v4 guards.
    (fixture.componentInstance as unknown as { toggle(id: string, on: boolean): void }).toggle(
      'be-terse',
      true,
    );
    expect(seen).toEqual([]);
  });
});

describe('SubpromptPicker — ids that no longer match a file (v4 :121-137)', () => {
  it('shows a checked, struck-through row that can be unticked', async () => {
    const fixture = await render(stub([record()]), {
      defaultOpen: true,
      selectedIds: ['be-terse', 'ghost'],
    });
    const missing = fixture.nativeElement.querySelector(
      'input[aria-label="Missing subprompt ghost"]',
    ) as HTMLInputElement;
    expect(missing).toBeTruthy();
    expect(missing.checked).toBe(true);
    const label = missing.closest('label') as HTMLLabelElement;
    expect(label.getAttribute('title')).toBe('This subprompt no longer exists in the vault');
    expect(label.querySelector('span')?.className).toContain('line-through');

    const seen: string[][] = [];
    fixture.componentInstance.selectionChange.subscribe((ids) => seen.push(ids));
    missing.click();
    await settle(fixture);
    expect(seen).toEqual([['be-terse']]);
  });

  it('is NOT shown while the list is still loading — an unloaded id is not a dead one', async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    const core = {
      ...coreStreamStub(),
      dispatchData: vi.fn(async () => {
        await gate;
        return { subprompts: [record()] };
      }),
    } as unknown as CoreClient;
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [SubpromptPicker],
      providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: core }],
    });
    const fixture = TestBed.createComponent(SubpromptPicker);
    fixture.componentRef.setInput('characterId', 'char-1');
    fixture.componentRef.setInput('defaultOpen', true);
    fixture.componentRef.setInput('selectedIds', ['be-terse']);
    fixture.detectChanges();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(
      fixture.nativeElement.querySelector('input[aria-label="Missing subprompt be-terse"]'),
    ).toBeNull();
    release();
    await settle(fixture);
    // …and once loaded it is a KNOWN id, so no missing row ever appears.
    expect(
      fixture.nativeElement.querySelector('input[aria-label="Missing subprompt be-terse"]'),
    ).toBeNull();
  });
});

describe('SubpromptPicker — the in-place editor', () => {
  it('`New subprompt…` opens the editor, and a CREATED record is ticked on (v4 :59-63)', async () => {
    const created = record({ id: 'no-spoilers', title: 'No spoilers' });
    const fixture = await render(stub([], [], created), { defaultOpen: true });
    const seen: string[][] = [];
    fixture.componentInstance.selectionChange.subscribe((ids) => seen.push(ids));

    const newButton = Array.from(
      fixture.nativeElement.querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
    ).find((b) => b.textContent?.trim() === 'New subprompt…') as HTMLButtonElement;
    newButton.click();
    await settle(fixture);
    expect(text(fixture)).toContain('New Subprompt');

    (
      fixture.componentInstance as unknown as {
        onSaved(e: { subprompt: typeof created; mode: 'created' | 'updated' }): void;
      }
    ).onSaved({ subprompt: created, mode: 'created' });
    expect(seen).toEqual([['no-spoilers']]);
  });

  it('an UPDATED record is never auto-ticked (v4 guards on `mode === "created"`)', async () => {
    const fixture = await render(stub([record()]), { defaultOpen: true });
    const seen: string[][] = [];
    fixture.componentInstance.selectionChange.subscribe((ids) => seen.push(ids));
    (
      fixture.componentInstance as unknown as {
        onSaved(e: { subprompt: SubpromptRecord; mode: 'created' | 'updated' }): void;
      }
    ).onSaved({ subprompt: record({ id: 'other' }), mode: 'updated' });
    expect(seen).toEqual([]);
  });

  it('a created record ALREADY ticked on is not appended twice', async () => {
    const fixture = await render(stub([record()]), {
      defaultOpen: true,
      selectedIds: ['be-terse'],
    });
    const seen: string[][] = [];
    fixture.componentInstance.selectionChange.subscribe((ids) => seen.push(ids));
    (
      fixture.componentInstance as unknown as {
        onSaved(e: { subprompt: SubpromptRecord; mode: 'created' | 'updated' }): void;
      }
    ).onSaved({ subprompt: record(), mode: 'created' });
    expect(seen).toEqual([]);
  });
});
