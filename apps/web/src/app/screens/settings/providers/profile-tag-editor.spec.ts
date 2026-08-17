import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { ToastService } from '../../../ui/toast.service';
import { ProfileTagEditor } from './profile-tag-editor';

/**
 * Bug 74, first layer — tagging a connection profile had never worked because
 * v4's entity-agnostic TagEditor swapped in `/api/v1/profiles/<id>`, a route
 * that has never existed, so every read and write 404'd silently.
 *
 * v4's own new cases (`__tests__/unit/components/tags/tag-editor-paths.test.
 * tsx` at `d123658d`) assert the URL each entity type reaches for, "because the
 * component's only contract with the server is that string". v5 has no URL
 * layer here — the dispatch VERB is that contract — so these mirror the same
 * three operations, asserting the verb and the id that leave the editor.
 */

interface Seen {
  type: string;
  [k: string]: unknown;
}

const TAG = { id: 'tag-1', name: 'fast-and-cheap', visualStyle: null };

function stubClient(seen: Seen[], fail: (req: Seen) => boolean = () => false): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Seen) => {
      seen.push(req);
      if (fail(req)) throw new Error('nope');
      switch (req.type) {
        case 'connectionProfileGetTags':
          return { tags: [TAG] };
        case 'tagList':
          return {
            tags: [TAG, { id: 'tag-2', name: 'slow-and-clever', visualStyle: null }],
          };
        case 'tagCreate':
          return { tag: { id: 'tag-new', name: (req['name'] as string) ?? '' } };
        default:
          return { success: true };
      }
    }) as CoreClient['dispatchData'],
  };
}

async function render(
  client: Partial<CoreClient>,
  toasts: Partial<ToastService> = { showError: vi.fn() as unknown as ToastService['showError'] },
): Promise<ComponentFixture<ProfileTagEditor>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ProfileTagEditor],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: client },
      { provide: ToastService, useValue: toasts },
    ],
  });
  const fixture = TestBed.createComponent(ProfileTagEditor);
  fixture.componentRef.setInput('profileId', 'entity-1');
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

function button(fixture: ComponentFixture<unknown>, label: string): HTMLButtonElement {
  const found = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll('button')).find(
    (b) => b.textContent?.trim() === label,
  );
  if (!found) throw new Error(`no button labelled ${label}`);
  return found as HTMLButtonElement;
}

describe('ProfileTagEditor (Bug 74)', () => {
  it('reads the profile’s own tags through connectionProfileGetTags', async () => {
    const seen: Seen[] = [];
    const fixture = await render(stubClient(seen));
    const read = seen.find((r) => r.type === 'connectionProfileGetTags');
    expect(read).toEqual({ type: 'connectionProfileGetTags', profileId: 'entity-1' });
    // The bug's signature: an entity verb belonging to something else.
    expect(seen.some((r) => r.type.startsWith('character'))).toBe(false);
    expect(seen.some((r) => r.type.startsWith('chat'))).toBe(false);
  });

  it('renders the FLAT get-tags shape, not the listing’s envelope', async () => {
    // Bug 74's third layer was reading `name` off `{tagId, tag}`; the editor's
    // list is the other shape and must be read as such.
    const fixture = await render(stubClient([]));
    expect(text(fixture)).toContain('fast-and-cheap');
  });

  it('attaches a new tag through connectionProfileAddTag', async () => {
    const seen: Seen[] = [];
    const fixture = await render(stubClient(seen));
    button(fixture, '+ Add Tag').click();
    fixture.detectChanges();
    const input = (fixture.nativeElement as HTMLElement).querySelector<HTMLInputElement>('input')!;
    input.value = 'slow-and-clever';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await settle(fixture);

    // v4 `addTag`: create-or-get first, then attach.
    expect(seen.find((r) => r.type === 'tagCreate')).toEqual({
      type: 'tagCreate',
      name: 'slow-and-clever',
    });
    expect(seen.find((r) => r.type === 'connectionProfileAddTag')).toEqual({
      type: 'connectionProfileAddTag',
      profileId: 'entity-1',
      tagId: 'tag-new',
    });
  });

  it('detaches a tag through connectionProfileRemoveTag', async () => {
    const seen: Seen[] = [];
    const fixture = await render(stubClient(seen));
    const remove = (fixture.nativeElement as HTMLElement).querySelector<HTMLButtonElement>(
      '[aria-label="Remove fast-and-cheap tag"]',
    )!;
    remove.click();
    await settle(fixture);
    expect(seen.find((r) => r.type === 'connectionProfileRemoveTag')).toEqual({
      type: 'connectionProfileRemoveTag',
      profileId: 'entity-1',
      tagId: 'tag-1',
    });
  });

  it('persists IMMEDIATELY rather than staging into a Save bag', async () => {
    // v4's TagEditor writes as it goes and the profile modal has no single Save
    // bag tags could ride — the character form's staged variant is a deliberate
    // deviation THERE, not here. A remove must reach the wire with no submit.
    const seen: Seen[] = [];
    const fixture = await render(stubClient(seen));
    (
      (fixture.nativeElement as HTMLElement).querySelector(
        '[aria-label="Remove fast-and-cheap tag"]',
      ) as HTMLButtonElement
    ).click();
    await settle(fixture);
    expect(seen.some((r) => r.type === 'connectionProfileRemoveTag')).toBe(true);
    expect(seen.some((r) => r.type === 'connectionProfileUpdate')).toBe(false);
  });

  it('toasts v4’s sentence when the attach fails (v4 `tag-editor.tsx:140`)', async () => {
    const showError = vi.fn();
    const seen: Seen[] = [];
    const fixture = await render(
      stubClient(seen, (r) => r.type === 'connectionProfileAddTag'),
      { showError: showError as unknown as ToastService['showError'] },
    );
    button(fixture, '+ Add Tag').click();
    fixture.detectChanges();
    const input = (fixture.nativeElement as HTMLElement).querySelector<HTMLInputElement>('input')!;
    input.value = 'slow-and-clever';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await settle(fixture);
    expect(showError).toHaveBeenCalledWith('Failed to add tag. Please try again.');
  });

  it('toasts v4’s sentence when the create leg fails, sharing the one catch', async () => {
    const showError = vi.fn();
    const fixture = await render(
      stubClient([], (r) => r.type === 'tagCreate'),
      {
        showError: showError as unknown as ToastService['showError'],
      },
    );
    button(fixture, '+ Add Tag').click();
    fixture.detectChanges();
    const input = (fixture.nativeElement as HTMLElement).querySelector<HTMLInputElement>('input')!;
    input.value = 'brand-new';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await settle(fixture);
    expect(showError).toHaveBeenCalledWith('Failed to add tag. Please try again.');
  });

  it('toasts v4’s other sentence when the detach fails (v4 `tag-editor.tsx:167`)', async () => {
    const showError = vi.fn();
    const fixture = await render(
      stubClient([], (r) => r.type === 'connectionProfileRemoveTag'),
      { showError: showError as unknown as ToastService['showError'] },
    );
    (
      (fixture.nativeElement as HTMLElement).querySelector(
        '[aria-label="Remove fast-and-cheap tag"]',
      ) as HTMLButtonElement
    ).click();
    await settle(fixture);
    expect(showError).toHaveBeenCalledWith('Failed to remove tag. Please try again.');
  });
});
