import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../../core/core-client';
import type { CharacterDetail, CharacterListItem } from '../../../../core/core-contract';
import { Icon } from '../../../../ui/icon';
import { Modal } from '../../../../ui/modal';
import { ToastService } from '../../../../ui/toast.service';
import { characterKeys } from '../../characters.api';
import { TemplateDisplay } from '../template-display';
import {
  applyTemplateTransform,
  collectTemplateFields,
  countTemplateLiterals,
  countTemplateReplacements,
  replaceTemplateWithName,
  replaceWithTemplate,
} from '../template-highlighter';

/** One prose field rendered with template highlighting. */
interface DetailField {
  label: string;
  content: string;
}

/**
 * The Details tab (v4 `components/CharacterDetails.tsx`): a read-only render
 * of identity/description/manifesto/personality/scenarios/firstMessage/
 * exampleDialogues with `{{char}}`/`{{user}}` highlighting, plus the four
 * template replace/reverse buttons (v4 `useCharacterView.ts:228-305`). A
 * forward click rewrites hard-coded names to template tokens; a reverse click
 * restores a concrete name. Saves fan out to the main `characterUpdate` PUT
 * (scalars/scenarios/physicalDescription) and, for changed system prompts, the
 * dedicated `characterPromptUpdate` op (the v4 PUT schema strips
 * `systemPrompts`) — then invalidate the shared detail query so every tab
 * resyncs.
 */
@Component({
  selector: 'qt-character-details-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, Icon, Modal, TemplateDisplay],
  template: `
    <div class="space-y-6">
      <div class="flex flex-wrap items-center justify-end gap-2">
        @if (templateCounts().charCount > 0) {
          <button
            type="button"
            [disabled]="templateBusy()"
            class="flex items-center gap-1.5 rounded-lg border qt-border-primary/40 qt-bg-primary/10 px-3 py-2 qt-label text-primary qt-shadow-sm transition hover:qt-bg-primary/15 disabled:cursor-not-allowed disabled:opacity-50"
            [title]="replaceCharTitle()"
            (click)="replaceTemplate('char')"
          >
            <qt-icon name="refresh" class="w-4 h-4" />
            <span class="hidden sm:inline">{{ character().name }}</span>
            <span>→</span>
            <code class="rounded qt-bg-primary/20 px-1 text-xs text-primary">{{ '{{char}}' }}</code>
            <span class="text-xs">({{ templateCounts().charCount }})</span>
          </button>
        }
        @if (defaultPartnerName() && templateCounts().userCount > 0) {
          <button
            type="button"
            [disabled]="templateBusy()"
            class="flex items-center gap-1.5 rounded-lg border qt-border-success/40 qt-bg-success/10 px-3 py-2 qt-label qt-text-success qt-shadow-sm transition hover:qt-bg-success/20 disabled:cursor-not-allowed disabled:opacity-50"
            [title]="replaceUserTitle()"
            (click)="replaceTemplate('user')"
          >
            <qt-icon name="refresh" class="w-4 h-4" />
            <span class="hidden sm:inline">{{ defaultPartnerName() }}</span>
            <span>→</span>
            <code class="rounded qt-bg-success/20 px-1 text-xs qt-text-success"
              >{{ '{{user}}' }}</code
            >
            <span class="text-xs">({{ templateCounts().userCount }})</span>
          </button>
        }
        @if (literalCounts().charCount > 0) {
          <button
            type="button"
            [disabled]="templateBusy()"
            class="flex items-center gap-1.5 rounded-lg border qt-border-primary/40 qt-bg-primary/10 px-3 py-2 qt-label text-primary qt-shadow-sm transition hover:qt-bg-primary/15 disabled:cursor-not-allowed disabled:opacity-50"
            [title]="reverseCharTitle()"
            (click)="reverseTemplate('char')"
          >
            <qt-icon name="refresh" class="w-4 h-4" />
            <code class="rounded qt-bg-primary/20 px-1 text-xs text-primary">{{ '{{char}}' }}</code>
            <span>→</span>
            <span class="hidden sm:inline">{{ character().name }}</span>
            <span class="text-xs">({{ literalCounts().charCount }})</span>
          </button>
        }
        @if (literalCounts().userCount > 0 && otherUserControlled().length > 0) {
          <button
            type="button"
            [disabled]="templateBusy()"
            class="flex items-center gap-1.5 rounded-lg border qt-border-success/40 qt-bg-success/10 px-3 py-2 qt-label qt-text-success qt-shadow-sm transition hover:qt-bg-success/20 disabled:cursor-not-allowed disabled:opacity-50"
            [title]="reverseUserTitle()"
            (click)="openReverseUserDialog()"
          >
            <qt-icon name="refresh" class="w-4 h-4" />
            <code class="rounded qt-bg-success/20 px-1 text-xs qt-text-success"
              >{{ '{{user}}' }}</code
            >
            <span>→</span>
            <span class="hidden sm:inline">name…</span>
            <span class="text-xs">({{ literalCounts().userCount }})</span>
          </button>
        }
        <!-- P4.D64 (v4 CharacterDetails.tsx:128-129): "A disabled fieldset
             can't inert a Link, so the edit door hides itself for an archived
             character (the write guard is the lock)." -->
        @if (!character().archivedAt) {
          <a
            [routerLink]="['/characters', characterId(), 'edit']"
            class="flex items-center gap-2 rounded-lg bg-primary px-4 py-2 text-sm font-semibold text-primary-foreground qt-shadow-sm transition hover:qt-bg-primary/90"
          >
            <qt-icon name="pencil" class="w-4 h-4" />
            Edit Character
          </a>
        }
      </div>

      <div class="space-y-6">
        @for (field of fields(); track field.label) {
          <div>
            <h2 class="qt-heading-4 text-foreground mb-2">{{ field.label }}</h2>
            <div class="qt-text-small whitespace-pre-wrap">
              <qt-template-display
                [content]="field.content"
                [characterName]="character().name"
                [userCharacterName]="defaultPartnerName()"
              />
            </div>
          </div>
        }

        @if (character().systemPrompts.length > 0) {
          <div class="rounded-lg border qt-border-default qt-bg-muted/30 p-4">
            <div class="flex items-center justify-between flex-wrap gap-2">
              <div class="flex items-center gap-2">
                <qt-icon name="code" class="w-5 h-5 text-primary" />
                <span class="text-sm qt-text-primary">Active System Prompt:</span>
                <span class="qt-text-small">{{ activePromptName() }}</span>
                @if (character().systemPrompts.length > 1) {
                  <span class="qt-text-xs">(+{{ character().systemPrompts.length - 1 }} more)</span>
                }
              </div>
              <a
                [routerLink]="['/characters', characterId()]"
                [queryParams]="{ tab: 'system-prompts' }"
                class="text-sm text-primary hover:text-primary/80"
              >
                View all →
              </a>
            </div>
          </div>
        }
      </div>
    </div>

    @if (showReverseUserDialog()) {
      <qt-modal
        title="Restore {{ '{{user}}' }} to a name"
        maxWidth="md"
        (close)="showReverseUserDialog.set(false)"
      >
        <div class="space-y-4">
          <p class="qt-text-small">
            Choose which user-controlled character's name should replace every
            <code class="rounded qt-bg-muted px-1 text-xs">{{ '{{user}}' }}</code> token in this
            character's prompts.
          </p>
          <!--
            dogfood-#6: the options are dynamic (otherUserControlled() — a computed
            over the async userControlledCharacters input), so bind the selection
            per-option with [selected] rather than [value] on the <select>. A
            [value] evaluated before the options render leaves the native control
            blank (the P4.6aa select-audit; this closes the standing finding-#6 audit).
          -->
          <select class="qt-select" (change)="reverseUserSelection.set($any($event.target).value)">
            @for (char of otherUserControlled(); track char.id) {
              <option [value]="char.id" [selected]="char.id === reverseUserSelection()">
                {{ char.name }}{{ char.title ? ' - ' + char.title : '' }}
              </option>
            }
          </select>
        </div>
        <div qt-modal-footer class="flex gap-3 justify-end">
          <button
            type="button"
            class="qt-button-secondary"
            (click)="showReverseUserDialog.set(false)"
          >
            Cancel
          </button>
          <button type="button" class="qt-button-primary" (click)="confirmReverseUser()">
            Replace {{ '{{user}}' }}
          </button>
        </div>
      </qt-modal>
    }
  `,
})
export class CharacterDetailsTab {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();
  private readonly toasts = inject(ToastService);

  readonly characterId = input.required<string>();
  readonly character = input.required<CharacterDetail>();
  readonly defaultPartnerName = input<string | null>(null);
  readonly userControlledCharacters = input<CharacterListItem[]>([]);

  protected readonly replacingTemplate = signal<'char' | 'user' | null>(null);
  protected readonly reversingTemplate = signal<'char' | 'user' | null>(null);
  protected readonly showReverseUserDialog = signal(false);
  protected readonly reverseUserSelection = signal<string>('');

  protected readonly templateBusy = computed(
    () => this.replacingTemplate() !== null || this.reversingTemplate() !== null,
  );

  protected readonly otherUserControlled = computed(() =>
    this.userControlledCharacters().filter((c) => c.id !== this.characterId()),
  );

  private readonly descriptors = computed(() => collectTemplateFields(this.character()));

  protected readonly templateCounts = computed(() => {
    const fields = Object.fromEntries(this.descriptors().map((d) => [d.key, d.value]));
    return countTemplateReplacements(fields, this.character().name, this.defaultPartnerName());
  });

  protected readonly literalCounts = computed(() =>
    countTemplateLiterals(this.descriptors().map((d) => d.value)),
  );

  protected readonly replaceCharTitle = computed(
    () =>
      `Replace ${this.templateCounts().charCount} occurrences of "${this.character().name}" with {{char}}`,
  );
  protected readonly replaceUserTitle = computed(
    () =>
      `Replace ${this.templateCounts().userCount} occurrences of "${this.defaultPartnerName()}" with {{user}}`,
  );
  protected readonly reverseCharTitle = computed(
    () =>
      `Replace ${this.literalCounts().charCount} occurrences of {{char}} with "${this.character().name}"`,
  );
  protected readonly reverseUserTitle = computed(
    () =>
      `Replace ${this.literalCounts().userCount} occurrences of {{user}} with a user-controlled character's name`,
  );

  protected readonly activePromptName = computed(() => {
    const prompts = this.character().systemPrompts;
    return prompts.find((p) => p.isDefault)?.name ?? prompts[0]?.name ?? 'None';
  });

  protected readonly fields = computed<DetailField[]>(() => {
    const c = this.character();
    const out: DetailField[] = [];
    if (c.identity) out.push({ label: 'Identity', content: c.identity });
    if (c.description) out.push({ label: 'Description', content: c.description });
    if (c.manifesto) out.push({ label: 'Manifesto', content: c.manifesto });
    if (c.personality) out.push({ label: 'Personality', content: c.personality });
    for (const scenario of c.scenarios) {
      out.push({
        label: c.scenarios.length > 1 ? scenario.title : 'Scenario',
        content: scenario.content,
      });
    }
    if (c.firstMessage) out.push({ label: 'First Message', content: c.firstMessage });
    if (c.exampleDialogues) out.push({ label: 'Example Dialogues', content: c.exampleDialogues });
    return out;
  });

  protected async replaceTemplate(type: 'char' | 'user'): Promise<void> {
    const name = type === 'char' ? this.character().name : this.defaultPartnerName();
    if (!name) return;
    const template = type === 'char' ? '{{char}}' : '{{user}}';
    this.replacingTemplate.set(type);
    try {
      await this.runTemplateSave(
        (text) => replaceWithTemplate(text, name, template),
        `Replaced ${type === 'char' ? 'character name' : 'user character name'} with ${template}`,
      );
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to replace template');
    } finally {
      this.replacingTemplate.set(null);
    }
  }

  protected async reverseTemplate(type: 'char' | 'user', chosenName?: string): Promise<void> {
    const name = type === 'char' ? this.character().name : chosenName;
    if (!name) return;
    this.reversingTemplate.set(type);
    try {
      await this.runTemplateSave(
        (text) => replaceTemplateWithName(text, type, name),
        type === 'char' ? `Restored {{char}} to ${name}` : `Restored {{user}} to ${name}`,
      );
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to restore template');
    } finally {
      this.reversingTemplate.set(null);
    }
  }

  protected openReverseUserDialog(): void {
    this.reverseUserSelection.set(this.otherUserControlled()[0]?.id ?? '');
    this.showReverseUserDialog.set(true);
  }

  protected confirmReverseUser(): void {
    const chosen =
      this.otherUserControlled().find((c) => c.id === this.reverseUserSelection()) ??
      this.otherUserControlled()[0];
    this.showReverseUserDialog.set(false);
    if (chosen) {
      void this.reverseTemplate('user', chosen.name);
    }
  }

  /**
   * v4 `useCharacterView.ts:237-262` over `applyCharacterFieldUpdates`: partial
   * failures are COLLECTED, not thrown (each endpoint is independent and
   * idempotent), so one failing prompt write doesn't stop the others or hide
   * behind a generic exception. `errors` empty ⇒ success toast; non-empty ⇒
   * every collected message joined into one error toast. An empty transform
   * (nothing changed) short-circuits before any dispatch.
   */
  private async runTemplateSave(
    transform: (text: string) => string,
    successMsg: string,
  ): Promise<void> {
    const { mainUpdates, changedSystemPrompts } = applyTemplateTransform(
      this.character(),
      transform,
    );
    if (Object.keys(mainUpdates).length === 0 && changedSystemPrompts.length === 0) {
      this.toasts.showSuccess('No replacements needed');
      return;
    }
    const characterId = this.characterId();
    const errors: string[] = [];
    try {
      for (const prompt of changedSystemPrompts) {
        try {
          await this.core.dispatchData({
            type: 'characterPromptUpdate',
            characterId,
            promptId: prompt.id,
            content: prompt.content,
          });
        } catch (err) {
          errors.push(err instanceof Error ? err.message : 'A system prompt could not be saved.');
        }
      }
      if (Object.keys(mainUpdates).length > 0) {
        try {
          await this.core.dispatchData({
            type: 'characterUpdate',
            characterId,
            character: mainUpdates,
          });
        } catch (err) {
          errors.push(err instanceof Error ? err.message : 'The character could not be updated.');
        }
      }
    } finally {
      await this.queryClient.invalidateQueries({ queryKey: characterKeys.detail(characterId) });
    }
    if (errors.length > 0) {
      this.toasts.showError(errors.join(' '));
    } else {
      this.toasts.showSuccess(successMsg);
    }
  }
}
