import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { Router, RouterLink } from '@angular/router';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { CharacterConnectionProfile } from '../../../core/core-contract';
import { Icon } from '../../../ui/icon';
import { fetchConnectionProfiles } from '../characters.api';

/** The CREATE page's form (v4 `NewCharacterView.tsx` `formData`). */
interface NewCharacterFormData {
  name: string;
  title: string;
  identity: string;
  description: string;
  manifesto: string;
  personality: string;
  scenario: string;
  firstMessage: string;
  exampleDialogues: string;
  systemPrompt: string;
  avatarUrl: string;
  defaultConnectionProfileId: string;
}

const INITIAL_FORM: NewCharacterFormData = {
  name: '',
  title: '',
  identity: '',
  description: '',
  manifesto: '',
  personality: '',
  scenario: '',
  firstMessage: '',
  exampleDialogues: '',
  systemPrompt: '',
  avatarUrl: '',
  defaultConnectionProfileId: '',
};

/** Drop empty optional-UUID/URL fields (v4 `handleSubmit`'s `submitData`). */
export function buildCreateCharacterBag(form: NewCharacterFormData): Record<string, unknown> {
  return {
    ...form,
    defaultConnectionProfileId: form.defaultConnectionProfileId || undefined,
    avatarUrl: form.avatarUrl || undefined,
  };
}

/**
 * The character CREATE page (v4 `app/aurora/new/NewCharacterView.tsx`): a
 * plain full-page form for the four vantage points (identity / description /
 * manifesto / personality — DISTINCT), a SINGULAR scenario field (the array
 * editor is an edit-only affordance), first message, example dialogues,
 * legacy system prompt, avatar URL, and a default connection profile picker.
 * The AI Wizard and "Import Template" are named deferrals (disabled, v4
 * microcopy). Markdown fields are plain `<textarea>`s this round. Copy +
 * `qt-*` classes carry over verbatim.
 */
@Component({
  selector: 'qt-new-character',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, Icon],
  template: `
    <div class="qt-page-container">
      <div class="mb-8">
        <a routerLink="/characters" class="qt-link mb-4 inline-block">← Back to Characters</a>
        <div class="flex items-center justify-between">
          <h1 class="qt-heading-1">Create Character</h1>
          <button
            type="button"
            class="qt-button-secondary flex items-center gap-2"
            disabled
            title="Use AI to generate character details"
          >
            <qt-icon name="wand" class="w-4 h-4" />
            AI Wizard
          </button>
        </div>
      </div>

      @if (error()) {
        <div class="qt-alert-error mb-4">{{ error() }}</div>
      }

      <form (submit)="onSubmit($event)" class="space-y-6">
        <div>
          <label for="name" class="block qt-label mb-2 text-foreground">Name *</label>
          <input
            type="text"
            id="name"
            class="qt-input"
            required
            [value]="form().name"
            (input)="setField('name', $any($event.target).value)"
          />
        </div>

        <div>
          <label for="title" class="block qt-label mb-2 text-foreground">Title (Optional)</label>
          <input
            type="text"
            id="title"
            class="qt-input"
            placeholder="Your private label for this character — e.g., the protagonist, the rival, the love interest. Not how strangers refer to them."
            [value]="form().title"
            (input)="setField('title', $any($event.target).value)"
          />
        </div>

        <div>
          <label for="identity" class="block qt-label mb-2 text-foreground"
            >Identity (Optional)</label
          >
          <p class="text-xs qt-text-secondary mb-2">
            What strangers know about the character on sight or by reputation &mdash; name, station,
            occupation, public reputation. The shallow first impression.
          </p>
          <textarea
            id="identity"
            class="qt-input"
            rows="4"
            [value]="form().identity"
            (input)="setField('identity', $any($event.target).value)"
          ></textarea>
        </div>

        <div>
          <label for="description" class="block qt-label mb-2 text-foreground"
            >Description (Optional)</label
          >
          <p class="text-xs qt-text-secondary mb-2">
            How acquaintances perceive the character &mdash; behaviour, mannerisms, frequent verbal
            patterns. Not physical appearance (that lives in physical descriptions).
          </p>
          <textarea
            id="description"
            class="qt-input"
            rows="5"
            [value]="form().description"
            (input)="setField('description', $any($event.target).value)"
          ></textarea>
        </div>

        <div>
          <label for="manifesto" class="block qt-label mb-2 text-foreground"
            >Manifesto (Optional)</label
          >
          <p class="text-xs qt-text-secondary mb-2">
            The foundational tenets of this character &mdash; the basic truths that anchor
            everything else. What this character is, at root.
          </p>
          <textarea
            id="manifesto"
            class="qt-input"
            rows="5"
            [value]="form().manifesto"
            (input)="setField('manifesto', $any($event.target).value)"
          ></textarea>
        </div>

        <div>
          <label for="personality" class="block qt-label mb-2 text-foreground"
            >Personality (Optional)</label
          >
          <p class="text-xs qt-text-secondary mb-2">
            What the character knows about themselves &mdash; inner drivers of speech and behaviour,
            motivations, beliefs.
          </p>
          <textarea
            id="personality"
            class="qt-input"
            rows="5"
            [value]="form().personality"
            (input)="setField('personality', $any($event.target).value)"
          ></textarea>
        </div>

        <div>
          <label for="scenario" class="block qt-label mb-2 text-foreground"
            >Scenario (Optional)</label
          >
          <p class="text-xs qt-text-secondary mb-2">
            Describe the setting and context for conversations.
          </p>
          <textarea
            id="scenario"
            class="qt-input"
            rows="5"
            [value]="form().scenario"
            (input)="setField('scenario', $any($event.target).value)"
          ></textarea>
        </div>

        <div>
          <label for="firstMessage" class="block qt-label mb-2 text-foreground"
            >First Message (Optional)</label
          >
          <p class="text-xs qt-text-secondary mb-2">
            The character&rsquo;s opening message to start conversations.
          </p>
          <textarea
            id="firstMessage"
            class="qt-input"
            rows="4"
            [value]="form().firstMessage"
            (input)="setField('firstMessage', $any($event.target).value)"
          ></textarea>
        </div>

        <div>
          <label for="exampleDialogues" class="block qt-label mb-2 text-foreground"
            >Example Dialogues (Optional)</label
          >
          <p class="text-xs qt-text-secondary mb-2">
            Example conversations to guide the AI&rsquo;s responses.
          </p>
          <textarea
            id="exampleDialogues"
            class="qt-input"
            rows="8"
            [value]="form().exampleDialogues"
            (input)="setField('exampleDialogues', $any($event.target).value)"
          ></textarea>
        </div>

        <div>
          <div class="flex items-center justify-between mb-2">
            <label for="systemPrompt" class="block qt-label text-foreground"
              >System Prompt (Optional)</label
            >
            <button
              type="button"
              class="qt-button-secondary text-xs px-2 py-1"
              disabled
              title="Import a system prompt from a template (not yet available)"
            >
              Import Template
            </button>
          </div>
          <p class="text-xs qt-text-secondary mb-2">
            Custom system instructions (will be combined with auto-generated prompt).
          </p>
          <textarea
            id="systemPrompt"
            class="qt-input"
            rows="5"
            [value]="form().systemPrompt"
            (input)="setField('systemPrompt', $any($event.target).value)"
          ></textarea>
        </div>

        <div>
          <label for="avatarUrl" class="block qt-label mb-2 text-foreground"
            >Avatar URL (Optional)</label
          >
          <input
            type="url"
            id="avatarUrl"
            class="qt-input"
            placeholder="https://example.com/avatar.png"
            [value]="form().avatarUrl"
            (input)="setField('avatarUrl', $any($event.target).value)"
          />
        </div>

        <div>
          <label for="defaultConnectionProfileId" class="block qt-label mb-2 text-foreground">
            Default Connection Profile (Optional)
          </label>
          <!--
            dogfood-#6: options come from an async query; bind per-option with
            [selected], not [value] on the <select> (a [value] set before the
            options render leaves the control blank — P4.6aa select-audit rider).
          -->
          <select
            id="defaultConnectionProfileId"
            class="qt-select"
            (change)="setField('defaultConnectionProfileId', $any($event.target).value)"
          >
            <option value="" [selected]="!form().defaultConnectionProfileId">No default profile</option>
            @for (profile of profiles(); track profile.id) {
              <option [value]="profile.id" [selected]="form().defaultConnectionProfileId === profile.id">
                {{ profile.name }}
              </option>
            }
          </select>
          <p class="mt-1 qt-text-xs">Can be overridden for individual chats</p>
        </div>

        <div class="flex gap-4">
          <button type="submit" class="qt-button flex-1 qt-button-primary" [disabled]="loading()">
            {{ loading() ? 'Creating...' : 'Create Character' }}
          </button>
          <a routerLink="/characters" class="qt-button px-6 py-3 qt-button-secondary text-center">
            Cancel
          </a>
        </div>
      </form>
    </div>
  `,
})
export class NewCharacter {
  private readonly core = inject(CoreClient);
  private readonly router = inject(Router);

  protected readonly form = signal<NewCharacterFormData>(INITIAL_FORM);
  protected readonly loading = signal(false);
  protected readonly error = signal<string | null>(null);

  private readonly profilesQuery = injectQuery(() => ({
    queryKey: ['connection-profiles'],
    queryFn: (): Promise<CharacterConnectionProfile[]> => fetchConnectionProfiles(this.core),
  }));

  protected profiles(): CharacterConnectionProfile[] {
    return this.profilesQuery.data() ?? [];
  }

  protected setField<K extends keyof NewCharacterFormData>(
    key: K,
    value: NewCharacterFormData[K],
  ): void {
    this.form.update((f) => ({ ...f, [key]: value }));
  }

  protected async onSubmit(event: Event): Promise<void> {
    event.preventDefault();
    this.loading.set(true);
    this.error.set(null);
    try {
      const data = await this.core.dispatchData({
        type: 'characterCreate',
        character: buildCreateCharacterBag(this.form()),
      });
      const character = data['character'] as { id: string } | undefined;
      if (!character?.id) {
        throw new Error('Failed to create character');
      }
      this.router.navigate(['/characters', character.id]);
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'An error occurred');
    } finally {
      this.loading.set(false);
    }
  }
}
