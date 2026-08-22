import {
  booleanAttribute,
  ChangeDetectionStrategy,
  Component,
  computed,
  input,
} from '@angular/core';

import type { PromptFieldHint } from './prompt-field-hints';

/**
 * `qt-prompt-field-label` — the shared labelled-field header for prompt-bearing
 * fields: label line, helper paragraph, and a worked example in the field's
 * correct form of address. v4 `components/prompt-fields/PromptFieldLabel.tsx`
 * (commit `a6870c5a`).
 *
 * It renders the header ONLY — it deliberately does not wrap the editor, so it
 * drops above any existing input (a `qt-markdown-field`, a `<textarea>`, an
 * `<input>`) without touching its state wiring. Hint copy comes from
 * `./prompt-field-hints` (`PROMPT_FIELD_HINTS`) — pass a `hint` to use it, or
 * override individual pieces via `label`/`helper`/`example`.
 *
 * v4's `actions` ReactNode prop (right-aligned controls on the label row) is an
 * `<ng-content>` slot here: project the control as a child and it lands in the
 * same flex row.
 *
 * The whole label LINE is one computed string rather than three adjacent
 * template nodes on purpose: Angular's default whitespace collapsing would
 * insert a space between `{{ label }}` and a separate `(Optional)` node, and
 * v4's JSX concatenates them with none. The `*` stays a real element because it
 * is separately styled; its leading space survives collapsing (a text node that
 * is not entirely whitespace, with no run of two).
 */
@Component({
  selector: 'qt-prompt-field-label',
  changeDetection: ChangeDetectionStrategy.OnPush,
  // An Angular custom-element host is `display: inline` by default, and a React
  // component has no host element at all — so without this the appearance tab,
  // where the header is a DIRECT child of a `space-y-4` stack, would silently
  // drop its gap (margin-top does not apply to an inline box). Same class of
  // bug as the `qt-tab-view` one dogfood finding #97 caught. `block`, not
  // `contents`: `contents` makes the host generate no box at all, which reads
  // like v4's zero-host React render but ALSO makes `space-y-*`'s
  // `> * + *` margin land on a boxless element and vanish.
  host: { class: 'block' },
  template: `
    <div class="mb-2">
      <div class="flex items-center justify-between">
        <label [attr.for]="htmlFor()" class="block qt-label text-foreground"
          >{{ labelLine()
          }}@if (required()) {<span class="qt-text-destructive"> *</span>}</label
        >
        <ng-content></ng-content>
      </div>
      @if (helperText()) {
        <p class="text-xs qt-text-secondary mt-1">{{ helperText() }}</p>
      }
      @if (exampleText()) {
        <p class="text-xs qt-text-secondary mt-1">Written as: <em>{{ exampleText() }}</em></p>
      }
    </div>
  `,
})
export class PromptFieldLabel {
  /** Hint bundle from PROMPT_FIELD_HINTS; individual inputs override it. */
  readonly hint = input<PromptFieldHint | undefined>(undefined);
  /** The label line. Required when no `hint` is given. */
  readonly label = input<string | undefined>(undefined);
  /**
   * Renders " (Optional)" after the label. `booleanAttribute` so call sites
   * write the bare attribute, exactly as v4's JSX writes the bare prop.
   */
  readonly optional = input(false, { transform: booleanAttribute });
  /** Renders a destructive-styled " *" after the label (bare attribute too). */
  readonly required = input(false, { transform: booleanAttribute });
  /** `id` of the field this labels, for `<label for>`. */
  readonly htmlFor = input<string | undefined>(undefined);
  /** Helper paragraph under the label (house voice). */
  readonly helper = input<string | undefined>(undefined);
  /** Worked example, rendered as `Written as: <em>…</em>` (plain voice). */
  readonly example = input<string | undefined>(undefined);

  /** v4 `label ?? hint?.label ?? ''`, plus the `" (Optional)"` suffix. */
  protected readonly labelLine = computed(
    () => `${this.label() ?? this.hint()?.label ?? ''}${this.optional() ? ' (Optional)' : ''}`,
  );
  protected readonly helperText = computed(() => this.helper() ?? this.hint()?.helper);
  protected readonly exampleText = computed(() => this.example() ?? this.hint()?.example);
}
