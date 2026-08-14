import { ChangeDetectionStrategy, Component, computed, signal, viewChild, ElementRef } from '@angular/core';

import { smartTypographyStep } from '../../../smart-typography/engine';
import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from './chat-settings.api';
import {
  DEFAULT_SMART_TYPOGRAPHY_SETTINGS,
  type SmartTypographySettings as SmartTypographyBag,
} from './chat-settings.types';

/**
 * The Smart Typography card (v4 `components/settings/chat-settings/
 * SmartTypographySettings.tsx`, Layer 1.6).
 *
 * Presents the feature as the two groups it actually is, because the difference
 * between them IS the feature: quotes are a display opinion applied at render
 * time and nothing is written down, while dashes and ellipsis are content and
 * become real characters as you type.
 *
 * Sibling of the Text Replacement card, and reads as one on purpose. Copy
 * carries over verbatim.
 *
 * The try-it box calls the SHARED engine, so the preview cannot disagree with
 * the live plugin about what a keystroke does. Its bail conditions are NOT
 * copied from the plugin: this is a plain `<textarea>`, so there is no code
 * context to be inside and no IME/selection plumbing to mirror beyond the
 * collapsed-caret check.
 */
@Component({
  selector: 'qt-smart-typography-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert],
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Loading smart-typography settings…</p>
    } @else {
      <div class="space-y-4">
        @if (saveError(); as msg) {
          <qt-error-alert [message]="msg" class="mb-3" />
        }

        <!-- Part A — render time -->
        <label class="qt-settings-toggle-row">
          <input
            type="checkbox"
            class="qt-checkbox mt-1"
            [checked]="displayQuotes()"
            [disabled]="saving()"
            (change)="onUpdate({ displayQuotes: $any($event.target).checked })"
          />
          <div class="flex-1">
            <div class="qt-settings-section-heading">Curly quotes when displaying messages</div>
            <div class="qt-text-small mt-1">
              Shows <span class="qt-code-inline">&ldquo;curly quotes&rdquo;</span> in the
              conversation. What you typed is stored and sent to the model exactly as you typed it
              &mdash; this only changes how it looks, and turning it off puts everything back.
              Code, math and link addresses are never touched. A roleplay template that claims a
              quote character as one of its own delimiters keeps its straight quotes.
            </div>
          </div>
        </label>

        <!-- Part B — type time -->
        <div class="qt-settings-shell qt-settings-field-group">
          <div class="qt-settings-section-heading">Dashes and ellipsis as you type</div>
          <div class="qt-text-small">
            Turns <span class="qt-code-inline">--</span> into an en dash,
            <span class="qt-code-inline">---</span> into an em dash, and
            <span class="qt-code-inline">...</span> into an ellipsis while you type in the Salon
            composer and the Document Mode rich editor. These become real characters in your text.
            One Backspace or Cmd/Ctrl+Z undoes any of them. Nothing fires inside code blocks,
            inline code, or source-mode editors, and pasted text is left alone.
          </div>

          <label class="qt-settings-toggle-row">
            <input
              type="checkbox"
              class="qt-checkbox mt-1"
              [checked]="dashes()"
              [disabled]="saving()"
              (change)="onUpdate({ dashes: $any($event.target).checked })"
            />
            <div class="flex-1">
              <div class="qt-text-small font-medium">
                Dashes (<span class="qt-code-inline">--</span> &rarr;
                <span class="qt-code-inline">&ndash;</span>,
                <span class="qt-code-inline">---</span> &rarr;
                <span class="qt-code-inline">&mdash;</span>)
              </div>
            </div>
          </label>

          <label class="qt-settings-toggle-row">
            <input
              type="checkbox"
              class="qt-checkbox mt-1"
              [checked]="ellipsis()"
              [disabled]="saving()"
              (change)="onUpdate({ ellipsis: $any($event.target).checked })"
            />
            <div class="flex-1">
              <div class="qt-text-small font-medium">
                Ellipsis (<span class="qt-code-inline">...</span> &rarr;
                <span class="qt-code-inline">&hellip;</span>)
              </div>
            </div>
          </label>

          <!-- Try it -->
          <div class="qt-settings-section-heading">Try it</div>
          <div class="qt-text-small text-muted-foreground">
            Type a couple of hyphens or three periods here to watch the substitution happen. This
            box does not save anything.{{ pausedNote() }}
          </div>
          <textarea
            #tryIt
            rows="3"
            class="qt-input w-full"
            aria-label="Try smart typography"
            placeholder="Type -- or ... here&hellip;"
            [value]="tryItText()"
            (input)="tryItText.set($any($event.target).value)"
            (keydown)="onTryItKeyDown($event)"
          ></textarea>
        </div>
      </div>
    }
  `,
})
export class SmartTypographySettings extends ChatSettingsCard {
  private readonly tryIt = viewChild<ElementRef<HTMLTextAreaElement>>('tryIt');

  protected readonly tryItText = signal('');

  private readonly bag = computed(
    () => (this.settings()?.['smartTypographySettings'] ?? {}) as SmartTypographyBag,
  );

  /** v4 `current.displayQuotes ?? false`. */
  protected readonly displayQuotes = computed(
    () => this.bag().displayQuotes ?? DEFAULT_SMART_TYPOGRAPHY_SETTINGS.displayQuotes ?? false,
  );
  /** v4 `current.dashes ?? true`. */
  protected readonly dashes = computed(
    () => this.bag().dashes ?? DEFAULT_SMART_TYPOGRAPHY_SETTINGS.dashes ?? true,
  );
  /** v4 `current.ellipsis ?? true`. */
  protected readonly ellipsis = computed(
    () => this.bag().ellipsis ?? DEFAULT_SMART_TYPOGRAPHY_SETTINGS.ellipsis ?? true,
  );

  /** v4's trailing sentence, appended only while both rules are off. */
  protected readonly pausedNote = computed(() =>
    !this.dashes() && !this.ellipsis()
      ? ' (Both toggles are off — substitutions are paused.)'
      : '',
  );

  /**
   * v4 `{ ...DEFAULT_SMART_TYPOGRAPHY_SETTINGS, ...(settings.smartTypographySettings ?? {}),
   * ...updates }` — the merge-then-PUT the sibling bags use, so the whole bag
   * lands and the server never sees a partial nested patch.
   *
   * v4 also invalidates its whole chats query here, because its persisted
   * messages arrive with server-PRE-RENDERED HTML baked in and a cached
   * conversation would keep its old quotes. v5 has no server pre-render: every
   * message is rendered client-side, and the renderer follows the settings row
   * this save seeds (`smart-typography/settings.ts`), so the repaint needs no
   * second invalidation.
   */
  protected async onUpdate(updates: Partial<SmartTypographyBag>): Promise<void> {
    const merged: SmartTypographyBag = {
      ...DEFAULT_SMART_TYPOGRAPHY_SETTINGS,
      ...this.bag(),
      ...updates,
    };
    await this.save(
      { smartTypographySettings: merged },
      'Failed to update smart typography settings',
    );
  }

  /**
   * The preview keystroke. Same engine as the live plugin, so the two cannot
   * disagree; the substitution is applied to the textarea directly (we have
   * claimed the keystroke, so nothing else will write to it) and the caret is
   * restored, which a plain value re-bind would otherwise drop to the end.
   */
  protected onTryItKeyDown(event: KeyboardEvent): void {
    if (!this.dashes() && !this.ellipsis()) return;
    if (event.isComposing) return;
    if (event.metaKey || event.ctrlKey || event.altKey) return;

    const ta = this.tryIt()?.nativeElement;
    if (!ta) return;
    const start = ta.selectionStart;
    if (start !== ta.selectionEnd) return; // collapsed caret only

    const value = ta.value;
    const before = value.slice(Math.max(0, start - 2), start);
    const result = smartTypographyStep({
      before,
      typed: event.key,
      options: { dashes: this.dashes(), ellipsis: this.ellipsis() },
    });
    if (result === null) return;

    event.preventDefault();
    const deleteFrom = start - result.deleteBefore;
    const next = value.slice(0, deleteFrom) + result.insert + value.slice(start);
    const cursor = deleteFrom + result.insert.length;
    ta.value = next;
    ta.selectionStart = cursor;
    ta.selectionEnd = cursor;
    this.tryItText.set(next);
  }
}
