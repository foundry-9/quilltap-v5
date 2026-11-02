import { Directive } from '@angular/core';

/**
 * Keyboard activation for an element given `role="button"` (WCAG 2.1.1) — v4
 * `components/ui/activate-on-key.ts` (`dff76ebef`), whole.
 *
 * Enter or Space dispatches a real click on the element, so it reaches the
 * same `(click)` a pointer does — its own, or a card's or row's further up.
 * Used where a native `<button>` won't do: inside a `<tr>`, or around text
 * that themes would restyle as a command (two bundled themes set every
 * `button` in tracked capitals).
 *
 * @example
 * <span role="button" tabindex="0" (keydown)="activateOnKey($event)">{{ name }}</span>
 */
export function activateOnKey(e: KeyboardEvent): void {
  if (e.key !== 'Enter' && e.key !== ' ') return;
  if (e.target !== e.currentTarget) return;
  e.preventDefault();
  (e.currentTarget as HTMLElement).click();
}

/**
 * The attribute form of {@link activateOnKey}, for templates that prefer
 * `<span role="button" tabindex="0" qtActivateOnKey>` to a `(keydown)` binding.
 */
@Directive({
  selector: '[qtActivateOnKey]',
  host: { '(keydown)': 'onKeydown($event)' },
})
export class ActivateOnKey {
  protected onKeydown(e: KeyboardEvent): void {
    activateOnKey(e);
  }
}
