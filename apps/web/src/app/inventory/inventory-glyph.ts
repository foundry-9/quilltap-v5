import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import { apiUrl } from '../core/api-url';
import type { ResolvedInventoryItem } from '../core/core-contract';
import { Icon } from '../ui/icon';

/**
 * The `<img src>` a glyph renders through (v4 `lib/inventory/svg.ts:226-228`).
 * Spec-pinned (R-15) — the client half of a server module.
 */
export function inventorySvgDataUri(svg: string): string {
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

/**
 * `Pocket Watch` → `PW`, `lantern` → `LA`: up to two letters for an object with
 * no SVG or emoji (v4 `inventory-glyph.tsx:23-32`, `27fb3ec77`, transcribed —
 * the `u`-flag `\p{L}` / `\p{N}` classes; `match(/…/gu)` yields code points,
 * so an astral letter counts once). Pinned by the tier-1 corpus
 * `inventory-abbreviation.oracle.ndjson`.
 */
export function inventoryAbbreviation(label: string): string {
  const words = label
    .trim()
    .split(/\s+/)
    .filter((w) => /[\p{L}\p{N}]/u.test(w));
  if (words.length === 0) return '?';
  const initial = (w: string) => w.match(/[\p{L}\p{N}]/u)?.[0] ?? '';
  const letters =
    words.length === 1
      ? (words[0].match(/[\p{L}\p{N}]/gu) ?? []).slice(0, 2).join('')
      : initial(words[0]) + initial(words[1]);
  return letters.toUpperCase();
}

type GlyphItem = Pick<ResolvedInventoryItem, 'svg' | 'emoji' | 'kind' | 'name' | 'title'>;

/**
 * An Inventory object's glyph — a port of v4 `components/inventory/
 * inventory-glyph.tsx` (`322a5488e`). What every row, chip and pane shows:
 * the object's SVG (drawn through an `<img>` data URI, so it is an icon and
 * nothing inside it can run), else its emoji, else a default glyph (`layers`
 * for a kit, `tag` otherwise — or, where asked, an abbreviation of its name,
 * `27fb3ec77`). The emoji span carries no `title` since `27fb3ec77`. **Never the picture** — the picture is for
 * choosing, shown only by {@link InventoryPicture}.
 */
@Component({
  selector: 'qt-inventory-glyph',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    @if (item().svg; as svg) {
      <img
        [src]="dataUri(svg)"
        alt=""
        aria-hidden="true"
        [attr.class]="box() + ' flex-shrink-0 object-contain'"
        data-testid="inventory-glyph-svg"
      />
    } @else if (item().emoji) {
      <span
        [attr.class]="box() + ' flex-shrink-0 inline-flex items-center justify-center'"
        aria-hidden="true"
        data-testid="inventory-glyph-emoji"
        >{{ item().emoji }}</span
      >
    } @else if (fallback() === 'abbreviation') {
      <span
        [attr.class]="
          box() +
          ' flex-shrink-0 inline-flex items-center justify-center rounded border qt-border-default qt-bg-muted qt-text-secondary font-semibold text-[10px] leading-none'
        "
        aria-hidden="true"
        data-testid="inventory-glyph-abbreviation"
        >{{ abbreviation() }}</span
      >
    } @else {
      <span
        [attr.class]="box() + ' flex-shrink-0 inline-flex items-center justify-center qt-text-secondary'"
        aria-hidden="true"
        data-testid="inventory-glyph-default"
      >
        <qt-icon [name]="item().kind === 'kit' ? 'layers' : 'tag'" [class]="size() === 'sm' ? 'w-3.5 h-3.5' : 'w-5 h-5'" />
      </span>
    }
  `,
})
export class InventoryGlyph {
  readonly item = input.required<GlyphItem>();
  readonly size = input<'sm' | 'md'>('md');
  /**
   * What to draw with neither SVG nor emoji: the default icon, or the name's
   * abbreviation (v4 `27fb3ec77` — the Salon's carried-items row asks for it).
   */
  readonly fallback = input<'icon' | 'abbreviation'>('icon');

  protected readonly dataUri = inventorySvgDataUri;
  protected readonly box = computed(() => (this.size() === 'sm' ? 'w-5 h-5 text-sm' : 'w-8 h-8 text-lg'));
  protected readonly label = computed(() => this.item().title || this.item().name);
  protected readonly abbreviation = computed(() => inventoryAbbreviation(this.label()));
}

/**
 * The object's current picture, for choosing — or its glyph when it has none
 * (v4 `InventoryPicture`, `:47-70`). The thumbnail goes through the
 * origin-aware `apiUrl` (the wardrobe thumbnail's idiom).
 */
@Component({
  selector: 'qt-inventory-picture',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [InventoryGlyph],
  template: `
    @if (item().imageFileId; as fileId) {
      <img
        [src]="thumbnail(fileId)"
        [alt]="'Picture of ' + (item().title || item().name)"
        class="rounded border qt-border-default object-cover flex-shrink-0"
        [style.width.px]="size()"
        [style.height.px]="size()"
        data-testid="inventory-picture"
      />
    } @else {
      <span
        class="inline-flex items-center justify-center rounded border qt-border-default qt-bg-muted flex-shrink-0"
        [style.width.px]="size()"
        [style.height.px]="size()"
      >
        <qt-inventory-glyph [item]="item()" />
      </span>
    }
  `,
})
export class InventoryPicture {
  readonly item = input.required<GlyphItem & Pick<ResolvedInventoryItem, 'imageFileId'>>();
  readonly size = input(48);

  protected thumbnail(fileId: string): string {
    return apiUrl(`/api/v1/files/${fileId}?action=thumbnail`);
  }
}
