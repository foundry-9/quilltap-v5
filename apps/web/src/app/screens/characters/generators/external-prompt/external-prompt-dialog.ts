import { ChangeDetectionStrategy, Component, inject, input, output, signal } from '@angular/core';

import type {
  CharacterConnectionProfile,
  CharacterScenario,
  CharacterSystemPrompt,
} from '../../../../core/core-contract';
import { CoreClient } from '../../../../core/core-client';
import { dispatchGenerateExternalPrompt } from '../detail-generators.api';

/**
 * `ExternalPromptDialog` — v4 `app/aurora/[id]/view/components/
 * ExternalPromptDialog.tsx` (270 lines): the options form for "Non-Quilltap
 * Prompt" — a standalone system prompt for use in external tools. v4 fetches
 * its own connection-profile list on mount (the character's physical
 * description and wardrobe are pulled in server-side; the dialog no longer
 * asks which record to use) and pre-selects the default profile / default
 * system prompt.
 */
@Component({
  selector: 'qt-external-prompt-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    '(document:keydown.escape)': 'onEscape()',
  },
  template: `
    <div
      class="bg-background/80 fixed inset-0 z-50 flex items-center justify-center p-4 backdrop-blur-sm"
      (click)="onBackdrop()"
    >
      <div
        class="qt-border-default qt-bg-card flex max-h-[90vh] w-full max-w-md flex-col rounded-2xl border p-6 shadow-2xl md:max-w-lg"
        (click)="$event.stopPropagation()"
      >
        <h3 class="qt-heading-4 mb-4 flex-shrink-0">
          Generate External Prompt{{ characterName() ? ' for ' + characterName() : '' }}
        </h3>

        @if (loadingProfiles()) {
          <div class="flex items-center justify-center py-8">
            <div
              class="h-6 w-6 animate-spin rounded-full border-2 border-foreground border-t-transparent"
            ></div>
          </div>
        } @else {
          <div class="-mr-2 flex-1 space-y-4 overflow-y-auto pr-2">
            <div>
              <label for="ext-profile" class="qt-text-primary mb-2 block text-sm">
                LLM Connection Profile *
              </label>
              <!-- dogfood-#6: async options ⇒ per-option [selected], never [value] on
                   the <select> (the details-tab.ts reverse-user precedent). -->
              <select
                id="ext-profile"
                class="qt-select"
                (change)="connectionProfileId.set($any($event.target).value)"
              >
                <option value="" [selected]="profiles().length === 0">Select a profile</option>
                @for (profile of profiles(); track profile.id) {
                  <option [value]="profile.id" [selected]="profile.id === effectiveProfileId()">
                    {{ profile.name }} ({{ profile.provider }} / {{ profile.modelName }})
                  </option>
                }
              </select>
            </div>

            <div>
              <label for="ext-system-prompt" class="qt-text-primary mb-2 block text-sm">
                System Prompt *
              </label>
              <select
                id="ext-system-prompt"
                class="qt-select"
                (change)="systemPromptId.set($any($event.target).value)"
              >
                <option value="" [selected]="systemPrompts().length === 0">Select a system prompt</option>
                @for (prompt of systemPrompts(); track prompt.id) {
                  <option [value]="prompt.id" [selected]="prompt.id === effectiveSystemPromptId()">
                    {{ prompt.name }}{{ prompt.isDefault ? ' (Default)' : '' }}
                  </option>
                }
              </select>
              @if (systemPrompts().length === 0) {
                <p class="qt-text-destructive mt-1 text-xs">
                  This character has no system prompts. Add one first.
                </p>
              }
            </div>

            @if (scenarios().length > 0) {
              <div>
                <label for="ext-scenario" class="qt-text-primary mb-2 block text-sm">
                  Scenario (Optional)
                </label>
                <select
                  id="ext-scenario"
                  class="qt-select"
                  (change)="scenarioId.set($any($event.target).value)"
                >
                  <option value="" [selected]="scenarioId() === ''">None</option>
                  @for (s of scenarios(); track s.id) {
                    <option [value]="s.id" [selected]="s.id === scenarioId()">{{ s.title }}</option>
                  }
                </select>
              </div>
            }

            <div>
              <label for="ext-max-tokens" class="qt-text-primary mb-2 block text-sm">
                Maximum Output Size
              </label>
              <input
                id="ext-max-tokens"
                type="range"
                min="1000"
                max="20000"
                step="500"
                class="qt-range w-full"
                [value]="maxTokens()"
                (input)="maxTokens.set(+$any($event.target).value)"
              />
              <div class="qt-text-secondary mt-1 flex justify-between text-xs">
                <span>{{ maxTokens().toLocaleString() }} tokens</span>
                <span>~{{ estimatedChars().toLocaleString() }} characters</span>
              </div>
            </div>

            @if (error()) {
              <div class="qt-border-destructive/50 qt-bg-destructive/10 qt-text-destructive rounded-lg border px-3 py-2 text-sm">
                {{ error() }}
              </div>
            }
          </div>
        }

        <div class="mt-4 flex flex-shrink-0 justify-end gap-3">
          <button
            type="button"
            [disabled]="generating()"
            class="qt-button qt-button-secondary"
            (click)="requestClose()"
          >
            Cancel
          </button>
          <button
            type="button"
            [disabled]="!canGenerate()"
            class="qt-button qt-button-primary"
            (click)="handleGenerate()"
          >
            @if (generating()) {
              <span class="inline-flex items-center gap-2">
                <span
                  class="qt-border-primary-foreground h-4 w-4 animate-spin rounded-full border-t-transparent"
                ></span>
                Generating...
              </span>
            } @else {
              Generate Prompt
            }
          </button>
        </div>
      </div>
    </div>
  `,
})
export class ExternalPromptDialog {
  private readonly core = inject(CoreClient);

  readonly characterId = input.required<string>();
  readonly characterName = input<string | null>(null);
  readonly systemPrompts = input<CharacterSystemPrompt[]>([]);
  readonly scenarios = input<CharacterScenario[]>([]);
  /** The character's connection profiles — fetched by the caller (v4 fetches its own; v5's
   *  detail screen already holds this list, so it's threaded in rather than re-fetched). */
  readonly profiles = input<CharacterConnectionProfile[]>([]);
  readonly loadingProfiles = input(false);

  readonly cancelled = output<void>();
  readonly generated = output<string>();

  protected readonly connectionProfileId = signal('');
  protected readonly systemPromptId = signal('');
  protected readonly scenarioId = signal('');
  protected readonly maxTokens = signal(4000);
  protected readonly generating = signal(false);
  protected readonly error = signal<string | null>(null);

  protected readonly estimatedChars = () => this.maxTokens() * 4;

  protected readonly canGenerate = () =>
    Boolean(this.effectiveProfileId()) && Boolean(this.effectiveSystemPromptId()) && !this.generating();

  /** v4 `:66-71` — falls back to the default (or first) profile until overridden. */
  protected readonly effectiveProfileId = (): string => {
    if (this.connectionProfileId()) return this.connectionProfileId();
    const list = this.profiles();
    return list.length > 0 ? list[0].id : '';
  };

  /** v4 `:84-90` — falls back to the default (or first) system prompt until overridden. */
  protected readonly effectiveSystemPromptId = (): string => {
    if (this.systemPromptId()) return this.systemPromptId();
    const list = this.systemPrompts();
    if (list.length === 0) return '';
    return list.find((p) => p.isDefault)?.id ?? list[0].id;
  };

  protected async handleGenerate(): Promise<void> {
    const connectionProfileId = this.effectiveProfileId();
    const systemPromptId = this.effectiveSystemPromptId();
    if (!connectionProfileId || !systemPromptId) return;

    this.generating.set(true);
    this.error.set(null);
    try {
      const result = await dispatchGenerateExternalPrompt(this.core, {
        type: 'characterGenerateExternalPrompt',
        characterId: this.characterId(),
        connectionProfileId,
        systemPromptId,
        scenarioId: this.scenarioId() || undefined,
        maxTokens: this.maxTokens(),
      });
      this.generated.emit(result.prompt);
    } catch (err) {
      this.error.set(err instanceof Error && err.message ? err.message : 'Generation failed');
    } finally {
      this.generating.set(false);
    }
  }

  protected requestClose(): void {
    if (!this.generating()) this.cancelled.emit();
  }
  protected onBackdrop(): void {
    this.requestClose();
  }
  protected onEscape(): void {
    this.requestClose();
  }
}
