import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { ProgressionEditorModal } from './progression-editor-modal';

/**
 * The progression editor's WCAG semantics (P4.D358): v4 `dff76ebef` gave the
 * card `role="dialog"` named by its heading, `useDialogFocus`, and
 * `useEscapeKey(onClose, !saving)` — Escape closes, but never mid-save.
 */

function mount(saving = false): ComponentFixture<ProgressionEditorModal> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [ProgressionEditorModal] });
  const fixture = TestBed.createComponent(ProgressionEditorModal);
  fixture.componentRef.setInput('existingIds', []);
  fixture.componentRef.setInput('saving', saving);
  fixture.detectChanges();
  return fixture;
}

function escape(): void {
  document.dispatchEvent(
    new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }),
  );
}

describe('ProgressionEditorModal — WCAG semantics (P4.D358)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('announces as a dialog named by its heading', () => {
    const fixture = mount();
    const host = fixture.nativeElement as HTMLElement;
    const dlg = host.querySelector('[role="dialog"]') as HTMLElement;
    expect(dlg.classList.contains('qt-card')).toBe(true);
    expect(dlg.getAttribute('aria-modal')).toBe('true');
    expect(dlg.getAttribute('tabindex')).toBe('-1');
    const id = dlg.getAttribute('aria-labelledby')!;
    expect(id).toMatch(/^qt-dialog-\d+$/);
    expect((host.querySelector('#' + id) as HTMLElement).textContent?.trim()).toBe(
      'New progression',
    );
  });

  it('a document Escape closes exactly once — but not while saving', () => {
    const fixture = mount(true);
    let closed = 0;
    fixture.componentInstance.close.subscribe(() => closed++);
    escape();
    expect(closed).toBe(0);
    fixture.componentRef.setInput('saving', false);
    fixture.detectChanges();
    escape();
    expect(closed).toBe(1);
  });
});
