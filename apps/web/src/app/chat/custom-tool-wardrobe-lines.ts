import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import type { CustomToolReferences } from './custom-tools.api';

/** One wardrobe sentence of the run dialog's reference panel. */
export interface WardrobeReferenceLine {
  key: 'wears' | 'takesOff' | 'clearsSlots';
  lead: string;
  values: string[];
  /** `' slot'` / `' slots'` after a `clearsSlots` list (by count); `''` otherwise. */
  suffix: string;
}

/** The sentence's close, after the values — a constant for its em dash. */
export const WARDROBE_LINE_TAIL = '. Only when you run it as a character — a run made as nobody dresses nobody.';

/**
 * The run dialog's wardrobe sentences (v4 `CustomToolRunDialog.tsx:926-932`,
 * `f0db8cbb8`, pinned at `25b8fab6a`): what a tool's wardrobe effects may do to
 * the RUNNING character, from the roster's `references` (the server's tool
 * vocabulary — sorted, the item references verbatim). Dressing, undressing and
 * clearing are three different promises, so each gets its own sentence, in
 * that order, and a list with nothing in it gets none. An older server sends no
 * keys, which reads as no lines.
 */
export function wardrobeReferenceLines(refs: CustomToolReferences | undefined): WardrobeReferenceLine[] {
  const lines: WardrobeReferenceLine[] = [
    { key: 'wears', lead: 'It may dress the running character in', values: refs?.wears ?? [], suffix: '' },
    { key: 'takesOff', lead: 'It may undress them of', values: refs?.takesOff ?? [], suffix: '' },
    { key: 'clearsSlots', lead: 'It may clear their', values: refs?.clearsSlots ?? [], suffix: '' },
  ];
  return lines
    .filter((line) => line.values.length > 0)
    .map((line) =>
      line.key === 'clearsSlots' ? { ...line, suffix: line.values.length > 1 ? ' slots' : ' slot' } : line,
    );
}

/**
 * The wardrobe sentences as rendered markup — one paragraph per line, in the
 * popup's writes-paragraph idiom (the values as `<code>`, joined by `, `).
 * Renders NOTHING when every list is empty or `references` is undefined.
 *
 * Standalone so it is proven alone: the custom-tool popup (`custom-tools-
 * popup.ts`, P4.D308's) is wired to it at unification, three edits —
 * the import, `|| wardrobeLines().length > 0` on the reference panel's `@if`,
 * and `<qt-custom-tool-wardrobe-lines [references]="tool.references" />` after
 * the writes paragraph.
 */
@Component({
  selector: 'qt-custom-tool-wardrobe-lines',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @for (line of lines(); track line.key) {
      <p class="text-xs qt-text-secondary mt-4 leading-relaxed">
        {{ line.lead }}
        @for (value of line.values; track value; let i = $index) {
          <span
            >@if (i > 0) {, }<code class="font-mono qt-text">{{ value }}</code></span
          >
        }{{ line.suffix }}{{ TAIL }}
      </p>
    }
  `,
})
export class CustomToolWardrobeLines {
  readonly references = input<CustomToolReferences | undefined>();

  protected readonly lines = computed(() => wardrobeReferenceLines(this.references()));
  protected readonly TAIL = WARDROBE_LINE_TAIL;
}
