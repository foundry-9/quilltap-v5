import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { coreStreamStub } from '../core/core-client.testing';
import type { SubpromptRecord } from '../core/core-contract';
import { RichEditor } from '../editor/rich-editor';
import { ToastService } from '../ui/toast.service';
import { SubpromptEditorModal } from './subprompt-editor-modal';

/**
 * The subprompt editor dialog — v4
 * `components/subprompts/SubpromptEditorModal.tsx` at `2f4254b42`.
 *
 * @module subprompts/subprompt-editor-modal.spec
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

class Toasts {
  readonly success: string[] = [];
  readonly errors: string[] = [];
  showSuccess(m: string): string {
    this.success.push(m);
    return 't';
  }
  showError(m: string): string {
    this.errors.push(m);
    return 't';
  }
}

function stub(seen: Req[] = [], fail?: Error): CoreClient {
  return {
    ...coreStreamStub(),
    dispatchData: vi.fn(async (req: Req) => {
      seen.push(req);
      if (fail) throw fail;
      if (req.type === 'characterSubpromptCreate') {
        return { subprompt: record({ id: 'no-spoilers', title: String(req['title']) }) };
      }
      if (req.type === 'characterSubpromptUpdate') {
        return { subprompt: record({ title: String(req['title']) }) };
      }
      return { subprompts: [] };
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
  toasts: Toasts,
  inputs: Record<string, unknown> = {},
): Promise<ComponentFixture<SubpromptEditorModal>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [SubpromptEditorModal],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: core },
      { provide: ToastService, useValue: toasts },
    ],
  });
  const fixture = TestBed.createComponent(SubpromptEditorModal);
  fixture.componentRef.setInput('open', true);
  fixture.componentRef.setInput('characterId', 'char-1');
  for (const [k, v] of Object.entries(inputs)) fixture.componentRef.setInput(k, v);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

const text = (f: ComponentFixture<unknown>) => (f.nativeElement as HTMLElement).textContent ?? '';
const titleInput = (f: ComponentFixture<unknown>) =>
  (f.nativeElement as HTMLElement).querySelector('#subprompt-title') as HTMLInputElement;
const button = (f: ComponentFixture<unknown>, label: string) =>
  Array.from(
    (f.nativeElement as HTMLElement).querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
  ).find((b) => b.textContent?.trim() === label) as HTMLButtonElement;
const editor = (f: ComponentFixture<unknown>) =>
  f.debugElement.query(By.directive(RichEditor)).componentInstance as RichEditor;

function type(f: ComponentFixture<unknown>, title: string, content: string): void {
  const input = titleInput(f);
  input.value = title;
  input.dispatchEvent(new Event('input'));
  editor(f).contentChange.emit(content);
  f.detectChanges();
}

describe('SubpromptEditorModal — the shell', () => {
  it('renders nothing when closed', async () => {
    const fixture = await render(stub(), new Toasts());
    fixture.componentRef.setInput('open', false);
    fixture.detectChanges();
    expect(titleInput(fixture)).toBeNull();
  });

  it('titles a create `New Subprompt for {name}` and an edit `Edit Subprompt — {name}`', async () => {
    const create = await render(stub(), new Toasts(), { characterName: 'Bertie' });
    expect(text(create)).toContain('New Subprompt for Bertie');
    const edit = await render(stub(), new Toasts(), {
      characterName: 'Bertie',
      editing: record(),
    });
    expect(text(edit)).toContain('Edit Subprompt — Bertie');
  });

  it('…and drops the name half entirely when none is passed', async () => {
    // Read the dialog TITLE, not the whole dialog: the shared hint's helper
    // carries "asked for more", so a page-wide `not.toContain(' for ')` would
    // fail on copy this assertion is not about.
    const heading = (f: ComponentFixture<unknown>) =>
      (
        (f.nativeElement as HTMLElement).querySelector('.qt-dialog-title')?.textContent ?? ''
      ).trim();
    const create = await render(stub(), new Toasts());
    expect(heading(create)).toBe('New Subprompt');
    const edit = await render(stub(), new Toasts(), { editing: record() });
    expect(heading(edit)).toBe('Edit Subprompt');
  });

  it('the title box carries v4’s placeholder and its 100-character cap', async () => {
    const fixture = await render(stub(), new Toasts());
    expect(titleInput(fixture).placeholder).toBe(
      'e.g., Keep it brief, Speak in verse, No spoilers',
    );
    expect(titleInput(fixture).maxLength).toBe(100);
  });

  it('an EDIT shows the file-name note; a create does not', async () => {
    const edit = await render(stub(), new Toasts(), { editing: record() });
    expect(text(edit)).toContain('Kept on file as');
    expect(text(edit)).toContain('Subprompts/be-terse.md');
    expect(text(edit)).toContain(
      'The file name stays put when the title changes, so chats that have this subprompt in play keep it.',
    );
    const create = await render(stub(), new Toasts());
    expect(text(create)).not.toContain('Kept on file as');
  });

  it('the Instruction header is the shared hint plus this surface’s suffix', async () => {
    const fixture = await render(stub(), new Toasts());
    expect(text(fixture)).toContain(
      'A smaller instruction the character may carry into a particular chat, switched on or off per conversation. Written to the character directly, in the second person, exactly as a system prompt is — it lands immediately after the system prompt and is read by nobody else. Markdown is supported, and {{char}} / {{user}} substitute the character and user names.',
    );
  });
});

describe('SubpromptEditorModal — the keyed remount (v4 `key={editing?.id ?? "new"}`)', () => {
  it('each opening mounts a FRESH form — typed text never survives a re-open', async () => {
    const fixture = await render(stub(), new Toasts());
    type(fixture, 'Half-typed', 'Body');
    expect(titleInput(fixture).value).toBe('Half-typed');

    fixture.componentRef.setInput('open', false);
    fixture.detectChanges();
    fixture.componentRef.setInput('open', true);
    fixture.detectChanges();
    await settle(fixture);
    expect(titleInput(fixture).value).toBe('');
  });

  it('switching the edited record re-seeds the form from the NEW record', async () => {
    const fixture = await render(stub(), new Toasts(), { editing: record() });
    expect(titleInput(fixture).value).toBe('Be terse');
    fixture.componentRef.setInput('editing', record({ id: 'verse', title: 'Speak in verse' }));
    fixture.detectChanges();
    await settle(fixture);
    expect(titleInput(fixture).value).toBe('Speak in verse');
  });
});

describe('SubpromptEditorModal — saving', () => {
  it('the primary button is disabled until BOTH title and content are non-blank', async () => {
    const fixture = await render(stub(), new Toasts());
    expect(button(fixture, 'Create').disabled).toBe(true);
    type(fixture, '   ', 'Body');
    expect(button(fixture, 'Create').disabled).toBe(true);
    type(fixture, 'Be terse', '   ');
    expect(button(fixture, 'Create').disabled).toBe(true);
    type(fixture, 'Be terse', 'Body');
    expect(button(fixture, 'Create').disabled).toBe(false);
  });

  it('a create sends the TRIMMED title with the content verbatim, toasts, and closes', async () => {
    const seen: Req[] = [];
    const toasts = new Toasts();
    const fixture = await render(stub(seen), toasts);
    const saved: unknown[] = [];
    const closed: unknown[] = [];
    fixture.componentInstance.saved.subscribe((e) => saved.push(e));
    fixture.componentInstance.close.subscribe(() => closed.push(1));

    type(fixture, '  Be terse  ', '  Body with edges  ');
    button(fixture, 'Create').click();
    await settle(fixture);

    expect(seen.find((r) => r.type === 'characterSubpromptCreate')).toEqual({
      type: 'characterSubpromptCreate',
      characterId: 'char-1',
      title: 'Be terse',
      content: '  Body with edges  ',
    });
    expect(toasts.success).toEqual(['Subprompt created']);
    expect(saved).toEqual([
      { subprompt: record({ id: 'no-spoilers', title: 'Be terse' }), mode: 'created' },
    ]);
    expect(closed.length).toBe(1);
  });

  it('an edit sends the id and toasts `Subprompt updated`', async () => {
    const seen: Req[] = [];
    const toasts = new Toasts();
    const fixture = await render(stub(seen), toasts, { editing: record() });
    const saved: { mode: string }[] = [];
    fixture.componentInstance.saved.subscribe((e) => saved.push(e));

    type(fixture, 'Terser still', 'Body');
    button(fixture, 'Update').click();
    await settle(fixture);

    expect(seen.find((r) => r.type === 'characterSubpromptUpdate')).toEqual({
      type: 'characterSubpromptUpdate',
      characterId: 'char-1',
      subpromptId: 'be-terse',
      title: 'Terser still',
      content: 'Body',
    });
    expect(toasts.success).toEqual(['Subprompt updated']);
    expect(saved[0].mode).toBe('updated');
  });

  it('the label reads `Saving...` and Cancel is disabled while the write is in flight', async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    const core = {
      ...coreStreamStub(),
      dispatchData: vi.fn(async (req: Req) => {
        if (req.type === 'characterSubpromptCreate') {
          await gate;
          return { subprompt: record() };
        }
        return { subprompts: [] };
      }),
    } as unknown as CoreClient;
    const fixture = await render(core, new Toasts());
    type(fixture, 'Be terse', 'Body');
    button(fixture, 'Create').click();
    fixture.detectChanges();
    expect(button(fixture, 'Saving...')).toBeTruthy();
    expect(button(fixture, 'Cancel').disabled).toBe(true);
    release();
    await settle(fixture);
  });

  it('a failed create reports the server’s own sentence, and does not close', async () => {
    const toasts = new Toasts();
    const fixture = await render(stub([], new Error('A subprompt needs a title')), toasts);
    const closed: unknown[] = [];
    fixture.componentInstance.close.subscribe(() => closed.push(1));
    type(fixture, 'Be terse', 'Body');
    button(fixture, 'Create').click();
    await settle(fixture);
    expect(toasts.errors).toEqual(['A subprompt needs a title']);
    expect(closed).toEqual([]);
  });

  it('a failure with no message falls back to v4’s two sentences, create vs edit', async () => {
    const blank = new Error('');
    const create = new Toasts();
    const f1 = await render(stub([], blank), create);
    type(f1, 'Be terse', 'Body');
    button(f1, 'Create').click();
    await settle(f1);
    expect(create.errors).toEqual(['Failed to create subprompt']);

    const edit = new Toasts();
    const f2 = await render(stub([], blank), edit, { editing: record() });
    type(f2, 'Be terse', 'Body');
    button(f2, 'Update').click();
    await settle(f2);
    expect(edit.errors).toEqual(['Failed to update subprompt']);
  });

  it('the editor never READS the list — it only ever writes (v4 `enabled: false`)', async () => {
    const seen: Req[] = [];
    const fixture = await render(stub(seen), new Toasts());
    type(fixture, 'Be terse', 'Body');
    button(fixture, 'Create').click();
    await settle(fixture);
    expect(seen.filter((r) => r.type === 'characterSubpromptList')).toEqual([]);
  });
});
