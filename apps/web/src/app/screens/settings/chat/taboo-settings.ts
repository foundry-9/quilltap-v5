import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';

import { CoreClient } from '../../../core/core-client';
import { Icon } from '../../../ui/icon';
import { ToastService } from '../../../ui/toast.service';

/** v4 `MAX_PHRASE_LENGTH` — mirrors `TabooSettingsSchema`'s per-entry bound. */
const MAX_PHRASE_LENGTH = 200;
/** v4 `MAX_PHRASES` — mirrors the schema's array bound. */
const MAX_PHRASES = 500;

/**
 * The Taboo settings card (v4
 * `components/settings/chat-settings/TabooSettings.tsx`,
 * `instance_settings['taboo']`) — the phrases no character may utter, rendered
 * into the universal, cache-stable portion of every character's system prompt.
 *
 * Instance-scoped, so it fetches for itself rather than riding the per-user
 * chat-settings blob, exactly as the Data Retention card does.
 *
 * Every add and remove sends the WHOLE array: the server owns normalization
 * (trim, drop empties, case-insensitive dedupe, order preserved) and the
 * response IS the stored list, which is what the card renders next — never its
 * own optimistic arithmetic. Copy carries over verbatim in v4's register.
 */
@Component({
  selector: 'qt-taboo-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Consulting the list of forbidden phrases&hellip;</p>
    } @else if (loadError()) {
      <p class="qt-text-small qt-text-error">{{ loadError() }}</p>
    } @else {
      <div class="space-y-4">
        <p class="qt-text-small qt-text-muted">
          Phrases the household has agreed never to utter &mdash; strike the tired ones from every
          tongue in the establishment. Each entry is forbidden outright: not merely word for word,
          but in every inflection, rewording, and near-variant of the same weary formula. Characters
          are told to say what they actually mean instead, in plain and particular words, and are
          under strict instruction never to mention the list itself.
        </p>

        <form
          class="qt-settings-shell space-y-3"
          aria-label="Add a forbidden phrase"
          (submit)="onSubmit($event)"
        >
          <div class="qt-settings-section-heading">Add a phrase</div>
          <div class="flex flex-col sm:flex-row gap-2 sm:items-end">
            <label class="flex flex-col gap-1 flex-1">
              <span class="qt-text-small text-muted-foreground">Forbidden phrase</span>
              <input
                type="text"
                class="qt-input"
                [attr.maxlength]="MAX_PHRASE_LENGTH"
                placeholder="e.g. that&rsquo;s not nothing"
                aria-label="Forbidden phrase"
                [value]="draft()"
                [disabled]="busy()"
                (input)="draft.set($any($event.target).value)"
                (keydown)="onKeyDown($event)"
              />
            </label>
            <button type="submit" class="qt-button-primary" [disabled]="busy() || !draft().trim()">
              Add
            </button>
          </div>
          <p class="qt-text-xs qt-text-secondary">
            One phrase at a time &mdash; commas are perfectly welcome inside a phrase, so they cannot
            serve as separators. Press Enter or use Add.
          </p>
          @if (addError()) {
            <div class="qt-text-small qt-text-error">{{ addError() }}</div>
          }
        </form>

        <div class="qt-settings-shell space-y-2">
          <div class="qt-settings-section-heading">Forbidden phrases</div>
          @if (phrases().length === 0) {
            <p class="qt-text-small text-muted-foreground italic">
              Nothing forbidden yet &mdash; every tongue in the house runs free.
            </p>
          } @else {
            <div class="flex flex-wrap gap-2">
              @for (phrase of phrases(); track $index) {
                <span class="qt-tag-badge qt-tag-badge-md">
                  <span>{{ phrase }}</span>
                  <button
                    type="button"
                    class="qt-tag-badge-remove"
                    [disabled]="busy()"
                    [attr.aria-label]="'Remove &quot;' + phrase + '&quot; from the Taboo list'"
                    (click)="remove($index)"
                  >
                    <qt-icon name="close" class="h-3 w-3" />
                  </button>
                </span>
              }
            </div>
            <p class="qt-text-xs qt-text-secondary">
              {{ countLine() }}
            </p>
          }
        </div>
      </div>
    }
  `,
})
export class TabooSettings {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  protected readonly MAX_PHRASE_LENGTH = MAX_PHRASE_LENGTH;

  protected readonly loading = signal(true);
  protected readonly busy = signal(false);
  protected readonly loadError = signal<string | null>(null);
  protected readonly addError = signal<string | null>(null);
  protected readonly draft = signal('');
  /** The server's list, verbatim — never locally recomputed. */
  protected readonly phrases = signal<string[]>([]);

  constructor() {
    void this.load();
  }

  private async load(): Promise<void> {
    try {
      const settings = await this.core.getTabooSettings();
      this.phrases.set(Array.isArray(settings?.phrases) ? settings.phrases : []);
    } catch (err) {
      // v4 surfaces the server's own sentence (`apiErrorMessage(error, …)`);
      // only a message-less failure gets the fixed line.
      this.loadError.set(
        (err instanceof Error && err.message) || 'Failed to load the Taboo list',
      );
    } finally {
      this.loading.set(false);
    }
  }

  protected countLine(): string {
    const n = this.phrases().length;
    return `${n} phrase${n === 1 ? '' : 's'} forbidden, in the order you placed them. Applies to every character’s conversational replies across the whole establishment.`;
  }

  protected onSubmit(event: Event): void {
    event.preventDefault();
    void this.addPhrase();
  }

  /**
   * Enter is wired explicitly rather than left to the form's implicit
   * submission — v4's own note: it keeps the promise the card makes ("Press
   * Enter or use Add") from depending on implicit-submission behaviour, which
   * varies with the browser and the input method.
   */
  protected onKeyDown(event: KeyboardEvent): void {
    if (event.key !== 'Enter') return;
    event.preventDefault();
    void this.addPhrase();
  }

  private async addPhrase(): Promise<void> {
    this.addError.set(null);
    const phrase = this.draft().trim();
    if (!phrase) return;
    if (phrase.length > MAX_PHRASE_LENGTH) {
      this.addError.set(`A phrase may run to ${MAX_PHRASE_LENGTH} characters at most.`);
      return;
    }
    if (this.phrases().length >= MAX_PHRASES) {
      this.addError.set(`The list is full at ${MAX_PHRASES} phrases.`);
      return;
    }
    if (this.phrases().some((p) => p.toLowerCase() === phrase.toLowerCase())) {
      this.addError.set('That phrase is already forbidden.');
      return;
    }

    if (await this.save([...this.phrases(), phrase])) {
      this.draft.set('');
      this.toasts.showSuccess('Phrase struck from the record');
    }
    // On failure the draft is kept for a retry (v4's catch does nothing else).
  }

  protected async remove(index: number): Promise<void> {
    this.addError.set(null);
    if (await this.save(this.phrases().filter((_, i) => i !== index))) {
      this.toasts.showSuccess('Phrase restored to polite company');
    }
  }

  /** Send the WHOLE array; the normalized echo is what lands in the card. */
  private async save(next: string[]): Promise<boolean> {
    this.busy.set(true);
    try {
      const saved = await this.core.updateTabooSettings(next);
      this.phrases.set(Array.isArray(saved?.phrases) ? saved.phrases : []);
      return true;
    } catch (err) {
      this.toasts.showError(
        (err instanceof Error && err.message) || 'Failed to save the Taboo list',
      );
      return false;
    } finally {
      this.busy.set(false);
    }
  }
}
