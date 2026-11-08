import { expect, type Locator } from '@playwright/test';

/**
 * Assert the Quilltap tooltip a control wears (P4.D385 item 22 — the composer
 * family's `title` → `<qt-tooltip>` sweep, v4 `df8d4d70a`).
 *
 * Resolves the nearest `qt-tooltip` host of `target` (or `target` itself),
 * fires `pointerenter` on it, asserts the bubble's text, then `pointerleave`s.
 * Dispatched events rather than a real hover (ruling PL-12): a DISABLED button
 * does not reliably deliver pointer events, and the Send / raise / Stop
 * ladders are asserted while disabled. The bubble is `aria-hidden` and
 * reparented to `document.body`, so it is found by its class, not its role —
 * the LAST one, because a closing bubble can linger a frame.
 *
 * Exported for P4.D384 (`salon-interrupt-flow`, HANDOFF H3) and P4.D387
 * (`salon-documents-flow`, HANDOFF H4).
 */
export async function expectTooltip(target: Locator, text: string | RegExp): Promise<void> {
  const host = target.locator('xpath=ancestor-or-self::qt-tooltip[1]');
  await expect(host).toHaveCount(1);
  await host.dispatchEvent('pointerenter');
  const page = target.page();
  await expect(page.locator('.qt-tooltip').last()).toHaveText(text);
  await host.dispatchEvent('pointerleave');
}

/**
 * Read the text of the Quilltap tooltip a control wears (the same dispatched
 * hover as {@link expectTooltip}) — for a CONDITIONAL read, where a spec
 * branches on the words rather than asserting them.
 */
export async function readTooltip(target: Locator): Promise<string> {
  const host = target.locator('xpath=ancestor-or-self::qt-tooltip[1]');
  await expect(host).toHaveCount(1);
  await host.dispatchEvent('pointerenter');
  const bubble = target.page().locator('.qt-tooltip').last();
  await expect(bubble).toBeVisible();
  const text = (await bubble.textContent())?.trim() ?? '';
  await host.dispatchEvent('pointerleave');
  return text;
}
