import { ChangeDetectionStrategy, Component, effect, inject, input, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { CharacterPhysicalDescription } from '../../../core/core-contract';
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
}

const EMPTY_PHYSICAL_FORM: PhysicalDescriptionForm = {
  name: '',
  headAndShouldersPrompt: '',
  shortPrompt: '',
  mediumPrompt: '',
  longPrompt: '',
  completePrompt: '',
  fullDescription: '',
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
  };
}

/**
 * The Appearance tab (v4 `view/components/DescriptionsTab.tsx` +
 * `AestheticEditorField` for the depiction guidelines): the physical
 * description prompt set — a SEPARATE `characterUpdate` PUT scoped to
 * `physicalDescription` (v4 PATCHes the character row directly; the vault
 * write overlay routes it) — and the depiction-guidelines (Ariel Clause) text,
 * saved via `characterDepictionGuidelinesUpdate`. Each block has its own
 * explicit Save, independent of the Details tab's "Save Character".
 */
@Component({
  selector: 'qt-character-appearance-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
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

        @if (physicalError()) {
          <div class="qt-alert-error">{{ physicalError() }}</div>
        }

        <div>
          <label class="block qt-label mb-2">Name</label>
          <input
            type="text"
            class="qt-input"
            [value]="physicalForm().name"
            (input)="setPhysicalField('name', $any($event.target).value)"
          />
        </div>
        <div>
          <label class="block qt-label mb-2">Head &amp; Shoulders Prompt</label>
          <textarea
            class="qt-input"
            rows="3"
            [value]="physicalForm().headAndShouldersPrompt"
            (input)="setPhysicalField('headAndShouldersPrompt', $any($event.target).value)"
          ></textarea>
        </div>
        <div>
          <label class="block qt-label mb-2">Short Prompt</label>
          <textarea
            class="qt-input"
            rows="3"
            [value]="physicalForm().shortPrompt"
            (input)="setPhysicalField('shortPrompt', $any($event.target).value)"
          ></textarea>
        </div>
        <div>
          <label class="block qt-label mb-2">Medium Prompt</label>
          <textarea
            class="qt-input"
            rows="4"
            [value]="physicalForm().mediumPrompt"
            (input)="setPhysicalField('mediumPrompt', $any($event.target).value)"
          ></textarea>
        </div>
        <div>
          <label class="block qt-label mb-2">Long Prompt</label>
          <textarea
            class="qt-input"
            rows="5"
            [value]="physicalForm().longPrompt"
            (input)="setPhysicalField('longPrompt', $any($event.target).value)"
          ></textarea>
        </div>
        <div>
          <label class="block qt-label mb-2">Complete Prompt</label>
          <textarea
            class="qt-input"
            rows="6"
            [value]="physicalForm().completePrompt"
            (input)="setPhysicalField('completePrompt', $any($event.target).value)"
          ></textarea>
        </div>
        <div>
          <label class="block qt-label mb-2">Full Description</label>
          <textarea
            class="qt-input"
            rows="8"
            [value]="physicalForm().fullDescription"
            (input)="setPhysicalField('fullDescription', $any($event.target).value)"
          ></textarea>
        </div>

        <div class="flex justify-end">
          <button
            type="button"
            class="qt-button-primary"
            [disabled]="savingPhysical()"
            (click)="savePhysicalDescription()"
          >
            {{ savingPhysical() ? 'Saving...' : 'Save Physical Descriptions' }}
          </button>
        </div>
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

        <textarea
          class="qt-input"
          rows="6"
          aria-label="Depiction guidelines"
          [value]="guidelinesDraft()"
          (input)="guidelinesDraft.set($any($event.target).value)"
        ></textarea>

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
      </div>
    </div>
  `,
})
export class CharacterAppearanceTab {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();

  readonly characterId = input.required<string>();

  protected readonly physicalForm = signal<PhysicalDescriptionForm>(EMPTY_PHYSICAL_FORM);
  protected readonly savingPhysical = signal(false);
  protected readonly physicalError = signal<string | null>(null);
  private physicalSeeded = false;

  protected readonly guidelinesDraft = signal('');
  protected readonly savingGuidelines = signal(false);
  protected readonly guidelinesError = signal<string | null>(null);
  private guidelinesSeeded = false;

  protected readonly characterQuery = injectQuery(() => ({
    queryKey: characterKeys.detail(this.characterId()),
    queryFn: () => fetchCharacter(this.core, this.characterId()),
  }));

  protected readonly guidelinesQuery = injectQuery(() => ({
    queryKey: characterKeys.depiction(this.characterId()),
    queryFn: () => fetchDepictionGuidelines(this.core, this.characterId()),
  }));

  constructor() {
    // Seed each editable draft once from its query (not on every background
    // refetch, so an in-progress edit isn't clobbered). `effect` re-runs
    // whenever the query's data signal transitions (e.g. pending → loaded).
    effect(() => {
      const data = this.characterQuery.data();
      if (data && !this.physicalSeeded) {
        this.physicalForm.set(loadPhysicalDescription(data.physicalDescription));
        this.physicalSeeded = true;
      }
    });
    effect(() => {
      const data = this.guidelinesQuery.data();
      if (data !== undefined && !this.guidelinesSeeded) {
        this.guidelinesDraft.set(data);
        this.guidelinesSeeded = true;
      }
    });
  }

  protected setPhysicalField<K extends keyof PhysicalDescriptionForm>(
    key: K,
    value: PhysicalDescriptionForm[K],
  ): void {
    this.physicalForm.update((f) => ({ ...f, [key]: value }));
  }

  protected async savePhysicalDescription(): Promise<void> {
    this.savingPhysical.set(true);
    this.physicalError.set(null);
    try {
      await this.core.dispatchData({
        type: 'characterUpdate',
        characterId: this.characterId(),
        character: { physicalDescription: { ...this.physicalForm() } },
      });
      await this.queryClient.invalidateQueries({
        queryKey: characterKeys.detail(this.characterId()),
      });
    } catch (err) {
      this.physicalError.set(
        err instanceof Error ? err.message : 'Failed to save physical descriptions',
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
