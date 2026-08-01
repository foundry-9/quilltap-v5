import { TestBed, type ComponentFixture } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { CharacterChooseOutfitCard } from './choose-outfit-card';
import { ToastService } from '../../ui/toast.service';

function render(
  dispatchData: (req: unknown) => Promise<Record<string, unknown>>,
): { fixture: ComponentFixture<CharacterChooseOutfitCard>; el: HTMLElement } {
  const stubCore = { dispatchData } as unknown as CoreClient;
  TestBed.configureTestingModule({
    imports: [CharacterChooseOutfitCard],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: stubCore }],
  });
  const fixture = TestBed.createComponent(CharacterChooseOutfitCard);
  return { fixture, el: fixture.nativeElement as HTMLElement };
}

function checkbox(el: HTMLElement): HTMLInputElement {
  return el.querySelector('input[type="checkbox"]') as HTMLInputElement;
}

async function flush(): Promise<void> {
  await new Promise((r) => setTimeout(r, 0));
}

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('CharacterChooseOutfitCard (v4 8bf3cb5f Wardrobe-tab checkbox)', () => {
  it('reflects canChooseOutfit and carries the v4 label + helper prose', () => {
    const { fixture, el } = render(async () => ({}));
    fixture.componentRef.setInput('characterId', 'c1');
    fixture.componentRef.setInput('canChooseOutfit', true);
    fixture.detectChanges();

    expect(checkbox(el).checked).toBe(true);
    expect(el.textContent).toContain('Let this character choose their opening outfit');
    expect(el.textContent).toContain('defaults its Starting Outfit');
  });

  it('is disabled while the character is still loading (characterId null)', () => {
    const { fixture, el } = render(async () => ({}));
    fixture.componentRef.setInput('characterId', null);
    fixture.detectChanges();
    expect(checkbox(el).disabled).toBe(true);
  });

  it('PUTs { canChooseOutfit } on toggle via characterUpdate', async () => {
    const dispatchData = vi.fn(async () => ({}));
    const { fixture, el } = render(dispatchData);
    fixture.componentRef.setInput('characterId', 'c1');
    fixture.componentRef.setInput('canChooseOutfit', false);
    fixture.detectChanges();

    const cb = checkbox(el);
    cb.checked = true;
    cb.dispatchEvent(new Event('change'));
    await flush();

    expect(dispatchData).toHaveBeenCalledWith({
      type: 'characterUpdate',
      characterId: 'c1',
      character: { canChooseOutfit: true },
    });
  });

  it('surfaces a failed save as v4’s toast', async () => {
    const dispatchData = vi.fn(async () => {
      throw new Error('nope');
    });
    const { fixture, el } = render(dispatchData);
    fixture.componentRef.setInput('characterId', 'c1');
    fixture.componentRef.setInput('canChooseOutfit', false);
    fixture.detectChanges();

    checkbox(el).dispatchEvent(new Event('change'));
    await flush();
    fixture.detectChanges();

    expect(toasts().at(-1)).toEqual({ type: 'error', message: 'nope' });
  });
});
