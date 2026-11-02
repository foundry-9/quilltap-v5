import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import { ExternalPromptDialog } from './external-prompt-dialog';

/**
 * The Generate External Prompt dialog's WCAG semantics (P4.D358, v4
 * `dff76ebef`): the card is a modal `role="dialog"` named by its `<h3>`.
 */

function mount(characterName: string | null): ComponentFixture<ExternalPromptDialog> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ExternalPromptDialog],
    providers: [{ provide: CoreClient, useValue: { dispatch: vi.fn(async () => ({})) } }],
  });
  const fixture = TestBed.createComponent(ExternalPromptDialog);
  fixture.componentRef.setInput('characterId', 'c-1');
  fixture.componentRef.setInput('characterName', characterName);
  fixture.detectChanges();
  return fixture;
}

describe('ExternalPromptDialog — WCAG semantics (P4.D358)', () => {
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
      'Generate External Prompt for Lorian',
    );
  });
});
