import type { Page } from '@playwright/test';

/**
 * The toast stack's e2e handles (P4.D358, v4 `58120a7fe`). v4's container
 * carries `role="status" aria-live="polite" data-toast-container` — a valid
 * polite live region where it used to carry the invalid `role="toast-container"`
 * — so specs select the `data-toast-container` handle, never the role.
 */
export type ToastClass = 'qt-toast-success' | 'qt-toast-error' | 'qt-toast-warning' | 'qt-toast-info';

/** The container itself (one per page, from boot). */
export const toastContainer = (page: Page) => page.locator('[data-toast-container]');

/** Every live toast, oldest first. */
export const toastStack = (page: Page) => page.locator('[data-toast-container] > div');

/** The live toasts of one type. */
export const toastByClass = (page: Page, klass: ToastClass) =>
  page.locator(`[data-toast-container] .${klass}`);
