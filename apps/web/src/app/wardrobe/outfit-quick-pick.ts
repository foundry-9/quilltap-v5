import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  ElementRef,
  OnDestroy,
  input,
  output,
  signal,
  viewChild,
} from '@angular/core';

import type { WardrobeItemDto } from '../core/core-contract';
import { Icon } from '../ui/icon';
import { selectComposedOutfits } from './composed-outfits';
import { WARDROBE_SLOT_META } from './slot-meta';

/**
 * Outfit Quick Pick — a port of v4 `components/wardrobe/outfit-quick-pick.tsx`
 * (new at `aec86a613`).
 *
 * The "Wear an outfit" pull-down that sits above the slot rows in the composer.
 * Lists every composed outfit (composite) in the character's wearable pool;
 * picking one wears it the same way the slot pickers wear a garment —
 * flag-driven through the parent's `wear` output, so an additive bundle layers
 * and one marked `replace` clears the slots it lands in.
 *
 * Composed outfits appear *only* here. The per-slot pickers list garments, so a
 * three-slot bundle no longer shows up in three separate slot menus.
 *
 * Renders nothing when the pool holds no composed outfits. v4 returns `null`,
 * so the node leaves the DOM entirely; an Angular host element cannot, so it
 * takes the `hidden` attribute instead — which both hides it and drops it out
 * of the composer's Tailwind `space-y-2` sibling chain, so the first bundle
 * card keeps exactly the spacing v4 gives it in that state.
 */
@Component({
  selector: 'qt-outfit-quick-pick',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  host: { '[attr.hidden]': 'outfits().length === 0 ? "" : null' },
  template: `
    @if (outfits().length > 0) {
      <div class="relative mb-2" #container>
        <button
          type="button"
          (click)="toggle()"
          aria-haspopup="listbox"
          [attr.aria-expanded]="open()"
          class="qt-button-secondary qt-button-sm flex w-full items-center justify-between gap-2"
          title="Put on a whole outfit at once"
        >
          <span>Wear an outfit…</span>
          <qt-icon name="chevron-down" [class]="chevronClass()" />
        </button>

        @if (open()) {
          <div
            role="listbox"
            class="absolute left-0 right-0 top-full z-30 mt-1 rounded border qt-border-default qt-bg-default shadow-md max-h-64 overflow-y-auto"
          >
            <div class="p-2">
              <input
                #searchInput
                type="search"
                [value]="search()"
                (input)="search.set($any($event.target).value)"
                placeholder="Search outfits…"
                class="qt-input qt-input-sm w-full"
              />
            </div>
            @if (candidates().length === 0) {
              <div class="px-3 py-2 qt-text-xs qt-text-secondary">No matching outfits.</div>
            } @else {
              <ul class="divide-y qt-border-default">
                @for (outfit of candidates(); track outfit.id) {
                  <li>
                    <button
                      type="button"
                      role="option"
                      [attr.aria-selected]="false"
                      (click)="pick(outfit)"
                      class="flex w-full items-center justify-between gap-2 px-3 py-2 text-left hover:qt-bg-muted"
                    >
                      <span class="truncate text-sm text-foreground">{{ outfit.title }}</span>
                      <span class="qt-text-xs qt-text-secondary whitespace-nowrap">{{
                        meta(outfit)
                      }}</span>
                    </button>
                  </li>
                }
              </ul>
            }
          </div>
        }
      </div>
    }
  `,
})
export class OutfitQuickPick implements OnDestroy {
  /** The character's full wearable pool (garments and composed outfits). */
  readonly items = input.required<WardrobeItemDto[]>();
  /** Wear the chosen outfit. The parent applies the usual equip rules. */
  readonly wear = output<WardrobeItemDto>();

  protected readonly open = signal(false);
  protected readonly search = signal('');

  private readonly container = viewChild<ElementRef<HTMLDivElement>>('container');
  private readonly searchInput = viewChild<ElementRef<HTMLInputElement>>('searchInput');

  protected readonly outfits = computed(() => selectComposedOutfits(this.items()));

  protected readonly chevronClass = computed(
    () => `w-4 h-4 transition-transform ${this.open() ? 'rotate-180' : ''}`,
  );

  /** v4 `:68-72` — the title filter over the already-sorted outfits. */
  protected readonly candidates = computed(() => {
    const term = this.search().trim().toLowerCase();
    const outfits = this.outfits();
    if (!term) return outfits;
    return outfits.filter((o) => o.title.toLowerCase().includes(term));
  });

  constructor() {
    // v4 `autoFocus` on the search box. React focuses it as it mounts; the
    // signal `viewChild` resolves on the render that creates it, so this effect
    // is the same moment.
    effect(() => {
      this.searchInput()?.nativeElement.focus();
    });
  }

  protected meta(outfit: WardrobeItemDto): string {
    const slots = outfit.types.map((t) => WARDROBE_SLOT_META[t].label).join(', ');
    return `${slots}${outfit.replace ? ' · replaces' : ''}`;
  }

  protected toggle(): void {
    this.open.update((v) => !v);
    if (this.open()) {
      document.addEventListener('mousedown', this.onDocMouseDown);
      // Capture phase + stopPropagation so the enclosing dialog's own Escape
      // handler doesn't dismiss the whole modal along with the menu (v4
      // `:56-66`). A template `(document:keydown.escape)` binding is a BUBBLE
      // listener and would let the dialog fire first — the house pattern is
      // this real capture registration (`screens/scenarios/shared/scenario-row.ts`).
      document.addEventListener('keydown', this.onKeyDown, true);
    } else {
      this.detach();
    }
  }

  protected pick(outfit: WardrobeItemDto): void {
    this.wear.emit(outfit);
    this.close();
  }

  ngOnDestroy(): void {
    this.detach();
  }

  /** v4 `:41-51` — close on outside mousedown, measured against the container. */
  private readonly onDocMouseDown = (event: MouseEvent): void => {
    const container = this.container()?.nativeElement;
    if (container && !container.contains(event.target as Node)) {
      this.open.set(false);
      this.detach();
    }
  };

  /** v4 `:55-66`. */
  private readonly onKeyDown = (event: KeyboardEvent): void => {
    if (event.key !== 'Escape') return;
    event.stopPropagation();
    event.preventDefault();
    this.close();
  };

  private close(): void {
    this.open.set(false);
    this.search.set('');
    this.detach();
  }

  private detach(): void {
    document.removeEventListener('mousedown', this.onDocMouseDown);
    document.removeEventListener('keydown', this.onKeyDown, true);
  }
}
