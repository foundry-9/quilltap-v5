import { computed, DestroyRef, inject, Injectable, signal } from '@angular/core';
import type { Subscription } from 'rxjs';

import { apiUrl } from '../../../../core/api-url';
import { CoreClient } from '../../../../core/core-client';
import type { CharacterConnectionProfile, FileEntry } from '../../../../core/core-contract';
import { fetchConnectionProfiles } from '../../characters.api';
import {
  asGeneratorProgress,
  dispatchAiImportStream,
  dispatchSystemImportExecute,
} from '../detail-generators.api';
import type { SystemImportExecuteResult } from '../detail-generators.api';
import { applyAiImportEvent, AI_IMPORT_FOLD_INITIAL, type AiImportFoldState } from './ai-import-fold';
import { AI_IMPORT_INITIAL_STEPS } from './ai-import.types';
import type { AIImportStepName, UploadedSourceFile, WizardUIStep } from './ai-import.types';

/**
 * Modal-scoped state for the "Summon From Lore" wizard (v4
 * `hooks/useAIImport.ts`, 433 lines, transcribed as an Angular injectable —
 * `providers: [AiImportState]` on `AiImportWizard` gives every open of the
 * dialog a fresh instance, matching v4's per-mount hook lifetime, exactly the
 * `OptimizerState` precedent).
 *
 * ## One deliberate fidelity gap: the "prefer the default profile" half
 *
 * v4 fetches its own connection-profile list (`fetch('/api/v1/connection-
 * profiles')`, `:74-104`) and auto-selects `profileList.find(p =>
 * p.isDefault)`, falling back to `profileList[0]` when no profile is marked
 * default. v5's shared {@link fetchConnectionProfiles} helper (used here per
 * the round's own instruction, matching the optimizer's profile-picker
 * precedent) narrows `ConnectionProfileDto` down to `{id, name, provider,
 * modelName}` and drops `isDefault` — so only v4's FALLBACK half survives
 * here: the first profile in the list is auto-selected. This is v4's own
 * fallback path when no default exists, just applied unconditionally.
 */
@Injectable()
export class AiImportState {
  private readonly core = inject(CoreClient);
  private readonly destroyRef = inject(DestroyRef);

  private readonly fold = signal<AiImportFoldState>(AI_IMPORT_FOLD_INITIAL);

  private readonly currentStepSig = signal<WizardUIStep>(1);

  // Step 1: source material
  private readonly uploadedFilesSig = signal<UploadedSourceFile[]>([]);
  private readonly sourceTextSig = signal('');
  private readonly uploadingSig = signal(false);
  private readonly uploadErrorSig = signal<string | null>(null);

  // Step 2: configuration
  private readonly profilesSig = signal<CharacterConnectionProfile[]>([]);
  private readonly loadingProfilesSig = signal(true);
  private readonly profileIdSig = signal('');
  private readonly includeMemoriesSig = signal(true);
  private readonly includeChatsSig = signal(false);

  // Import
  private readonly importingSig = signal(false);
  private readonly importResultSig = signal<SystemImportExecuteResult | null>(null);

  private progressSub: Subscription | null = null;

  constructor() {
    this.destroyRef.onDestroy(() => this.progressSub?.unsubscribe());
    void this.loadProfiles();
  }

  readonly currentStep = this.currentStepSig.asReadonly();

  readonly uploadedFiles = this.uploadedFilesSig.asReadonly();
  readonly sourceText = this.sourceTextSig.asReadonly();
  readonly uploading = this.uploadingSig.asReadonly();
  readonly uploadError = this.uploadErrorSig.asReadonly();

  readonly profiles = this.profilesSig.asReadonly();
  readonly loadingProfiles = this.loadingProfilesSig.asReadonly();
  readonly profileId = this.profileIdSig.asReadonly();
  readonly includeMemories = this.includeMemoriesSig.asReadonly();
  readonly includeChats = this.includeChatsSig.asReadonly();

  readonly generating = computed(() => this.fold().generating);
  readonly steps = computed(() => this.fold().steps);
  readonly result = computed(() => this.fold().result);
  readonly stepResults = computed(() => this.fold().stepResults);
  readonly stepErrors = computed(() => this.fold().errors);
  readonly error = computed(() => this.fold().error);

  readonly importing = this.importingSig.asReadonly();
  readonly importResult = this.importResultSig.asReadonly();

  /** v4 `canProceed` (`useAIImport.ts:107-120`). */
  readonly canProceed = computed<boolean>(() => {
    switch (this.currentStepSig()) {
      case 1:
        return this.uploadedFilesSig().length > 0 || this.sourceTextSig().trim().length > 0;
      case 2:
        return !!this.profileIdSig();
      case 3:
        return !this.fold().generating;
      case 4:
        return !!this.fold().result;
    }
  });

  // ---------------------------------------------------------------------
  // Navigation (v4 `nextStep`/`prevStep`, `:130-142`)
  // ---------------------------------------------------------------------

  nextStep(): void {
    if (this.currentStepSig() < 4) {
      this.currentStepSig.update((step) => (step + 1) as WizardUIStep);
      this.fold.update((s) => ({ ...s, error: null }));
    }
  }

  prevStep(): void {
    if (this.currentStepSig() > 1) {
      this.currentStepSig.update((step) => (step - 1) as WizardUIStep);
      this.fold.update((s) => ({ ...s, error: null }));
    }
  }

  // ---------------------------------------------------------------------
  // Step 1: source material
  // ---------------------------------------------------------------------

  setSourceText(text: string): void {
    this.sourceTextSig.set(text);
  }

  /**
   * v4 `uploadFiles` (`:150-184`) — uploads each file in turn via `POST
   * /api/v1/files?action=upload` (multipart), the general upload leg
   * `crates/quilltap-web/src/files_routes.rs:938` exposes for real. One
   * failure aborts the remaining files in the batch, exactly as v4's `for`
   * loop + single `catch` does.
   */
  async uploadFiles(files: File[]): Promise<void> {
    this.uploadingSig.set(true);
    this.uploadErrorSig.set(null);

    try {
      const newFiles: UploadedSourceFile[] = [];
      for (const file of files) {
        newFiles.push(await this.uploadOneFile(file));
      }
      this.uploadedFilesSig.update((prev) => [...prev, ...newFiles]);
    } catch (err) {
      this.uploadErrorSig.set(err instanceof Error ? err.message : 'Upload failed');
    } finally {
      this.uploadingSig.set(false);
    }
  }

  removeFile(fileId: string): void {
    this.uploadedFilesSig.update((prev) => prev.filter((f) => f.id !== fileId));
  }

  private async uploadOneFile(file: File): Promise<UploadedSourceFile> {
    const form = new FormData();
    form.append('file', file);
    const res = await fetch(apiUrl('/api/v1/files?action=upload'), { method: 'POST', body: form });
    const body = (await res.json().catch(() => ({}))) as { data?: FileEntry; error?: string };
    if (!res.ok) {
      throw new Error(body.error || `Failed to upload ${file.name}`);
    }
    const id = body.data?.id;
    if (!id) {
      throw new Error(`Failed to upload ${file.name}`);
    }
    // v4 records the file's own `name`/`size` rather than reading them back
    // off the response (`:171-175`).
    return { id, name: file.name, size: file.size };
  }

  // ---------------------------------------------------------------------
  // Step 2: configuration
  // ---------------------------------------------------------------------

  setProfileId(id: string): void {
    this.profileIdSig.set(id);
  }

  setIncludeMemories(value: boolean): void {
    this.includeMemoriesSig.set(value);
  }

  setIncludeChats(value: boolean): void {
    this.includeChatsSig.set(value);
  }

  /** v4's mount effect (`useAIImport.ts:74-104`). */
  private async loadProfiles(): Promise<void> {
    this.loadingProfilesSig.set(true);
    try {
      const list = await fetchConnectionProfiles(this.core);
      this.profilesSig.set(list);
      if (!this.profileIdSig() && list.length > 0) {
        // v4 `useAIImport.ts:87-92`: the default profile, else the first.
        this.profileIdSig.set((list.find((p) => p.isDefault) ?? list[0]).id);
      }
    } catch (err) {
      this.fold.update((s) => ({
        ...s,
        // v4 `useAIImport.ts:97` — always the fixed sentence.
        error: 'Failed to load connection profiles',
      }));
    } finally {
      this.loadingProfilesSig.set(false);
    }
  }

  // ---------------------------------------------------------------------
  // Step 3 & 4: generation (v4 `startGeneration`, `:191-320`)
  // ---------------------------------------------------------------------

  /**
   * Starts (or re-runs) the generation stream. `regenerateSteps` mirrors v4's
   * latent per-step regeneration capability (`existingResult`/
   * `regenerateSteps` on the request) — the current wizard UI never calls it
   * with an argument (`AIImportWizard.tsx:771`), so this is exercised today
   * only through {@link regenerateStep}.
   */
  async startGeneration(regenerateSteps?: AIImportStepName[]): Promise<void> {
    this.fold.update((s) => ({
      ...s,
      generating: true,
      steps: { ...AI_IMPORT_INITIAL_STEPS },
      errors: {},
      error: null,
    }));
    // Auto-advance to step 3 (v4 `:202`).
    this.currentStepSig.set(3);

    const progressId =
      typeof crypto !== 'undefined' && crypto.randomUUID ? crypto.randomUUID() : `${Date.now()}`;

    this.progressSub?.unsubscribe();
    this.progressSub = this.core.events$.subscribe((frame) => {
      const gp = asGeneratorProgress(frame, progressId);
      if (!gp || gp.generator !== 'aiImport') return;
      // The terminal `done` is applied once from the dispatch's own
      // resolution below, never from the stream.
      if (gp.event['type'] === 'done') return;
      this.fold.update((s) => applyAiImportEvent(s, gp.event));
    });

    try {
      const terminal = await dispatchAiImportStream(this.core, {
        type: 'aiImportStream',
        progressId,
        profileId: this.profileIdSig(),
        sourceFileIds: this.uploadedFilesSig().map((f) => f.id),
        sourceText: this.sourceTextSig(),
        includeMemories: this.includeMemoriesSig(),
        includeChats: this.includeChatsSig(),
        existingResult: regenerateSteps ? this.fold().stepResults : undefined,
        regenerateSteps,
      });
      this.fold.update((s) => applyAiImportEvent(s, terminal as unknown as Record<string, unknown>));
      // Auto-advance to review on the DONE frame's own `result` (v4 `:297-300`
      // tests `event.result`) — never on state, which may hold a prior run's.
      if ((terminal as { result?: unknown }).result) {
        this.currentStepSig.set(4);
      }
    } catch (err) {
      this.fold.update((s) => ({
        ...s,
        generating: false,
        error: err instanceof Error && err.message ? err.message : 'Generation failed',
      }));
    } finally {
      this.progressSub?.unsubscribe();
      this.progressSub = null;
    }
  }

  /** Re-runs generation for exactly one step, preserving the rest of `stepResults`. */
  regenerateStep(stepName: AIImportStepName): Promise<void> {
    return this.startGeneration([stepName]);
  }

  // ---------------------------------------------------------------------
  // Import (v4 `importCharacter`, `:325-363`)
  // ---------------------------------------------------------------------

  async runImport(): Promise<SystemImportExecuteResult | null> {
    if (!this.fold().result) return null;

    this.importingSig.set(true);
    this.fold.update((s) => ({ ...s, error: null }));

    try {
      const result = await dispatchSystemImportExecute(this.core, {
        type: 'systemImportExecute',
        exportData: this.fold().result,
        options: { conflictStrategy: 'duplicate', importMemories: true },
      });
      this.importResultSig.set(result);
      return result;
    } catch (err) {
      this.fold.update((s) => ({
        ...s,
        error: err instanceof Error && err.message ? err.message : 'Import failed',
      }));
      return null;
    } finally {
      this.importingSig.set(false);
    }
  }

  // ---------------------------------------------------------------------
  // Reset (v4 `reset`/`addMoreMaterial`, `:366-387`)
  // ---------------------------------------------------------------------

  reset(): void {
    this.currentStepSig.set(1);
    this.uploadedFilesSig.set([]);
    this.sourceTextSig.set('');
    this.uploadErrorSig.set(null);
    this.fold.set(AI_IMPORT_FOLD_INITIAL);
    this.importResultSig.set(null);
  }

  /** Go back to add more material, preserving existing results (v4 `:383-387`). */
  addMoreMaterial(): void {
    this.currentStepSig.set(1);
    this.fold.update((s) => ({ ...s, error: null }));
    this.importResultSig.set(null);
  }
}
