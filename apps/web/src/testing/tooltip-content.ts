import type { ComponentFixture } from '@angular/core/testing';
import { By } from '@angular/platform-browser';

import { Tooltip } from '../app/ui/tooltip';

/**
 * The spec-side reader for a `qt-tooltip`'s words (P4.D385, the composer
 * family's `title` → tooltip sweep): the nearest `qt-tooltip` host wrapping
 * `target` (or `target` itself), and its `content` input — the string a
 * native `title` used to carry. `null` when `target` sits under no tooltip, or
 * the tooltip carries a template rather than a string.
 *
 * Read through the component instance rather than by hovering, because the
 * bubble only mounts after a delay and is reparented to `document.body`; the
 * hover path is the e2e helper's (`e2e/support/tooltips.ts`).
 */
export function tooltipContentOf(fixture: ComponentFixture<unknown>, target: Element | null): string | null {
  const host = target?.closest('qt-tooltip') ?? null;
  if (!host) return null;
  const match = fixture.debugElement
    .queryAll(By.directive(Tooltip))
    .find((d) => d.nativeElement === host);
  return match ? (match.componentInstance as Tooltip).content() : null;
}
