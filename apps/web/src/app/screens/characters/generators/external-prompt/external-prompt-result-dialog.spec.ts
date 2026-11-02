import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { ExternalPromptResultDialog } from './external-prompt-result-dialog';

/**
 * The Generated Prompt dialog's WCAG semantics (P4.D358, v4 `dff76ebef`): the
 * card is a modal `role="dialog"` named by its `<h3>`.
 */

function mount(characterName: string | null): ComponentFixture<ExternalPromptResultDialog> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [ExternalPromptResultDialog] });
  const fixture = TestBed.createComponent(ExternalPromptResultDialog);
  fixture.componentRef.setInput('characterName', characterName);
  fixture.componentRef.setInput('prompt', 'You are Lorian, a navigator.');
  fixture.detectChanges();
  return fixture;
}

describe('ExternalPromptResultDialog — WCAG semantics (P4.D358)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('announces as a dialog named by its title', () => {
    const fixture = mount('Lorian');
    const host = fixture.nativeElement as HTMLElement;
    const dlg = host.querySelector('[role="dialog"]') as HTMLElement;
    expect(dlg.classList.contains('qt-bg-card')).toBe(true);
    expect(dlg.getAttribute('aria-modal')).toBe('true');
    expect(dlg.getAttribute('tabindex')).toBe('-1');
    const id = dlg.getAttribute('aria-labelledby')!;
    expect(id).toMatch(/^qt-dialog-\d+$/);
    expect((host.querySelector('#' + id) as HTMLElement).textContent?.trim()).toBe(
      'Generated Prompt for Lorian',
    );
  });
});
