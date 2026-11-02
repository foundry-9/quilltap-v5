import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import { CharacterOptimizerModal } from './character-optimizer-modal';

/**
 * The Refine from Memories modal's WCAG semantics (P4.D358, v4 `dff76ebef`):
 * the card is a modal `role="dialog"` named by its title.
 */

function mount(): ComponentFixture<CharacterOptimizerModal> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [CharacterOptimizerModal],
    providers: [
      {
        provide: CoreClient,
        useValue: { dispatch: vi.fn(async () => ({})), dispatchData: vi.fn(async () => ({})) },
      },
    ],
  });
  const fixture = TestBed.createComponent(CharacterOptimizerModal);
  fixture.componentRef.setInput('characterId', 'c-1');
  fixture.componentRef.setInput('characterName', 'Lorian');
  fixture.componentRef.setInput('profiles', []);
  fixture.detectChanges();
  return fixture;
}

describe('CharacterOptimizerModal — WCAG semantics (P4.D358)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('announces as a dialog named "Refine from Memories"', () => {
    const fixture = mount();
    const host = fixture.nativeElement as HTMLElement;
    const dlg = host.querySelector('[role="dialog"]') as HTMLElement;
    expect(dlg.classList.contains('qt-dialog')).toBe(true);
    expect(dlg.getAttribute('aria-modal')).toBe('true');
    expect(dlg.getAttribute('tabindex')).toBe('-1');
    const id = dlg.getAttribute('aria-labelledby')!;
    expect(id).toMatch(/^qt-dialog-\d+$/);
    expect((host.querySelector('#' + id) as HTMLElement).textContent?.trim()).toBe(
      'Refine from Memories',
    );
  });
});
