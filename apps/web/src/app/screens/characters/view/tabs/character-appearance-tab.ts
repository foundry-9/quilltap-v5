import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../../core/core-client';
import type { CharacterDetail } from '../../../../core/core-contract';
import { MarkdownField } from '../../../../editor/markdown-field';
import { characterKeys, fetchDepictionGuidelines } from '../../characters.api';

/**
 * The Appearance tab (v4 tab id `descriptions`): a read-only render of the
 * physical-description prompt set (`character.physicalDescription` — editing
 * that lives on the character-edit screen, out of this order's scope) plus
 * the Ariel Clause depiction-guidelines editor (`characterDepictionGuidelines`
 * / `characterDepictionGuidelinesUpdate`). Note: in v4 the depiction-
 * guidelines editor actually lives on the EDIT screen
 * (`CharacterEditView.tsx`), not the view-only `DescriptionsTab.tsx` (which is
 * itself the editable physical-description form) — this tab deliberately
 * combines "read the physical description" + "edit the depiction guidelines"
 * per this order's explicit spec, since the vault-write physical-description
 * form is deferred to the edit screen.
 *
 * The guidelines editor is a `qt-markdown-field` carrying
 * `AestheticEditorField`'s `minHeight="8rem"` (`:119`) — v4 renders the
 * guidelines through that component (`CharacterEditView.tsx:338`), not through
 * `DescriptionsTab`. It mounts only once the query settles (the pre-existing
 * gate below, which the swap now depends on: the field absorbs exactly one emit,
 * so an editor mounting ahead of its content would surface the load as an edit).
 */
@Component({
  selector: 'qt-character-appearance-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [MarkdownField],
  template: `
    <div class="space-y-6">
      <div class="character-section-card rounded-lg border qt-border-default qt-bg-card p-6">
        <h2 class="qt-heading-4 text-foreground mb-2">Physical Description</h2>
        @if (physicalDescription(); as pd) {
          <div class="space-y-4">
            @if (pd.usageContext) {
              <div>
                <h3 class="qt-text-label mb-1">Usage Context</h3>
                <p class="qt-text-small whitespace-pre-wrap">{{ pd.usageContext }}</p>
              </div>
            }
            @if (pd.headAndShouldersPrompt) {
              <div>
                <h3 class="qt-text-label mb-1">Head &amp; Shoulders</h3>
                <p class="qt-text-small whitespace-pre-wrap">{{ pd.headAndShouldersPrompt }}</p>
              </div>
            }
            @if (pd.shortPrompt) {
              <div>
                <h3 class="qt-text-label mb-1">Short Prompt</h3>
                <p class="qt-text-small whitespace-pre-wrap">{{ pd.shortPrompt }}</p>
              </div>
            }
            @if (pd.mediumPrompt) {
              <div>
                <h3 class="qt-text-label mb-1">Medium Prompt</h3>
                <p class="qt-text-small whitespace-pre-wrap">{{ pd.mediumPrompt }}</p>
              </div>
            }
            @if (pd.longPrompt) {
              <div>
                <h3 class="qt-text-label mb-1">Long Prompt</h3>
                <p class="qt-text-small whitespace-pre-wrap">{{ pd.longPrompt }}</p>
              </div>
            }
            @if (pd.completePrompt) {
              <div>
                <h3 class="qt-text-label mb-1">Complete Prompt</h3>
                <p class="qt-text-small whitespace-pre-wrap">{{ pd.completePrompt }}</p>
              </div>
            }
            @if (pd.fullDescription) {
              <div>
                <h3 class="qt-text-label mb-1">Full Description</h3>
                <p class="qt-text-small whitespace-pre-wrap">{{ pd.fullDescription }}</p>
              </div>
            }
          </div>
        } @else {
          <p class="qt-text-small qt-text-secondary">
            No physical description set for this character yet. Add one from the character-edit
            screen.
          </p>
        }
      </div>

      <div class="character-section-card rounded-lg border qt-border-default qt-bg-card p-6">
        <h2 class="qt-heading-4 text-foreground mb-2">Depiction Guidelines</h2>
        <p class="qt-text-small mb-4">
          The Ariel Clause: freeform guidance steering how this character is depicted in generated
          images.
        </p>
        @if (guidelinesQuery.isPending()) {
          <p class="qt-text-small qt-text-secondary">Loading…</p>
        } @else {
          <qt-markdown-field
            ariaLabel="Depiction guidelines"
            minHeight="8rem"
            [value]="draft()"
            (contentChange)="draft.set($event)"
          />
          <div class="mt-3 flex items-center gap-3">
            <button type="button" class="qt-button-primary" [disabled]="saving()" (click)="save()">
              {{ saving() ? 'Saving…' : 'Save' }}
            </button>
            @if (saved()) {
              <span class="qt-text-small qt-text-success">Saved</span>
            }
          </div>
        }
      </div>
    </div>
  `,
})
export class CharacterAppearanceTab {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();

  readonly characterId = input.required<string>();
  readonly character = input.required<CharacterDetail>();

  protected readonly physicalDescription = computed(
    () => this.character().physicalDescription ?? null,
  );

  protected readonly draft = signal('');
  protected readonly saving = signal(false);
  protected readonly saved = signal(false);

  protected readonly guidelinesQuery = injectQuery(() => ({
    queryKey: characterKeys.depiction(this.characterId()),
    queryFn: async (): Promise<string> => {
      const content = await fetchDepictionGuidelines(this.core, this.characterId());
      this.draft.set(content);
      return content;
    },
  }));

  protected async save(): Promise<void> {
    this.saving.set(true);
    this.saved.set(false);
    try {
      await this.core.dispatchData({
        type: 'characterDepictionGuidelinesUpdate',
        characterId: this.characterId(),
        content: this.draft(),
      });
      this.saved.set(true);
      await this.queryClient.invalidateQueries({
        queryKey: characterKeys.depiction(this.characterId()),
      });
    } finally {
      this.saving.set(false);
    }
  }
}
