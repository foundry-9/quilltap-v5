import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import type { CustomToolDestinations } from '../../core/core-contract';
import type { WardrobeVantage } from '../../pascal/tool-draft';
import {
  GENERAL_CONTAINER,
  GENERAL_WARDROBE_NAME,
  decodeWardrobeContainer,
  encodeWardrobeContainer,
  wardrobeReachLabel,
  type WardrobeOrigin,
} from '../../wardrobe/wardrobe-container';

const NONE_VALUE = 'none';

/** A vantage as a `<select>` value: `none`, `char:<id>`, or an encoded container. */
export function encodeWardrobeVantage(vantage: WardrobeVantage): string {
  if (vantage.kind === 'character') return `char:${vantage.characterId}`;
  if (vantage.kind === 'container') return encodeWardrobeContainer(vantage.container);
  return NONE_VALUE;
}

/** Parse a `<select>` value back into a vantage; anything mangled reads as `none`. */
export function decodeWardrobeVantage(value: string): WardrobeVantage {
  if (value.startsWith('char:')) {
    const characterId = value.slice('char:'.length);
    return characterId ? { kind: 'character', characterId } : { kind: 'none' };
  }
  const container = value === NONE_VALUE ? null : decodeWardrobeContainer(value);
  return container ? { kind: 'container', container } : { kind: 'none' };
}

/**
 * The vantage as an origin — the character's or the store's name — so reach
 * and origin labels are spelled by the same functions the wardrobe dialog
 * uses. Null for `none`, or for an id the destinations no longer list.
 */
export function wardrobeVantageOrigin(
  vantage: WardrobeVantage,
  destinations: CustomToolDestinations | undefined,
): WardrobeOrigin | null {
  if (vantage.kind === 'none') return null;
  if (vantage.kind === 'character') {
    const character = destinations?.characters.find((c) => c.characterId === vantage.characterId);
    return { scope: 'character', id: vantage.characterId, name: character?.characterName ?? '' };
  }
  const { container } = vantage;
  switch (container.scope) {
    case 'general':
      return { scope: 'general', id: null, name: GENERAL_WARDROBE_NAME };
    case 'project': {
      const project = destinations?.projects.find((p) => p.projectId === container.id);
      return { scope: 'project', id: container.id, name: project?.projectName ?? 'this project' };
    }
    case 'group': {
      const group = destinations?.groups.find((g) => g.groupId === container.id);
      return { scope: 'group', id: container.id, name: group?.groupName ?? 'this group' };
    }
    case 'character':
      return { scope: 'character', id: container.id, name: '' };
  }
}

/** The vantage's display name: "Imogen", "Quilltap General", "Thornfield"… */
export function wardrobeVantageName(
  vantage: WardrobeVantage,
  destinations: CustomToolDestinations | undefined,
): string | null {
  const origin = wardrobeVantageOrigin(vantage, destinations);
  if (!origin) return null;
  return origin.name || (origin.scope === 'character' ? 'this character' : null);
}

/** What the vantage is browsed for: garments, objects, or both (a tool with both kinds of effect). */
export type VantageBrowsing = 'wardrobe' | 'inventory' | 'both';

/** The nouns each kind of browsing is spoken of in (v4 `102a23648`). */
export const BROWSING_WORDS: Record<
  VantageBrowsing,
  { store: string; stores: string; things: string; shared: string }
> = {
  wardrobe: { store: 'wardrobe', stores: 'wardrobes', things: 'garments', shared: 'Shared wardrobes' },
  inventory: { store: 'inventory', stores: 'inventories', things: 'objects', shared: 'Shared inventories' },
  both: {
    store: 'wardrobe and inventory',
    stores: 'wardrobes and inventories',
    things: 'garments and objects',
    shared: 'Shared stores',
  },
};

/**
 * The Workbench's whose-wardrobe (or inventory) select — a port of v4
 * `components/custom-tools/WardrobeVantageSelect.tsx` (NEW at `5cb0e0190`).
 * It names who can reach what from the vantage chosen: a character's own
 * wardrobe, their groups' and Quilltap General (the lens leaves the project
 * tier out — the Workbench has no chat), or one shared wardrobe and who can
 * reach it.
 *
 * Every sentence is assembled in TS: v4's JSX joins its pieces with `{' '}`
 * and `&rsquo;`, and Angular drops whitespace-only template nodes, so a
 * template spelling would lose the spaces (the P4.D250 guidance-paragraph
 * defect). The bytes are U+2019 apostrophes throughout, as v4's entities render.
 *
 * `browsing` picks the words; the vantage is the same. One rendering divergence
 * is recorded, not ported (`VANTAGE_HINT_JSX_SPACE`, P4.D334 R-7): v4's JSX
 * breaks `…{home}’s` and `{words.store} will also…` across two lines, which
 * drops the space, so v4 prints "Saga’swardrobe"; the space is kept here.
 */
@Component({
  selector: 'qt-wardrobe-vantage-select',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="space-y-1 text-sm">
      <label class="flex items-center gap-2 flex-wrap">
        <span
          >{{ 'Browsing ' }}@if (label(); as l) {<strong>{{ l }}</strong>}{{ ' ' + words().store }}</span
        >
        <select
          [value]="value()"
          (change)="onSelect($event)"
          [disabled]="disabled()"
          class="qt-select qt-select-sm"
          [attr.aria-label]="'Whose ' + words().store + ' to browse'"
        >
          <option value="none" [selected]="value() === 'none'">— choose —</option>
          @if (characters().length > 0) {
            <optgroup label="Characters">
              @for (c of characters(); track c.characterId) {
                <option
                  [value]="'char:' + c.characterId"
                  [selected]="value() === 'char:' + c.characterId"
                >{{ c.characterName }}</option>
              }
            </optgroup>
          }
          <optgroup [label]="words().shared">
            <option [value]="generalValue" [selected]="value() === generalValue">{{ generalName }}</option>
            @for (s of sharedOptions(); track s.value) {
              <option [value]="s.value" [selected]="value() === s.value">{{ s.label }}</option>
            }
          </optgroup>
        </select>
      </label>
      @if (showHint()) {
        <p class="text-xs qt-text-secondary">{{ hint() }}</p>
      }
    </div>
  `,
})
export class WardrobeVantageSelect {
  readonly vantage = input.required<WardrobeVantage>();
  readonly destinations = input<CustomToolDestinations | undefined>(undefined);
  /**
   * The project whose store the file lives in, when it does. Named under a
   * character vantage: the lens leaves the project tier out (the Workbench has
   * no chat), but a run there will reach it.
   */
  readonly homeProjectName = input<string | null>(null);
  /** Show the explanatory line beneath the select. Default true. */
  readonly showHint = input(true);
  /** What is being browsed, for the wording. Default `'wardrobe'`. */
  readonly browsing = input<VantageBrowsing>('wardrobe');
  readonly disabled = input(false);

  readonly vantageChange = output<WardrobeVantage>();

  protected readonly generalValue = encodeWardrobeContainer(GENERAL_CONTAINER);
  protected readonly generalName = GENERAL_WARDROBE_NAME;

  protected readonly words = computed(() => BROWSING_WORDS[this.browsing()]);

  protected readonly value = computed(() => encodeWardrobeVantage(this.vantage()));
  protected readonly characters = computed(() => this.destinations()?.characters ?? []);
  protected readonly sharedOptions = computed(() => {
    const d = this.destinations();
    return [
      ...(d?.projects ?? []).map((p) => ({
        value: encodeWardrobeContainer({ scope: 'project', id: p.projectId }),
        label: `Project · ${p.projectName}`,
      })),
      ...(d?.groups ?? []).map((g) => ({
        value: encodeWardrobeContainer({ scope: 'group', id: g.groupId }),
        label: `Group · ${g.groupName}`,
      })),
    ];
  });

  private readonly name = computed(() =>
    wardrobeVantageName(this.vantage(), this.destinations()),
  );

  /** The `<strong>` in "Browsing <strong>Imogen’s</strong> wardrobe", or null. */
  protected readonly label = computed(() => {
    const name = this.name();
    const kind = this.vantage().kind;
    if (kind === 'character' && name) return `${name}’s`;
    if (kind === 'container' && name) return name;
    return null;
  });

  protected readonly hint = computed(() => {
    const vantage = this.vantage();
    const name = this.name();
    const words = this.words();
    if (vantage.kind === 'character') {
      const home = this.homeProjectName();
      return (
        `What ${name} can reach: ${name}’s own ${words.store}, the ${words.stores} of ${name}’s groups, and ` +
        `${GENERAL_WARDROBE_NAME}. Another character rolling this tool sees their own.` +
        (home
          ? ` This tool lives in ${home}’s store; ${words.things} from ${home}’s ${words.store} will also be in reach when it runs there.`
          : '')
      );
    }
    if (vantage.kind === 'container') {
      const origin = wardrobeVantageOrigin(vantage, this.destinations());
      return `Every one of the ${words.things} here, and who can reach them: ${wardrobeReachLabel(origin)}.`;
    }
    return `Choose whose ${words.store} to browse before picking ${words.things}.`;
  });

  protected onSelect(event: Event): void {
    this.vantageChange.emit(decodeWardrobeVantage((event.target as HTMLSelectElement).value));
  }
}
