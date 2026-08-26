import { ComponentFixture, TestBed } from '@angular/core/testing';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { ToastService } from '../ui/toast.service';
import { PROMPT_FIELD_HINTS } from '../ui/prompt-field-hints';
import type { WardrobeContainer } from './wardrobe-container';
import { WardrobeInstructionsSection } from './wardrobe-instructions-section';
import { instructionsGetRequest, instructionsSetRequest } from './wardrobe-instructions.api';

/**
 * P4.D121 unit 2 — the Dressing Instructions section (v4 `b86bb1a5`
 * `WardrobeInstructionsSection.tsx` + `use-wardrobe-instructions.ts`).
 *
 * The state table v4 spells out and this pins: collapsed by default; the status
 * ternary; the trimmed-draft-vs-untrimmed-stored dirty rule; the UNTRIMMED send
 * for a non-blank draft and `null` for a blank one; the echo adoption; the two
 * success sentences and the failure one; the null container rendering nothing.
 */

interface Dispatched {
  type: string;
  [k: string]: unknown;
}

const CHARACTER: WardrobeContainer = { scope: 'character', id: 'char-1' };

function setup(opts: {
  get?: () => Promise<Record<string, unknown>>;
  set?: (req: Dispatched) => Promise<Record<string, unknown>>;
}): {
  calls: Dispatched[];
  success: string[];
  errors: string[];
} {
  const calls: Dispatched[] = [];
  const success: string[] = [];
  const errors: string[] = [];
  const core = {
    dispatchData: async (req: Dispatched) => {
      calls.push(req);
      if (req.type.endsWith('InstructionsSet')) {
        return opts.set ? opts.set(req) : { instructions: null };
      }
      return opts.get ? opts.get() : { instructions: null };
    },
  };
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [WardrobeInstructionsSection],
    providers: [
      { provide: CoreClient, useValue: core },
      {
        provide: ToastService,
        useValue: {
          showSuccess: (m: string) => success.push(m),
          showError: (m: string) => errors.push(m),
        },
      },
    ],
  });
  return { calls, success, errors };
}

async function render(
  container: WardrobeContainer | null,
): Promise<ComponentFixture<WardrobeInstructionsSection>> {
  const fixture = TestBed.createComponent(WardrobeInstructionsSection);
  fixture.componentRef.setInput('container', container);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 6): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await Promise.resolve();
    fixture.detectChanges();
  }
}

function host(fixture: ComponentFixture<unknown>): HTMLElement {
  return fixture.nativeElement as HTMLElement;
}

function toggle(fixture: ComponentFixture<unknown>): HTMLButtonElement {
  return host(fixture).querySelector('button[aria-expanded]') as HTMLButtonElement;
}

function saveButton(fixture: ComponentFixture<unknown>): HTMLButtonElement | null {
  return (
    (Array.from(host(fixture).querySelectorAll('button')).find((b) =>
      /Save Instructions|Saving…/.test(b.textContent ?? ''),
    ) as HTMLButtonElement | undefined) ?? null
  );
}

/** Drive the draft the way the editor does, without mounting ProseMirror. */
function setDraft(fixture: ComponentFixture<WardrobeInstructionsSection>, value: string): void {
  (fixture.componentInstance as unknown as { draft: { set(v: string): void } }).draft.set(value);
  fixture.detectChanges();
}

describe('WardrobeInstructionsSection (v4 b86bb1a5)', () => {
  beforeEach(() => {
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
  });

  it('renders nothing for a null container (v4 `if (!container) return null`)', async () => {
    const { calls } = setup({});
    const fixture = await render(null);
    expect(host(fixture).textContent?.trim()).toBe('');
    expect(calls).toEqual([]);
  });

  it('opens collapsed, notes "None on file", and expands to the editor', async () => {
    setup({});
    const fixture = await render(CHARACTER);
    expect(toggle(fixture).getAttribute('aria-expanded')).toBe('false');
    expect(host(fixture).textContent).toContain('Dressing Instructions');
    expect(host(fixture).textContent).toContain('None on file');
    expect(saveButton(fixture)).toBeNull();

    toggle(fixture).click();
    fixture.detectChanges();
    expect(toggle(fixture).getAttribute('aria-expanded')).toBe('true');
    expect(saveButton(fixture)).not.toBeNull();
  });

  it('reads the container-correct verb and reports "On file" with the fetched text', async () => {
    const { calls } = setup({ get: async () => ({ instructions: 'You favour tweed.' }) });
    const fixture = await render(CHARACTER);
    expect(calls).toEqual([instructionsGetRequest(CHARACTER)]);
    expect(host(fixture).textContent).toContain('On file');
    toggle(fixture).click();
    fixture.detectChanges();
    // The draft was seeded from the load, so nothing is dirty yet.
    expect(saveButton(fixture)!.disabled).toBe(true);
  });

  it('a failed read warns and reads as "None on file" (v4 `catch` → null, fetched)', async () => {
    setup({
      get: async () => {
        throw new Error('boom');
      },
    });
    const fixture = await render(CHARACTER);
    expect(host(fixture).textContent).toContain('None on file');
    expect(console.warn).toHaveBeenCalled();
  });

  it('re-reads on a container switch and re-seeds the field', async () => {
    const seen: string[] = [];
    const { calls } = setup({
      get: async () => {
        seen.push('read');
        return { instructions: seen.length === 1 ? 'first' : 'second' };
      },
    });
    const fixture = await render(CHARACTER);
    const group: WardrobeContainer = { scope: 'group', id: 'g-9' };
    fixture.componentRef.setInput('container', group);
    fixture.detectChanges();
    await settle(fixture);
    expect(calls).toEqual([instructionsGetRequest(CHARACTER), instructionsGetRequest(group)]);
    expect(
      (fixture.componentInstance as unknown as { draft: () => string }).draft(),
    ).toBe('second');
  });

  it('the dirty rule compares TRIMMED draft against UNTRIMMED stored', async () => {
    setup({ get: async () => ({ instructions: 'tweed' }) });
    const fixture = await render(CHARACTER);
    toggle(fixture).click();
    fixture.detectChanges();

    setDraft(fixture, '  tweed  ');
    expect(saveButton(fixture)!.disabled).toBe(true);

    setDraft(fixture, 'tweed and brass');
    expect(saveButton(fixture)!.disabled).toBe(false);
  });

  it('saves the UNTRIMMED draft, adopts the trimmed echo, and toasts "saved"', async () => {
    const { calls, success } = setup({
      get: async () => ({ instructions: null }),
      set: async (req) => ({ instructions: String(req['instructions']).trim() }),
    });
    const fixture = await render(CHARACTER);
    toggle(fixture).click();
    fixture.detectChanges();
    setDraft(fixture, '  You favour tweed.  ');
    saveButton(fixture)!.click();
    await settle(fixture);

    expect(calls[1]).toEqual(instructionsSetRequest(CHARACTER, '  You favour tweed.  '));
    expect(success).toEqual(['Dressing instructions saved']);
    // The echo replaced the draft, so the field is clean again.
    expect((fixture.componentInstance as unknown as { draft: () => string }).draft()).toBe(
      'You favour tweed.',
    );
    expect(saveButton(fixture)!.disabled).toBe(true);
    expect(host(fixture).textContent).toContain('On file');
  });

  it('a blank draft sends null, toasts "cleared", and the note goes back', async () => {
    const { calls, success } = setup({
      get: async () => ({ instructions: 'tweed' }),
      set: async () => ({ instructions: null }),
    });
    const fixture = await render(CHARACTER);
    toggle(fixture).click();
    fixture.detectChanges();
    setDraft(fixture, '   ');
    saveButton(fixture)!.click();
    await settle(fixture);

    expect(calls[1]).toEqual(instructionsSetRequest(CHARACTER, null));
    expect(success).toEqual(['Dressing instructions cleared']);
    expect(host(fixture).textContent).toContain('None on file');
  });

  it('a failed save toasts the failure sentence and keeps the draft', async () => {
    const { errors } = setup({
      get: async () => ({ instructions: null }),
      set: async () => {
        throw new Error('nope');
      },
    });
    const fixture = await render(CHARACTER);
    toggle(fixture).click();
    fixture.detectChanges();
    setDraft(fixture, 'tweed');
    saveButton(fixture)!.click();
    await settle(fixture);

    expect(errors).toEqual(['Failed to save dressing instructions']);
    expect((fixture.componentInstance as unknown as { draft: () => string }).draft()).toBe('tweed');
  });

  it('renders the shared field hint, marked optional', async () => {
    setup({});
    const fixture = await render(CHARACTER);
    toggle(fixture).click();
    fixture.detectChanges();
    const label = host(fixture).querySelector('qt-prompt-field-label') as HTMLElement;
    expect(label.textContent).toContain('Dressing Instructions (Optional)');
    expect(label.textContent).toContain(PROMPT_FIELD_HINTS.wardrobeInstructions.helper);
    expect(label.textContent).toContain(PROMPT_FIELD_HINTS.wardrobeInstructions.example);
  });

  it('the chevron carries -rotate-90 only while collapsed (v4 :79-82)', async () => {
    setup({});
    const fixture = await render(CHARACTER);
    const glyph = () => host(fixture).querySelector('qt-icon span') as HTMLElement;
    expect(glyph().className).toContain('-rotate-90');
    toggle(fixture).click();
    fixture.detectChanges();
    expect(glyph().className).not.toContain('-rotate-90');
  });
});

describe('the instructions verb router (Shared contract A1)', () => {
  it('routes all four containers, both directions', () => {
    expect(instructionsGetRequest({ scope: 'character', id: 'c1' })).toEqual({
      type: 'characterWardrobeInstructionsGet',
      characterId: 'c1',
    });
    expect(instructionsGetRequest({ scope: 'group', id: 'g1' })).toEqual({
      type: 'groupWardrobeInstructionsGet',
      groupId: 'g1',
    });
    expect(instructionsGetRequest({ scope: 'project', id: 'p1' })).toEqual({
      type: 'projectWardrobeInstructionsGet',
      projectId: 'p1',
    });
    expect(instructionsGetRequest({ scope: 'general', id: null })).toEqual({
      type: 'wardrobeInstructionsGet',
    });

    expect(instructionsSetRequest({ scope: 'character', id: 'c1' }, 'x')).toEqual({
      type: 'characterWardrobeInstructionsSet',
      characterId: 'c1',
      instructions: 'x',
    });
    expect(instructionsSetRequest({ scope: 'group', id: 'g1' }, null)).toEqual({
      type: 'groupWardrobeInstructionsSet',
      groupId: 'g1',
      instructions: null,
    });
    expect(instructionsSetRequest({ scope: 'project', id: 'p1' }, null)).toEqual({
      type: 'projectWardrobeInstructionsSet',
      projectId: 'p1',
      instructions: null,
    });
    // `instructions` is REQUIRED on every SET — never omitted, even when null.
    expect(instructionsSetRequest({ scope: 'general', id: null }, null)).toEqual({
      type: 'wardrobeInstructionsSet',
      instructions: null,
    });
  });

  it('refuses a scoped container with no id rather than addressing the wrong tier', () => {
    expect(() => instructionsGetRequest({ scope: 'project', id: null })).toThrow(/has no id/);
  });
});
