import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import { Icon } from '../ui/icon';

/**
 * The "something is happening" quill (v4 `components/chat/QuillAnimation.tsx`).
 *
 * Shown while a reply is awaited, while tokens stream in, and while a tool call
 * is still outstanding. Both halves are theme-replaceable and independent:
 *
 *   - the GLYPH is the `thinking` icon, so a theme pack can swap it by
 *     re-declaring `[data-icon="thinking"]` in its own unlayered stylesheet
 *     (see `_icons.css`);
 *   - the MOTION is `.qt-thinking-indicator` (see `_chat.css`), which a theme
 *     can retune through its `--qt-thinking-*` custom properties or replace
 *     outright with a different animation.
 *
 * Nothing here drives the animation frame by frame — the component decides only
 * whether the indicator is mounted.
 */
@Component({
  selector: 'qt-quill-animation',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `<span [class]="wrapperClass()">
    <qt-icon name="thinking" [title]="label() ?? undefined" [class]="iconClass()" />
  </span>`,
})
export class QuillAnimation {
  readonly size = input<'lg' | 'sm'>('lg');
  /** Extra classes (e.g. Tailwind `ml-auto`, `qt-text-secondary`). */
  readonly klass = input<string>('', { alias: 'class' });
  /**
   * Accessible label. Defaults to "Writing…"; pass `null` where the indicator
   * sits inside an already-labelled live region (the status strip, a tool row)
   * and would otherwise be announced twice.
   */
  readonly label = input<string | null>('Writing…');

  /** v4's `sizeClasses` — carried by BOTH the wrapper and the glyph. */
  protected readonly sizeClasses = computed(() => (this.size() === 'lg' ? 'w-12 h-12' : 'w-4 h-4'));

  protected readonly wrapperClass = computed(() =>
    `inline-flex items-center justify-center ${this.sizeClasses()} ${this.klass()}`.trim(),
  );

  protected readonly iconClass = computed(() => `qt-thinking-indicator ${this.sizeClasses()}`);
}
