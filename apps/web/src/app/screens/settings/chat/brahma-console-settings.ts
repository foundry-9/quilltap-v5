import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';

import { CoreClient } from '../../../core/core-client';
import { ToastService } from '../../../ui/toast.service';
import {
  DEFAULT_MAX_AGENT_TURNS,
  MAX_TURNS,
  MIN_TURNS,
  getBrahmaConsoleSettings,
  updateBrahmaConsoleSettings,
} from './brahma-console-settings.api';

/**
 * The Brahma Console budget card (v4
 * `components/settings/chat-settings/BrahmaConsoleSettings.tsx`, v4 `6452e2c3`,
 * `instance_settings['brahmaConsole']`). One number input capping how many
 * tool-use turns the Console — and every one-shot `@Brahma` consultation — may
 * take before it must answer. Global only; there is deliberately no per-chat dial.
 *
 * Instance-scoped, so it fetches for itself rather than riding the per-user
 * chat-settings blob, exactly as the Data Retention and Taboo cards do.
 *
 * Autosaves — the number input commits on blur (v4's `commit`): an unusable
 * entry (non-finite / out of `[5, 200]`) is reverted rather than nagged (the
 * bounds live in the copy), and an unchanged value is a no-op. Copy carries over
 * verbatim in v4's steampunk register.
 */
@Component({
  selector: 'qt-brahma-console-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Loading Console settings&hellip;</p>
    } @else {
      <div class="space-y-4">
        <p class="qt-text-small qt-text-muted">
          Put a knotty question to the Brahma Console &mdash; &ldquo;where in the ledgers is
          such-and-such buried?&rdquo; &mdash; and it sets about the search one step at a time: a query
          here, a document read there, each a <em>turn</em> at the telegraph key. This dial sets how
          many turns it may take on a single question before it must down tools and tell you what it
          has found so far. Raise it when the Console keeps running out of rope mid-investigation; the
          higher ceiling costs nothing on questions it answers quickly.
        </p>

        <div>
          <label for="brahma-max-turns" class="qt-text-label block mb-2">
            Let the Console take up to
          </label>
          <div class="flex items-center gap-2">
            <input
              id="brahma-max-turns"
              type="number"
              [min]="MIN_TURNS"
              [max]="MAX_TURNS"
              class="qt-input w-28"
              [value]="draft()"
              (input)="draft.set($any($event.target).value)"
              (blur)="commit()"
              (keydown.enter)="$any($event.target).blur()"
              [disabled]="saving()"
            />
            <span class="qt-text-small qt-text-secondary">
              turns ({{ MIN_TURNS }}&ndash;{{ MAX_TURNS }}; the default is {{ DEFAULT_MAX_AGENT_TURNS }})
            </span>
          </div>
          <p class="qt-text-xs qt-text-secondary mt-1">
            A generous budget only helps a Console that is making headway. Should it fall to asking the
            same question twice over, Quilltap notices the engine chasing its own tail and calls a halt
            regardless of this figure &mdash; so raising the ceiling never lets a truly stuck search run
            on and on.
          </p>
        </div>

        @if (error()) {
          <p class="qt-text-small qt-text-error">{{ error() }}</p>
        }
      </div>
    }
  `,
})
export class BrahmaConsoleSettings {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  protected readonly MIN_TURNS = MIN_TURNS;
  protected readonly MAX_TURNS = MAX_TURNS;
  protected readonly DEFAULT_MAX_AGENT_TURNS = DEFAULT_MAX_AGENT_TURNS;

  protected readonly loading = signal(true);
  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);
  protected readonly draft = signal<string>(String(DEFAULT_MAX_AGENT_TURNS));

  /** The last persisted value — an unusable draft reverts to it, an unchanged
   *  draft skips the round-trip (v4's `turns` state + `savedTurns` ref, which
   *  v4 keeps in lockstep). */
  private turns = DEFAULT_MAX_AGENT_TURNS;

  constructor() {
    void this.load();
  }

  private async load(): Promise<void> {
    try {
      const settings = await getBrahmaConsoleSettings(this.core);
      this.turns = settings.maxAgentTurns;
      this.draft.set(String(settings.maxAgentTurns));
    } catch (err) {
      // v4 (`BrahmaConsoleSettings.tsx:41`) surfaces the server's own sentence
      // (`getErrorMessage(err, …)`); only a message-less failure gets the line.
      this.error.set(
        (err instanceof Error && err.message) || 'Failed to load Brahma Console settings',
      );
    } finally {
      this.loading.set(false);
    }
  }

  protected async commit(): Promise<void> {
    const parsed = Math.floor(Number(this.draft()));
    if (!Number.isFinite(parsed) || parsed < MIN_TURNS || parsed > MAX_TURNS) {
      // Revert an unusable entry rather than nag — the bounds live in the copy.
      this.draft.set(String(this.turns));
      return;
    }
    if (parsed === this.turns) {
      this.draft.set(String(parsed));
      return;
    }

    this.saving.set(true);
    this.error.set(null);
    try {
      const saved = await updateBrahmaConsoleSettings(this.core, parsed);
      this.turns = saved.maxAgentTurns;
      this.draft.set(String(saved.maxAgentTurns));
      this.toasts.showSuccess('Console turn budget saved');
    } catch (err) {
      // v4 (`BrahmaConsoleSettings.tsx:80-83`) surfaces the server's own sentence
      // (`getErrorMessage(err, …)`); only a message-less failure gets the line.
      const msg =
        (err instanceof Error && err.message) || 'Failed to save Brahma Console settings';
      this.error.set(msg);
      this.toasts.showError(msg);
      this.draft.set(String(this.turns));
    } finally {
      this.saving.set(false);
    }
  }
}
