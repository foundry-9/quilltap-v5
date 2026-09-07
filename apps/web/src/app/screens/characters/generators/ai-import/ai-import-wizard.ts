import { ChangeDetectionStrategy, Component, computed, inject, output } from '@angular/core';

import type { SystemImportExecuteResult } from '../detail-generators.api';
import { formatBytes } from '../../../../ui/format-bytes';
import { Icon } from '../../../../ui/icon';
import { PROMPT_FIELD_HINTS } from '../../../../ui/prompt-field-hints';
import type { PromptFieldHintKey } from '../../../../ui/prompt-field-hints';
import { AiImportState } from './ai-import-state';
import {
  CORE_STEPS,
  REVIEW_FIELD_NAMES,
  STEP_DISPLAY_NAMES,
  WIZARD_STEP_LABELS,
} from './ai-import.types';
import type { AIImportStepName, StepStatus } from './ai-import.types';

/**
 * `AiImportWizard` — v4 `components/settings/ai-import/AIImportWizard.tsx`
 * (801 lines): the "Summon From Lore" AI character-import wizard. Four
 * steps: source material → configuration → generation progress → review &
 * import. Mounted from two places in v4 (Aurora's own dialog,
 * `AuroraView.tsx:694`, and the Salon's `SummonFromLoreModal.tsx` wrapping
 * it for the Add-Character picker) — this port carries no caller-specific
 * logic; both mount sites' concerns live entirely in the `imported` output.
 *
 * v4 wraps the wizard in a bespoke `fixed inset-0 … bg-background/80
 * backdrop-blur-sm` div at BOTH mount sites (there is no shared
 * `BaseModal`/overlay component for it) — this port normalizes that to the
 * house `qt-dialog-overlay`/`qt-dialog` classes, matching every other v5
 * modal (the `CharacterOptimizerModal` precedent) rather than reinventing
 * v4's ad hoc wrapper markup twice.
 *
 * A fresh {@link AiImportState} per open (component-level `providers`,
 * matching v4's per-mount hook lifetime).
 *
 * ## Deferred: nothing. File upload IS ported.
 *
 * The task that produced this file initially expected the multipart
 * `POST /api/v1/files?action=upload` leg to be unported and told this
 * component to defer it with a disabled control. It is NOT unported —
 * `crates/quilltap-web/src/files_routes.rs:938` (`files_upload_post`,
 * mounted at `POST /api/v1/files`) already exposes v4's exact general
 * upload leg (`{data: FileEntry}`), so {@link AiImportState.uploadFiles}
 * calls it directly (the `chat-files.api.ts` multipart-`fetch` precedent).
 * Both of v4's Step 1 affordances — file upload and pasted text — are live.
 */
@Component({
  selector: 'qt-ai-import-wizard',
  changeDetection: ChangeDetectionStrategy.OnPush,
  providers: [AiImportState],
  imports: [Icon],
  host: {
    '(document:keydown.escape)': 'onEscape()',
  },
  template: `
    <div class="qt-dialog-overlay" (click)="onBackdrop()">
      <div class="qt-dialog m-4 flex h-[92vh] w-full max-w-4xl flex-col" (click)="$event.stopPropagation()">
        <div class="qt-dialog-header flex flex-shrink-0 items-center justify-between">
          <h2 class="qt-dialog-title flex items-center gap-2">
            <qt-icon name="sparkles" class="w-5 h-5 text-primary" />
            {{ stepLabel() }}
          </h2>
          <button
            type="button"
            class="qt-button-icon qt-button-ghost flex-shrink-0"
            aria-label="Close"
            (click)="requestClose()"
          >
            <qt-icon name="close" class="w-5 h-5" />
          </button>
        </div>

        <div class="flex flex-1 flex-col gap-5 overflow-y-auto px-6 pt-5 pb-8">
          <!-- Step indicator (v4 StepIndicator, :30-58) -->
          <div class="flex items-center gap-2">
            @for (step of [1, 2, 3, 4]; track step) {
              <div class="flex items-center gap-2">
                <div [class]="stepBadgeClass(step)">
                  {{ step < state.currentStep() ? '✓' : step }}
                </div>
                @if (step < 4) {
                  <div [class]="'w-8 h-0.5 ' + (step < state.currentStep() ? 'qt-bg-success' : 'qt-bg-muted')"></div>
                }
              </div>
            }
          </div>

          @switch (state.currentStep()) {
            <!-- ================================================================
                 Step 1: Source Material (v4 SourceMaterialStep, :64-181)
                 ================================================================ -->
            @case (1) {
              <div class="flex flex-col gap-4">
                <div>
                  <h3 class="qt-section-title mb-2">Upload Source Files</h3>
                  <p class="qt-body-sm qt-text-secondary mb-3">
                    Upload wiki pages, character sheets, story documents, or any text files describing a
                    character. Supports .txt, .md, and .pdf files.
                  </p>

                  <div
                    class="qt-border-default border-2 border-dashed rounded-lg p-6 text-center cursor-pointer hover:qt-border-primary/50 transition-colors"
                    (drop)="onDrop($event)"
                    (dragover)="onDragOver($event)"
                    (click)="fileInput.click()"
                  >
                    <input
                      #fileInput
                      type="file"
                      class="hidden"
                      accept=".txt,.md,.pdf"
                      multiple
                      (change)="onFileSelect($event)"
                    />
                    <p class="qt-text-secondary">
                      {{ state.uploading() ? 'Uploading…' : 'Drop files here or click to browse' }}
                    </p>
                  </div>

                  @if (state.uploadError(); as uploadErr) {
                    <p class="qt-body-sm qt-text-destructive mt-2">{{ uploadErr }}</p>
                  }

                  @if (state.uploadedFiles().length > 0) {
                    <div class="mt-3 flex flex-col gap-2">
                      @for (file of state.uploadedFiles(); track file.id) {
                        <div class="flex items-center justify-between qt-bg-muted rounded px-3 py-2">
                          <span class="qt-body-sm truncate mr-2">{{ file.name }} ({{ formatBytes(file.size) }})</span>
                          <button
                            type="button"
                            class="qt-button-ghost qt-button-sm qt-text-destructive flex-shrink-0"
                            (click)="state.removeFile(file.id)"
                          >
                            <qt-icon name="trash" class="w-4 h-4" />
                          </button>
                        </div>
                      }
                    </div>
                  }
                </div>

                <div>
                  <h3 class="qt-section-title mb-2">Freeform Text</h3>
                  <p class="qt-body-sm qt-text-secondary mb-3">
                    Paste any additional character information, backstory, or notes here.
                  </p>
                  <textarea
                    class="qt-input w-full"
                    rows="8"
                    [value]="state.sourceText()"
                    (input)="state.setSourceText($any($event.target).value)"
                    placeholder="Paste character descriptions, wiki content, backstory, personality notes…"
                  ></textarea>
                </div>
              </div>
            }

            <!-- ================================================================
                 Step 2: Configuration (v4 ConfigurationStep, :187-267)
                 ================================================================ -->
            @case (2) {
              <div class="flex flex-col gap-6">
                <div>
                  <h3 class="qt-section-title mb-2">Connection Profile</h3>
                  <p class="qt-body-sm qt-text-secondary mb-3">
                    Select the AI provider to use for character generation.
                  </p>
                  @if (state.loadingProfiles()) {
                    <p class="qt-text-secondary">Loading profiles…</p>
                  } @else if (state.profiles().length === 0) {
                    <p class="qt-text-destructive">
                      No connection profiles found. Create one in Settings first.
                    </p>
                  } @else {
                    <!-- dogfood-#6 idiom: options are dynamic, so bind the
                         selection per-option with [selected] rather than
                         [value] on the <select> itself. -->
                    <select
                      class="qt-select w-full"
                      (change)="state.setProfileId($any($event.target).value)"
                    >
                      @for (profile of state.profiles(); track profile.id) {
                        <option [value]="profile.id" [selected]="profile.id === state.profileId()">
                          {{ profile.name }} ({{ profile.provider }} / {{ profile.modelName }})
                        </option>
                      }
                    </select>
                  }
                </div>

                <div class="flex flex-col gap-3">
                  <h3 class="qt-section-title">Options</h3>

                  <label class="flex items-center gap-3 cursor-pointer">
                    <input
                      type="checkbox"
                      class="qt-checkbox"
                      [checked]="state.includeMemories()"
                      (change)="state.setIncludeMemories($any($event.target).checked)"
                    />
                    <span class="flex flex-col">
                      <span class="qt-body font-medium">Generate Memories</span>
                      <span class="qt-body-sm qt-text-secondary">
                        Extract key facts and experiences as Commonplace Book memories
                      </span>
                    </span>
                  </label>

                  <label class="flex items-center gap-3 cursor-pointer">
                    <input
                      type="checkbox"
                      class="qt-checkbox"
                      [checked]="state.includeChats()"
                      (change)="state.setIncludeChats($any($event.target).checked)"
                    />
                    <span class="flex flex-col">
                      <span class="qt-body font-medium">Generate Example Chat</span>
                      <span class="qt-body-sm qt-text-secondary">
                        Create a sample conversation demonstrating the character&rsquo;s voice
                      </span>
                    </span>
                  </label>
                </div>
              </div>
            }

            <!-- ================================================================
                 Step 3: Generation Progress (v4 GenerationProgressStep, :273-359)
                 ================================================================ -->
            @case (3) {
              <div class="flex flex-col gap-4">
                @if (state.generating()) {
                  <p class="qt-body-sm qt-text-secondary">
                    Generating character data… This may take a minute depending on source material length.
                  </p>
                }

                <div class="flex flex-col gap-2">
                  @for (stepName of visibleSteps(); track stepName) {
                    <div class="flex items-start gap-3 py-2">
                      <span [class]="'text-lg flex-shrink-0 ' + statusColorClass(stepStatus(stepName))">
                        @switch (stepStatus(stepName)) {
                          @case ('complete') {
                            <qt-icon name="check" class="w-4 h-4" />
                          }
                          @case ('error') {
                            <qt-icon name="alert-triangle" class="w-4 h-4" />
                          }
                          @case ('in_progress') {
                            <qt-icon name="refresh" class="w-4 h-4 animate-spin" />
                          }
                          @default {
                            <span class="qt-bg-muted-foreground/40 block h-2 w-2 rounded-full"></span>
                          }
                        }
                      </span>
                      <div class="flex-1 min-w-0">
                        <span [class]="stepStatus(stepName) === 'pending' ? 'qt-text-secondary' : 'qt-body'">
                          {{ stepDisplayName(stepName) }}
                        </span>
                        @if (state.steps()[stepName].snippet; as snippet) {
                          <p class="qt-body-sm qt-text-secondary truncate">{{ snippet }}</p>
                        }
                        @if (state.steps()[stepName].error; as stepErr) {
                          <p class="qt-body-sm qt-text-destructive">{{ stepErr }}</p>
                        }
                      </div>
                    </div>
                  }
                </div>

                @if (state.error(); as err) {
                  <div class="qt-card qt-bg-destructive/10 qt-border-destructive/30 p-3 mt-4">
                    <p class="qt-text-destructive text-sm">{{ err }}</p>
                  </div>
                }
              </div>
            }

            <!-- ================================================================
                 Step 4: Review & Import (v4 ReviewStep, :392-638)
                 ================================================================ -->
            @case (4) {
              @if (!state.result() || !state.stepResults()) {
                <p class="qt-text-secondary">No generated data available.</p>
              } @else if (state.importResult()?.success) {
                <div class="flex flex-col gap-4">
                  <div class="qt-card qt-bg-success/10 qt-border-success/30 border p-4">
                    <h3 class="qt-section-title qt-text-success mb-2">Import Successful!</h3>
                    <p class="qt-body">
                      <strong>{{ basics()?.name }}</strong> has been imported successfully.
                      @if (importedCount() > 1) {
                        ({{ importedCount() }} entities imported)
                      }
                    </p>
                    @if ((state.importResult()?.warnings?.length ?? 0) > 0) {
                      <div class="mt-2">
                        <p class="qt-body-sm qt-text-secondary">Warnings:</p>
                        <ul class="list-disc list-inside qt-body-sm qt-text-secondary">
                          @for (w of state.importResult()?.warnings ?? []; track $index) {
                            <li>{{ w }}</li>
                          }
                        </ul>
                      </div>
                    }
                  </div>
                  <div class="flex gap-3">
                    <button type="button" class="qt-button-primary" (click)="state.reset()">
                      Import Another Character
                    </button>
                    <button type="button" class="qt-button-secondary" (click)="requestClose()">
                      Done
                    </button>
                  </div>
                </div>
              } @else {
                <div class="flex flex-col gap-6">
                  <div>
                    <h3 class="qt-section-title mb-2">Character Summary</h3>
                    <div class="qt-card flex flex-col gap-2 p-4">
                      <p class="qt-body"><strong>Name:</strong> {{ basics()?.name || 'Unknown' }}</p>
                      @if (basics()?.title; as title) {
                        <p class="qt-body"><strong>Title:</strong> {{ title }}</p>
                      }
                      @if (pronouns(); as pn) {
                        <p class="qt-body">
                          <strong>Pronouns:</strong> {{ pn.subject }}/{{ pn.object }}/{{ pn.possessive }}
                        </p>
                      }
                      @if ((aliases() ?? []).length > 0) {
                        <p class="qt-body"><strong>Aliases:</strong> {{ (aliases() ?? []).join(', ') }}</p>
                      }
                      @if (basics()?.description; as description) {
                        <p class="qt-body-sm qt-text-secondary mt-2">{{ truncatedDescription(description) }}</p>
                      }
                    </div>
                  </div>

                  @if (proseFields().length > 0 || reviewedSystemPrompts().length > 0) {
                    <div>
                      <h3 class="qt-section-title mb-2">Generated Wording</h3>
                      <p class="qt-body-sm qt-text-secondary mb-2">
                        Cast an eye over each passage before importing — the line beneath each title shows the
                        form of address the field expects.
                      </p>
                      <div class="flex flex-col gap-2">
                        @for (field of proseFields(); track field.hintKey) {
                          <details class="qt-bg-muted rounded-lg px-3 py-2">
                            <summary class="cursor-pointer qt-body-sm font-medium">
                              {{ fieldHintLabel(field.hintKey) }}
                            </summary>
                            @if (fieldHintExample(field.hintKey); as example) {
                              <p class="text-xs qt-text-secondary mt-1">Written as: <em>{{ example }}</em></p>
                            }
                            <p class="qt-body-sm mt-2 whitespace-pre-wrap">{{ field.text }}</p>
                          </details>
                        }
                        @for (prompt of reviewedSystemPrompts(); track $index) {
                          <details class="qt-bg-muted rounded-lg px-3 py-2">
                            <summary class="cursor-pointer qt-body-sm font-medium">
                              {{ prompt.name ? 'System Prompt: ' + prompt.name : 'System Prompt' }}
                            </summary>
                            @if (systemPromptExample(); as example) {
                              <p class="text-xs qt-text-secondary mt-1">Written as: <em>{{ example }}</em></p>
                            }
                            <p class="qt-body-sm mt-2 whitespace-pre-wrap">{{ prompt.content }}</p>
                          </details>
                        }
                      </div>
                    </div>
                  }

                  <div>
                    <h3 class="qt-section-title mb-2">Generated Content</h3>
                    <div class="grid grid-cols-2 gap-2">
                      @for (field of completedFields(); track field) {
                        <div class="flex items-center gap-2 qt-text-success qt-body-sm">
                          <qt-icon name="check" class="w-4 h-4" /> {{ field }}
                        </div>
                      }
                      @for (field of failedFields(); track field) {
                        <div class="flex items-center gap-2 qt-text-warning qt-body-sm">
                          <qt-icon name="alert-triangle" class="w-4 h-4" /> {{ field }}
                        </div>
                      }
                    </div>
                  </div>

                  <div class="flex flex-wrap gap-4 qt-body-sm">
                    @if (physicalDescriptions()) {
                      <span class="qt-text-secondary">Physical descriptions: 5 variants</span>
                    }
                    @if (systemPrompts(); as prompts) {
                      <span class="qt-text-secondary">System prompts: {{ prompts.length }}</span>
                    }
                    @if ((wardrobeItems() ?? []).length > 0) {
                      <span class="qt-text-secondary">
                        Wardrobe: {{ (wardrobeItems() ?? []).length }} item(s)
                        @if (outfitCount() > 0) {
                          , {{ outfitCount() }} outfit(s)
                        }
                      </span>
                    }
                    @if (memories(); as mems) {
                      <span class="qt-text-secondary">Memories: {{ mems.length }}</span>
                    }
                    @if (chatsField()?.messages; as chatMessages) {
                      <span class="qt-text-secondary">Chat messages: {{ chatMessages.length }}</span>
                    }
                  </div>

                  @if (nonFatalErrors().length > 0) {
                    <div class="qt-bg-muted rounded-lg p-3">
                      <p class="qt-body-sm qt-text-warning mb-1">Some steps had issues (non-critical):</p>
                      @for (entry of nonFatalErrors(); track entry[0]) {
                        <p class="qt-body-sm qt-text-secondary">{{ reviewFieldLabel(entry[0]) }}: {{ entry[1] }}</p>
                      }
                    </div>
                  }

                  @if (state.error(); as err) {
                    <div class="qt-card qt-bg-destructive/10 qt-border-destructive/30 p-3">
                      <p class="qt-text-destructive text-sm">{{ err }}</p>
                    </div>
                  }

                  <div class="flex flex-wrap gap-3">
                    <button
                      type="button"
                      [disabled]="state.importing()"
                      class="qt-button-primary disabled:opacity-50"
                      (click)="handleImport()"
                    >
                      {{ state.importing() ? 'Importing…' : 'Import Character' }}
                    </button>
                    <button type="button" class="qt-button-secondary" (click)="state.addMoreMaterial()">
                      Add More &amp; Regenerate
                    </button>
                    <button type="button" class="qt-button-ghost" (click)="state.reset()">Start Over</button>
                  </div>
                </div>
              }
            }
          }
        </div>

        @if (state.currentStep() < 3) {
          <div class="qt-dialog-footer flex flex-shrink-0 justify-between">
            <button
              type="button"
              [disabled]="state.currentStep() === 1"
              class="qt-button-secondary disabled:opacity-50"
              (click)="state.prevStep()"
            >
              Back
            </button>
            @if (state.currentStep() === 2) {
              <button
                type="button"
                [disabled]="!state.canProceed() || state.generating()"
                class="qt-button-primary disabled:opacity-50"
                (click)="state.startGeneration()"
              >
                Generate Character
              </button>
            } @else {
              <button
                type="button"
                [disabled]="!state.canProceed()"
                class="qt-button-primary disabled:opacity-50"
                (click)="state.nextStep()"
              >
                Next
              </button>
            }
          </div>
        }

        @if (state.currentStep() === 3 && !state.generating() && state.result()) {
          <div class="qt-dialog-footer flex flex-shrink-0 justify-end">
            <button type="button" class="qt-button-primary" (click)="state.nextStep()">
              Review Results
            </button>
          </div>
        }
      </div>
    </div>
  `,
})
export class AiImportWizard {
  protected readonly state = inject(AiImportState);
  protected readonly formatBytes = formatBytes;

  /**
   * Fired after every import ATTEMPT completes (v4 `handleImport`,
   * `AIImportWizard.tsx:691-697`, always calls `onImportSuccess` — with
   * `[]` on a failed attempt, real ids on success). Carries the whole
   * {@link SystemImportExecuteResult} alongside the ids so a caller that
   * wants warnings/counts has them without a second dispatch.
   *
   * - Aurora's list mount ignores the payload and just refetches (v4
   *   `onImportSuccess={() => { mutateCharacters() }}`, `AuroraView.tsx:696-698`).
   * - The Salon "Summon from Lore" wrapper reads exactly one id: empty ⇒ an
   *   error toast ("came back empty-handed"), more than one ⇒ a different
   *   error toast, exactly one ⇒ `onSummoned(ids[0])` then closes
   *   (v4 `SummonFromLoreModal.tsx:43-64`).
   */
  readonly imported = output<{ importedCharacterIds: string[]; result: SystemImportExecuteResult | null }>();
  /** v4 `onClose` — header close, Escape, backdrop, and the review step's "Done" button. */
  readonly closed = output<void>();

  protected readonly stepLabel = computed(() => WIZARD_STEP_LABELS[this.state.currentStep() - 1]);

  // -------------------------------------------------------------------
  // Step 1: source material
  // -------------------------------------------------------------------

  protected onDrop(event: DragEvent): void {
    event.preventDefault();
    const files = Array.from(event.dataTransfer?.files ?? []).filter(
      (f) =>
        ['text/plain', 'text/markdown', 'application/pdf'].includes(f.type) ||
        f.name.endsWith('.txt') ||
        f.name.endsWith('.md') ||
        f.name.endsWith('.pdf'),
    );
    if (files.length > 0) {
      void this.state.uploadFiles(files);
    }
  }

  protected onDragOver(event: DragEvent): void {
    event.preventDefault();
  }

  protected onFileSelect(event: Event): void {
    const input = event.target as HTMLInputElement;
    const files = Array.from(input.files ?? []);
    if (files.length > 0) {
      void this.state.uploadFiles(files);
    }
    input.value = '';
  }

  // -------------------------------------------------------------------
  // Step 3: generation progress
  // -------------------------------------------------------------------

  /** v4's `visibleSteps` (`AIImportWizard.tsx:286-292`). */
  protected readonly visibleSteps = computed<AIImportStepName[]>(() => {
    const steps = this.state.steps();
    return [
      ...(steps.analyzing.status !== 'pending' ? (['analyzing'] as AIImportStepName[]) : []),
      ...CORE_STEPS,
      ...(this.state.includeMemories() ? (['memories'] as AIImportStepName[]) : []),
      ...(this.state.includeChats() ? (['chats'] as AIImportStepName[]) : []),
      ...(steps.repair.status !== 'pending' ? (['repair'] as AIImportStepName[]) : []),
    ];
  });

  protected stepStatus(stepName: AIImportStepName): StepStatus {
    return this.state.steps()[stepName].status;
  }

  protected stepDisplayName(stepName: AIImportStepName): string {
    return STEP_DISPLAY_NAMES[stepName];
  }

  /** v4 `getStatusColor` (`:307-318`). */
  protected statusColorClass(status: StepStatus): string {
    switch (status) {
      case 'complete':
        return 'qt-text-success';
      case 'error':
        return 'qt-text-warning';
      case 'in_progress':
        return 'qt-text-info';
      default:
        return 'qt-text-secondary';
    }
  }

  protected stepBadgeClass(step: number): string {
    const current = this.state.currentStep();
    const isActive = step === current;
    const isComplete = step < current;
    const base = 'w-8 h-8 rounded-full flex items-center justify-center qt-label';
    if (isActive) return `${base} qt-bg-primary qt-text-on-accent`;
    if (isComplete) return `${base} qt-bg-success qt-text-on-accent`;
    return `${base} qt-bg-muted qt-text-secondary`;
  }

  // -------------------------------------------------------------------
  // Step 4: review & import
  // -------------------------------------------------------------------

  protected readonly basics = computed<ReviewBasics | undefined>(
    () => this.state.stepResults()?.['character_basics'] as ReviewBasics | undefined,
  );
  protected readonly pronouns = computed<ReviewPronouns | undefined>(
    () => this.state.stepResults()?.['pronouns'] as ReviewPronouns | undefined,
  );
  protected readonly aliases = computed<string[] | undefined>(
    () => this.state.stepResults()?.['aliases'] as string[] | undefined,
  );
  protected readonly memories = computed<unknown[] | undefined>(
    () => this.state.stepResults()?.['memories'] as unknown[] | undefined,
  );
  protected readonly chatsField = computed<ReviewChats | undefined>(
    () => this.state.stepResults()?.['chats'] as ReviewChats | undefined,
  );
  protected readonly physicalDescriptions = computed<unknown>(
    () => this.state.stepResults()?.['physical_descriptions'],
  );
  protected readonly systemPrompts = computed<ReviewSystemPrompt[] | undefined>(
    () => this.state.stepResults()?.['system_prompts'] as ReviewSystemPrompt[] | undefined,
  );
  protected readonly wardrobeItems = computed<ReviewWardrobeItem[] | undefined>(
    () => this.state.stepResults()?.['wardrobe_items'] as ReviewWardrobeItem[] | undefined,
  );
  protected readonly outfitCount = computed(
    () => (this.wardrobeItems() ?? []).filter((item) => (item.components?.length ?? 0) > 0).length,
  );

  protected readonly completedFields = computed<string[]>(() => {
    const results = this.state.stepResults();
    if (!results) return [];
    const errors = this.state.stepErrors();
    return Object.entries(REVIEW_FIELD_NAMES)
      .filter(([key]) => !errors[key] && key in results)
      .map(([, label]) => label);
  });
  protected readonly failedFields = computed<string[]>(() => {
    const errors = this.state.stepErrors();
    return Object.entries(REVIEW_FIELD_NAMES)
      .filter(([key]) => !!errors[key])
      .map(([, label]) => label);
  });
  /** v4's errors listing, excluding the `_fatal` sentinel key (`:610-611`). */
  protected readonly nonFatalErrors = computed<[string, string][]>(() =>
    Object.entries(this.state.stepErrors()).filter(([key]) => key !== '_fatal'),
  );
  protected reviewFieldLabel(key: string): string {
    return REVIEW_FIELD_NAMES[key] ?? key;
  }

  /** v4's `proseCandidates` (`:459-468`). */
  protected readonly proseFields = computed<ProseField[]>(() => {
    const b = this.basics();
    if (!b) return [];
    const candidates: { hintKey: PromptFieldHintKey; text?: string }[] = [
      { hintKey: 'identity', text: b.identity },
      { hintKey: 'description', text: b.description },
      { hintKey: 'manifesto', text: b.manifesto },
      { hintKey: 'personality', text: b.personality },
      { hintKey: 'scenario', text: b.scenario },
    ];
    return candidates.filter((f): f is ProseField => !!f.text);
  });
  /** v4's `reviewedSystemPrompts` (`:469-471`). */
  protected readonly reviewedSystemPrompts = computed<{ name?: string; content: string }[]>(() =>
    (this.systemPrompts() ?? []).filter((p): p is { name?: string; content: string } => !!p.content),
  );

  protected fieldHintLabel(key: PromptFieldHintKey): string {
    return PROMPT_FIELD_HINTS[key].label;
  }
  protected fieldHintExample(key: PromptFieldHintKey): string | undefined {
    return (PROMPT_FIELD_HINTS[key] as { example?: string }).example;
  }
  protected systemPromptExample(): string | undefined {
    return (PROMPT_FIELD_HINTS.systemPrompt as { example?: string }).example;
  }

  /** v4's inline 200-char truncation (`:533-536`). */
  protected truncatedDescription(description: string): string {
    return description.length > 200 ? description.substring(0, 200) + '...' : description;
  }

  /**
   * v4's `importResult.importedCount` is a single number
   * (`useAIImport.ts:349-354`, `data.imported || 0`). The wire's
   * `SystemImportExecuteResult.imported` is `Record<string, number>` (the
   * counts-by-entity-type shape the real `systemImportExecute` verb
   * returns) — summed here to the same "total entities imported" reading.
   */
  protected readonly importedCount = computed(() => {
    const imported = this.state.importResult()?.imported;
    if (!imported) return 0;
    return Object.values(imported).reduce((sum, n) => sum + n, 0);
  });

  protected async handleImport(): Promise<void> {
    const result = await this.state.runImport();
    this.imported.emit({
      importedCharacterIds: result?.success ? (result.importedCharacterIds ?? []) : [],
      result: result ?? null,
    });
  }

  // -------------------------------------------------------------------
  // Dismissal
  // -------------------------------------------------------------------

  protected requestClose(): void {
    this.closed.emit();
  }
  protected onBackdrop(): void {
    this.requestClose();
  }
  protected onEscape(): void {
    this.requestClose();
  }
}

// ---------------------------------------------------------------------
// Local view types for the `stepResults` bag (v4's inline casts, ReviewStep
// `:417-433`) — `stepResults` is `Record<string, unknown>` on the wire, so
// these shapes are read defensively, never validated.
// ---------------------------------------------------------------------

interface ReviewBasics {
  name?: string;
  title?: string;
  identity?: string;
  description?: string;
  manifesto?: string;
  personality?: string;
  scenario?: string;
}
interface ReviewPronouns {
  subject?: string;
  object?: string;
  possessive?: string;
}
interface ReviewSystemPrompt {
  name?: string;
  content?: string;
}
interface ReviewWardrobeItem {
  title?: string;
  components?: string[];
}
interface ReviewChats {
  title?: string;
  messages?: unknown[];
}
interface ProseField {
  hintKey: PromptFieldHintKey;
  text: string;
}
