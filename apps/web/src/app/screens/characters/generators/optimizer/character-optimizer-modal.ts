import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import type { CharacterConnectionProfile } from '../../../../core/core-contract';
import { Icon } from '../../../../ui/icon';
import { ProgressBar, type ProgressSegment } from '../../../../ui/progress-bar';
import type { OptimizerFilterOptions, OptimizerOutputMode } from '../detail-generators.api';
import { AnalysisSummary } from './analysis-summary';
import { ApplyConfirmation, type OptimizerAcceptedChangeView } from './apply-confirmation';
import { OptimizerState } from './optimizer-state';
import { SuggestionCard } from './suggestion-card';

/**
 * v4's own three progress segments (`components/characters/optimizer/
 * components/ProgressBar.tsx:19-23`) — the durations pace the shared bar's
 * fill, so they are v4's numbers, not chosen display values (the §3
 * unification review's correction; the long phase sentences live in
 * `STEP_LABELS` and are shown beside the bar, not on it).
 */
const SEGMENTS: ProgressSegment[] = [
  { key: 'loading', label: 'Retrieving', estimatedDurationMs: 3000 },
  { key: 'analyzing', label: 'Analysing', estimatedDurationMs: 15000 },
  { key: 'generating', label: 'Composing', estimatedDurationMs: 20000 },
];

/**
 * `CharacterOptimizerModal` — v4 `components/characters/optimizer/
 * CharacterOptimizerModal.tsx` (580 lines): the "Refine from Memories"
 * feature. Four phases (preflight → progress → review → apply), plus the
 * suggestions-file outcome when `vaultAvailable` and the operator opts to
 * write suggestions to the vault instead of applying them directly.
 *
 * A fresh {@link OptimizerState} per open (component-level `providers`,
 * matching v4's per-mount hook lifetime).
 */
@Component({
  selector: 'qt-character-optimizer-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  providers: [OptimizerState],
  imports: [Icon, ProgressBar, AnalysisSummary, SuggestionCard, ApplyConfirmation],
  host: {
    '(document:keydown.escape)': 'onEscape()',
  },
  template: `
    <div class="qt-dialog-overlay" (click)="onBackdrop()">
      <div class="qt-dialog m-4 flex h-[92vh] w-full max-w-2xl flex-col" (click)="$event.stopPropagation()">
        <div class="qt-dialog-header flex flex-shrink-0 items-start justify-between">
          <div class="flex flex-col gap-0.5">
            <h2 class="qt-dialog-title flex items-center gap-2">
              <qt-icon name="book" class="w-5 h-5 text-primary" />
              Refine from Memories
            </h2>
            <p class="qt-dialog-description">{{ characterName() }} &mdash; {{ phaseDescription() }}</p>
          </div>
          <button
            type="button"
            [disabled]="state.loading() || state.applying()"
            class="qt-button-icon qt-button-ghost flex-shrink-0 disabled:opacity-50"
            aria-label="Close"
            (click)="requestClose()"
          >
            <qt-icon name="close" class="w-5 h-5" />
          </button>
        </div>

        <div class="flex flex-1 flex-col gap-5 overflow-y-auto px-6 pt-5 pb-8">
          @switch (state.phase()) {
            @case ('preflight') {
              <div class="flex flex-col gap-5">
                <div class="qt-card p-4">
                  <p class="qt-body text-sm leading-relaxed">
                    This instrument shall consult the character&rsquo;s Commonplace Book — their
                    accumulated memoirs — and propose refinements to their profile based upon the
                    patterns of behaviour and personality therein observed. You shall review each
                    proposal and accept, reject, or amend it as your editorial judgement sees fit.
                  </p>
                </div>

                @if (profiles().length === 0) {
                  <div class="qt-card qt-border-destructive/30 qt-bg-destructive/5 p-4">
                    <div class="qt-text-destructive mb-2 flex items-center gap-2">
                      <qt-icon name="alert-triangle" class="w-4 h-4" />
                      <span class="text-sm font-semibold">No Connection Profile Available</span>
                    </div>
                    <p class="qt-body-sm qt-text-secondary">
                      The refinement proceedings require an AI connection profile to be configured.
                      Please visit The Foundry&rsquo;s Connection Profiles section to establish one
                      before proceeding.
                    </p>
                  </div>
                } @else {
                  <div class="flex flex-col gap-2">
                    <label class="qt-label" for="optimizer-profile-select">Select AI Model</label>
                    <!-- dogfood-#6: options are dynamic (async profiles list), so bind the
                         selection per-option with [selected] rather than [value] on the
                         select — a [value] evaluated before the options render leaves
                         the native control blank (the P4.6aa select-audit). -->
                    <select
                      id="optimizer-profile-select"
                      class="qt-select"
                      (change)="selectedProfileIdOverride.set($any($event.target).value)"
                    >
                      @for (profile of profiles(); track profile.id) {
                        <option [value]="profile.id" [selected]="profile.id === selectedProfileId()">
                          {{ profile.name }}
                        </option>
                      }
                    </select>
                    <p class="qt-helper">
                      The selected model shall perform the analysis and compose the proposed
                      refinements.
                    </p>
                  </div>

                  <div class="flex flex-col gap-2">
                    <label class="qt-label" for="optimizer-max-memories">
                      Maximum Memories to Analyse:
                      <span class="font-semibold text-primary">{{ maxMemories() }}</span>
                    </label>
                    <input
                      id="optimizer-max-memories"
                      type="range"
                      min="5"
                      max="200"
                      step="5"
                      class="qt-range w-full"
                      [value]="maxMemories()"
                      (input)="maxMemories.set(+$any($event.target).value)"
                    />
                    <div class="qt-text-secondary flex justify-between text-[10px]">
                      <span>5</span>
                      <span>200</span>
                    </div>
                  </div>

                  <details class="qt-card">
                    <summary class="qt-label cursor-pointer px-4 py-3 select-none">Filter Memories</summary>
                    <div class="qt-border-default flex flex-col gap-4 border-t px-4 pt-3 pb-4">
                      <div class="flex flex-col gap-2">
                        <label class="qt-label" for="optimizer-search-query">Search Query</label>
                        <input
                          id="optimizer-search-query"
                          type="text"
                          class="qt-input"
                          placeholder="e.g. &ldquo;betrayal&rdquo; or &ldquo;relationship with the duke&rdquo;"
                          maxlength="500"
                          [value]="searchQuery()"
                          (input)="searchQuery.set($any($event.target).value)"
                        />
                        <label class="qt-text-secondary flex items-center gap-2 text-sm">
                          <input
                            type="checkbox"
                            class="qt-checkbox"
                            [checked]="useSemanticSearch()"
                            (change)="useSemanticSearch.set($any($event.target).checked)"
                          />
                          Use semantic search (finds conceptually related memories)
                        </label>
                      </div>

                      <div class="flex gap-4">
                        <div class="flex flex-1 flex-col gap-1">
                          <label class="qt-label" for="optimizer-since-date">Since</label>
                          <input
                            id="optimizer-since-date"
                            type="date"
                            class="qt-input"
                            [value]="sinceDate()"
                            (input)="sinceDate.set($any($event.target).value)"
                          />
                        </div>
                        <div class="flex flex-1 flex-col gap-1">
                          <label class="qt-label" for="optimizer-before-date">Before</label>
                          <input
                            id="optimizer-before-date"
                            type="date"
                            class="qt-input"
                            [value]="beforeDate()"
                            (input)="beforeDate.set($any($event.target).value)"
                          />
                        </div>
                      </div>
                    </div>
                  </details>

                  @if (vaultAvailable()) {
                    <div class="qt-card flex flex-col gap-2 p-4">
                      <label class="flex cursor-pointer items-start gap-3">
                        <input
                          type="checkbox"
                          class="qt-checkbox mt-0.5"
                          [checked]="saveToVault()"
                          (change)="saveToVault.set($any($event.target).checked)"
                        />
                        <span class="flex flex-col gap-1">
                          <span class="qt-label text-sm">Save as suggestions in the vault (for later discussion)</span>
                          <span class="qt-helper">
                            In lieu of applying amendments directly, the proceedings shall be
                            inscribed as a markdown dossier in the character&rsquo;s vault — at
                            <code class="qt-code mx-1 text-[11px]">Suggestions/refinement-&lt;timestamp&gt;.md</code>
                            — so that author and character may review and debate the proposals at
                            leisure before any are commissioned.
                          </span>
                        </span>
                      </label>
                    </div>
                  }
                }
              </div>
            }

            @case ('progress') {
              <div class="flex flex-col gap-4">
                <div class="flex flex-col gap-1">
                  <h3 class="qt-section-title text-sm">Consulting the Memoirs</h3>
                  <p class="qt-section-subtitle text-xs">The automata are at their labours. Please stand by.</p>
                </div>

                <qt-progress-bar
                  [segments]="segments"
                  [currentKey]="state.progressStep()"
                  [startedAt]="state.startedAt()"
                  [hideLabels]="true"
                />

                <div class="qt-card flex flex-col gap-1 p-3">
                  @for (segment of segments; track segment.key) {
                    <div
                      [class]="
                        'flex items-center gap-3 rounded-md py-2 px-3 transition-colors ' +
                        (state.progressStep() === segment.key
                          ? 'qt-bg-primary/10'
                          : stepDone(segment.key)
                            ? 'opacity-60'
                            : 'opacity-40')
                      "
                    >
                      <div class="flex h-6 w-6 flex-shrink-0 items-center justify-center">
                        @if (state.progressStep() === segment.key) {
                          <qt-icon name="refresh" class="w-4 h-4 text-primary animate-spin" />
                        } @else if (stepDone(segment.key)) {
                          <qt-icon name="check" class="qt-text-success w-4 h-4" />
                        } @else {
                          <div class="qt-bg-muted-foreground h-2 w-2 rounded-full"></div>
                        }
                      </div>
                      <span
                        [class]="
                          'text-sm ' +
                          (state.progressStep() === segment.key
                            ? 'font-medium text-foreground'
                            : 'qt-text-secondary')
                        "
                        >{{ segment.label }}</span
                      >
                    </div>
                  }
                </div>

                @if (state.progressStep() === 'generating' && state.progressSubStep(); as subStep) {
                  <p class="qt-caption text-center">
                    {{ subStep.label }}
                    @if (subStep.total > 1) {
                      &mdash; pass {{ subStep.index }} of {{ subStep.total }}
                    }
                  </p>
                }

                @if (state.memoryCount() > 0) {
                  <p class="qt-caption text-center">
                    @if (state.filteredCount() > state.memoryCount()) {
                      {{ state.filteredCount() }} {{ state.filteredCount() === 1 ? 'memoir' : 'memoirs' }}
                      matched; top {{ state.memoryCount() }} selected for analysis
                    } @else {
                      {{ state.memoryCount() }} {{ state.memoryCount() === 1 ? 'memory' : 'memories' }}
                      retrieved from the Commonplace Book
                    }
                  </p>
                }

                @if (state.analysis(); as analysis) {
                  <qt-analysis-summary [analysis]="analysis" [memoryCount]="state.memoryCount()" />
                }

                @if (state.noSuggestionsMessage() && !state.loading()) {
                  <div class="qt-card flex flex-col items-center gap-3 p-4 text-center">
                    <qt-icon name="check-circle" class="qt-text-secondary w-10 h-10" />
                    <p class="qt-body-sm qt-text-secondary max-w-sm leading-relaxed">
                      {{ state.noSuggestionsMessage() }}
                    </p>
                    <button type="button" class="qt-button-secondary qt-button-sm" (click)="requestClose()">
                      Close
                    </button>
                  </div>
                }

                @if (state.error()) {
                  <div class="qt-card qt-border-destructive/30 qt-bg-destructive/5 p-3">
                    <p class="qt-text-destructive text-sm">{{ state.error() }}</p>
                  </div>
                }
              </div>
            }

            @case ('review') {
              @if (state.suggestions().length > 0) {
                <div class="flex min-h-0 flex-1 flex-col gap-4">
                  <div class="flex items-center justify-between">
                    <div class="flex items-center gap-1.5">
                      @for (suggestion of state.suggestions(); track suggestion.id; let idx = $index) {
                        <button
                          type="button"
                          [attr.aria-label]="'Go to suggestion ' + (idx + 1)"
                          [class]="'rounded-full transition-all ' + dotClass(idx)"
                          (click)="state.goToSuggestion(idx)"
                        ></button>
                      }
                    </div>
                    <span class="qt-caption">{{ reviewedCount() }} of {{ state.suggestions().length }} reviewed</span>
                  </div>

                  @if (currentSuggestion(); as suggestion) {
                    <qt-suggestion-card
                      [suggestion]="suggestion"
                      [decision]="state.decisions().get(suggestion.id)"
                      [editedValue]="state.editedValues().get(suggestion.id)"
                      [index]="state.currentIndex()"
                      [total]="state.suggestions().length"
                      (accept)="state.decideSuggestion(suggestion.id, 'accepted')"
                      (reject)="state.decideSuggestion(suggestion.id, 'rejected')"
                      (edit)="state.editSuggestion(suggestion.id, $event)"
                    />
                  }

                  @if (allReviewed() && state.currentIndex() < state.suggestions().length - 1) {
                    <div class="text-center">
                      <button type="button" class="qt-action text-xs" (click)="state.setPhase('apply')">
                        All proposals reviewed — proceed to Apply Changes
                      </button>
                    </div>
                  }
                </div>
              }
            }

            @case ('suggestions-file-written') {
              <div class="flex flex-col items-center gap-4 py-8 text-center">
                <div class="qt-bg-success/10 flex h-14 w-14 items-center justify-center rounded-full">
                  <qt-icon name="file" class="qt-text-success w-8 h-8" />
                </div>
                <div class="flex flex-col gap-1">
                  <h3 class="qt-section-title">Suggestions Inscribed</h3>
                  <p class="qt-section-subtitle max-w-md text-sm">
                    A dossier of {{ state.suggestions().length }}
                    {{ state.suggestions().length === 1 ? 'proposal has' : 'proposals have' }} been
                    deposited in {{ characterName() }}&rsquo;s vault. Author and character are invited
                    to peruse, deliberate, and commission refinements in their own good time.
                  </p>
                  @if (state.suggestionsFilePath(); as path) {
                    <p class="qt-caption mt-2 font-mono text-xs break-all">{{ path }}</p>
                  }
                </div>
              </div>
            }

            @case ('apply') {
              @if (applySuccess()) {
                <div class="flex flex-col items-center gap-4 py-8 text-center">
                  <div class="qt-bg-success/10 flex h-14 w-14 items-center justify-center rounded-full">
                    <qt-icon name="check" class="qt-text-success w-8 h-8" />
                  </div>
                  <div class="flex flex-col gap-1">
                    <h3 class="qt-section-title">Refinements Commissioned</h3>
                    <p class="qt-section-subtitle text-sm">
                      The amendments have been inscribed into {{ characterName() }}&rsquo;s permanent
                      record with all due ceremony. The character is now the beneficiary of your
                      editorial wisdom.
                    </p>
                  </div>
                </div>
              } @else {
                <qt-apply-confirmation
                  [changes]="acceptedChangeViews()"
                  [applying]="state.applying()"
                  (apply)="handleApply()"
                  (back)="state.setPhase('review')"
                />
              }

              @if (state.error() && !applySuccess()) {
                <div class="qt-card qt-border-destructive/30 qt-bg-destructive/5 p-3">
                  <p class="qt-text-destructive text-sm">{{ state.error() }}</p>
                </div>
              }
            }
          }
        </div>

        @if (state.phase() === 'preflight') {
          <div class="qt-dialog-footer flex flex-shrink-0 justify-between">
            <button type="button" class="qt-button-secondary" (click)="requestClose()">Cancel</button>
            <button
              type="button"
              [disabled]="!selectedProfileId() || profiles().length === 0"
              class="qt-button-primary disabled:opacity-50"
              (click)="handleStart()"
            >
              <qt-icon name="book" class="w-4 h-4" />
              Commence Refinement
            </button>
          </div>
        }

        @if (state.phase() === 'review') {
          <div class="qt-dialog-footer flex flex-shrink-0 flex-col gap-2">
            <div class="flex items-center gap-2">
              <button
                type="button"
                [disabled]="state.currentIndex() === 0"
                class="qt-button-ghost qt-button-sm disabled:opacity-30"
                (click)="state.prevSuggestion()"
              >
                <qt-icon name="chevron-left" class="w-4 h-4" />
                Previous
              </button>

              <div class="flex-1"></div>

              @if (state.currentIndex() < state.suggestions().length - 1) {
                <button type="button" class="qt-button-secondary qt-button-sm" (click)="state.nextSuggestion()">
                  Next
                  <qt-icon name="chevron-right" class="w-4 h-4" />
                </button>
              } @else {
                <button type="button" class="qt-button-primary qt-button-sm" (click)="state.setPhase('apply')">
                  Review Changes
                  <qt-icon name="chevron-right" class="w-4 h-4" />
                </button>
              }
            </div>
            <div class="flex items-center justify-between">
              <span class="qt-caption">
                {{ state.acceptedChanges().length }}
                {{ state.acceptedChanges().length === 1 ? 'change' : 'changes' }} accepted so far
              </span>
              <button type="button" class="qt-button-primary" (click)="state.setPhase('apply')">
                Review &amp; Apply Changes
              </button>
            </div>
          </div>
        }
      </div>
    </div>
  `,
})
export class CharacterOptimizerModal {
  protected readonly state = inject(OptimizerState);
  protected readonly segments = SEGMENTS;

  readonly characterId = input.required<string>();
  readonly characterName = input.required<string>();
  readonly profiles = input.required<CharacterConnectionProfile[]>();
  readonly defaultConnectionProfileId = input<string | null>(null);
  readonly vaultAvailable = input(false);

  readonly closed = output<void>();
  readonly applied = output<void>();

  /**
   * A manual pick overrides the derived default; until then it tracks
   * `defaultConnectionProfileId` / the first profile the way v4's
   * `useState(defaultConnectionProfileId ?? profiles[0]?.id ?? '')` does on
   * its one mount — computed from the inputs rather than read once in the
   * constructor, which is not a safe time to read signal inputs (the
   * P4.D115 idiom: derive, don't snapshot).
   */
  protected readonly selectedProfileIdOverride = signal<string | null>(null);
  protected readonly selectedProfileId = computed(
    () => this.selectedProfileIdOverride() ?? this.defaultConnectionProfileId() ?? this.profiles()[0]?.id ?? '',
  );
  protected readonly applySuccess = signal(false);
  protected readonly maxMemories = signal(30);
  protected readonly searchQuery = signal('');
  protected readonly useSemanticSearch = signal(true);
  protected readonly sinceDate = signal('');
  protected readonly beforeDate = signal('');
  protected readonly saveToVault = signal(false);

  protected readonly phaseDescription = computed(() => {
    switch (this.state.phase()) {
      case 'preflight':
        return 'Configure & commence the refinement proceedings';
      case 'progress':
        return 'The automata are consulting the memoirs…';
      case 'review': {
        const n = this.state.suggestions().length;
        return `Review ${n} proposed ${n === 1 ? 'amendment' : 'amendments'}`;
      }
      case 'apply':
        return 'Confirm amendments for commission';
      case 'suggestions-file-written':
        return 'Suggestions inscribed in the vault';
      default:
        return '';
    }
  });

  protected readonly currentSuggestion = computed(
    () => this.state.suggestions()[this.state.currentIndex()],
  );
  protected readonly allReviewed = computed(
    () =>
      this.state.suggestions().length > 0 &&
      this.state.suggestions().every((s) => this.state.decisions().has(s.id)),
  );
  protected readonly reviewedCount = computed(
    () => this.state.suggestions().filter((s) => this.state.decisions().has(s.id)).length,
  );
  protected readonly acceptedChangeViews = computed<OptimizerAcceptedChangeView[]>(
    () => this.state.acceptedChanges(),
  );

  protected stepDone(key: string): boolean {
    const current = this.state.progressStep();
    if (key === 'loading') return current !== null && current !== 'loading';
    if (key === 'analyzing') return current === 'generating';
    if (key === 'generating') return current === null;
    return false;
  }

  protected dotClass(idx: number): string {
    if (idx === this.state.currentIndex()) return 'w-6 h-2.5 qt-bg-primary';
    const id = this.state.suggestions()[idx].id;
    const decision = this.state.decisions().get(id);
    if (decision === 'rejected') return 'w-2.5 h-2.5 qt-bg-destructive/60';
    if (decision) return 'w-2.5 h-2.5 qt-bg-success/60';
    return 'w-2.5 h-2.5 qt-bg-muted-foreground/30 hover:qt-bg-muted-foreground/60';
  }

  protected async handleStart(): Promise<void> {
    if (!this.selectedProfileId()) return;
    const filterOptions: OptimizerFilterOptions = {
      maxMemories: this.maxMemories(),
      searchQuery: this.searchQuery(),
      useSemanticSearch: this.useSemanticSearch(),
      sinceDate: this.sinceDate() || null,
      beforeDate: this.beforeDate() || null,
    };
    const outputMode: OptimizerOutputMode =
      this.vaultAvailable() && this.saveToVault() ? 'suggestions-file' : 'apply';
    await this.state.startOptimization(this.characterId(), this.selectedProfileId(), filterOptions, outputMode);
  }

  protected async handleApply(): Promise<void> {
    await this.state.applyChanges(this.characterId());
    if (!this.state.error()) {
      this.applySuccess.set(true);
      setTimeout(() => this.applied.emit(), 1500);
    }
  }

  protected requestClose(): void {
    if (!this.state.loading() && !this.state.applying()) this.closed.emit();
  }
  protected onBackdrop(): void {
    this.requestClose();
  }
  protected onEscape(): void {
    this.requestClose();
  }
}
