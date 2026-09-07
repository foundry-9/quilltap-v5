/**
 * The AI Wizard's state + actions — v4 `components/characters/ai-wizard/
 * hooks/useAIWizard.ts` (493 lines at the `f699da6f6` pin) transcribed as an
 * Angular injectable, component-provided so `AiWizardModal` gets a fresh
 * instance per mount (the work order's "the hook as a service-per-modal"
 * instruction). `configure()` takes the host's own input SIGNALS (not
 * snapshots) so `characterName`/`currentData` stay live for the whole modal
 * lifetime, exactly as v4's hook re-reads its props on every host render.
 *
 * @module screens/characters/generators/wizard/wizard-state
 */

import { Injectable, Signal, computed, inject, signal } from '@angular/core';

import { CoreClient, coreErrorMessage } from '../../../../core/core-client';
import type { ConnectionProfileDto } from '../../../../core/core-contract';
import {
  mintProgressId,
  streamGenerator,
  type CharacterWizardStreamRequest,
  type DescriptionSourceType,
  type GeneratableField,
  type GeneratedCharacterData,
} from '../edit-generators.api';
import {
  initialGenerationProgress,
  normalizeGeneratedScenarios,
  type GenerationProgress,
  type WizardCharacterData,
  type WizardStep,
} from './wizard-types';

export type { GenerationProgress, WizardStep } from './wizard-types';

/** The values the host component supplies at open time (v4's hook props). */
export interface WizardConfig {
  characterId: Signal<string | undefined>;
  characterName: Signal<string>;
  currentData: Signal<WizardCharacterData>;
  onApply: (data: GeneratedCharacterData) => void;
  onClose: () => void;
}

const NOOP_CONFIG: WizardConfig = {
  characterId: signal(undefined),
  characterName: signal(''),
  currentData: signal({}),
  onApply: () => {},
  onClose: () => {},
};

@Injectable()
export class WizardState {
  private readonly core = inject(CoreClient);
  private config: WizardConfig = NOOP_CONFIG;

  /** Wire the modal's inputs; called once from the host component's constructor. */
  configure(config: WizardConfig): void {
    this.config = config;
  }

  /** The host's live `currentData` (v4's `currentData` prop, read fresh each call). */
  currentData(): WizardCharacterData {
    return this.config.currentData();
  }

  // ---------------------------------------------------------------------
  // State (v4 `useState` calls, `:40-62`)
  // ---------------------------------------------------------------------

  readonly currentStep = signal<WizardStep>(1);
  readonly profiles = signal<ConnectionProfileDto[]>([]);
  readonly loadingProfiles = signal(true);
  readonly primaryProfileId = signal('');
  readonly descriptionSource = signal<DescriptionSourceType>('existing');
  readonly uploadedImageId = signal<string | null>(null);
  readonly uploadedImageUrl = signal<string | null>(null);
  readonly selectedGalleryImageId = signal<string | null>(null);
  readonly selectedGalleryImageUrl = signal<string | null>(null);
  readonly uploadedDocumentId = signal<string | null>(null);
  readonly uploadedDocumentName = signal<string | null>(null);
  readonly visionProfileId = signal<string | null>(null);
  readonly backgroundText = signal('');
  readonly selectedFields = signal<Set<GeneratableField>>(new Set());
  readonly generating = signal(false);
  readonly generationProgress = signal<GenerationProgress>(initialGenerationProgress());
  readonly generatedData = signal<GeneratedCharacterData | null>(null);
  readonly error = signal<string | null>(null);

  // ---------------------------------------------------------------------
  // Computed (v4 `useMemo` calls, `:98-190`)
  // ---------------------------------------------------------------------

  /** v4 `visionProfiles` (`:99-101`) — `filterProfilesBySupportedMimeType(profiles, 'image/jpeg')`. */
  readonly visionProfiles = computed(() =>
    this.profiles().filter((p) => p.supportsImageUpload === true),
  );

  private readonly primaryProfile = computed(
    () => this.profiles().find((p) => p.id === this.primaryProfileId()) ?? null,
  );

  /** v4 `primarySupportsVision` (`:108-111`) — `profileSupportsMimeType(profile, 'image/jpeg')`. */
  readonly primarySupportsVision = computed(() => this.primaryProfile()?.supportsImageUpload === true);

  /** v4 `needsVisionProfile` (`:114-117`). */
  readonly needsVisionProfile = computed(() => {
    const isImageSource = this.descriptionSource() === 'upload' || this.descriptionSource() === 'gallery';
    return isImageSource && !this.primarySupportsVision();
  });

  /** v4 `availableFields` (`:120-148`). */
  readonly availableFields = computed((): GeneratableField[] => {
    const characterName = this.config.characterName();
    const currentData = this.config.currentData();
    const fields: GeneratableField[] = [];
    if (!characterName.trim()) fields.push('name');
    if (!currentData.title?.trim()) fields.push('title');
    if (!currentData.identity?.trim()) fields.push('identity');
    if (!currentData.description?.trim()) fields.push('description');
    if (!currentData.manifesto?.trim()) fields.push('manifesto');
    if (!currentData.personality?.trim()) fields.push('personality');
    // Scenarios are always available — you can always generate more.
    fields.push('scenarios');
    if (!currentData.exampleDialogues?.trim()) fields.push('exampleDialogues');
    if (!currentData.firstMessage?.trim()) fields.push('firstMessage');
    if (!currentData.systemPrompt?.trim()) fields.push('systemPrompt');
    // Properties are available when pronouns are unset (aliases ride along).
    if (!currentData.pronouns) fields.push('properties');
    if (this.descriptionSource() !== 'skip') fields.push('physicalDescription');
    // Wardrobe items are always available.
    fields.push('wardrobeItems');
    return fields;
  });

  /** v4 `canProceed` (`:151-190`). */
  readonly canProceed = computed((): boolean => {
    switch (this.currentStep()) {
      case 1:
        return !!this.primaryProfileId();
      case 2: {
        const source = this.descriptionSource();
        if (source === 'skip' || source === 'existing') return true;
        if (source === 'upload') {
          const hasImage = !!this.uploadedImageId();
          const hasVisionIfNeeded = !this.needsVisionProfile() || !!this.visionProfileId();
          return hasImage && hasVisionIfNeeded;
        }
        if (source === 'gallery') {
          const hasImage = !!this.selectedGalleryImageId();
          const hasVisionIfNeeded = !this.needsVisionProfile() || !!this.visionProfileId();
          return hasImage && hasVisionIfNeeded;
        }
        if (source === 'document') return !!this.uploadedDocumentId();
        return false;
      }
      case 3:
        return this.selectedFields().size > 0;
      case 4:
        return true;
      default:
        return false;
    }
  });

  // ---------------------------------------------------------------------
  // Actions (v4 `useCallback`s)
  // ---------------------------------------------------------------------

  /** v4 the mount `useEffect` (`:65-95`). */
  async fetchProfiles(): Promise<void> {
    this.loadingProfiles.set(true);
    try {
      const data = await this.core.dispatchData({ type: 'connectionProfileList' });
      const profileList = (data['profiles'] as ConnectionProfileDto[] | undefined) ?? [];
      this.profiles.set(profileList);
      const defaultProfile = profileList.find((p) => p.isDefault);
      if (defaultProfile) {
        this.primaryProfileId.set(defaultProfile.id);
      } else if (profileList.length > 0) {
        this.primaryProfileId.set(profileList[0].id);
      }
    } catch {
      this.error.set('Failed to load connection profiles');
    } finally {
      this.loadingProfiles.set(false);
    }
  }

  goToStep(step: WizardStep): void {
    this.currentStep.set(step);
    this.error.set(null);
  }

  nextStep(): void {
    if (this.currentStep() < 4) {
      this.currentStep.set((this.currentStep() + 1) as WizardStep);
      this.error.set(null);
    }
  }

  prevStep(): void {
    if (this.currentStep() > 1) {
      this.currentStep.set((this.currentStep() - 1) as WizardStep);
      this.error.set(null);
    }
  }

  handleImageUpload(imageId: string, imageUrl: string): void {
    this.uploadedImageId.set(imageId || null);
    this.uploadedImageUrl.set(imageUrl || null);
  }

  handleGallerySelect(imageId: string, imageUrl: string): void {
    this.selectedGalleryImageId.set(imageId || null);
    this.selectedGalleryImageUrl.set(imageUrl || null);
  }

  handleDocumentUpload(documentId: string, documentName: string): void {
    this.uploadedDocumentId.set(documentId || null);
    this.uploadedDocumentName.set(documentName || null);
  }

  setDescriptionSource(source: DescriptionSourceType): void {
    this.descriptionSource.set(source);
  }

  setPrimaryProfileId(id: string): void {
    this.primaryProfileId.set(id);
  }

  setVisionProfileId(id: string): void {
    this.visionProfileId.set(id || null);
  }

  setBackgroundText(text: string): void {
    this.backgroundText.set(text);
  }

  toggleField(field: GeneratableField): void {
    this.selectedFields.update((prev) => {
      const next = new Set(prev);
      if (next.has(field)) {
        next.delete(field);
      } else {
        next.add(field);
      }
      return next;
    });
  }

  selectAllFields(): void {
    this.selectedFields.set(new Set(this.availableFields()));
  }

  clearAllFields(): void {
    this.selectedFields.set(new Set());
  }

  /** v4 `startGeneration` (`:248-401`) — the streaming generation call. */
  async startGeneration(): Promise<void> {
    if (this.selectedFields().size === 0) {
      this.error.set('Please select at least one field to generate');
      return;
    }

    this.generating.set(true);
    this.error.set(null);
    this.generationProgress.set(initialGenerationProgress());

    try {
      let imageId: string | undefined;
      if (this.descriptionSource() === 'upload' && this.uploadedImageId()) {
        imageId = this.uploadedImageId()!;
      } else if (this.descriptionSource() === 'gallery' && this.selectedGalleryImageId()) {
        imageId = this.selectedGalleryImageId()!;
      }
      const documentId =
        this.descriptionSource() === 'document' && this.uploadedDocumentId()
          ? this.uploadedDocumentId()!
          : undefined;

      const progressId = mintProgressId();
      const request: CharacterWizardStreamRequest = {
        type: 'characterWizardStream',
        progressId,
        primaryProfileId: this.primaryProfileId(),
        visionProfileId: this.needsVisionProfile() ? this.visionProfileId() ?? undefined : undefined,
        sourceType: this.descriptionSource(),
        imageId,
        documentId,
        characterName: this.config.characterName(),
        existingData: this.config.currentData() as unknown as Record<string, unknown>,
        background: this.backgroundText(),
        fieldsToGenerate: Array.from(this.selectedFields()),
        characterId: this.config.characterId(),
      };

      const result = await streamGenerator(
        this.core,
        request,
        progressId,
        (event) => this.applyProgressEvent(event),
      );

      // §B.1: the dispatch resolves with `{ terminal: <the wizard's `done` event> }`.
      const terminal = result['terminal'] as Record<string, unknown> | undefined;
      if (terminal && terminal['type'] === 'done') {
        this.applyProgressEvent(terminal);
      }
    } catch (err) {
      this.error.set(coreErrorMessage(err, 'Generation failed'));
    } finally {
      this.generating.set(false);
    }
  }

  /** v4's SSE `switch (event.type)` (`useAIWizard.ts:327-369`), idempotent so it
   *  can be fed either the live frame or the resolved `terminal` value. */
  private applyProgressEvent(event: Record<string, unknown>): void {
    switch (event['type']) {
      case 'field_start':
        this.generationProgress.update((prev) => ({
          ...prev,
          currentField: event['field'] as GeneratableField,
        }));
        break;
      case 'field_complete':
        this.generationProgress.update((prev) => ({
          ...prev,
          currentField: null,
          completedFields: [...prev.completedFields, event['field'] as GeneratableField],
          snippets: { ...prev.snippets, [event['field'] as string]: (event['snippet'] as string) || '' },
        }));
        break;
      case 'field_error':
        this.generationProgress.update((prev) => ({
          ...prev,
          currentField: null,
          errors: {
            ...prev.errors,
            [event['field'] as string]: (event['error'] as string) || 'Generation failed',
          },
        }));
        break;
      case 'done': {
        this.generatedData.set(event['fullContent'] as GeneratedCharacterData);
        if (event['error']) {
          this.error.set(event['error'] as string);
        }
        this.generationProgress.update((prev) => ({
          ...prev,
          currentField: null,
          errors: (event['errors'] as Record<string, string> | undefined) ?? prev.errors,
        }));
        break;
      }
      default:
        break;
    }
  }

  /** v4 `applyGenerated` (`:404-422`) — merges new scenarios ahead of existing ones. */
  applyGenerated(): void {
    const generated = this.generatedData();
    if (!generated) return;
    let dataToApply = generated;
    const newScenarios = normalizeGeneratedScenarios(generated.scenarios);
    if (newScenarios.length > 0) {
      const existingAsNew = (this.config.currentData().scenarios ?? []).map((s) => ({
        title: s.title,
        content: s.content,
      }));
      dataToApply = { ...generated, scenarios: [...existingAsNew, ...newScenarios] };
    }
    this.config.onApply(dataToApply);
    this.config.onClose();
  }

  /** v4 `reset` (`:425-446`). */
  reset(): void {
    this.currentStep.set(1);
    this.descriptionSource.set('existing');
    this.uploadedImageId.set(null);
    this.uploadedImageUrl.set(null);
    this.selectedGalleryImageId.set(null);
    this.selectedGalleryImageUrl.set(null);
    this.uploadedDocumentId.set(null);
    this.uploadedDocumentName.set(null);
    this.visionProfileId.set(null);
    this.backgroundText.set('');
    this.selectedFields.set(new Set());
    this.generating.set(false);
    this.generationProgress.set(initialGenerationProgress());
    this.generatedData.set(null);
    this.error.set(null);
  }
}
