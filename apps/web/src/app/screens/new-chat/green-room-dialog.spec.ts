import { ComponentFixture, TestBed } from '@angular/core/testing';
import { Subject } from 'rxjs';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { GreenRoomDialog } from './green-room-dialog';
import { GreenRoomStore } from './green-room.state';

/**
 * The Green Room's WCAG semantics (P4.D358, v4 `dff76ebef`): a modal
 * `role="dialog"` named by its title — and, as v4's `ChatCreationProgressModal`
 * is, NOT dismissable by Escape (it closes itself when creation resolves).
 */

function mount(): { fixture: ComponentFixture<GreenRoomDialog>; store: GreenRoomStore } {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [GreenRoomDialog],
    providers: [{ provide: CoreClient, useValue: { events$: new Subject().asObservable() } }],
  });
  const store = TestBed.inject(GreenRoomStore);
  store.begin('progress-1');
  const fixture = TestBed.createComponent(GreenRoomDialog);
  fixture.detectChanges();
  return { fixture, store };
}

describe('GreenRoomDialog — WCAG semantics (P4.D358)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('announces as a dialog named by its title', () => {
    const { fixture } = mount();
    const host = fixture.nativeElement as HTMLElement;
    const dlg = host.querySelector('[role="dialog"]') as HTMLElement;
    expect(dlg.classList.contains('qt-dialog')).toBe(true);
    expect(dlg.getAttribute('aria-modal')).toBe('true');
    expect(dlg.getAttribute('tabindex')).toBe('-1');
    const id = dlg.getAttribute('aria-labelledby')!;
    expect(id).toMatch(/^qt-dialog-\d+$/);
    expect((host.querySelector('#' + id) as HTMLElement).textContent?.trim()).toBe(
      'The Green Room',
    );
  });

  it('a document Escape does NOT close it — running or failed', () => {
    const { fixture, store } = mount();
    const escape = (): void => {
      document.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }),
      );
    };
    escape();
    fixture.detectChanges();
    expect(store.state().open).toBe(true);
    expect((fixture.nativeElement as HTMLElement).querySelector('[role="dialog"]')).not.toBeNull();

    store.fail('The understudy fainted.');
    fixture.detectChanges();
    escape();
    fixture.detectChanges();
    expect(store.state().open).toBe(true);
    expect(store.state().phase).toBe('error');
  });
});
