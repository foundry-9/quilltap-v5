import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { ToastService } from '../../../ui/toast.service';
import { TagChipEditor } from './tag-chip-editor';

function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

function stubClient(tagCreate: () => unknown): Partial<CoreClient> {
  return {
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      if (req.type === 'tagList') {
        return { tags: [] };
      }
      if (req.type === 'tagCreate') {
        const out = tagCreate();
        if (out instanceof Error) throw out;
        return out as Record<string, unknown>;
      }
      return {};
    }) as CoreClient['dispatchData'],
  };
}

async function render(client: Partial<CoreClient>): Promise<ComponentFixture<TagChipEditor>> {
  TestBed.configureTestingModule({
    imports: [TagChipEditor],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(TagChipEditor);
  fixture.componentRef.setInput('tagIds', []);
  fixture.detectChanges();
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

function typeAndEnter(fixture: ComponentFixture<TagChipEditor>, name: string): void {
  (fixture.nativeElement.querySelector('button') as HTMLButtonElement) // "+ Add Tag"
    .click();
  fixture.detectChanges();
  const input = fixture.nativeElement.querySelector('input') as HTMLInputElement;
  input.value = name;
  input.dispatchEvent(new Event('input'));
  fixture.detectChanges();
  input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
}

/**
 * v4 `tag-editor.tsx:102-171` (P4.29 unit 12): the add-tag toast is ported
 * onto v5's one real network call (creating a brand-new tag name); the
 * remove-tag toast has no v5 analogue since add/remove here stage locally
 * and persist through the parent's already-toasted characterUpdate Save.
 */
describe('TagChipEditor create-tag toast', () => {
  it('emits the new tag id and adds no toast on success', async () => {
    const fixture = await render(stubClient(() => ({ tag: { id: 't1', name: 'brand-new' } })));
    let emitted: string[] | undefined;
    fixture.componentInstance.tagIdsChange.subscribe((ids) => (emitted = ids));

    typeAndEnter(fixture, 'brand-new');
    await fixture.whenStable();
    fixture.detectChanges();

    expect(emitted).toEqual(['t1']);
    expect(toasts()).toEqual([]);
  });

  it('toasts "Failed to add tag. Please try again." on a create failure', async () => {
    const fixture = await render(stubClient(() => new Error('the ledger is full')));
    typeAndEnter(fixture, 'brand-new');
    await fixture.whenStable();
    fixture.detectChanges();

    expect(toasts()).toEqual([{ type: 'error', message: 'Failed to add tag. Please try again.' }]);
  });
});
