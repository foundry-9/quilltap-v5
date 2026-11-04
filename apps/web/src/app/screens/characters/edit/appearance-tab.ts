import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  linkedSignal,
  signal,
} from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { CharacterPhysicalDescription } from '../../../core/core-contract';
import { MarkdownField } from '../../../editor/markdown-field';
import { PROMPT_FIELD_HINTS } from '../../../ui/prompt-field-hints';
import { PromptFieldLabel } from '../../../ui/prompt-field-label';
import { ToastService } from '../../../ui/toast.service';
import { fetchCharacter, fetchDepictionGuidelines, characterKeys } from '../characters.api';

/** The physical-description editable form (the fields New/Edit both send, v4-faithful). */
interface PhysicalDescriptionForm {
  name: string;
  headAndShouldersPrompt: string;
  shortPrompt: string;
  mediumPrompt: string;
  longPrompt: string;
  completePrompt: string;
  fullDescription: string;
  /** v4 `b27e5089a` — false = a figure with no head, shoulders or body to speak of. */
  humanoid: boolean;
}

const EMPTY_PHYSICAL_FORM: PhysicalDescriptionForm = {
  name: '',
  headAndShouldersPrompt: '',
  shortPrompt: '',
  mediumPrompt: '',
  longPrompt: '',
  completePrompt: '',
  fullDescription: '',
  humanoid: true,
};

function loadPhysicalDescription(pd: CharacterPhysicalDescription | null): PhysicalDescriptionForm {
  if (!pd) {
    return EMPTY_PHYSICAL_FORM;
  }
  return {
    name: pd.name ?? '',
    headAndShouldersPrompt: pd.headAndShouldersPrompt ?? '',
    shortPrompt: pd.shortPrompt ?? '',
    mediumPrompt: pd.mediumPrompt ?? '',
    longPrompt: pd.longPrompt ?? '',
    completePrompt: pd.completePrompt ?? '',
    fullDescription: pd.fullDescription ?? '',
    // Absent means true (v4 `DescriptionsTab.tsx:52`: `pd.humanoid !== false`).
    humanoid: pd.humanoid !== false,
  };
}

/**
 * The Appearance tab (v4 `view/components/DescriptionsTab.tsx` +
 * `AestheticEditorField` for the depiction guidelines — v4's edit screen stacks
 * exactly these two, `CharacterEditView.tsx:333-347`): the physical
 * description prompt set — a SEPARATE `characterUpdate` PUT scoped to
 * `physicalDescription` (v4 PATCHes the character row directly; the vault
 * write overlay routes it) — and the depiction-guidelines (Ariel Clause) text,
 * saved via `characterDepictionGuidelinesUpdate`. Each block has its own
 * explicit Save, independent of the Details tab's "Save Character".
 *
 * All seven fields are `qt-markdown-field`s carrying v4's per-field `minHeight`
 * (`DescriptionsTab.tsx:235-337`: 4/4/6/8/10/10rem; the guidelines take
 * `AestheticEditorField`'s `8rem`). Each block renders its editors only once its
 * query settles — v4's own gates (`DescriptionsTab.tsx:157`,
 * `AestheticEditorField.tsx:117`) and load-bearing: the field absorbs exactly
 * one emit, so an editor that mounts ahead of its content would surface the load
 * as an edit and let a plain Save rewrite the stored bytes in normalized form.
 * The physical draft is therefore DERIVED from the query's data (a
 * `linkedSignal`), so it is seeded the moment data exists — including data the
 * edit page's own `characterKeys.detail` query already cached, which never runs
 * this tab's `queryFn` (the unify-236c04e35 finding: a seed inside the `queryFn`
 * left the form empty whenever the tab opened within the 5 s `staleTime`, and a
 * Save then wrote blank prompts). (v4 pairs its gate with a
 * post-load `remountKey` bump; that is redundant once the editor cannot mount
 * empty, and a re-key alongside a value change would double-emit.)
 *
 * The guidelines block carries v4's `disabledHint` arm (P4.6aw): a character with
 * no document vault has nowhere to store guidelines, so the editor and its Save
 * are replaced by the warning and NO load fires — v4 suppresses proactively
 * (`AestheticEditorField.tsx:54-56,113-114`) rather than letting the user type,
 * Save, and only then meet the server's reactive `bad_request` ("Character has no
 * document vault to store depiction guidelines", `api/characters.rs:287-289`).
 * The three-arm order below is v4's ternary (`:113-117`): hint, then loading,
 * then the editor.
 */
@Component({
  selector: 'qt-character-appearance-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [MarkdownField, PromptFieldLabel],
  template: `
    <div class="space-y-6">
      <div class="qt-card space-y-4">
        <div>
          <h3 class="qt-heading-4 text-foreground">Physical Descriptions</h3>
          <p class="qt-text-small">
            Prompt fragments used when generating images of this character, at different levels of
            detail.
          </p>
        </div>

        @if (characterQuery.isPending()) {
          <div class="py-6 text-center qt-text-secondary">Loading physical description...</div>
        } @else {
          <div>
            <label for="physical-name" class="block qt-label mb-2">Name</label>
            <input
              id="physical-name"
              type="text"
              class="qt-input"
              [value]="physicalForm().name"
              (input)="setPhysicalField('name', $any($event.target).value)"
            />
          </div>
          <!-- Humanoid — whether the figure has a body to draw at all
               (v4 b27e5089a, DescriptionsTab.tsx:218-241). -->
          <div class="flex items-start justify-between gap-4">
            <div class="flex-1">
              <label for="physical-humanoid" class="block qt-text-label">Humanoid</label>
              <p class="text-xs qt-text-secondary mt-1">
                Untick for a character with no head, shoulders or body to speak of — a cluster of
                interlocking spheres, a drifting lattice of light. The Lantern and Aurora will then
                paint the whole of its form and never so much as hint at a face, a neck or a pair of
                shoulders, and the character is reminded it has none.
              </p>
            </div>
            <label class="inline-flex items-center cursor-pointer select-none">
              <input
                id="physical-humanoid"
                type="checkbox"
                class="h-5 w-5 qt-accent-primary"
                [checked]="physicalForm().humanoid"
                (change)="setPhysicalField('humanoid', $any($event.target).checked)"
              />
            </label>
          </div>
          <!-- Shared form-of-address note for every prompt variant below —
               attached once here rather than repeated per field
               (v4 DescriptionsTab.tsx:216-218). -->
          <qt-prompt-field-label [hint]="hints.physicalDescription" />

          <div>
            <label class="block qt-label mb-2">Head &amp; Shoulders Prompt</label>
            <!-- v4 b27e5089a (DescriptionsTab.tsx:257-262): the hint follows the
                 Humanoid box. v5 had no per-field sentence here before, so both
                 arms are new. -->
            <p class="text-xs qt-text-secondary mb-2" data-testid="physical-headshoulders-hint">
              {{
                physicalForm().humanoid
                  ? 'Used for avatar generation — describe only the head, face, hair, expression and neckline; nothing below the shoulders.'
                  : 'Not used for a non-humanoid character — its avatar is drawn from the Medium prompt, whole form in frame.'
              }}
            </p>
            <qt-markdown-field
              ariaLabel="Head and shoulders prompt"
              minHeight="4rem"
              [value]="physicalForm().headAndShouldersPrompt"
              (contentChange)="setPhysicalField('headAndShouldersPrompt', $event)"
            />
          </div>
          <div>
            <label class="block qt-label mb-2">Short Prompt</label>
            <qt-markdown-field
              ariaLabel="Short prompt"
              minHeight="4rem"
              [value]="physicalForm().shortPrompt"
              (contentChange)="setPhysicalField('shortPrompt', $event)"
            />
          </div>
          <div>
            <label class="block qt-label mb-2">Medium Prompt</label>
            <qt-markdown-field
              ariaLabel="Medium prompt"
              minHeight="6rem"
              [value]="physicalForm().mediumPrompt"
              (contentChange)="setPhysicalField('mediumPrompt', $event)"
            />
          </div>
          <div>
            <label class="block qt-label mb-2">Long Prompt</label>
            <qt-markdown-field
              ariaLabel="Long prompt"
              minHeight="8rem"
              [value]="physicalForm().longPrompt"
              (contentChange)="setPhysicalField('longPrompt', $event)"
            />
          </div>
          <div>
            <label class="block qt-label mb-2">Complete Prompt</label>
            <qt-markdown-field
              ariaLabel="Complete prompt"
              minHeight="10rem"
              [value]="physicalForm().completePrompt"
              (contentChange)="setPhysicalField('completePrompt', $event)"
            />
          </div>
          <div>
            <label class="block qt-label mb-2">Full Description</label>
            <qt-markdown-field
              ariaLabel="Full description"
              minHeight="10rem"
              [value]="physicalForm().fullDescription"
              (contentChange)="setPhysicalField('fullDescription', $event)"
            />
          </div>

          <div class="flex justify-end gap-3">
            @if (hasPhysicalDescription()) {
              <button
                type="button"
                class="qt-button-secondary"
                [disabled]="savingPhysical()"
                (click)="clearPhysicalDescription()"
              >
                Clear
              </button>
            }
            <button
              type="button"
              class="qt-button-primary"
              [disabled]="savingPhysical()"
              (click)="savePhysicalDescription()"
            >
              {{ savingPhysical() ? 'Saving...' : 'Save Physical Descriptions' }}
            </button>
          </div>
        }
      </div>

      <div class="qt-card space-y-3">
        <div>
          <h3 class="qt-heading-4 text-foreground">Depiction Guidelines (the Ariel Clause)</h3>
          <p class="qt-text-small">
            This character&apos;s own rules about how they may or may not be depicted in story
            backgrounds and ad-hoc images. These are mandatory constraints, passed to the
            image-prompt generator whenever this character appears &mdash; they are never applied to
            plain avatars.
          </p>
        </div>

        @if (guidelinesError()) {
          <div class="qt-alert-error">{{ guidelinesError() }}</div>
        }

        @if (noVault()) {
          <p class="qt-text-small qt-text-warning">{{ NO_VAULT_HINT }}</p>
        } @else if (guidelinesQuery.isPending()) {
          <div class="qt-text-secondary qt-text-small py-4">Loading…</div>
        } @else {
          <qt-markdown-field
            ariaLabel="Depiction guidelines"
            minHeight="8rem"
            [value]="guidelinesDraft()"
            (contentChange)="guidelinesDraft.set($event)"
          />

          <div class="flex justify-end">
            <button
              type="button"
              class="qt-button-primary"
              [disabled]="savingGuidelines()"
              (click)="saveGuidelines()"
            >
              {{ savingGuidelines() ? 'Saving...' : 'Save Guidelines' }}
            </button>
          </div>
        }
      </div>
    </div>
  `,
})
export class CharacterAppearanceTab {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();
  private readonly toasts = inject(ToastService);

  readonly characterId = input.required<string>();

  protected readonly hints = PROMPT_FIELD_HINTS;

  protected readonly savingPhysical = signal(false);

  protected readonly guidelinesDraft = signal('');
  protected readonly savingGuidelines = signal(false);
  protected readonly guidelinesError = signal<string | null>(null);

  protected readonly characterQuery = injectQuery(() => ({
    queryKey: characterKeys.detail(this.characterId()),
    queryFn: () => fetchCharacter(this.core, this.characterId()),
  }));

  // Derived from the query's data, so it is already set the moment the query
  // leaves the pending state — whichever `queryFn` filled the shared cache — and
  // the template never mounts an editor ahead of its content (see the class
  // doc). v4 re-seeds on every fetch too (`DescriptionsTab.tsx:65-67` runs on
  // each `fetchCharacter`), including the post-save refetch.
  protected readonly physicalForm = linkedSignal<PhysicalDescriptionForm>(() => {
    const character = this.characterQuery.data();
    return character ? loadPhysicalDescription(character.physicalDescription) : EMPTY_PHYSICAL_FORM;
  });

  /**
   * v4 `CharacterEditView.tsx:343-347`: `character?.characterDocumentMountPointId
   * ? undefined : '<the warning>'`. A character with no vault has nowhere to
   * store guidelines, so the editor is suppressed behind the warning rather than
   * letting a Save hit the server's reactive `bad_request`. Note this is TRUE
   * while the character is still loading (`character?.` is nullish then) — v4
   * behaves identically, showing the hint until the row arrives.
   */
  protected readonly noVault = computed(
    () => !this.characterQuery.data()?.characterDocumentMountPointId,
  );

  /** v4 `DescriptionsTab.tsx:132-133`: the Clear button only shows once a
   *  physical description already exists to remove. */
  protected readonly hasPhysicalDescription = computed(
    () => !!this.characterQuery.data()?.physicalDescription,
  );

  // `enabled` is v4's load short-circuit (`AestheticEditorField.tsx:54-56` — the
  // effect returns before `fetch` when `disabledHint` is set). Its dep array
  // (`:76`) carries `disabledHint`, so the fetch fires once the character lands
  // WITH a vault; gating on `noVault()` reproduces that re-run exactly.
  protected readonly guidelinesQuery = injectQuery(() => ({
    queryKey: characterKeys.depiction(this.characterId()),
    enabled: !this.noVault(),
    queryFn: async () => {
      const content = await fetchDepictionGuidelines(this.core, this.characterId());
      this.guidelinesDraft.set(content);
      return content;
    },
  }));

  /** v4's warning, verbatim (`CharacterEditView.tsx:346`). */
  protected readonly NO_VAULT_HINT =
    'This character has no document vault yet, so depiction guidelines cannot be stored. Save the character once to provision its vault, then return here.';

  protected setPhysicalField<K extends keyof PhysicalDescriptionForm>(
    key: K,
    value: PhysicalDescriptionForm[K],
  ): void {
    this.physicalForm.update((f) => ({ ...f, [key]: value }));
  }

  protected async savePhysicalDescription(): Promise<void> {
    // v4 `DescriptionsTab.tsx:92-96` — the client-side name guard.
    if (!this.physicalForm().name.trim()) {
      this.toasts.showError('Name is required');
      return;
    }
    const hadOne = this.hasPhysicalDescription();
    this.savingPhysical.set(true);
    try {
      await this.core.dispatchData({
        type: 'characterUpdate',
        characterId: this.characterId(),
        character: { physicalDescription: { ...this.physicalForm() } },
      });
      await this.queryClient.invalidateQueries({
        queryKey: characterKeys.detail(this.characterId()),
      });
      // v4 `:121` — "updated" once one already existed, else "created".
      this.toasts.showSuccess(
        hadOne ? 'Physical description updated' : 'Physical description created',
      );
    } catch (err) {
      // v4 `DescriptionsTab.tsx` is toast-only — no inline error surface.
      this.toasts.showError(
        err instanceof Error ? err.message : 'Failed to save physical description',
      );
    } finally {
      this.savingPhysical.set(false);
    }
  }

  /** v4 `DescriptionsTab.tsx:132-155` — confirm, PUT `physicalDescription: null`. */
  protected async clearPhysicalDescription(): Promise<void> {
    if (!this.hasPhysicalDescription()) {
      return;
    }
    if (!window.confirm('Remove the physical description for this character?')) {
      return;
    }
    this.savingPhysical.set(true);
    try {
      await this.core.dispatchData({
        type: 'characterUpdate',
        characterId: this.characterId(),
        character: { physicalDescription: null },
      });
      await this.queryClient.invalidateQueries({
        queryKey: characterKeys.detail(this.characterId()),
      });
      this.toasts.showSuccess('Physical description cleared');
    } catch (err) {
      this.toasts.showError(
        err instanceof Error ? err.message : 'Failed to clear physical description',
      );
    } finally {
      this.savingPhysical.set(false);
    }
  }

  protected async saveGuidelines(): Promise<void> {
    this.savingGuidelines.set(true);
    this.guidelinesError.set(null);
    try {
      await this.core.dispatchData({
        type: 'characterDepictionGuidelinesUpdate',
        characterId: this.characterId(),
        content: this.guidelinesDraft(),
      });
      await this.queryClient.invalidateQueries({
        queryKey: characterKeys.depiction(this.characterId()),
      });
    } catch (err) {
      this.guidelinesError.set(err instanceof Error ? err.message : 'Failed to save guidelines');
    } finally {
      this.savingGuidelines.set(false);
    }
  }
}
