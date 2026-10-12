import { afterNextRender, Directive, ElementRef, inject } from '@angular/core';

/**
 * React's `autoFocus` for the wardrobe's inline inputs (the browser's New
 * folder and Rename fields, the picker's New folder… field): focus the element
 * once, after the render that inserted it. A bare `autofocus` attribute does
 * nothing on a node inserted after load (the `project-characters-card.ts`
 * finding), and these inputs all mount on a click.
 */
@Directive({ selector: '[qtAutofocus]' })
export class QtAutofocus {
  constructor() {
    const el = inject<ElementRef<HTMLElement>>(ElementRef);
    afterNextRender(() => el.nativeElement.focus());
  }
}
