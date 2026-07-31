import { Injectable, signal } from '@angular/core';

/** v4 `ToastType` (`lib/toast.tsx:3`). */
export type ToastType = 'success' | 'error' | 'info' | 'warning';

/** v4's `Toast` record (`lib/toast.tsx:10-15`). */
export interface Toast {
  id: string;
  message: string;
  type: ToastType;
  duration: number;
}

/** v4 `showToast`'s options (`lib/toast.tsx:5-8`). */
export interface ToastOptions {
  duration?: number;
  type?: ToastType;
}

/** v4's default duration (`lib/toast.tsx:99`). */
export const TOAST_DEFAULT_DURATION = 3000;

/**
 * The toast subsystem — v5's port of v4's `lib/toast.tsx` (151 LOC), the one
 * notification surface 106 v4 files reach for.
 *
 * ## The mechanism difference (deliberate, and the only one)
 *
 * v4's module is imperative and host-less: the first `showToast` call lazily
 * appends a `<div role="toast-container">` to `document.body` and mounts a
 * React root into it. The Angular analog is this root-provided service holding
 * the toast array as a signal, plus `qt-toast-container` rendered ONCE in the
 * app root (`app.ts`). Two consequences, both harmless:
 *
 *  - the container element exists from boot rather than from the first toast
 *    (it is `pointer-events: none` and zero-sized while empty);
 *  - `role="toast-container"` is carried verbatim — it is v4's own testability
 *    handle and this lane's e2e anchor.
 *
 * ## Behavior ported exactly
 *
 * Default duration 3000 ms with a per-call override; default type `info`;
 * newest toast appended LAST (v4 `toasts.push`, `:113`); auto-removal by id
 * after `duration` (`:117-120`); the returned id (`:122`). v4 has **no** dismiss
 * button, no hover pause, no dedup and no cap — none are invented here.
 *
 * The one detail not carried byte-for-byte is the id's shape: v4 mints
 * `toast-${Date.now()}-${Math.random()}` (`:104`), v5 uses a monotonic counter
 * instead of `Math.random()`. Ids are internal — nothing renders or asserts one
 * — and a counter cannot collide, where v4's pair can in principle.
 */
@Injectable({ providedIn: 'root' })
export class ToastService {
  private readonly toastsSignal = signal<readonly Toast[]>([]);
  private sequence = 0;

  /** The live stack, oldest first — `qt-toast-container` renders it in order. */
  readonly toasts = this.toastsSignal.asReadonly();

  /** v4 `showToast` (`lib/toast.tsx:96-124`). */
  show(message: string, options: ToastOptions = {}): string {
    const { duration = TOAST_DEFAULT_DURATION, type = 'info' } = options;
    const id = `toast-${Date.now()}-${this.sequence++}`;

    this.toastsSignal.update((list) => [...list, { id, message, type, duration }]);

    setTimeout(() => this.dismiss(id), duration);

    return id;
  }

  /** v4 `showSuccessToast` (`:129-131`). */
  showSuccess(message: string, duration?: number): string {
    return this.show(message, { type: 'success', duration });
  }

  /** v4 `showErrorToast` (`:136-138`). */
  showError(message: string, duration?: number): string {
    return this.show(message, { type: 'error', duration });
  }

  /** v4 `showWarningToast` (`:143-145`). */
  showWarning(message: string, duration?: number): string {
    return this.show(message, { type: 'warning', duration });
  }

  /** v4 `showInfoToast` (`:150-152`). */
  showInfo(message: string, duration?: number): string {
    return this.show(message, { type: 'info', duration });
  }

  /** v4's auto-remove filter (`:118`). Not exposed as an affordance: v4 has no
   *  dismiss control, and this lane does not invent one. */
  private dismiss(id: string): void {
    this.toastsSignal.update((list) => list.filter((t) => t.id !== id));
  }
}
