import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { coreStreamStub } from '../core/core-client.testing';
import type { SubpromptRecord } from '../core/core-contract';
import { ToastService } from '../ui/toast.service';
import { SubpromptsSection } from './subprompts-section';

/**
 * The Aurora Subprompts section — v4
 * `components/characters/system-prompts-editor/SubpromptsSection.tsx` at
 * `2f4254b42`.
 *
 * @module subprompts/subprompts-section.spec
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

function stub(subprompts: SubpromptRecord[], seen: Req[] = [], fail?: Error): CoreClient {
  return {
    ...coreStreamStub(),
    dispatchData: vi.fn(async (req: Req) => {
      seen.push(req);
      if (req.type === 'characterSubpromptDelete' && fail) throw fail;
      if (req.type === 'characterSubpromptList') return { subprompts };
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
  toasts: Toasts = new Toasts(),
): Promise<ComponentFixture<SubpromptsSection>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [SubpromptsSection],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: core },
      { provide: ToastService, useValue: toasts },
    ],
  });
  const fixture = TestBed.createComponent(SubpromptsSection);
  fixture.componentRef.setInput('characterId', 'char-1');
  fixture.componentRef.setInput('characterName', 'Bertie');
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

const text = (f: ComponentFixture<unknown>) => (f.nativeElement as HTMLElement).textContent ?? '';
const button = (f: ComponentFixture<unknown>, label: string) =>
  Array.from(
    (f.nativeElement as HTMLElement).querySelectorAll('button') as NodeListOf<HTMLButtonElement>,
  ).find((b) => b.textContent?.trim() === label) as HTMLButtonElement;

describe('SubpromptsSection — the header', () => {
  it('carries v4’s heading and its whole explanatory paragraph, with the character named', async () => {
    const fixture = await render(stub([]));
    expect(text(fixture)).toContain('Subprompts');
    const paragraph = (fixture.nativeElement as HTMLElement)
      .querySelector('p.qt-text-small')!
      .textContent!.replace(/\s+/g, ' ')
      .trim();
    expect(paragraph).toBe(
      'Smaller instructions Bertie may carry into a particular chat. Each lives as a Markdown file ' +
        'in the vault’s Subprompts/ folder, and is switched on or off per conversation from the New ' +
        'Chat dialog or the Participants drawer. Write them to the character in the second person, ' +
        'as you would a system prompt.',
    );
  });

  it('uses the typographic apostrophe in “the vault’s”, never an ASCII one', async () => {
    const fixture = await render(stub([]));
    const paragraph = (fixture.nativeElement as HTMLElement).querySelector(
      'p.qt-text-small',
    )!.textContent!;
    expect(paragraph).toContain('the vault’s');
    expect(paragraph).not.toContain("vault's");
  });
});

describe('SubpromptsSection — the three states', () => {
  it('an empty vault offers `Create First Subprompt`', async () => {
    const fixture = await render(stub([]));
    expect(text(fixture)).toContain(
      'No subprompts yet. Add one to have it on offer when a chat begins.',
    );
    expect(button(fixture, 'Create First Subprompt')).toBeTruthy();
  });

  it('shows `Loading subprompts...` before the list answers', async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    const core = {
      ...coreStreamStub(),
      dispatchData: vi.fn(async () => {
        await gate;
        return { subprompts: [] };
      }),
    } as unknown as CoreClient;
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [SubpromptsSection],
      providers: [
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: core },
        { provide: ToastService, useValue: new Toasts() },
      ],
    });
    const fixture = TestBed.createComponent(SubpromptsSection);
    fixture.componentRef.setInput('characterId', 'char-1');
    fixture.componentRef.setInput('characterName', 'Bertie');
    fixture.detectChanges();
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
    expect(text(fixture)).toContain('Loading subprompts...');
    release();
    await settle(fixture);
    expect(text(fixture)).not.toContain('Loading subprompts...');
  });

  it('a listed record shows its title, its path, and the content preview', async () => {
    const fixture = await render(stub([record()]));
    expect(text(fixture)).toContain('Be terse');
    const path = (fixture.nativeElement as HTMLElement).querySelector(
      'span.text-xs.qt-text-secondary',
    ) as HTMLElement;
    expect(path.textContent?.trim()).toBe('Subprompts/be-terse.md');
    expect(path.getAttribute('title')).toBe('Subprompts/be-terse.md');
    expect(text(fixture)).toContain('You keep every reply under three sentences.');
  });

  it('a long body is cut at 150 characters with an ellipsis; a short one is left alone', async () => {
    const long = 'y'.repeat(200);
    const fixture = await render(stub([record({ content: long })]));
    expect(text(fixture)).toContain(`${'y'.repeat(150)}...`);
    expect(text(fixture)).not.toContain('y'.repeat(151));

    const exact = 'z'.repeat(150);
    const short = await render(stub([record({ content: exact })]));
    // 150 is NOT "> 150" — v4's boundary, so no ellipsis here.
    expect(text(short)).toContain(exact);
    expect(text(short)).not.toContain(`${exact}...`);
  });
});

describe('SubpromptsSection — the inline delete confirm', () => {
  const trash = (f: ComponentFixture<unknown>, title: string) =>
    (f.nativeElement as HTMLElement).querySelector(
      `button[aria-label="Delete subprompt ${title}"]`,
    ) as HTMLButtonElement;

  it('the row buttons carry v4’s ARIA labels', async () => {
    const fixture = await render(stub([record()]));
    expect(
      (fixture.nativeElement as HTMLElement).querySelector(
        'button[aria-label="Edit subprompt Be terse"]',
      ),
    ).toBeTruthy();
    expect(trash(fixture, 'Be terse')).toBeTruthy();
  });

  it('the trash TOGGLES the popover, and a second row’s trash moves it', async () => {
    const fixture = await render(
      stub([record(), record({ id: 'verse', title: 'Verse', path: 'Subprompts/verse.md' })]),
    );
    expect(text(fixture)).not.toContain('Delete this subprompt?');

    trash(fixture, 'Be terse').click();
    await settle(fixture);
    expect(text(fixture)).toContain('Delete this subprompt? Any chat with it in play drops it.');

    trash(fixture, 'Be terse').click();
    await settle(fixture);
    expect(text(fixture)).not.toContain('Delete this subprompt?');

    trash(fixture, 'Be terse').click();
    await settle(fixture);
    trash(fixture, 'Verse').click();
    await settle(fixture);
    // Exactly one popover at a time — the state is a single id, not a set.
    expect((fixture.nativeElement as HTMLElement).querySelectorAll('.qt-shadow-lg').length).toBe(1);

    button(fixture, 'Cancel').click();
    await settle(fixture);
    expect(text(fixture)).not.toContain('Delete this subprompt?');
  });

  it('confirming deletes, toasts, closes the popover, and re-reads the list', async () => {
    const seen: Req[] = [];
    const toasts = new Toasts();
    const fixture = await render(stub([record()], seen), toasts);
    trash(fixture, 'Be terse').click();
    await settle(fixture);
    button(fixture, 'Delete').click();
    await settle(fixture);

    expect(seen.find((r) => r.type === 'characterSubpromptDelete')).toEqual({
      type: 'characterSubpromptDelete',
      characterId: 'char-1',
      subpromptId: 'be-terse',
    });
    expect(toasts.success).toEqual(['Subprompt deleted']);
    expect(text(fixture)).not.toContain('Delete this subprompt?');
    expect(seen.filter((r) => r.type === 'characterSubpromptList').length).toBe(2);
  });

  it('a FAILED delete reports the server’s sentence and still closes the popover', async () => {
    const toasts = new Toasts();
    const fixture = await render(
      stub([record()], [], new Error('Character is archived; subprompts cannot be deleted')),
      toasts,
    );
    trash(fixture, 'Be terse').click();
    await settle(fixture);
    button(fixture, 'Delete').click();
    await settle(fixture);
    expect(toasts.errors).toEqual(['Character is archived; subprompts cannot be deleted']);
    expect(toasts.success).toEqual([]);
    // v4's `finally` — the confirm clears either way.
    expect(text(fixture)).not.toContain('Delete this subprompt?');
  });
});

describe('SubpromptsSection — the shared editor', () => {
  it('`+ Add Subprompt` opens a CREATE dialog', async () => {
    const fixture = await render(stub([]));
    expect((fixture.nativeElement as HTMLElement).querySelector('#subprompt-title')).toBeNull();
    button(fixture, '+ Add Subprompt').click();
    await settle(fixture);
    expect(text(fixture)).toContain('New Subprompt for Bertie');
  });

  it('the pencil opens an EDIT dialog seeded from that record', async () => {
    const fixture = await render(stub([record()]));
    (
      (fixture.nativeElement as HTMLElement).querySelector(
        'button[aria-label="Edit subprompt Be terse"]',
      ) as HTMLButtonElement
    ).click();
    await settle(fixture);
    expect(text(fixture)).toContain('Edit Subprompt — Bertie');
    expect(
      ((fixture.nativeElement as HTMLElement).querySelector('#subprompt-title') as HTMLInputElement)
        .value,
    ).toBe('Be terse');
  });

  it('closing clears BOTH the create flag and the edited record (v4’s one onClose)', async () => {
    const fixture = await render(stub([record()]));
    button(fixture, '+ Add Subprompt').click();
    await settle(fixture);
    (fixture.componentInstance as unknown as { closeEditor(): void }).closeEditor();
    await settle(fixture);
    expect((fixture.nativeElement as HTMLElement).querySelector('#subprompt-title')).toBeNull();
  });
});
