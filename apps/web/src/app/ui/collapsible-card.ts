import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  effect,
  inject,
  input,
  signal,
} from '@angular/core';

import { Icon, type IconName } from './icon';

/**
 * A card with a clickable header toggling its body (v4
 * `components/ui/CollapsibleCard.tsx`). Supports deep-linking: `sectionId` becomes
 * the element id and `forceOpen` opens the card and scrolls it into view once.
 * The `qt-collapsible-card-*` classes carry over verbatim.
 */
@Component({
  selector: 'qt-collapsible-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="qt-collapsible-card" [id]="sectionId() || null">
      <button
        type="button"
        class="qt-collapsible-card-header"
        [attr.aria-expanded]="isOpen()"
        (click)="toggle()"
      >
        <div class="flex items-center gap-3 min-w-0">
          @if (icon(); as name) {
            <span class="qt-collapsible-card-icon"><qt-icon [name]="name" class="w-5 h-5" /></span>
          }
          <div class="min-w-0">
            <h3 class="qt-card-title">{{ title() }}</h3>
            @if (description()) {
              <p class="qt-card-description">{{ description() }}</p>
            }
          </div>
        </div>
        <qt-icon
          name="chevron-down"
          [class]="
            'qt-collapsible-card-chevron ' + (isOpen() ? 'qt-collapsible-card-chevron-open' : '')
          "
        />
      </button>
      @if (isOpen()) {
        <div class="qt-collapsible-card-divider"></div>
        <div class="qt-collapsible-card-body">
          <ng-content></ng-content>
        </div>
      }
    </div>
  `,
})
export class CollapsibleCard {
  private readonly host = inject(ElementRef<HTMLElement>);

  readonly title = input.required<string>();
  readonly description = input<string | undefined>(undefined);
  readonly icon = input<IconName | undefined>(undefined);
  readonly defaultOpen = input(false);
  /** Stable, URL-friendly id — rendered as the element's id attribute. */
  readonly sectionId = input<string | undefined>(undefined);
  /** When true, forces the card open and scrolls it into view (one-shot). */
  readonly forceOpen = input(false);

  protected readonly isOpen = signal(false);
  private scrolled = false;

  constructor() {
    // Seed the open state from defaultOpen/forceOpen, and react to forceOpen
    // toggling on (deep-link) by opening + scrolling once.
    this.isOpen.set(this.defaultOpen() || this.forceOpen());
    effect(() => {
      if (this.forceOpen()) {
        this.isOpen.set(true);
        if (!this.scrolled) {
          this.scrolled = true;
          requestAnimationFrame(() =>
            this.host.nativeElement.scrollIntoView({ behavior: 'smooth', block: 'start' }),
          );
        }
      } else {
        this.scrolled = false;
      }
    });
  }

  protected toggle(): void {
    this.isOpen.update((v) => !v);
  }
}
