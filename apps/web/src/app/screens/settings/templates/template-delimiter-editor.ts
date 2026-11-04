import { ChangeDetectionStrategy, Component, model } from '@angular/core';

import { TemplateAddOnsEditor } from './template-add-ons-editor';
import {
  DEFAULT_TAG_TOKEN_PATTERN,
  STYLE_OPTIONS,
  emptyDelimiterEntry,
  type DelimiterFormEntry,
  type DelimiterKind,
  type FullAddOns,
} from './template-form';

/** Per-instance id seed for the label ↔ control pairs (v4 uses `useId()`). */
let delimiterEditorCounter = 0;

/**
 * The Formatting Delimiters editor (v4 `components/settings/roleplay-templates/
 * index.tsx` delimiter section): a list of kind-discriminated toolbar entries
 * (wrap / linePrefix / tagPrefix), each with a name, button label, style class,
 * kind-specific markers, a conceal toggle, and the flourish add-ons. Two-way
 * bound to the parent form's `entries` array; every mutation replaces the array
 * immutably (OnPush + signals). Copy is v4-verbatim.
 *
 * P4.D352 (v4 `778b97b2b` at 236c04e35): the flourishes are the shared
 * `qt-template-add-ons-editor` (v4 `AddOnsEditor`, also used for unmarked
 * text); a `wrap` entry gains the two checkboxes `matchCurlyQuotes` /
 * `closeOptional`; the style list leads with `Plain` from the ONE
 * `STYLE_OPTIONS` in `template-form.ts` (`qt-roleplay-plain` has no CSS rule,
 * by design — see there). The labels carry v4's `eed72d60c` pairing: a visible
 * `<label for>` + `id` where v4 has one, v4's `aria-label` where it has none.
 */
@Component({
  selector: 'qt-template-delimiter-editor',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [TemplateAddOnsEditor],
  template: `
    <div>
      <div class="flex items-center justify-between mb-2">
        <label class="qt-text-label">Formatting Delimiters</label>
        <button type="button" class="qt-button-secondary qt-button-sm" (click)="add()">
          + Add Delimiter
        </button>
      </div>
      <p class="qt-text-xs qt-text-secondary mb-3">
        Define formatting types with toolbar buttons and CSS styles. Each entry creates a button in
        the chat composer toolbar.
      </p>

      @if (entries().length === 0) {
        <p class="qt-text-xs qt-text-muted italic">
          No delimiters defined. Add one to create toolbar formatting buttons.
        </p>
      }

      <div class="space-y-4">
        @for (entry of entries(); track $index) {
          <div class="p-3 rounded-lg qt-border qt-bg-surface space-y-3">
            <div class="flex items-center justify-between">
              <h5 class="qt-text-label">Delimiter {{ $index + 1 }}</h5>
              <button
                type="button"
                class="qt-button-ghost qt-button-sm qt-text-destructive"
                (click)="remove($index)"
              >
                Remove
              </button>
            </div>

            <div>
              <label class="block qt-text-xs qt-text-secondary mb-1" [for]="fid($index, 'kind')"
                >Kind</label
              >
              <select
                class="qt-select w-full"
                [id]="fid($index, 'kind')"
                [value]="entry.kind"
                (change)="setKind($index, $any($event.target).value)"
              >
                <option value="wrap">Wrap — open…close around a span (e.g. *action*)</option>
                <option value="linePrefix">
                  Line prefix — marker styles the whole line (e.g. // OOC)
                </option>
                <option value="tagPrefix">
                  Tag prefix — [TOKEN] at line start styles the whole line
                </option>
              </select>
            </div>

            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
              <div>
                <label class="block qt-text-xs qt-text-secondary mb-1" [for]="fid($index, 'name')"
                  >Name</label
                >
                <input
                  type="text"
                  class="qt-input"
                  placeholder="Narration"
                  maxlength="50"
                  [id]="fid($index, 'name')"
                  [value]="entry.name"
                  (input)="patch($index, { name: $any($event.target).value })"
                />
              </div>
              <div>
                <label
                  class="block qt-text-xs qt-text-secondary mb-1"
                  [for]="fid($index, 'button')"
                  >Button Label</label
                >
                <input
                  type="text"
                  class="qt-input"
                  placeholder="Nar"
                  maxlength="10"
                  [id]="fid($index, 'button')"
                  [value]="entry.buttonName"
                  (input)="patch($index, { buttonName: $any($event.target).value })"
                />
              </div>
            </div>

            <div>
              <label class="block qt-text-xs qt-text-secondary mb-1" [for]="fid($index, 'style')"
                >Style (CSS class)</label
              >
              <input
                type="text"
                class="qt-input font-mono"
                list="qt-delimiter-styles"
                placeholder="qt-chat-narration"
                maxlength="50"
                [id]="fid($index, 'style')"
                [value]="entry.style"
                (input)="patch($index, { style: $any($event.target).value })"
              />
              <p class="qt-text-xs qt-text-muted mt-1">
                Pick a built-in style or enter your own theme class.
              </p>
            </div>

            @switch (entry.kind) {
              @case ('wrap') {
                <div>
                  <label class="block qt-text-xs qt-text-secondary mb-1">Delimiters:</label>
                  <div class="flex items-center gap-4 mb-2">
                    <label class="flex items-center gap-1 qt-text-xs">
                      <input
                        type="radio"
                        [name]="'delimMode' + $index"
                        [checked]="entry.delimiterMode === 'single'"
                        (change)="patch($index, { delimiterMode: 'single' })"
                      />
                      Same
                    </label>
                    <label class="flex items-center gap-1 qt-text-xs">
                      <input
                        type="radio"
                        [name]="'delimMode' + $index"
                        [checked]="entry.delimiterMode === 'pair'"
                        (change)="patch($index, { delimiterMode: 'pair' })"
                      />
                      Pair
                    </label>
                  </div>
                  @if (entry.delimiterMode === 'pair') {
                    <div class="flex items-center gap-2">
                      <input
                        type="text"
                        class="qt-input font-mono w-24"
                        aria-label="Opening delimiter"
                        placeholder="["
                        [value]="entry.delimiterOpen"
                        (input)="patch($index, { delimiterOpen: $any($event.target).value })"
                      />
                      <span class="qt-text-xs">to</span>
                      <input
                        type="text"
                        class="qt-input font-mono w-24"
                        aria-label="Closing delimiter"
                        placeholder="]"
                        [value]="entry.delimiterClose"
                        (input)="patch($index, { delimiterClose: $any($event.target).value })"
                      />
                    </div>
                  } @else {
                    <input
                      type="text"
                      class="qt-input font-mono w-24"
                      aria-label="Delimiter"
                      placeholder="*"
                      [value]="entry.delimiterOpen"
                      (input)="
                        patch($index, {
                          delimiterOpen: $any($event.target).value,
                          delimiterClose: $any($event.target).value
                        })
                      "
                    />
                  }
                </div>
                <div class="space-y-1">
                  <label class="flex items-center gap-2 cursor-pointer">
                    <input
                      type="checkbox"
                      class="qt-checkbox"
                      [checked]="entry.matchCurlyQuotes"
                      (change)="patch($index, { matchCurlyQuotes: $any($event.target).checked })"
                    />
                    <span class="qt-text-xs"
                      >Straight quotes also match curly ones (&quot; and &ldquo;&hellip;&rdquo;
                      alike)</span
                    >
                  </label>
                  <label class="flex items-center gap-2 cursor-pointer">
                    <input
                      type="checkbox"
                      class="qt-checkbox"
                      [checked]="entry.closeOptional"
                      (change)="patch($index, { closeOptional: $any($event.target).checked })"
                    />
                    <span class="qt-text-xs"
                      >May run unclosed to the end of a paragraph (speech carried over
                      several)</span
                    >
                  </label>
                </div>
              }
              @case ('linePrefix') {
                <div>
                  <label
                    class="block qt-text-xs qt-text-secondary mb-1"
                    [for]="fid($index, 'marker')"
                    >Line marker</label
                  >
                  <input
                    type="text"
                    class="qt-input font-mono w-28"
                    placeholder="// "
                    [id]="fid($index, 'marker')"
                    [value]="entry.marker"
                    (input)="patch($index, { marker: $any($event.target).value })"
                  />
                  <p class="qt-text-xs qt-text-muted mt-1">
                    A line beginning with this marker is styled in full.
                  </p>
                </div>
              }
              @case ('tagPrefix') {
                <div class="space-y-2">
                  <div>
                    <label
                      class="block qt-text-xs qt-text-secondary mb-1"
                      [for]="fid($index, 'tag-open')"
                      >Brackets:</label
                    >
                    <div class="flex items-center gap-2">
                      <input
                        type="text"
                        class="qt-input font-mono w-16"
                        placeholder="["
                        [id]="fid($index, 'tag-open')"
                        [value]="entry.tagOpen"
                        (input)="patch($index, { tagOpen: $any($event.target).value })"
                      />
                      <span class="qt-text-xs font-mono">TOKEN</span>
                      <input
                        type="text"
                        class="qt-input font-mono w-16"
                        aria-label="Closing bracket"
                        placeholder="]"
                        [value]="entry.tagClose"
                        (input)="patch($index, { tagClose: $any($event.target).value })"
                      />
                    </div>
                  </div>
                  <div>
                    <label
                      class="block qt-text-xs qt-text-secondary mb-1"
                      [for]="fid($index, 'token')"
                    >
                      Token pattern (regex, Unicode)
                    </label>
                    <input
                      type="text"
                      class="qt-input font-mono"
                      [id]="fid($index, 'token')"
                      [placeholder]="defaultTagTokenPattern"
                      [value]="entry.tokenPattern"
                      (input)="patch($index, { tokenPattern: $any($event.target).value })"
                    />
                    <p class="qt-text-xs qt-text-muted mt-1">
                      The token between the brackets must match this. Default = uppercase /
                      non-cased, never lowercase. A line like {{ tagSample(entry) }} at the start of a
                      line is then styled in full.
                    </p>
                  </div>
                </div>
              }
            }

            <div class="pt-2 border-t qt-border space-y-2">
              <label class="flex items-center gap-2 qt-text-xs">
                <input
                  type="checkbox"
                  [checked]="entry.hideDelimiter"
                  (change)="patch($index, { hideDelimiter: $any($event.target).checked })"
                />
                Conceal the marks when rendered
              </label>

              <qt-template-add-ons-editor
                [addOns]="entry.addOns"
                (addOnsChange)="setAddOns($index, $event)"
              />
            </div>
          </div>
        }
      </div>

      <datalist id="qt-delimiter-styles">
        @for (opt of styleOptions; track opt.value) {
          <option [value]="opt.value">{{ opt.label }}</option>
        }
      </datalist>
    </div>
  `,
})
export class TemplateDelimiterEditor {
  readonly entries = model.required<DelimiterFormEntry[]>();

  protected readonly styleOptions = STYLE_OPTIONS;
  protected readonly defaultTagTokenPattern = DEFAULT_TAG_TOKEN_PATTERN;
  private readonly idBase = `qt-delim-${++delimiterEditorCounter}`;

  /** A per-instance, per-row field id (v4 `${id}-delim-${index}-<field>`). */
  protected fid(index: number, field: string): string {
    return `${this.idBase}-${index}-${field}`;
  }

  protected tagSample(entry: DelimiterFormEntry): string {
    return `${entry.tagOpen || '['}CAPTAIN${entry.tagClose || ']'}`;
  }

  protected add(): void {
    this.entries.update((list) => [...list, emptyDelimiterEntry()]);
  }

  protected remove(index: number): void {
    this.entries.update((list) => list.filter((_, i) => i !== index));
  }

  /** Replace kind; seed the tag token pattern when switching to tagPrefix. */
  protected setKind(index: number, kind: DelimiterKind): void {
    this.entries.update((list) =>
      list.map((entry, i) => {
        if (i !== index) {
          return entry;
        }
        const next = { ...entry, kind };
        if (kind === 'tagPrefix' && !next.tokenPattern) {
          next.tokenPattern = DEFAULT_TAG_TOKEN_PATTERN;
        }
        return next;
      }),
    );
  }

  protected patch(index: number, patch: Partial<DelimiterFormEntry>): void {
    this.entries.update((list) =>
      list.map((entry, i) => (i === index ? { ...entry, ...patch } : entry)),
    );
  }

  protected setAddOns(index: number, addOns: FullAddOns): void {
    this.patch(index, { addOns });
  }
}
