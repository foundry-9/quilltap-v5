import { ChangeDetectionStrategy, Component, model } from '@angular/core';

import {
  DEFAULT_TAG_TOKEN_PATTERN,
  emptyDelimiterEntry,
  type DelimiterFormEntry,
  type DelimiterKind,
  type FullAddOns,
} from './template-form';

interface Option {
  value: string;
  label: string;
}

/** v4 `STYLE_OPTIONS` — the built-in theme classes offered in the datalist. */
const STYLE_OPTIONS: Option[] = [
  { value: 'qt-chat-narration', label: 'Narration' },
  { value: 'qt-chat-dialogue', label: 'Dialogue' },
  { value: 'qt-chat-ooc', label: 'Out of Character' },
  { value: 'qt-chat-inner-monologue', label: 'Inner Monologue' },
  { value: 'qt-roleplay-1', label: 'Style 1 (high contrast)' },
  { value: 'qt-roleplay-2', label: 'Style 2 (high contrast)' },
  { value: 'qt-roleplay-3', label: 'Style 3 (high contrast)' },
  { value: 'qt-roleplay-4', label: 'Style 4 (high contrast)' },
  { value: 'qt-roleplay-danger', label: 'Danger' },
  { value: 'qt-roleplay-warning', label: 'Warning' },
  { value: 'qt-roleplay-success', label: 'Success' },
  { value: 'qt-roleplay-info', label: 'Info' },
  { value: 'qt-roleplay-muted', label: 'Muted' },
  { value: 'qt-roleplay-code', label: 'Code' },
];

/** v4 `FONT_OPTIONS` — the flourish font choices. */
const FONT_OPTIONS: Option[] = [
  { value: '', label: 'Default' },
  { value: 'sans', label: 'Sans' },
  { value: 'serif', label: 'Serif' },
  { value: 'mono', label: 'Mono' },
  { value: 'display', label: 'Display' },
  { value: 'script', label: 'Script' },
];

/**
 * The Formatting Delimiters editor (v4 `components/settings/roleplay-templates/
 * index.tsx` delimiter section): a list of kind-discriminated toolbar entries
 * (wrap / linePrefix / tagPrefix), each with a name, button label, style class,
 * kind-specific markers, a conceal toggle, and the flourish add-ons. Two-way
 * bound to the parent form's `entries` array; every mutation replaces the array
 * immutably (OnPush + signals). Copy is v4-verbatim.
 */
@Component({
  selector: 'qt-template-delimiter-editor',
  changeDetection: ChangeDetectionStrategy.OnPush,
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

            <select
              class="qt-select w-full"
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

            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
              <div>
                <label class="block qt-text-xs qt-text-secondary mb-1">Name</label>
                <input
                  type="text"
                  class="qt-input"
                  placeholder="Narration"
                  maxlength="50"
                  [value]="entry.name"
                  (input)="patch($index, { name: $any($event.target).value })"
                />
              </div>
              <div>
                <label class="block qt-text-xs qt-text-secondary mb-1">Button Label</label>
                <input
                  type="text"
                  class="qt-input"
                  placeholder="Nar"
                  maxlength="10"
                  [value]="entry.buttonName"
                  (input)="patch($index, { buttonName: $any($event.target).value })"
                />
              </div>
            </div>

            <div>
              <label class="block qt-text-xs qt-text-secondary mb-1">Style (CSS class)</label>
              <input
                type="text"
                class="qt-input font-mono"
                list="qt-delimiter-styles"
                placeholder="qt-chat-narration"
                maxlength="50"
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
                        placeholder="["
                        [value]="entry.delimiterOpen"
                        (input)="patch($index, { delimiterOpen: $any($event.target).value })"
                      />
                      <span class="qt-text-xs">to</span>
                      <input
                        type="text"
                        class="qt-input font-mono w-24"
                        placeholder="]"
                        [value]="entry.delimiterClose"
                        (input)="patch($index, { delimiterClose: $any($event.target).value })"
                      />
                    </div>
                  } @else {
                    <input
                      type="text"
                      class="qt-input font-mono w-24"
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
              }
              @case ('linePrefix') {
                <div>
                  <label class="block qt-text-xs qt-text-secondary mb-1">Line marker</label>
                  <input
                    type="text"
                    class="qt-input font-mono w-28"
                    placeholder="// "
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
                    <label class="block qt-text-xs qt-text-secondary mb-1">Brackets:</label>
                    <div class="flex items-center gap-2">
                      <input
                        type="text"
                        class="qt-input font-mono w-16"
                        placeholder="["
                        [value]="entry.tagOpen"
                        (input)="patch($index, { tagOpen: $any($event.target).value })"
                      />
                      <span class="qt-text-xs font-mono">TOKEN</span>
                      <input
                        type="text"
                        class="qt-input font-mono w-16"
                        placeholder="]"
                        [value]="entry.tagClose"
                        (input)="patch($index, { tagClose: $any($event.target).value })"
                      />
                    </div>
                  </div>
                  <div>
                    <label class="block qt-text-xs qt-text-secondary mb-1">
                      Token pattern (regex, Unicode)
                    </label>
                    <input
                      type="text"
                      class="qt-input font-mono"
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

            <div class="pt-2 border-t qt-border">
              <label class="flex items-center gap-2 qt-text-xs mb-2">
                <input
                  type="checkbox"
                  [checked]="entry.hideDelimiter"
                  (change)="patch($index, { hideDelimiter: $any($event.target).checked })"
                />
                Conceal the marks when rendered
              </label>

              <p class="qt-text-xs qt-text-secondary mb-1">Flourishes</p>
              <div class="flex flex-wrap items-center gap-3">
                <label class="flex items-center gap-1 qt-text-xs">
                  <input
                    type="checkbox"
                    [checked]="entry.addOns.bold"
                    (change)="patchAddOn($index, { bold: $any($event.target).checked })"
                  />
                  Bold
                </label>
                <label class="flex items-center gap-1 qt-text-xs">
                  <input
                    type="checkbox"
                    [checked]="entry.addOns.italic"
                    (change)="patchAddOn($index, { italic: $any($event.target).checked })"
                  />
                  Italic
                </label>
                <label
                  class="flex items-center gap-1 qt-text-xs"
                  title="Swap foreground and background"
                >
                  <input
                    type="checkbox"
                    [checked]="entry.addOns.reverse"
                    (change)="patchAddOn($index, { reverse: $any($event.target).checked })"
                  />
                  Reverse
                </label>
                <label class="flex items-center gap-1 qt-text-xs">
                  Underline
                  <select
                    class="qt-select qt-select-sm"
                    [value]="entry.addOns.underline"
                    (change)="patchAddOn($index, { underline: $any($event.target).value })"
                  >
                    <option value="none">None</option>
                    <option value="single">Single</option>
                    <option value="double">Double</option>
                  </select>
                </label>
                <label class="flex items-center gap-1 qt-text-xs">
                  Border
                  <select
                    class="qt-select qt-select-sm"
                    [value]="entry.addOns.border"
                    (change)="patchAddOn($index, { border: $any($event.target).value })"
                  >
                    <option value="none">None</option>
                    <option value="solid">Solid</option>
                    <option value="dashed">Dashed</option>
                  </select>
                </label>
                <label class="flex items-center gap-1 qt-text-xs">
                  Font
                  <select
                    class="qt-select qt-select-sm"
                    [value]="entry.addOns.font"
                    (change)="patchAddOn($index, { font: $any($event.target).value })"
                  >
                    @for (opt of fontOptions; track opt.value) {
                      <option [value]="opt.value">{{ opt.label }}</option>
                    }
                  </select>
                </label>
              </div>
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
  protected readonly fontOptions = FONT_OPTIONS;
  protected readonly defaultTagTokenPattern = DEFAULT_TAG_TOKEN_PATTERN;

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

  protected patchAddOn(index: number, patch: Partial<FullAddOns>): void {
    this.entries.update((list) =>
      list.map((entry, i) =>
        i === index ? { ...entry, addOns: { ...entry.addOns, ...patch } } : entry,
      ),
    );
  }
}
