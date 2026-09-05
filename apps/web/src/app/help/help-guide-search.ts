/**
 * The Guide's search box (port of v4 `components/help-chat/HelpGuideSearch.tsx`).
 *
 * Byte-faithful on the two strings that are contractual: the `Search topics...`
 * placeholder and the `Search help topics` / `Clear search` aria labels.
 *
 * @module help/help-guide-search
 */

import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  input,
  output,
  viewChild,
} from '@angular/core';

import { Icon } from '../ui/icon';

@Component({
  selector: 'qt-help-guide-search',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  host: { class: 'contents' },
  template: `
    <div class="qt-help-guide-search">
      <qt-icon name="search" class="qt-help-guide-search-icon" />
      <input
        #input
        type="text"
        class="qt-help-guide-search-input"
        placeholder="Search topics..."
        aria-label="Search help topics"
        [value]="value()"
        (input)="valueChange.emit($any($event.target).value)"
      />
      @if (value()) {
        <button
          type="button"
          class="qt-help-guide-search-clear"
          aria-label="Clear search"
          (click)="clear()"
        >
          <qt-icon name="close" class="w-3.5 h-3.5" />
        </button>
      }
    </div>
  `,
})
export class HelpGuideSearch {
  readonly value = input.required<string>();
  readonly valueChange = output<string>();

  private readonly inputEl = viewChild.required<ElementRef<HTMLInputElement>>('input');

  protected clear(): void {
    // v4 clears AND re-focuses, so the next keystroke lands in the box.
    this.valueChange.emit('');
    this.inputEl().nativeElement.focus();
  }
}
