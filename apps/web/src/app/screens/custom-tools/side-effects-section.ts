import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  Injector,
  afterNextRender,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
  untracked,
} from '@angular/core';

import { CoreClient } from '../../core/core-client';
import type { CustomToolDestinations } from '../../core/core-contract';

import {
  MAX_EFFECTS,
  MAX_EFFECT_TARGET_LENGTH,
  MAX_MESSAGE_LENGTH,
  type OutcomeState,
} from '../../pascal/custom-tool-types';
import {
  newDraftEffect,
  type DraftEffect,
  type DraftInventoryGesture,
  type DraftIssue,
  type DraftWardrobeGesture,
  type ToolDraft,
  type WardrobeVantage,
} from '../../pascal/tool-draft';
import { Icon } from '../../ui/icon';
import {
  WardrobeReferencePicker,
  type WardrobeReferenceForm,
} from '../../wardrobe/wardrobe-reference-picker';
import { WardrobeVantageItemsLoader } from '../../wardrobe/wardrobe-vantage-items';
import { InventoryEffectFields } from './inventory-effect-fields';
import { inventoryLabel } from '../../inventory/inventory-display';
import { InventoryReferencePicker, type InventoryReferenceForm } from '../../inventory/inventory-reference-picker';
import { InventoryVantageItemsLoader } from '../../inventory/inventory-vantage-items';
import { WardrobeFields, slotIsCoupled } from './wardrobe-fields';
import {
  WardrobeVantageSelect,
  encodeWardrobeVantage,
  wardrobeVantageName,
  type VantageBrowsing,
} from './wardrobe-vantage-select';

/** Which item row the picker is filling, and the item it currently names. */
interface PickTarget {
  effectId: string;
  index: number;
  highlightId: string | null;
}

const NO_VANTAGE: WardrobeVantage = { kind: 'none' };

/**
 * SideEffectsSection — the Workbench's "Side effects" card, between the
 * builder form and the outcome table: what a run writes back once the deal is
 * done (v4 `components/custom-tools/SideEffectsSection.tsx`, 428 at `25b8fab6a`).
 *
 * Each row is one of four kinds (v4 `f0db8cbb8`, `e1872c94f`, `e4a60ad9e`). A
 * WRITE has a target and a value that is a literal number, a literal boolean,
 * or an expression. A WARDROBE row changes what the rolling character has on:
 * an action (wear / take off / clear a slot), a short list of item references
 * (title or id, templated), and the mode and slot where the action wants them
 * (`./wardrobe-fields`). An INVENTORY row changes what they carry: equip,
 * unequip or pick up a short list of object references
 * (`./inventory-effect-fields`). A LOG row writes one templated line (like an
 * outcome message) to the instance's log, recorded against this chat and the
 * rolling character, where log_grep and log_read find it. Switching kind keeps
 * the other halves on the draft until save, so nothing typed is lost.
 *
 * The When is the common shapes only: "Always" or "on a given outcome
 * state". A hand-written richer condition (comparators,
 * metadata, the oracle) is carried verbatim behind a read-only badge — the
 * `$state`-default precedent: the builder round-trips what it cannot author.
 * Everything else arrives as issues from `validateDraft` and renders inline.
 *
 * A wardrobe row's item references are read through a VANTAGE — whose
 * wardrobe the author is browsing (editor state, never saved; v4
 * `5cb0e0190`). Each reference resolves by the one rule (`resolveWearableRef`)
 * against that pool and renders as a chip with the garment's picture, slots,
 * origin and reach; a templated reference keeps its text box; an unresolved one
 * keeps its text box under an advisory, never an error — the lens is partial and
 * the applier fail-soft. The *Choose…* picker fills a reference by title (or by
 * id, on request). Nothing is read until a wardrobe row exists.
 *
 * v4 offers the vantage select when an `onVantageChange` prop is bound; an
 * Angular output cannot be asked whether anyone listens, so the host says so
 * through `showVantage` (the editor sets it; P4.D320 item 11).
 */

/** The hint's `{{…}}` would close an Angular interpolation early — hence a constant. */
const EFFECT_VALUE_PLACEHOLDER = '{{state.encounter.count}} + 1';

const EFFECTS_HINT =
  'Effects apply in order, after the outcome is chosen. A JSON string value is always an ' +
  'expression — quote literal prose:';

const EFFECTS_HINT_TAIL =
  ', never bare words. Expressions take + − × ÷, parentheses, and the same {{…}} references as a ' +
  'message. An outfit change dresses or undresses the rolling character and nobody else; an item it ' +
  'cannot find is skipped and the rest still go on. An inventory change equips, sets down or picks up ' +
  'objects for the rolling character alone; an object it cannot find is skipped, and a gesture that ' +
  'would carry too many of a kind is skipped whole, before anything is made.';

/**
 * The log row's textarea placeholder and hint (v4 `e4a60ad9e`). Constants: both
 * carry `{{…}}`, which would open an Angular interpolation. The hint's
 * `{{state.path}}` keeps the leading space JSX gave it (`{' {{state.path}}'}`).
 */
const LOG_PLACEHOLDER = '{{character}} rolled {{roll}} against the lock.';
const LOG_HINT =
  'Plain prose, a template like an outcome message: {{value}}, {{roll}}, {{params.x}}, {{state.path}} and ' +
  'friends are filled in when the deal is done. The line is entered in the ledger against this chat and ' +
  'the rolling character, where the log_grep and log_read tools may find it again.';

/**
 * v4's third prefix title (`SideEffectsSection.tsx` at `25f534c0b`). A constant
 * rather than a literal attribute because it carries `<id>` / `<field>` angle
 * brackets and a typographic apostrophe, and the two siblings above read better
 * beside it than a wrapped literal would.
 */
const PROGRESS_PREFIX_TITLE =
  'Adjust one of the rolling character\u2019s timed progressions — progress.<id>.<field>. An id nobody authored is created; write true to progress.<id>.remove to delete one.';


const OUTCOME_STATES: readonly OutcomeState[] = ['success', 'partial', 'failure', 'info'] as const;

/** The When select's flat values; 'verbatim' never appears as an option. */
type WhenChoice = 'always' | OutcomeState;

export function whenChoiceOf(effect: DraftEffect): WhenChoice | 'verbatim' {
  if (effect.when.kind === 'always') return 'always';
  if (effect.when.kind === 'outcome-state') return effect.when.state;
  return 'verbatim';
}

@Component({
  selector: 'qt-side-effects-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    Icon,
    InventoryEffectFields,
    InventoryReferencePicker,
    WardrobeFields,
    WardrobeReferencePicker,
    WardrobeVantageSelect,
  ],
  template: `
    <section class="qt-card p-4 space-y-3">
      <div class="flex items-center justify-between">
        <h2 class="qt-card-title text-sm">Side effects</h2>
        <button
          type="button"
          class="qt-button qt-button-secondary qt-button-sm"
          (click)="addEffect()"
          [disabled]="disabled() || effects().length >= MAX_EFFECTS"
          [attr.title]="effects().length >= MAX_EFFECTS ? atMostTitle : null"
        >
          <qt-icon name="plus" class="w-3.5 h-3.5" />
          Add effect ({{ effects().length }}/{{ MAX_EFFECTS }})
        </button>
      </div>

      @if ((hasWardrobe() || hasInventory()) && showVantage()) {
        <qt-wardrobe-vantage-select
          [vantage]="vantage()"
          (vantageChange)="vantageChange.emit($event)"
          [destinations]="destinations()"
          [homeProjectName]="homeProjectName()"
          [browsing]="browsing()"
          [disabled]="disabled()"
        />
      }

      @if (effects().length === 0) {
        <p class="text-xs qt-text-secondary">
          None declared. An effect lets a roll record its consequences &mdash; tick a counter in the
          story&rsquo;s state, note a fact on the rolling character&rsquo;s sheet, put a garment on
          them or take one off, hand them an object or take one away &mdash; applied server-side as
          part of the deal, beyond any model&rsquo;s power to fudge.
        </p>
      }

      @for (effect of effects(); track effect.id) {
        <div class="qt-card p-3 space-y-2" [attr.data-effect-id]="effect.id">
          <div class="flex items-center gap-2 flex-wrap text-sm">
            <label class="flex items-center gap-2">
              Kind
              <select
                [value]="effect.kind"
                (change)="onKindChange(effect.id, selectValue($event))"
                [disabled]="disabled()"
                class="qt-select qt-select-sm"
                aria-label="Effect kind"
              >
                <option value="write" [selected]="effect.kind === 'write'">Write a value</option>
                <option value="wardrobe" [selected]="effect.kind === 'wardrobe'">Change the outfit</option>
                <option value="inventory" [selected]="effect.kind === 'inventory'">
                  Change the inventory
                </option>
                <option value="log" [selected]="effect.kind === 'log'">Enter a log line</option>
              </select>
            </label>
            <label class="flex items-center gap-2">
              When
              @if (whenChoice(effect) === 'verbatim') {
                <span
                  class="text-xs qt-text-secondary font-mono rounded qt-bg-muted px-2 py-1"
                  title="This condition is richer than the form authors. Edit it in the raw JSON."
                  >{{ verbatimWhen(effect) }}</span
                >
              } @else {
                <select
                  [value]="whenChoice(effect)"
                  (change)="onWhenChange(effect.id, selectValue($event))"
                  [disabled]="disabled()"
                  class="qt-select qt-select-sm"
                  aria-label="When this effect applies"
                >
                  <option value="always">every run</option>
                  @for (state of OUTCOME_STATES; track state) {
                    <option [value]="state">outcome is {{ state }}</option>
                  }
                </select>
              }
            </label>
            <button
              type="button"
              class="qt-button qt-button-ghost qt-button-sm ml-auto"
              (click)="removeEffect(effect.id)"
              [disabled]="disabled()"
              title="Delete this effect"
            >
              <qt-icon name="trash" class="w-4 h-4" />
            </button>
          </div>

          @if (effect.kind === 'log') {
          <div class="space-y-1 text-sm">
            <label class="flex flex-col gap-1">
              Log line
              <textarea
                [value]="effect.log"
                [maxLength]="MAX_MESSAGE_LENGTH"
                rows="2"
                (input)="updateEffect(effect.id, { log: textValue($event) })"
                [placeholder]="LOG_PLACEHOLDER"
                [disabled]="disabled()"
                class="qt-textarea w-full"
                [class.qt-input-error]="effectErrors(effect.id).length > 0"
                aria-label="Log line"
              ></textarea>
            </label>
            <p class="qt-hint">{{ LOG_HINT }}</p>
          </div>
          } @else if (effect.kind === 'inventory') {
          <qt-inventory-effect-fields
            [gesture]="effect.inventory"
            [hasErrors]="effectErrors(effect.id).length > 0"
            [disabled]="disabled()"
            (gestureChange)="onInventoryChange(effect.id, $event)"
            [pool]="bag.pool()"
            [poolFetched]="bag.fetched()"
            [vantage]="vantage()"
            [vantageName]="vantageName()"
            (choose)="onChoose(effect.id, $event)"
          />
          } @else if (effect.kind === 'wardrobe') {
          <qt-wardrobe-fields
            [gesture]="effect.wardrobe"
            [hasErrors]="effectErrors(effect.id).length > 0"
            [disabled]="disabled()"
            (gestureChange)="onGestureChange(effect.id, $event)"
            [pool]="loader.items()"
            [poolFetched]="loader.fetched()"
            [vantage]="vantage()"
            [vantageName]="vantageName()"
            [characterName]="characterName()"
            (choose)="onChoose(effect.id, $event)"
          />
          } @else {
          <div class="flex items-center gap-2 flex-wrap text-sm">
            <label class="flex items-center gap-2 flex-1 min-w-56">
              Target
              <input
                type="text"
                [value]="effect.target"
                [maxLength]="MAX_EFFECT_TARGET_LENGTH"
                (input)="updateEffect(effect.id, { target: inputValue($event) })"
                placeholder="state.encounter.count"
                [disabled]="disabled()"
                class="qt-input flex-1 font-mono"
                [class.qt-input-error]="effectErrors(effect.id).length > 0"
                aria-label="Effect target"
              />
            </label>
            @if (effect.target.trim() === '') {
              <span class="flex gap-1">
                <button
                  type="button"
                  class="qt-button qt-button-ghost qt-button-sm font-mono"
                  (click)="updateEffect(effect.id, { target: 'state.' })"
                  [disabled]="disabled()"
                  title="Write into tiered persistent state, at the tier where the key already lives"
                >
                  state.
                </button>
                <button
                  type="button"
                  class="qt-button qt-button-ghost qt-button-sm font-mono"
                  (click)="updateEffect(effect.id, { target: 'metadata.' })"
                  [disabled]="disabled()"
                  title="Write onto the rolling character's own fact sheet (metadata.json)"
                >
                  metadata.
                </button>
                <button
                  type="button"
                  class="qt-button qt-button-ghost qt-button-sm font-mono"
                  (click)="updateEffect(effect.id, { target: 'progress.' })"
                  [disabled]="disabled()"
                  [title]="PROGRESS_PREFIX_TITLE"
                >
                  progress.
                </button>
              </span>
            }
          </div>

          <div class="flex items-center gap-2 flex-wrap text-sm">
            <label class="flex items-center gap-2">
              Value
              <select
                [value]="effect.valueKind"
                (change)="onValueKindChange(effect, selectValue($event))"
                [disabled]="disabled()"
                class="qt-select qt-select-sm"
                aria-label="Value kind"
              >
                <option value="expression">expression</option>
                <option value="literal-number">number</option>
                <option value="literal-boolean">true/false</option>
              </select>
            </label>
            @if (effect.valueKind === 'literal-boolean') {
              <select
                [value]="effect.value === 'true' ? 'true' : 'false'"
                (change)="updateEffect(effect.id, { value: selectValue($event) })"
                [disabled]="disabled()"
                class="qt-select qt-select-sm"
                aria-label="Boolean value"
              >
                <option value="true">true</option>
                <option value="false">false</option>
              </select>
            } @else {
              <input
                [type]="effect.valueKind === 'literal-number' ? 'number' : 'text'"
                step="any"
                [value]="effect.value"
                (input)="updateEffect(effect.id, { value: inputValue($event) })"
                [placeholder]="
                  effect.valueKind === 'literal-number' ? '1' : EFFECT_VALUE_PLACEHOLDER
                "
                [disabled]="disabled()"
                class="qt-input flex-1 min-w-48 font-mono"
                [class.qt-input-error]="effectErrors(effect.id).length > 0"
                aria-label="Effect value"
              />
            }
          </div>
          }

          @for (message of effectErrors(effect.id); track message) {
            <p class="text-xs qt-text-destructive">{{ message }}</p>
          }
          @for (message of effectWarnings(effect.id); track message) {
            <p class="text-xs qt-text-secondary">⚠ {{ message }}</p>
          }
        </div>
      }

      <p class="qt-hint">
        {{ EFFECTS_HINT }} <code class="font-mono">'broken pick'</code>{{ EFFECTS_HINT_TAIL }}
      </p>

      <qt-inventory-reference-picker
        [open]="inventoryPickerOpen()"
        (closed)="onPickerClosed()"
        [vantage]="vantage()"
        [action]="inventoryPickerAction()"
        [highlightId]="picking()?.highlightId ?? null"
        [reference]="inventoryReference()"
        (referenceChange)="inventoryReference.set($event)"
        (picked)="onInventoryPicked($event.item, $event.reference)"
      >
        @if (showVantage()) {
          <qt-wardrobe-vantage-select
            qtPickerHeader
            [vantage]="vantage()"
            (vantageChange)="vantageChange.emit($event)"
            [destinations]="destinations()"
            [homeProjectName]="homeProjectName()"
            browsing="inventory"
          />
        }
      </qt-inventory-reference-picker>

      <qt-wardrobe-reference-picker
        [open]="pickerOpen()"
        (closed)="onPickerClosed()"
        [vantage]="vantage()"
        [characterName]="characterName()"
        [action]="pickerAction()"
        [slot]="pickerSlot()"
        [highlightId]="picking()?.highlightId ?? null"
        [reference]="referenceForm()"
        (referenceChange)="referenceForm.set($event)"
        (picked)="onPicked($event.item, $event.reference)"
      >
        @if (showVantage()) {
          <qt-wardrobe-vantage-select
            qtPickerHeader
            [vantage]="vantage()"
            (vantageChange)="vantageChange.emit($event)"
            [destinations]="destinations()"
            [homeProjectName]="homeProjectName()"
          />
        }
      </qt-wardrobe-reference-picker>
    </section>
  `,
})
export class SideEffectsSection {
  readonly draft = input.required<ToolDraft>();
  readonly issues = input.required<DraftIssue[]>();
  readonly disabled = input(false);
  /** Whose wardrobe the item references are read against. Default: none. */
  readonly vantage = input<WardrobeVantage>(NO_VANTAGE);
  /** Offer the vantage select (v4: an `onVantageChange` listener is bound). */
  readonly showVantage = input(false);
  /** The destinations payload — names for characters, projects and groups. */
  readonly destinations = input<CustomToolDestinations | undefined>(undefined);
  /** The project whose store holds this file, if any (named in the vantage hint). */
  readonly homeProjectName = input<string | null>(null);

  readonly draftChange = output<ToolDraft>();
  readonly vantageChange = output<WardrobeVantage>();

  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef);
  private readonly injector = inject(Injector);
  protected readonly loader = new WardrobeVantageItemsLoader(inject(CoreClient));
  /** The Inventory a vantage looks at — read only once an inventory row exists (P4.D330's loader). */
  protected readonly bag = new InventoryVantageItemsLoader(inject(CoreClient));

  protected readonly hasWardrobe = computed(() =>
    this.draft().effects.some((effect) => effect.kind === 'wardrobe'),
  );
  protected readonly hasInventory = computed(() =>
    this.draft().effects.some((effect) => effect.kind === 'inventory'),
  );
  /** What the shared vantage select is browsing for (v4 `SideEffectsSection.tsx:179-188`). */
  protected readonly browsing = computed<VantageBrowsing>(() =>
    this.hasWardrobe() && this.hasInventory() ? 'both' : this.hasInventory() ? 'inventory' : 'wardrobe',
  );
  protected readonly vantageName = computed(() =>
    wardrobeVantageName(this.vantage(), this.destinations()),
  );
  protected readonly characterName = computed(() =>
    this.vantage().kind === 'character' ? this.vantageName() : null,
  );
  /** The picker's title / id switch — lasts the editor session. */
  protected readonly referenceForm = signal<WardrobeReferenceForm>('title');
  /** The Inventory picker's name-or-id switch — lasts the editor session (default: by label). */
  protected readonly inventoryReference = signal<InventoryReferenceForm>('label');
  protected readonly picking = signal<PickTarget | null>(null);
  private readonly pickingEffect = computed(() => {
    const picking = this.picking();
    return picking ? this.draft().effects.find((effect) => effect.id === picking.effectId) : undefined;
  });
  protected readonly pickerOpen = computed(
    () => this.picking() !== null && this.pickingEffect()?.kind === 'wardrobe',
  );
  protected readonly inventoryPickerOpen = computed(
    () => this.picking() !== null && this.pickingEffect()?.kind === 'inventory',
  );
  protected readonly inventoryPickerAction = computed(
    () => this.pickingEffect()?.inventory.action ?? 'equip',
  );
  protected readonly pickerAction = computed(() => {
    const action = this.pickingEffect()?.wardrobe.action;
    return action === 'take_off' ? 'take_off' : action === 'wear' ? 'wear' : undefined;
  });
  protected readonly pickerSlot = computed(() => {
    const effect = this.pickingEffect();
    return effect && slotIsCoupled(effect.wardrobe) ? effect.wardrobe.slot || null : null;
  });

  /** The element that opened the picker, so focus can go home on close. */
  private opener: HTMLElement | null = null;

  /** Read nothing until a wardrobe row exists — keyed so a draft edit re-reads nothing. */
  private readonly loadKey = computed(() =>
    this.hasWardrobe() ? encodeWardrobeVantage(this.vantage()) : null,
  );

  /** Likewise the Inventory: nothing is read until an inventory row exists. */
  private readonly bagKey = computed(() =>
    this.hasInventory() ? encodeWardrobeVantage(this.vantage()) : null,
  );

  constructor() {
    effect(() => {
      this.loadKey();
      const vantage = this.hasWardrobe() ? this.vantage() : null;
      untracked(() => void this.loader.load(vantage));
    });
    effect(() => {
      this.bagKey();
      const vantage = this.hasInventory() ? this.vantage() : null;
      untracked(() => void this.bag.load(vantage));
    });
  }

  protected onChoose(effectId: string, target: { index: number; highlightId: string | null }): void {
    this.opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    this.picking.set({ effectId, index: target.index, highlightId: target.highlightId });
  }

  protected onPicked(
    item: { id: string; title: string },
    reference: WardrobeReferenceForm,
  ): void {
    const picking = this.picking();
    const effect = this.pickingEffect();
    if (!picking || !effect) return;
    const gesture = effect.wardrobe;
    const items = gesture.items.length > 0 ? gesture.items : [''];
    const value = reference === 'id' ? item.id : item.title;
    this.updateEffect(effect.id, {
      wardrobe: { ...gesture, items: items.map((ref, i) => (i === picking.index ? value : ref)) },
    });
  }

  protected onInventoryPicked(
    item: Parameters<typeof inventoryLabel>[0] & { id: string },
    reference: InventoryReferenceForm,
  ): void {
    const picking = this.picking();
    const effect = this.pickingEffect();
    if (!picking || !effect) return;
    const gesture = effect.inventory;
    const rows = gesture.items.length > 0 ? gesture.items : [''];
    const value = reference === 'id' ? item.id : inventoryLabel(item);
    this.updateEffect(effect.id, {
      inventory: { ...gesture, items: rows.map((ref, i) => (i === picking.index ? value : ref)) },
    });
  }

  /**
   * Close the picker and send focus home — to the control that opened it or,
   * when a pick turned that row into a chip, to the row's chip.
   */
  protected onPickerClosed(): void {
    const picking = this.picking();
    const opener = this.opener;
    this.picking.set(null);
    this.opener = null;
    afterNextRender(
      () => {
        if (opener?.isConnected) {
          opener.focus();
          return;
        }
        if (!picking) return;
        const kind = this.effects().find((e) => e.id === picking.effectId)?.kind;
        const chip = this.host.nativeElement.querySelector<HTMLElement>(
          kind === 'inventory'
            ? `[data-effect-id="${picking.effectId}"] [data-testid="inventory-reference-chip"][aria-label^="Object ${picking.index + 1}: "]`
            : `[data-effect-id="${picking.effectId}"] [data-testid="wardrobe-reference-chip"][aria-label^="Item ${picking.index + 1}: "]`,
        );
        chip?.focus();
      },
      { injector: this.injector },
    );
  }

  protected readonly MAX_EFFECTS = MAX_EFFECTS;
  protected readonly MAX_EFFECT_TARGET_LENGTH = MAX_EFFECT_TARGET_LENGTH;
  protected readonly MAX_MESSAGE_LENGTH = MAX_MESSAGE_LENGTH;
  protected readonly LOG_PLACEHOLDER = LOG_PLACEHOLDER;
  protected readonly LOG_HINT = LOG_HINT;
  protected readonly PROGRESS_PREFIX_TITLE = PROGRESS_PREFIX_TITLE;
  protected readonly OUTCOME_STATES = OUTCOME_STATES;
  protected readonly EFFECT_VALUE_PLACEHOLDER = EFFECT_VALUE_PLACEHOLDER;
  protected readonly EFFECTS_HINT = EFFECTS_HINT;
  protected readonly EFFECTS_HINT_TAIL = EFFECTS_HINT_TAIL;
  protected readonly atMostTitle = `At most ${MAX_EFFECTS} effects`;

  readonly effects = computed(() => this.draft().effects);

  protected whenChoice(effect: DraftEffect): WhenChoice | 'verbatim' {
    return whenChoiceOf(effect);
  }

  /** The verbatim condition's JSON, for the read-only badge. */
  protected verbatimWhen(effect: DraftEffect): string {
    return effect.when.kind === 'verbatim' ? JSON.stringify(effect.when.when) : '';
  }

  private setEffects(next: DraftEffect[]): void {
    this.draftChange.emit({ ...this.draft(), effects: next });
  }

  updateEffect(id: string, partial: Partial<DraftEffect>): void {
    this.setEffects(
      this.effects().map((effect) => (effect.id === id ? { ...effect, ...partial } : effect)),
    );
  }

  protected onWhenChange(id: string, value: string): void {
    const choice = value as WhenChoice;
    this.updateEffect(id, {
      when: choice === 'always' ? { kind: 'always' } : { kind: 'outcome-state', state: choice },
    });
  }

  /**
   * Switch a row's kind. BOTH halves are kept (v4: "switching kind keeps the
   * other half on the draft until save"); the effects are read once into a
   * local before emitting.
   */
  protected onKindChange(id: string, raw: string): void {
    const kind = raw as DraftEffect['kind'];
    const current = this.effects();
    this.setEffects(current.map((effect) => (effect.id === id ? { ...effect, kind } : effect)));
  }

  protected onGestureChange(id: string, wardrobe: DraftWardrobeGesture): void {
    this.updateEffect(id, { wardrobe });
  }

  protected onInventoryChange(id: string, inventory: DraftInventoryGesture): void {
    this.updateEffect(id, { inventory });
  }

  protected onValueKindChange(effect: DraftEffect, raw: string): void {
    const valueKind = raw as DraftEffect['valueKind'];
    this.updateEffect(effect.id, {
      valueKind,
      value:
        valueKind === 'literal-boolean'
          ? 'true'
          : valueKind === 'literal-number'
            ? ''
            : effect.value,
    });
  }

  addEffect(): void {
    this.setEffects([...this.effects(), newDraftEffect()]);
  }

  removeEffect(id: string): void {
    this.setEffects(this.effects().filter((effect) => effect.id !== id));
  }

  effectErrors(id: string): string[] {
    return this.issues()
      .filter(
        (issue) =>
          issue.severity === 'error' && issue.where.section === 'effect' && issue.where.id === id,
      )
      .map((issue) => issue.message);
  }

  effectWarnings(id: string): string[] {
    return this.issues()
      .filter(
        (issue) =>
          issue.severity === 'warning' && issue.where.section === 'effect' && issue.where.id === id,
      )
      .map((issue) => issue.message);
  }

  protected inputValue(event: Event): string {
    return (event.target as HTMLInputElement).value;
  }

  protected selectValue(event: Event): string {
    return (event.target as HTMLSelectElement).value;
  }

  protected textValue(event: Event): string {
    return (event.target as HTMLTextAreaElement).value;
  }
}
