import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { AvatarPicker } from './avatar-picker';

/**
 * The Select Avatar picker's WCAG semantics (P4.D358, v4 `dff76ebef`): a modal
 * `role="dialog"` named by its title through `aria-labelledby` (the old
 * `aria-label` is gone), and v4's `useEscapeKey(handleClose, …)`.
 */

async function mount(): Promise<ComponentFixture<AvatarPicker>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [AvatarPicker],
    providers: [{ provide: CoreClient, useValue: { filesList: vi.fn(async () => ({ files: [] })) } }],
  });
  const fixture = TestBed.createComponent(AvatarPicker);
  fixture.detectChanges();
  for (let i = 0; i < 4; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

describe('AvatarPicker — WCAG semantics (P4.D358)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('announces as a dialog named by its title', async () => {
    const fixture = await mount();
    const host = fixture.nativeElement as HTMLElement;
    const dlg = host.querySelector('[role="dialog"]') as HTMLElement;
    expect(dlg.classList.contains('qt-dialog')).toBe(true);
    expect(dlg.getAttribute('aria-modal')).toBe('true');
    expect(dlg.getAttribute('tabindex')).toBe('-1');
    expect(dlg.hasAttribute('aria-label')).toBe(false);
    const id = dlg.getAttribute('aria-labelledby')!;
    expect(id).toMatch(/^qt-dialog-\d+$/);
    expect((host.querySelector('#' + id) as HTMLElement).textContent?.trim()).toBe('Select Avatar');
  });

  it('a document Escape cancels exactly once', async () => {
    const fixture = await mount();
    let cancelled = 0;
    const chosen: Array<string | null> = [];
    fixture.componentInstance.cancelled.subscribe(() => cancelled++);
    fixture.componentInstance.chosen.subscribe((v) => chosen.push(v));
    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }),
    );
    expect(cancelled).toBe(1);
    expect(chosen).toEqual([]);
  });
});
