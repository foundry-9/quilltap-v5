import {
  afterRenderEffect,
  booleanAttribute,
  ChangeDetectionStrategy,
  Component,
  computed,
  ElementRef,
  input,
  output,
  viewChild,
} from '@angular/core';

import {
  ARCHIVED_OPTION_SUFFIX,
  BROWSE_SCENARIO_VALUE,
  CUSTOM_SCENARIO_VALUE,
  GENERAL_SCENARIO_PREFIX,
  GROUP_SCENARIO_PREFIX,
  PROJECT_SCENARIO_PREFIX,
  scenarioSelectionToValue,
  scenarioValueToSelection,
  type CharacterScenario,
  type GeneralScenarioOption,
  type GroupScenarioOption,
  type ProjectScenarioOption,
  type ScenarioSelection,
} from './scenario.types';
import { characterSortFields, sortScenarios, storeSortFields } from './scenario-sort';

/** One rendered group optgroup (v4's `groupScenariosByGroup` Map entry). */
interface GroupOptgroup {
  groupId: string;
  groupName: string;
  scenarios: GroupScenarioOption[];
}

/**
 * The scenario dropdown, shared by the New Chat dialog and the Salon sidebar's
 * in-chat picker (v4 `components/scenario/ScenarioSelect.tsx`, NEW at
 * `44a8137e`). Renders the four tiers as optgroups — project, general, group,
 * character — beneath a standing "Custom…" entry and, when the surface offers
 * the browser (`offerBrowse`), a "Browse all scenarios…" entry that opens the
 * scenario browser instead of selecting anything (v4 `545e299bb`). A pick from
 * outside the hierarchy shows in an "Elsewhere" optgroup holding exactly that
 * one pick. The dropdown ALWAYS renders — v4 deleted `hasAnyScenarioOptions`
 * with the browser, so Browse is reachable from an empty instance.
 *
 * The component is presentational: it takes already-fetched options and a
 * selection, and reports a new selection. What the surrounding surface does
 * with free text (New Chat layers notes beneath a preset; the sidebar swaps in
 * a textarea only for "Custom…") is the surface's own business.
 *
 * Archived entries only reach this component when the surface asked the server
 * for them; when they do, they're marked "(archived)" and remain selectable —
 * archiving hides an entry from the default view, it does not forbid a human
 * who has deliberately gone looking for it (v4 `d25dacc1`).
 *
 * ## Three Angular-port notes
 *
 * **The host is a block.** v4's root node IS the `<select class="qt-select">`,
 * which `.qt-select` makes `w-full`. An Angular custom element defaults to
 * `display: inline`, which would leave the percentage width resolving against
 * an inline box (memory `angular-custom-element-host-is-inline`), so the host
 * carries `block` and the select fills it exactly as v4's fills its parent.
 *
 * **Each option's text is ONE interpolation.** v4 concatenates three adjacent
 * JSX expressions with nothing between them; separate Angular template lines
 * would collapse to a single space and ship `Foo  (project default)`. Both
 * label runs are therefore computed in TypeScript and interpolated once.
 *
 * **The selected row is assigned post-render.** React assigns `select.value`
 * after the children mount, so a value matching NO option leaves the control
 * blank (`selectedIndex === -1`); Angular's `[value]` runs before the `@for`
 * fills the list and `[selected]` falls back to row 0. The faithful port is
 * the `afterRenderEffect` assignment below (memory
 * `angular-select-cannot-mirror-react-controlled-value`) — reachable here
 * whenever a tier's list refetches without the row the current selection names.
 */
@Component({
  selector: 'qt-scenario-select',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'block' },
  template: `
    <select
      #scenarioSelect
      [id]="selectId()"
      [attr.data-qt-value]="currentValue()"
      (change)="onChange($event)"
      [disabled]="disabled()"
      [attr.class]="selectClass()"
      [attr.aria-label]="ariaLabel()"
    >
      <option [value]="CUSTOM_SCENARIO_VALUE">Custom...</option>
      @if (offerBrowse()) {
        <option [value]="BROWSE_SCENARIO_VALUE">Browse all scenarios…</option>
      }
      @if (storeSelection(); as store) {
        <optgroup label="Elsewhere">
          <option [value]="currentValue()">{{ store.label }}</option>
        </optgroup>
      }
      @if (sortedProjectScenarios().length > 0) {
        <optgroup label="Project Scenarios">
          @for (s of sortedProjectScenarios(); track s.path) {
            <option [value]="PROJECT_SCENARIO_PREFIX + s.path">
              {{ fileOptionLabel(s, ' (project default)') }}
            </option>
          }
        </optgroup>
      }
      @if (sortedGeneralScenarios().length > 0) {
        <optgroup label="General Scenarios">
          @for (s of sortedGeneralScenarios(); track s.path) {
            <option [value]="GENERAL_SCENARIO_PREFIX + s.path">
              {{ fileOptionLabel(s, ' (general default)') }}
            </option>
          }
        </optgroup>
      }
      @for (g of groupOptgroups(); track g.groupId) {
        <optgroup [label]="'Group Scenarios: ' + g.groupName">
          @for (s of g.scenarios; track s.path) {
            <option [value]="GROUP_SCENARIO_PREFIX + g.groupId + ':' + s.path">
              {{ fileOptionLabel(s, ' (group default)') }}
            </option>
          }
        </optgroup>
      }
      @if (characterScenarioList().length > 0) {
        <optgroup label="Character Scenarios">
          @for (s of characterScenarioList(); track s.id) {
            <option [value]="s.id">{{ characterOptionLabel(s) }}</option>
          }
        </optgroup>
      }
    </select>
  `,
})
export class ScenarioSelect {
  /** v4 `id` — the `<select>`'s DOM id (both surfaces name theirs). */
  readonly selectId = input<string | undefined>(undefined);
  readonly selection = input.required<ScenarioSelection>();
  /** v4 `onChange` — reports the newly parsed selection. */
  readonly selectionChange = output<ScenarioSelection>();

  readonly projectScenarios = input<readonly ProjectScenarioOption[]>([]);
  readonly generalScenarios = input<readonly GeneralScenarioOption[]>([]);
  readonly groupScenarios = input<readonly GroupScenarioOption[]>([]);
  /**
   * Character scenarios, offered only when a single LLM character is in play —
   * with two characters present there is no unambiguous "the character". A
   * `null` (v4's default) and an empty list render identically; the surfaces
   * pass `null` deliberately, so the distinction is preserved on the input.
   */
  readonly characterScenarios = input<readonly CharacterScenario[] | null>(null);
  /** Marks the character scenario the character itself calls default. */
  readonly characterDefaultScenarioId = input<string | null>(null);
  readonly disabled = input(false, { transform: booleanAttribute });
  /** v4 `className`, same default. */
  readonly selectClass = input('qt-select mb-2');
  /** v4 `aria-label`. */
  readonly ariaLabel = input<string | undefined>(undefined);
  /**
   * Offer "Browse all scenarios…" (v4: present only when the surface passes
   * `onBrowse`). An Angular `output()` cannot be tested for a subscriber, so
   * the entry rides this flag (P4.D302 L-A); both surfaces set it.
   */
  readonly offerBrowse = input(false, { transform: booleanAttribute });
  /** v4 `onBrowse` — emitted INSTEAD of {@link selectionChange} for the browse entry. */
  readonly browse = output<void>();

  protected readonly CUSTOM_SCENARIO_VALUE = CUSTOM_SCENARIO_VALUE;
  protected readonly BROWSE_SCENARIO_VALUE = BROWSE_SCENARIO_VALUE;
  protected readonly PROJECT_SCENARIO_PREFIX = PROJECT_SCENARIO_PREFIX;
  protected readonly GENERAL_SCENARIO_PREFIX = GENERAL_SCENARIO_PREFIX;
  protected readonly GROUP_SCENARIO_PREFIX = GROUP_SCENARIO_PREFIX;

  private readonly selectRef = viewChild<ElementRef<HTMLSelectElement>>('scenarioSelect');

  protected readonly currentValue = computed(() => scenarioSelectionToValue(this.selection()));
  // Every optgroup lists its entries in title order (leading "The" ignored), the
  // same order the server's list already comes in — so a freshly saved scenario
  // lands in its place rather than at the end.
  protected readonly sortedProjectScenarios = computed(() =>
    sortScenarios(this.projectScenarios(), storeSortFields),
  );
  protected readonly sortedGeneralScenarios = computed(() =>
    sortScenarios(this.generalScenarios(), storeSortFields),
  );
  protected readonly characterScenarioList = computed(() =>
    sortScenarios(this.characterScenarios() ?? [], characterSortFields),
  );
  /** The one pick the "Elsewhere" optgroup holds, when the selection is from elsewhere. */
  protected readonly storeSelection = computed(() => {
    const selection = this.selection();
    return selection.kind === 'store' ? selection : null;
  });

  /** v4's `groupScenariosByGroup` Map — insertion order, first sighting wins the name. */
  protected readonly groupOptgroups = computed<GroupOptgroup[]>(() => {
    const groups = new Map<string, GroupOptgroup>();
    for (const scenario of this.groupScenarios()) {
      if (!groups.has(scenario.groupId)) {
        groups.set(scenario.groupId, {
          groupId: scenario.groupId,
          groupName: scenario.groupName,
          scenarios: [],
        });
      }
      groups.get(scenario.groupId)!.scenarios.push(scenario);
    }
    // The GROUP order stays first-sighting; only the entries inside each sort.
    for (const group of groups.values()) {
      group.scenarios = sortScenarios(group.scenarios, storeSortFields);
    }
    return [...groups.values()];
  });

  constructor() {
    afterRenderEffect(() => {
      // Read every source the option list depends on, so the assignment re-runs
      // whenever the rows move under a selection that was already set.
      this.currentValue();
      this.sortedProjectScenarios();
      this.sortedGeneralScenarios();
      this.groupOptgroups();
      this.characterScenarioList();
      const select = this.selectRef()?.nativeElement;
      if (!select) return;
      select.value = select.dataset['qtValue'] ?? '';
    });
  }

  /** v4 `:104-113` — name, default marker, ARCHIVED marker, then the description. */
  protected fileOptionLabel(
    s: ProjectScenarioOption | GeneralScenarioOption | GroupScenarioOption,
    defaultSuffix: string,
  ): string {
    return `${s.name}${s.isDefault ? defaultSuffix : ''}${s.archived ? ARCHIVED_OPTION_SUFFIX : ''}${s.description ? ` — ${s.description}` : ''}`;
  }

  protected characterOptionLabel(s: CharacterScenario): string {
    const isDefault = this.characterDefaultScenarioId() === s.id;
    return `${s.title}${isDefault ? ' (character default)' : ''}${s.archived ? ARCHIVED_OPTION_SUFFIX : ''}${s.description ? ` — ${s.description}` : ''}`;
  }

  protected onChange(event: Event): void {
    const select = event.target as HTMLSelectElement;
    const value = select.value;
    // The browse entry is an action, not a selection. v4's controlled value
    // snaps back on React's next render; here no tracked signal moves, so the
    // `afterRenderEffect` above would never re-run — restore it explicitly,
    // BEFORE the browser opens (P4.D302 L-B).
    if (value === BROWSE_SCENARIO_VALUE) {
      select.value = this.currentValue();
      this.browse.emit();
      return;
    }
    this.selectionChange.emit(scenarioValueToSelection(value, this.selection()));
  }
}
