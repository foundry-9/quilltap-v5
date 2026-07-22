import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  OnInit,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import { Icon, type IconName } from './icon';

/**
 * A card with a clickable header toggling its body (v4
 * `components/ui/CollapsibleCard.tsx`). Supports deep-linking: `sectionId` becomes
 * the element id and `forceOpen` opens the card and scrolls it into view once.
 * The `qt-collapsible-card-*` classes carry over verbatim.
 *
 * Two modes, as in v4: UNCONTROLLED (the default — the card owns its open
 * state) and CONTROLLED (`isOpen` bound; the parent owns it and hears
 * `openChange`), which is what a single-open accordion needs. The controlled
 * arm arrived with P4.9H1's chat sidebar; v4 has had it since the sidebar did.
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
export class CollapsibleCard implements OnInit {
  private readonly host = inject(ElementRef<HTMLElement>);

  readonly title = input.required<string>();
  readonly description = input<string | undefined>(undefined);
  readonly icon = input<IconName | undefined>(undefined);
  readonly defaultOpen = input(false);
  /** Stable, URL-friendly id — rendered as the element's id attribute. */
  readonly sectionId = input<string | undefined>(undefined);
  /** When true, forces the card open and scrolls it into view (one-shot). */
  readonly forceOpen = input(false);
  /**
   * Controlled-mode open state (v4 `isOpen`). When bound, it is the source of
   * truth and the internal store is ignored; pair with {@link openChange} so a
   * parent can drive single-open accordion behaviour (v4's ChatSidebar does).
   * `undefined` ⇒ uncontrolled, byte-identical to the pre-P4.9H1 behaviour.
   */
  readonly open = input<boolean | undefined>(undefined, { alias: 'isOpen' });
  /** Controlled-mode setter (v4 `onOpenChange`) — the next desired open state. */
  readonly openChange = output<boolean>();

  private readonly uncontrolledOpen = signal(false);
  /** v4's `isControlled = controlledIsOpen !== undefined`. */
  protected readonly isControlled = computed(() => this.open() !== undefined);
  protected readonly isOpen = computed(() => this.open() ?? this.uncontrolledOpen());
  private scrolled = false;

  constructor() {
    // React to forceOpen toggling on (deep-link) by opening + scrolling once.
    // (The initial defaultOpen/forceOpen seed happens in ngOnInit, where bound
    // signal inputs have resolved — reading them in the constructor would see
    // only their defaults.)
    effect(() => {
      if (this.forceOpen()) {
        // In controlled mode the parent owns open state — only notify (v4).
        if (this.isControlled()) {
          if (!this.open()) {
            this.openChange.emit(true);
          }
        } else {
          this.uncontrolledOpen.set(true);
        }
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

  ngOnInit(): void {
    // Bound inputs have resolved by ngOnInit — seed the initial open state now.
    this.uncontrolledOpen.set(this.defaultOpen() || this.forceOpen());
  }

  protected toggle(): void {
    if (this.isControlled()) {
      this.openChange.emit(!this.open());
      return;
    }
    this.uncontrolledOpen.update((v) => !v);
  }
}
