import { ChangeDetectionStrategy, Component, input } from '@angular/core';

/**
 * `qt-prompt-field-example` — the `Written as: <em>…</em>` line, shared with the
 * review surfaces that show a prompt field's worked example without the rest of
 * the labelled header. v4 `components/prompt-fields/PromptFieldLabel.tsx:41-58`
 * (`export function PromptFieldExample`, commit `2f4254b42`).
 *
 * v4's two callers are the optimizer's `SuggestionCard` (`SuggestionCard.tsx:161`,
 * default classes) and the AI Wizard's `GenerationStep` (`GenerationStep.tsx:379`,
 * default classes); its own `PromptFieldLabel` header renders it with the
 * `mt-1` override (`PromptFieldLabel.tsx:88`). The default class string is v4's
 * `className = 'text-xs qt-text-secondary'` parameter default, verbatim.
 *
 * `display: contents` on the host, NOT `block`: v4's is a React function
 * component, so it contributes NO element to the box tree — the `<p>` is a
 * direct child of whatever the caller's container is, and every adoption site
 * here sits in a container (`flex`, `<details>`, a plain `<div>`) whose layout
 * would change if a real box appeared between them. This is the opposite call
 * from {@link PromptFieldLabel}'s host, and for the opposite reason: that one
 * IS a direct child of a `space-y-*` stack, where `contents` would make the
 * `> * + *` margin land on a boxless element and vanish. No adoption site of
 * THIS component is a `space-y-*` child (measured at the port).
 */
@Component({
  selector: 'qt-prompt-field-example',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'contents' },
  // `[attr.class]`, NOT `[class]`: Angular's class binding dedups AND REORDERS
  // the tokens, so a `[class]` binding cannot reproduce v4's `className` string
  // byte-for-byte (the memory-noted trap — the spec pins the attribute).
  template: `<p [attr.class]="className()">Written as: <em>{{ example() }}</em></p>`,
})
export class PromptFieldExample {
  /** The worked example text (v4's required `example` prop). */
  readonly example = input.required<string>();
  /** v4's `className` prop, same default. */
  readonly className = input<string>('text-xs qt-text-secondary');
}
