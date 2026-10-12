import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

import type { WardrobeSlotType } from '../../core/core-contract';
import { GROUP_LABEL, GROUP_ORDER } from './constants';
import { isComposite } from '../composite-types';
import { formatSlotLabels } from '../slot-meta';
import { SlotBadge, SlotCheckboxGroup } from '../slot-ui';
import { wardrobeOriginLabel } from '../wardrobe-container';
import type { CandidateGroup, CandidateItem } from './types';

/**
 * The bundle-mode components section (v4
 * `components/wardrobe/wardrobe-item-editor/WardrobeComponentPicker.tsx`): the
 * auto-derived coverage badges, the currently-selected component chips, the
 * searchable grouped candidate list, and the slot designation. Purely
 * presentational — all state lives in the parent WardrobeItemEditor.
 *
 * At the pin `4a38222e6` (`9ecebf986`): slots print through `slot-ui`'s
 * labels, an archived candidate's meta ends ` · archived`, and the slot
 * designation is ALWAYS shown — every composite covers its components'
 * slots plus the ones it designates (bug 190), so an additive outfit may
 * designate too; only the sentence above it depends on `replace`.
 */
@Component({
  selector: 'qt-wardrobe-component-picker',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [SlotBadge, SlotCheckboxGroup],
  template: `
    <div>
      <div class="flex items-center justify-between mb-1 gap-2">
        <span class="qt-label">
          Components <span class="qt-text-xs qt-text-secondary">(auto types)</span>
        </span>
        <div class="flex items-center gap-1 flex-wrap">
          @if (effectiveTypes().length === 0) {
            <span class="qt-text-xs qt-text-secondary italic">no slots covered yet</span>
          } @else {
            @for (t of effectiveTypes(); track t) {
              <qt-slot-badge [slot]="t" />
            }
          }
        </div>
      </div>
      <p class="qt-text-xs qt-text-small mb-2">Pick the items this outfit bundles together.</p>

      @if (selectedComponents().length > 0) {
        <div class="mb-2">
          <p class="qt-text-xs qt-text-secondary mb-1">Currently in this outfit:</p>
          <div class="flex flex-wrap gap-2">
            @for (c of selectedComponents(); track c.id) {
              <span
                class="inline-flex items-center gap-1 rounded-full qt-bg-muted border qt-border-default px-2 py-0.5 qt-text-xs"
              >
                {{ c.title }}
                @if (originLabelOf(c); as originLabel) {
                  <span class="qt-badge qt-badge-wardrobe-shared ml-1">{{ originLabel }}</span>
                }
                <button
                  type="button"
                  [attr.aria-label]="'Remove ' + c.title"
                  (click)="toggleComponent.emit(c.id)"
                  class="qt-text-secondary hover:text-foreground"
                >
                  ×
                </button>
              </span>
            }
          </div>
        </div>
      }

      <input
        type="search"
        [value]="componentSearch()"
        (input)="componentSearchChange.emit($any($event.target).value)"
        (blur)="componentsBlur.emit()"
        placeholder="Search items to add as components…"
        class="qt-input mb-2"
      />

      <div class="max-h-64 overflow-y-auto rounded border qt-border-default qt-bg-muted/40">
        @if (candidatesLoading()) {
          <div class="px-3 py-2 qt-text-small qt-text-secondary">Loading…</div>
        } @else if (eligibleCandidates().length === 0) {
          <div class="px-3 py-2 qt-text-small qt-text-secondary">
            {{
              candidates().length === 0
                ? 'No other wardrobe items to bundle.'
                : 'No candidates match your filter.'
            }}
          </div>
        } @else {
          @for (group of groupOrder; track group) {
            @if ((groupedCandidates().get(group) ?? []).length > 0) {
              <div>
                <button
                  type="button"
                  (click)="toggleGroup.emit(group)"
                  class="w-full flex items-center justify-between px-3 py-1.5 qt-bg-muted/60 hover:qt-bg-muted text-sm font-medium qt-text-primary"
                >
                  <span class="flex items-center gap-2">
                    <span aria-hidden="true">{{ expandedGroups().has(group) ? '▼' : '▶' }}</span>
                    <span>{{ groupLabel[group] }}</span>
                    <span class="qt-text-xs qt-text-secondary">
                      ({{ (groupedCandidates().get(group) ?? []).length }})
                    </span>
                  </span>
                </button>
                @if (expandedGroups().has(group)) {
                  <ul class="divide-y qt-border-default">
                    @for (c of groupedCandidates().get(group) ?? []; track c.id) {
                      <li>
                        <!-- v4 cc80dc89d :163-183 — the title wraps; the origin
                             chip is a SIBLING outside it. -->
                        <label
                          class="flex items-start gap-2 px-3 py-2 cursor-pointer hover:qt-bg-muted"
                        >
                          <input
                            type="checkbox"
                            class="qt-checkbox mt-0.5"
                            [checked]="componentItemIds().includes(c.id)"
                            (change)="toggleComponent.emit(c.id)"
                          />
                          <span class="min-w-0 flex-1 break-words text-sm text-foreground">{{
                            c.title
                          }}</span>
                          @if (originLabelOf(c); as originLabel) {
                            <span class="qt-badge qt-badge-wardrobe-shared shrink-0">{{
                              originLabel
                            }}</span>
                          }
                          <span
                            class="shrink-0 max-w-[45%] text-right qt-text-xs qt-text-secondary"
                            >{{ formatSlotLabels(c.types) }}{{ isComposite(c) ? ' · bundle' : ''
                            }}{{ c.archived ? ' · archived' : '' }}</span
                          >
                        </label>
                      </li>
                    }
                  </ul>
                }
              </div>
            }
          }
        }
      </div>
      @if (showComponentsError()) {
        <p class="mt-1 text-xs qt-text-destructive">Add at least one component</p>
      }

      <!-- Composite equip behaviour: additive (default) vs replace. -->
      <div class="mt-3 border-t qt-border-default pt-3">
        <label
          class="inline-flex items-start gap-2 cursor-pointer"
          title="Check this if the outfit should replace everything in its designated slots."
        >
          <input
            type="checkbox"
            class="qt-checkbox mt-0.5"
            [checked]="replace()"
            (change)="replaceChange.emit($any($event.target).checked)"
          />
          <span class="text-sm text-foreground">
            Replace everything in its designated slots
            <span class="block qt-text-xs qt-text-muted">
              Off by default this outfit layers onto whatever's already worn. Check it to clear
              its designated slots first.
            </span>
          </span>
        </label>

        <!-- The slots this outfit covers. Its components' slots are always
             in; more may be designated — a replace outfit clears them all
             (v4 9ecebf986 :215-231). -->
        <div class="mt-2">
          <p class="qt-text-xs qt-text-secondary mb-1">
            {{
              replace()
                ? 'Slots this outfit clears (its components’ slots are always included):'
                : 'Slots this outfit covers (its components’ slots are always included):'
            }}
          </p>
          <qt-slot-checkbox-group
            [value]="effectiveTypes()"
            [locked]="computedTypes()"
            lockedTitle="Covered by a component — always included"
            (toggle)="toggleType.emit($event)"
          />
        </div>
      </div>
    </div>
  `,
})
export class WardrobeComponentPicker {
  readonly effectiveTypes = input.required<WardrobeSlotType[]>();
  readonly selectedComponents = input.required<CandidateItem[]>();
  readonly componentSearch = input.required<string>();
  readonly candidatesLoading = input.required<boolean>();
  /** Full candidate list, for the empty-state message ("none to bundle" vs "no match"). */
  readonly candidates = input.required<CandidateItem[]>();
  readonly eligibleCandidates = input.required<CandidateItem[]>();
  readonly groupedCandidates = input.required<Map<CandidateGroup, CandidateItem[]>>();
  readonly expandedGroups = input.required<Set<CandidateGroup>>();
  readonly componentItemIds = input.required<string[]>();
  readonly replace = input.required<boolean>();
  readonly computedTypes = input.required<WardrobeSlotType[]>();
  readonly showComponentsError = input.required<boolean>();

  readonly componentSearchChange = output<string>();
  readonly componentsBlur = output<void>();
  readonly toggleComponent = output<string>();
  readonly toggleGroup = output<CandidateGroup>();
  readonly toggleType = output<WardrobeSlotType>();
  readonly replaceChange = output<boolean>();

  protected readonly groupOrder = GROUP_ORDER;
  protected readonly groupLabel = GROUP_LABEL;
  protected readonly formatSlotLabels = formatSlotLabels;
  protected readonly isComposite = isComposite;

  protected originLabelOf(c: CandidateItem): string | null {
    return wardrobeOriginLabel(c.origin);
  }
}
