import { ChangeDetectionStrategy, Component, inject, input, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { CharacterSystemPrompt } from '../../../core/core-contract';
import { Icon } from '../../../ui/icon';
import { characterKeys } from '../characters.api';
import { PromptModal, type PromptFormData } from './prompt-modal';

/**
 * The System Prompts tab (v4
 * `components/characters/system-prompts-editor/index.tsx`): the prompt list
 * (name, default badge, preview, edit / set-default / delete) plus a
 * create/edit modal. "Import Template" is a named deferral (disabled, no
 * `promptTemplate*` contract yet). Preview-modal and per-row inline delete
 * confirmation collapse into a single delete button for this round; copy
 * carries over verbatim.
 */
@Component({
  selector: 'qt-character-system-prompts-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, PromptModal],
  template: `
    <div class="space-y-4">
      <div class="flex justify-between items-center">
        <div>
          <h3 class="qt-heading-4 text-foreground">System Prompts</h3>
          <p class="qt-text-small">
            Manage multiple system prompts for {{ characterName() }}. Select which one to use when
            creating or configuring a chat.
          </p>
        </div>
        <div class="flex gap-2">
          <button
            type="button"
            class="qt-button-secondary"
            disabled
            title="Import from a prompt template (not yet available)"
          >
            Import Template
          </button>
          <button type="button" class="qt-button-primary" (click)="openCreate()">
            + Add Prompt
          </button>
        </div>
      </div>

      @if (error()) {
        <div class="qt-alert-error">{{ error() }}</div>
      }

      @if (promptsQuery.isPending()) {
        <div class="text-center py-8 qt-text-secondary">Loading prompts...</div>
      } @else if (prompts().length === 0) {
        <div class="qt-card text-center">
          <p class="qt-text-small mb-4">
            No system prompts yet. Add your first prompt or import from a template.
          </p>
          <button type="button" class="qt-button-primary" (click)="openCreate()">
            Create First Prompt
          </button>
        </div>
      } @else {
        <div class="space-y-3">
          @for (prompt of prompts(); track prompt.id) {
            <div class="qt-card">
              <div class="flex justify-between items-start">
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2 mb-1">
                    <h4 class="qt-text-primary truncate">{{ prompt.name }}</h4>
                    @if (prompt.isDefault) {
                      <span class="qt-badge-primary">Default</span>
                    }
                  </div>
                  <p class="qt-text-small line-clamp-2">{{ prompt.content.slice(0, 150) }}...</p>
                </div>
                <div class="flex items-center gap-1 ml-4">
                  <button
                    type="button"
                    class="qt-button-icon qt-button-ghost"
                    title="Edit"
                    (click)="openEdit(prompt)"
                  >
                    <qt-icon name="pencil" class="w-4 h-4" />
                  </button>
                  @if (!prompt.isDefault) {
                    <button
                      type="button"
                      class="qt-button-icon qt-button-ghost hover:text-primary"
                      title="Set as default"
                      [disabled]="saving()"
                      (click)="setDefault(prompt.id)"
                    >
                      <qt-icon name="star" class="w-4 h-4" />
                    </button>
                  }
                  <button
                    type="button"
                    class="qt-button-icon qt-button-ghost hover:qt-text-destructive"
                    title="Delete"
                    [disabled]="saving()"
                    (click)="deletePrompt(prompt.id)"
                  >
                    <qt-icon name="trash" class="w-4 h-4" />
                  </button>
                </div>
              </div>
            </div>
          }
        </div>
      }
    </div>

    @if (modalOpen()) {
      <qt-prompt-modal
        [editingPrompt]="editingPrompt()"
        [saving]="saving()"
        (close)="modalOpen.set(false)"
        (save)="onSave($event)"
      />
    }
  `,
})
export class CharacterSystemPromptsTab {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();

  readonly characterId = input.required<string>();
  readonly characterName = input<string>('Character');

  protected readonly modalOpen = signal(false);
  protected readonly editingPrompt = signal<CharacterSystemPrompt | null>(null);
  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);

  protected readonly promptsQuery = injectQuery(() => ({
    queryKey: characterKeys.prompts(this.characterId()),
    queryFn: async (): Promise<CharacterSystemPrompt[]> => {
      const data = await this.core.dispatchData({
        type: 'characterPromptList',
        characterId: this.characterId(),
      });
      return (data['prompts'] as CharacterSystemPrompt[]) ?? [];
    },
  }));

  protected prompts(): CharacterSystemPrompt[] {
    return this.promptsQuery.data() ?? [];
  }

  protected openCreate(): void {
    this.editingPrompt.set(null);
    this.error.set(null);
    this.modalOpen.set(true);
  }

  protected openEdit(prompt: CharacterSystemPrompt): void {
    this.editingPrompt.set(prompt);
    this.error.set(null);
    this.modalOpen.set(true);
  }

  protected async onSave(form: PromptFormData): Promise<void> {
    this.saving.set(true);
    this.error.set(null);
    try {
      const editing = this.editingPrompt();
      if (editing) {
        await this.core.dispatchData({
          type: 'characterPromptUpdate',
          characterId: this.characterId(),
          promptId: editing.id,
          name: form.name,
          content: form.content,
          isDefault: form.isDefault,
        });
      } else {
        await this.core.dispatchData({
          type: 'characterPromptCreate',
          characterId: this.characterId(),
          name: form.name,
          content: form.content,
          isDefault: form.isDefault,
        });
      }
      this.modalOpen.set(false);
      await this.refresh();
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to save prompt');
    } finally {
      this.saving.set(false);
    }
  }

  protected async setDefault(promptId: string): Promise<void> {
    this.saving.set(true);
    this.error.set(null);
    try {
      await this.core.dispatchData({
        type: 'characterPromptSetDefault',
        characterId: this.characterId(),
        promptId,
      });
      await this.refresh();
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to set default');
    } finally {
      this.saving.set(false);
    }
  }

  protected async deletePrompt(promptId: string): Promise<void> {
    this.saving.set(true);
    this.error.set(null);
    try {
      await this.core.dispatchData({
        type: 'characterPromptDelete',
        characterId: this.characterId(),
        promptId,
      });
      await this.refresh();
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to delete prompt');
    } finally {
      this.saving.set(false);
    }
  }

  private async refresh(): Promise<void> {
    await this.queryClient.invalidateQueries({
      queryKey: characterKeys.prompts(this.characterId()),
    });
  }
}
