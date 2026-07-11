import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import { highlightSegments } from './template-highlighter';

/**
 * v4 `components/characters/TemplateHighlighter.tsx` (`TemplateDisplay`): text
 * with `{{char}}`/`{{user}}` templates and replaceable hardcoded names
 * highlighted. A standalone component (as in v4) rather than inline `@switch`
 * markup in each tab — besides the reuse, this keeps the segment markup out of
 * any `<pre>` element in a consuming template: Angular preserves a template's
 * literal whitespace inside `<pre>`, so inlined control-flow markup there
 * renders its own indentation into the displayed text (dogfood finding #5).
 */
@Component({
  selector: 'qt-template-display',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @for (seg of segments(); track $index) {
      @switch (seg.kind) {
        @case ('char-template') {
          <span
            class="px-0.5 rounded border-b-2 qt-badge-chat qt-border-info"
            [title]="seg.title"
            >{{ seg.text }}</span
          >
        }
        @case ('user-template') {
          <span
            class="px-0.5 rounded border-b-2 qt-badge-user-character qt-border-success"
            [title]="seg.title"
            >{{ seg.text }}</span
          >
        }
        @case ('char-hardcoded') {
          <span
            class="px-0.5 rounded border-b-2 border-dashed qt-bg-warning/10 qt-border-warning"
            [title]="seg.title"
            >{{ seg.text }}</span
          >
        }
        @case ('user-hardcoded') {
          <span
            class="px-0.5 rounded border-b-2 border-dashed qt-bg-warning/10 qt-border-warning"
            [title]="seg.title"
            >{{ seg.text }}</span
          >
        }
        @default {
          <span>{{ seg.text }}</span>
        }
      }
    }
  `,
})
export class TemplateDisplay {
  readonly content = input.required<string>();
  readonly characterName = input.required<string>();
  readonly userCharacterName = input<string | null>(null);

  protected readonly segments = computed(() =>
    highlightSegments(this.content(), this.characterName(), this.userCharacterName()),
  );
}
