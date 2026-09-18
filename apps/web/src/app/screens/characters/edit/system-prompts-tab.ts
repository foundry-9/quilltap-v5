import { ChangeDetectionStrategy, Component, inject, input, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { CharacterSystemPrompt } from '../../../core/core-contract';
import { Icon } from '../../../ui/icon';
import { characterKeys } from '../characters.api';
import { CharacterPromptImportModal } from '../generators/prompts-editor/import-modal';
import {
  fetchPromptTemplates,
  type PromptTemplateRecord,
} from '../generators/prompts-editor/prompt-templates.api';
import { CharacterPromptPreviewModal } from '../generators/prompts-editor/preview-modal';
import { INITIAL_PROMPT_FORM_DATA, PromptModal, type PromptFormData } from './prompt-modal';
import { ProgressionsSection } from '../../../progressions/progressions-section';
import { SubpromptsSection } from '../../../subprompts/subprompts-section';

/**
 * The System Prompts tab (v4
 * `components/characters/system-prompts-editor/index.tsx`): the prompt list
 * (name, default badge, preview, edit / set-default / delete) plus a
 * create/edit modal, the Preview modal, "Import Template" (P4.9K3, filled
 * in by P4.83 — `openImportModal` ALWAYS refetches the catalogue, v4
 * `useSystemPrompts.ts:265-268`), and — under the
 * prompt list, before the modals, exactly where v4 mounts it
 * (`index.tsx:86-88`) — the Subprompts section (P4.D165, v4 `2f4254b42`).
 * Per-row inline
 * delete confirmation collapses into a single delete button for this round;
 * copy carries over verbatim.
 */
@Component({
  selector: 'qt-character-system-prompts-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    Icon,
    PromptModal,
    CharacterPromptPreviewModal,
    CharacterPromptImportModal,
    ProgressionsSection,
    SubpromptsSection,
  ],
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
          <button type="button" class="qt-button-secondary" (click)="openImportModal()">
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
                    title="Preview"
                    (click)="previewPrompt.set(prompt)"
                  >
                    <qt-icon name="eye" class="w-4 h-4" />
                  </button>
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

    <!-- Subprompts — the smaller, per-chat instructions kept in the vault's
         Subprompts/ folder. Listed under the primary prompts and before the
         modals, exactly where v4 mounts it (index.tsx:86-88). No backticks in
         here: a backtick inside an inline template comment terminates the TS
         template literal, and the errors then blame everything but the
         comment (backtick-in-an-angular-inline-template-comment). -->
    <qt-subprompts-section [characterId]="characterId()" [characterName]="characterName()" />

    <!-- Progressions - the timed conditions this character carries, kept under
         one reserved key in the vault's metadata.json. A sibling of the
         subprompts above: both are things attached to the character rather
         than prose fields of them, and both are read at the top of a turn.
         (v4 index.tsx:91-95, right after SubpromptsSection.) -->
    <qt-progressions-section [characterId]="characterId()" [characterName]="characterName()" />

    @if (modalOpen()) {
      <qt-prompt-modal
        [editingPrompt]="editingPrompt()"
        [initialForm]="importedForm()"
        [saving]="saving()"
        (close)="modalOpen.set(false)"
        (save)="onSave($event)"
      />
    }

    @if (previewPrompt(); as prompt) {
      <qt-character-prompt-preview-modal
        [prompt]="prompt"
        [characterName]="characterName()"
        (close)="previewPrompt.set(null)"
        (edit)="openEdit($event)"
      />
    }

    @if (importModalOpen()) {
      <qt-character-prompt-import-modal
        [templates]="templates()"
        [loading]="loadingTemplates()"
        (close)="importModalOpen.set(false)"
        (importPrompt)="onImport($event)"
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
  protected readonly previewPrompt = signal<CharacterSystemPrompt | null>(null);
  protected readonly importModalOpen = signal(false);
  /** v4 `templates` / `loadingTemplates` (`useSystemPrompts.ts:84-107`). */
  protected readonly templates = signal<PromptTemplateRecord[]>([]);
  protected readonly loadingTemplates = signal(false);
  /** v4 `handleImport` (`useSystemPrompts.ts:134-142`) staged for the create modal. */
  protected readonly importedForm = signal<PromptFormData | null>(null);
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

  /**
   * v4 `openImportModal` (`useSystemPrompts.ts:265-268`): fire the fetch and
   * open, in that order — and ALWAYS refetch, unlike the new-character host,
   * which fetches only when its list is still empty. The two semantics are
   * v4's and they differ; the parity specs pin each one.
   */
  protected openImportModal(): void {
    void this.loadTemplates();
    this.importModalOpen.set(true);
  }

  /** v4 `fetchTemplates` — `templates` is left UNCHANGED on a failure. */
  private async loadTemplates(): Promise<void> {
    this.loadingTemplates.set(true);
    try {
      this.templates.set(await fetchPromptTemplates(this.core));
    } catch (err) {
      console.error('Error fetching templates', {
        error: err instanceof Error ? err.message : String(err),
      });
    } finally {
      this.loadingTemplates.set(false);
    }
  }

  /**
   * v4 `openCreateModal` (`useSystemPrompts.ts:111-117`): a character's FIRST
   * system prompt opens the create form already starred — `isDefault:
   * prompts.length === 0`, "First prompt is default". Seeded through the same
   * `initialForm` door `onImport` uses. (A pre-existing v5 gap the P4.D202 lane
   * recorded and the `baa85e19b` round's §3 review closed — unrelated to bug
   * 154, but the star beat's own gesture had to tick the box by hand for it.)
   */
  protected openCreate(): void {
    this.editingPrompt.set(null);
    this.importedForm.set(
      this.prompts().length === 0 ? { ...INITIAL_PROMPT_FORM_DATA, isDefault: true } : null,
    );
    this.error.set(null);
    this.modalOpen.set(true);
  }

  protected openEdit(prompt: CharacterSystemPrompt): void {
    this.editingPrompt.set(prompt);
    this.importedForm.set(null);
    this.error.set(null);
    this.modalOpen.set(true);
  }

  /** v4 `handleImport` (`useSystemPrompts.ts:134-142`) — stage the imported
   *  content into the create modal, first prompt defaults to default. */
  protected onImport(event: { content: string; suggestedName: string }): void {
    this.editingPrompt.set(null);
    this.importedForm.set({
      name: event.suggestedName,
      content: event.content,
      isDefault: this.prompts().length === 0,
    });
    this.importModalOpen.set(false);
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

  /**
   * v4 `handleSetDefault` (`useSystemPrompts.ts:222-268` post-`baa85e19b`,
   * bug 154). Move the badge in the query cache BEFORE the round trip, so the
   * star reads as a switch rather than a request; the refresh confirms it, and
   * a failure restores the snapshot and THEN refetches the server's word —
   * both, in that order.
   *
   * ⚠ **NO-COUNTERPART on v5:** the other half of v4's hunk moved the star off
   * `PUT ?action=update-prompt`, an action nothing served, onto the prompt's
   * own route. v5's star never had the dead route — it has always posted the
   * `characterPromptSetDefault` verb — so only the optimistic write and the
   * rollback port.
   *
   * v5 has no `success` signal here and renders no success sentence, so v4's
   * `setSuccess('Default prompt updated')` + its 3 s clear have nowhere to
   * land. That is a PRE-EXISTING gap in this tab (v4 shows the sentence on
   * every mutation, not just this one), recorded rather than invented here.
   */
  protected async setDefault(promptId: string): Promise<void> {
    const key = characterKeys.prompts(this.characterId());
    const previous = this.queryClient.getQueryData<CharacterSystemPrompt[]>(key);
    this.queryClient.setQueryData<CharacterSystemPrompt[]>(key, (current) =>
      current?.map((p) => ({ ...p, isDefault: p.id === promptId })),
    );

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
      if (previous) {
        this.queryClient.setQueryData<CharacterSystemPrompt[]>(key, previous);
      }
      await this.refetch();
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

  /**
   * v4's `fetchPrompts()` — the rollback's second half. An invalidate would be
   * a no-op against a cache the rollback has just rewritten (the data is fresh
   * and nothing is observing a stale key), so the server's word is fetched
   * outright.
   */
  private async refetch(): Promise<void> {
    await this.queryClient.refetchQueries({
      queryKey: characterKeys.prompts(this.characterId()),
    });
  }
}
