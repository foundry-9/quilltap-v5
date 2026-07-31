import { TestBed } from '@angular/core/testing';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ToastContainer } from './toast-container';
import { TOAST_DEFAULT_DURATION, ToastService } from './toast.service';

function service(): ToastService {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ providers: [ToastService] });
  return TestBed.inject(ToastService);
}

describe('ToastService (v4 lib/toast.tsx)', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("carries v4's 3000 ms default and expires the toast at it", () => {
    const toasts = service();
    toasts.showInfo('Chat renamed');

    expect(toasts.toasts()).toHaveLength(1);
    expect(toasts.toasts()[0].duration).toBe(TOAST_DEFAULT_DURATION);
    expect(TOAST_DEFAULT_DURATION).toBe(3000);

    vi.advanceTimersByTime(2999);
    expect(toasts.toasts()).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(toasts.toasts()).toHaveLength(0);
  });

  it('honours a per-call duration override (v4 showXToast(message, duration))', () => {
    const toasts = service();
    toasts.showError('Failed to save', 8000);

    vi.advanceTimersByTime(3000);
    expect(toasts.toasts()).toHaveLength(1);
    vi.advanceTimersByTime(5000);
    expect(toasts.toasts()).toHaveLength(0);
  });

  it('defaults to the info type and maps each helper to v4\'s type', () => {
    const toasts = service();
    toasts.show('bare');
    toasts.showSuccess('ok');
    toasts.showError('bad');
    toasts.showWarning('careful');
    toasts.showInfo('fyi');

    expect(toasts.toasts().map((t) => t.type)).toEqual([
      'info',
      'success',
      'error',
      'warning',
      'info',
    ]);
  });

  it('stacks oldest-first and expires each toast on its OWN timer', () => {
    const toasts = service();
    toasts.showInfo('first', 1000);
    vi.advanceTimersByTime(400);
    toasts.showInfo('second');

    expect(toasts.toasts().map((t) => t.message)).toEqual(['first', 'second']);

    vi.advanceTimersByTime(600); // first hits 1000
    expect(toasts.toasts().map((t) => t.message)).toEqual(['second']);

    vi.advanceTimersByTime(2400); // second hits 3000
    expect(toasts.toasts()).toHaveLength(0);
  });

  it('returns a distinct id per toast (v4 :122)', () => {
    const toasts = service();
    const a = toasts.showInfo('a');
    const b = toasts.showInfo('b');
    expect(a).not.toBe(b);
    expect(toasts.toasts().map((t) => t.id)).toEqual([a, b]);
  });

  it('does not dedup identical messages — v4 has no dedup', () => {
    const toasts = service();
    toasts.showError('Failed to save');
    toasts.showError('Failed to save');
    expect(toasts.toasts()).toHaveLength(2);
  });
});

describe('ToastContainer', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  function render(): { host: HTMLElement; toasts: ToastService; detect: () => void } {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [ToastContainer] });
    const fixture = TestBed.createComponent(ToastContainer);
    fixture.detectChanges();
    return {
      host: fixture.nativeElement as HTMLElement,
      toasts: TestBed.inject(ToastService),
      detect: () => fixture.detectChanges(),
    };
  }

  it("renders v4's role=\"toast-container\" handle, empty at rest", () => {
    const { host } = render();
    const container = host.querySelector('[role="toast-container"]');
    expect(container).not.toBeNull();
    expect(container?.children).toHaveLength(0);
  });

  it('renders each toast with the message text and v4\'s per-type class', () => {
    const { host, toasts, detect } = render();
    toasts.showSuccess('Chat renamed');
    toasts.showError('Failed to rename chat');
    toasts.showWarning('Nothing selected');
    toasts.showInfo('Working…');
    detect();

    const nodes = Array.from(host.querySelectorAll('[role="toast-container"] > div'));
    expect(nodes.map((n) => n.textContent?.trim())).toEqual([
      'Chat renamed',
      'Failed to rename chat',
      'Nothing selected',
      'Working…',
    ]);
    expect(nodes.map((n) => n.className)).toEqual([
      'qt-toast qt-toast-success',
      'qt-toast qt-toast-error',
      'qt-toast qt-toast-warning',
      'qt-toast qt-toast-info',
    ]);
  });

  it('drops the node when the toast expires', () => {
    const { host, toasts, detect } = render();
    toasts.showInfo('gone in a moment');
    detect();
    expect(host.querySelectorAll('[role="toast-container"] > div')).toHaveLength(1);

    vi.advanceTimersByTime(3000);
    detect();
    expect(host.querySelectorAll('[role="toast-container"] > div')).toHaveLength(0);
  });
});
