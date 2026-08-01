import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import { CoreClient, coreErrorMessage } from '../../core/core-client';
import { CoreDispatchError } from '../../core/core-contract';
import {
  TOOLS_FOLDER,
  TOOL_FILE_SUFFIX,
  collectUnknownKeys,
  definitionIssueLines,
  displayTitle,
  formatDefinitionIssues,
  safeParse,
} from '../../pascal/custom-tool-types';
import {
  draftFromDefinition,
  draftIsValid,
  newDraft,
  serializeDraft,
  validateDraft,
  type ToolDraft,
} from '../../pascal/tool-draft';
import { Icon } from '../../ui/icon';
import { BuilderForm } from './builder-form';
import { DestinationPicker, type PickedDestination } from './destination-picker';
import { OutcomesSection } from './outcomes-section';
import { SideEffectsSection } from './side-effects-section';
import { ProvingBench } from './proving-bench';
import * as api from './workbench.api';
import { ToastService } from '../../ui/toast.service';

/**
 * WorkbenchEditor — the single-definition editor of Pascal's Workbench (v4
 * `components/custom-tools/WorkbenchEditor.tsx`, 635).
 *
 * A main form column (identity, parameters, roll, outcome cascade) beside the
 * collapsible proving bench, under a header carrying the source store + path,
 * dirty state, Save / Save As…, and the Form ⇄ JSON switch.
 *
 * The validity gate (§6.1): **nothing invalid is ever written.** Form mode is
 * valid by construction plus a belt-and-braces parse before save; JSON mode
 * saves only what parses and validates. The one deliberate exception is REPAIR
 * mode — a file already broken on disk may be saved broken again, with an
 * explicit confirm, because refusing a partial repair would chase the user back
 * to the raw Scriptorium editor.
 *
 * All file I/O goes through the mount-file verbs — the Workbench adds no second
 * write path into stores.
 */

/** The JSON-mode validation readout, recomputed on a 300ms debounce. */
interface JsonState {
  valid: boolean;
  issues: string[];
  summary?: string;
  unknownKeys: string[];
}

/** {@link coreErrorMessage} with the bench's own fallback (v4 `0246c6c8`). */
function extractErrorMessage(err: unknown): string {
  return coreErrorMessage(err, 'Something went sideways at the bench.');
}

@Component({
  selector: 'qt-workbench-editor',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    Icon,
    BuilderForm,
    SideEffectsSection,
    OutcomesSection,
    ProvingBench,
    DestinationPicker,
  ],
  template: `
    @if (loading()) {
      <div class="p-6 text-sm qt-text-secondary">Fetching the card from its store…</div>
    } @else if (loadError(); as message) {
      <div class="p-6 space-y-2">
        <p class="text-sm qt-text-destructive">{{ message }}</p>
        <button type="button" class="qt-button qt-button-secondary" (click)="back.emit()">
          Back to the library
        </button>
      </div>
    } @else if (!initialized()) {
      <div class="p-6 text-sm qt-text-secondary">Setting up the bench…</div>
    } @else {
      <div class="h-full overflow-y-auto">
        <div class="max-w-6xl mx-auto p-4 space-y-3">
          <!-- Header -->
          <div class="flex items-center gap-3 flex-wrap">
            <button
              type="button"
              class="qt-button qt-button-ghost qt-button-sm"
              (click)="handleBack()"
            >
              <qt-icon name="arrow-left" class="w-4 h-4" />
              Library
            </button>
            <div class="min-w-0">
              <h1 class="qt-card-title text-base truncate">
                {{ headerTitle() }}
                @if (dirty()) {
                  <span class="qt-text-secondary" title="Unsaved changes"> •</span>
                }
              </h1>
              <p class="text-xs qt-text-secondary truncate">
                {{ location()?.path ?? 'unsaved — Pascal has nowhere to keep it yet' }}
              </p>
            </div>

            <div class="ml-auto flex items-center gap-2">
              <div
                class="flex rounded overflow-hidden border"
                role="radiogroup"
                aria-label="Editor mode"
              >
                <button
                  type="button"
                  role="radio"
                  [attr.aria-checked]="editorMode() === 'form'"
                  [class]="
                    editorMode() === 'form'
                      ? 'px-3 py-1 text-sm qt-button qt-button-primary'
                      : 'px-3 py-1 text-sm qt-button qt-button-ghost'
                  "
                  (click)="switchToForm()"
                  [disabled]="editorMode() === 'form' ? false : !canSwitchToForm()"
                  [attr.title]="formSwitchTitle()"
                >
                  Form
                </button>
                <button
                  type="button"
                  role="radio"
                  [attr.aria-checked]="editorMode() === 'json'"
                  [class]="
                    editorMode() === 'json'
                      ? 'px-3 py-1 text-sm qt-button qt-button-primary'
                      : 'px-3 py-1 text-sm qt-button qt-button-ghost'
                  "
                  (click)="switchToJson()"
                  [disabled]="editorMode() === 'json' || !draft()"
                >
                  JSON
                </button>
              </div>

              <button
                type="button"
                class="qt-button qt-button-ghost qt-button-sm"
                (click)="benchOpen.set(!benchOpen())"
                [attr.aria-pressed]="benchOpen()"
                [title]="benchOpen() ? 'Hide the proving bench' : 'Show the proving bench'"
              >
                <qt-icon name="dice" class="w-4 h-4" />
              </button>

              <button
                type="button"
                class="qt-button qt-button-primary qt-button-sm"
                (click)="handleSave()"
                [disabled]="(saveIsBlocked() && repairReason() === null) || saving()"
                [attr.title]="
                  saveIsBlocked() && repairReason() === null
                    ? 'The draft has errors to resolve first'
                    : null
                "
              >
                {{ saving() ? 'Saving…' : 'Save' }}
              </button>
              <button
                type="button"
                class="qt-button qt-button-secondary qt-button-sm"
                (click)="pickerOpen.set('save-as')"
                [disabled]="saveIsBlocked() || saving()"
                title="Write a copy to another store; the original stays put"
              >
                Save As…
              </button>
            </div>
          </div>

          <!-- Repair banner -->
          @if (repairReason(); as reason) {
            <div class="qt-card p-3 border qt-input-error space-y-1">
              <p class="text-sm">
                <strong>Repair mode.</strong> This card would not read, so the form is off the table
                until it validates.
              </p>
              <p class="text-xs qt-text-destructive">{{ reason }}</p>
            </div>
          }


          <!-- Body -->
          <div class="flex gap-4 items-start">
            <div class="flex-1 min-w-0 space-y-4">
              @if (editorMode() === 'form' && draft(); as current) {
                <qt-builder-form
                  [draft]="current"
                  [issues]="issues()"
                  [disabled]="saving()"
                  (draftChange)="onDraftChange($event)"
                />
                <qt-side-effects-section
                  [draft]="current"
                  [issues]="issues()"
                  [disabled]="saving()"
                  (draftChange)="onDraftChange($event)"
                />
                <qt-outcomes-section
                  [draft]="current"
                  [issues]="issues()"
                  [flashOutcomeId]="flashOutcomeId()"
                  [disabled]="saving()"
                  (draftChange)="onDraftChange($event)"
                />
              } @else {
                <div class="space-y-2">
                  <textarea
                    [value]="jsonText()"
                    (input)="onJsonInput($event)"
                    rows="28"
                    spellcheck="false"
                    class="qt-textarea w-full font-mono text-xs"
                    aria-label="Definition JSON"
                  ></textarea>
                  @if (jsonState(); as state) {
                    @if (!state.valid) {
                      <div class="qt-card p-2 space-y-1">
                        @for (issue of state.issues; track issue) {
                          <p class="text-xs qt-text-destructive">{{ issue }}</p>
                        }
                      </div>
                    }
                    @if (state.valid && state.unknownKeys.length > 0) {
                      <p class="text-xs qt-text-secondary">
                        Carries keys this build doesn&rsquo;t know:
                        {{ quoted(state.unknownKeys) }} — they&rsquo;ll be kept as-is.
                      </p>
                    }
                  }
                </div>
              }
              @if (editorMode() === 'form' && unknownKeysInDraft().length > 0) {
                <p class="text-xs qt-text-secondary">
                  Carries keys this build doesn&rsquo;t know: {{ quoted(unknownKeysInDraft()) }} —
                  they&rsquo;ll be kept as-is.
                </p>
              }
            </div>

            @if (benchOpen() && editorMode() === 'form' && draft(); as current) {
              <div class="w-80 flex-shrink-0 sticky top-0">
                <qt-proving-bench
                  [draft]="current"
                  [valid]="formValid()"
                  (matched)="onMatched($event)"
                />
              </div>
            }
          </div>
        </div>

        <!-- Destination picker -->
        @if (pickerOpen()) {
          <qt-destination-picker
            [toolName]="currentName() ?? ''"
            (pick)="saveToStore($event)"
            (cancel)="pickerOpen.set(null)"
            (openExisting)="onOpenExisting($event)"
          />
        }

        <!-- Conflict dialog -->
        @if (conflictContent() !== null && location(); as where) {
          <div class="qt-dialog-overlay" role="dialog" aria-modal="true">
            <div class="qt-card qt-shadow-lg rounded-lg border w-full max-w-md p-4 space-y-3">
              <h2 class="qt-card-title text-base">The file has moved under your hand</h2>
              <p class="text-sm">
                {{ where.path }} was changed by someone else since you opened it. Take theirs, or
                press yours?
              </p>
              <div class="flex justify-end gap-2">
                <button
                  type="button"
                  class="qt-button qt-button-secondary"
                  (click)="reloadTheirs()"
                >
                  Reload theirs
                </button>
                <button
                  type="button"
                  class="qt-button qt-button-destructive"
                  (click)="save(where, true)"
                >
                  Overwrite with mine
                </button>
              </div>
            </div>
          </div>
        }
      </div>
    }
  `,
})
export class WorkbenchEditor {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  /** Edit an existing file. */
  readonly source = input<{ mountPointId: string; path: string } | null>(null);
  /** Create a new definition; `template` pre-fills (duplicate flow, forces save-as). */
  readonly create = input<{ mountPointId?: string; template?: string } | null>(null);

  readonly back = output<void>();
  /** Open a different definition (the duplicate-name escape hatch). */
  readonly openOther = output<{ mountPointId: string; path: string }>();

  // -- Editor state ---------------------------------------------------------

  readonly draft = signal<ToolDraft | null>(null);
  readonly jsonText = signal('');
  readonly editorMode = signal<'form' | 'json'>('form');
  readonly repairReason = signal<string | null>(null);
  readonly dirty = signal(false);
  readonly location = signal<{ mountPointId: string; path: string } | null>(null);
  readonly mtime = signal<number | null>(null);
  readonly loadedName = signal<string | null>(null);
  readonly benchOpen = signal(true);
  readonly pickerOpen = signal<'save' | 'save-as' | null>(null);
  readonly conflictContent = signal<string | null>(null);
  readonly flashOutcomeId = signal<string | null>(null);

  readonly initialized = signal(false);
  readonly loading = signal(false);
  readonly loadError = signal<string | null>(null);
  readonly saving = signal(false);

  /** The 300ms-debounced mirror of {@link jsonText}. */
  private readonly debouncedJson = signal('');
  private debounceTimer: ReturnType<typeof setTimeout> | null = null;
  private flashTimer: ReturnType<typeof setTimeout> | null = null;

  /**
   * One-shot seed latch. Deliberately a PLAIN field, not a signal: the effect
   * below must not re-run when the seed's own state changes. Latching on
   * `initialized()`/`loading()` instead would re-enter the load every time
   * either flipped — and on a FAILED load (where both end up false) that is an
   * infinite retry loop against the store.
   */
  private seeded = false;

  constructor() {
    // v4 seeds from props at mount; here the inputs are read in an effect so
    // they are available, and the latch keeps it to exactly once per editor.
    effect(() => {
      if (this.seeded) return;
      const source = this.source();
      const create = this.create();
      this.seeded = true;
      if (source) {
        void this.loadFromStore(source);
      } else if (create?.template) {
        this.initializeFromContent(create.template, null);
        // A duplicate is a COPY: it has no location and must save-as.
        this.location.set(null);
        this.loadedName.set(null);
      } else {
        this.draft.set(newDraft());
        this.initialized.set(true);
      }
    });
  }

  private async loadFromStore(source: { mountPointId: string; path: string }): Promise<void> {
    this.loading.set(true);
    this.loadError.set(null);
    this.location.set(source);
    try {
      const file = await api.readDefinitionFile(this.core, source.mountPointId, source.path);
      this.initializeFromContent(file.content, file.mtime);
    } catch (err) {
      this.loadError.set(
        extractErrorMessage(err),
      );
    } finally {
      this.loading.set(false);
    }
  }

  /**
   * Seed editor state from raw file bytes (also used by reload-theirs). A file
   * that will not parse, or will not validate, lands in JSON mode with
   * `repairReason` set — the form is off the table until it validates.
   */
  private initializeFromContent(content: string, fileMtime: number | null): void {
    const broken = (reason: string) => {
      this.repairReason.set(reason);
      this.jsonText.set(content);
      this.debouncedJson.set(content);
      this.draft.set(null);
      this.editorMode.set('json');
      this.mtime.set(fileMtime);
      this.dirty.set(false);
      this.initialized.set(true);
    };

    let parsed: unknown;
    try {
      parsed = JSON.parse(content);
    } catch (error) {
      broken(
        `This file is not valid JSON: ${error instanceof Error ? error.message : String(error)}`,
      );
      return;
    }

    const validated = safeParse(parsed);
    if (!validated.success) {
      broken(formatDefinitionIssues(validated.issues));
      return;
    }

    this.repairReason.set(null);
    this.draft.set(draftFromDefinition(parsed));
    this.jsonText.set(content);
    this.debouncedJson.set(content);
    this.editorMode.set('form');
    this.loadedName.set(validated.data.name);
    this.mtime.set(fileMtime);
    this.dirty.set(false);
    this.initialized.set(true);
  }

  // -- Derived validation ---------------------------------------------------

  readonly issues = computed(() => {
    const d = this.draft();
    return d ? validateDraft(d) : [];
  });

  readonly formValid = computed(() => {
    const d = this.draft();
    return d ? draftIsValid(d) : false;
  });

  readonly jsonState = computed<JsonState | null>(() => {
    if (this.editorMode() !== 'json') return null;
    const text = this.debouncedJson();
    let parsed: unknown;
    try {
      parsed = JSON.parse(text);
    } catch (error) {
      return {
        valid: false,
        issues: [
          `Not valid JSON: ${error instanceof Error ? error.message : String(error)}`,
        ],
        unknownKeys: [],
      };
    }
    const validated = safeParse(parsed);
    if (!validated.success) {
      return {
        valid: false,
        issues: definitionIssueLines(validated.issues),
        summary: formatDefinitionIssues(validated.issues),
        unknownKeys: collectUnknownKeys(parsed),
      };
    }
    return { valid: true, issues: [], unknownKeys: collectUnknownKeys(parsed) };
  });

  readonly unknownKeysInDraft = computed(
    () => this.draft()?.unknownKeys.map(([key]) => key) ?? [],
  );

  protected quoted(keys: string[]): string {
    return keys.map((k) => `\`${k}\``).join(', ');
  }

  // -- Mode switch ----------------------------------------------------------

  switchToJson(): void {
    const d = this.draft();
    if (d) {
      const text = serializeDraft(d);
      this.jsonText.set(text);
      this.debouncedJson.set(text);
    }
    this.editorMode.set('json');
  }

  switchToForm(): void {
    if (this.editorMode() === 'form') return;
    let parsed: unknown;
    try {
      parsed = JSON.parse(this.jsonText());
    } catch {
      return;
    }
    const loaded = draftFromDefinition(parsed);
    if (!loaded) return;
    this.draft.set(loaded);
    this.repairReason.set(null);
    this.editorMode.set('form');
  }

  readonly canSwitchToForm = computed(
    () => this.editorMode() === 'json' && this.jsonState()?.valid === true,
  );

  protected formSwitchTitle(): string | null {
    if (this.editorMode() !== 'json' || this.canSwitchToForm()) return null;
    return this.jsonState()?.issues[0] ?? 'The JSON must validate before the form can hold it';
  }

  protected onJsonInput(event: Event): void {
    const text = (event.target as HTMLTextAreaElement).value;
    this.jsonText.set(text);
    this.dirty.set(true);
    if (this.debounceTimer) clearTimeout(this.debounceTimer);
    this.debounceTimer = setTimeout(() => this.debouncedJson.set(text), 300);
  }

  // -- Save -----------------------------------------------------------------

  /**
   * Form mode writes the canonical serialization; JSON mode writes the user's
   * bytes VERBATIM — canonicalization applies to form-mode emission only (§6.2).
   */
  contentToWrite(): string | null {
    if (this.editorMode() === 'form') {
      const d = this.draft();
      return d ? serializeDraft(d) : null;
    }
    return this.jsonText();
  }

  currentName(): string | null {
    if (this.editorMode() === 'form') return this.draft()?.name ?? null;
    try {
      const parsed = safeParse(JSON.parse(this.jsonText()));
      return parsed.success ? parsed.data.name : null;
    } catch {
      return null;
    }
  }

  readonly saveIsBlocked = computed(() => {
    if (this.editorMode() === 'form') return !this.formValid();
    const state = this.jsonState();
    if (!state) return true;
    return !state.valid && this.repairReason() === null;
  });

  headerTitle(): string {
    const d = this.draft();
    if (this.editorMode() === 'form' && d) {
      return d.name || d.title
        ? displayTitle({ name: d.name || 'contrivance', title: d.title || undefined })
        : 'A new contrivance';
    }
    return this.location()?.path ?? 'A new contrivance';
  }

  /** The one write path. `force` is the operator's overwrite decision. */
  async save(
    destination: { mountPointId: string; path: string },
    force = false,
  ): Promise<void> {
    const content = this.contentToWrite();
    if (content === null) return;

    this.saving.set(true);
    const here = this.location();
    const sameFile =
      here?.path === destination.path && here?.mountPointId === destination.mountPointId;
    try {
      const result = await api.writeDefinitionFile(this.core, {
        mountPointId: destination.mountPointId,
        path: destination.path,
        content,
        // Only a save back to the SAME location has an mtime to guard.
        expectedMtime: sameFile && !force ? (this.mtime() ?? undefined) : undefined,
        force: force || undefined,
      });
      this.location.set(destination);
      this.mtime.set(result.mtime);
      this.loadedName.set(this.currentName());
      this.dirty.set(false);
      this.conflictContent.set(null);
      this.toasts.showSuccess('Pascal has filed the contrivance.');
      this.saved.emit();
    } catch (err) {
      if (isConflict(err)) {
        // Conflict: someone else edited the file since we read it.
        this.conflictContent.set(content);
        return;
      }
      this.toasts.showError(extractErrorMessage(err));
    } finally {
      this.saving.set(false);
    }
  }

  /** Fired after a successful write so the shell can invalidate the library. */
  readonly saved = output<void>();

  /**
   * The full save pipeline for a chosen store: figure out the path (create =
   * `Tools/<name>.tool.json`) and refuse to clobber a file already there.
   */
  async saveToStore(store: PickedDestination): Promise<void> {
    const name = this.currentName();
    if (this.contentToWrite() === null) return;
    const here = this.location();

    if (!here || this.pickerOpen() === 'save-as' || here.mountPointId !== store.mountPointId) {
      // A fresh file (create, save-as, or new store — a copy; the original stays).
      const fileName = `${TOOLS_FOLDER}/${name ?? 'contrivance'}${TOOL_FILE_SUFFIX}`;
      try {
        if (await api.definitionFileExists(this.core, store.mountPointId, fileName)) {
          this.toasts.showError(`${store.mountName} already holds ${fileName} — rename this contrivance first.`);
          return;
        }
      } catch (err) {
        this.toasts.showError(extractErrorMessage(err));
        return;
      }
      this.pickerOpen.set(null);
      await this.save({ mountPointId: store.mountPointId, path: fileName });
      return;
    }

    this.pickerOpen.set(null);
    await this.save(here);
  }

  /** Save in place, offering the filename realignment when `name` changed. */
  async handleSave(): Promise<void> {
    if (this.saveIsBlocked() && this.repairReason() === null) return;

    // Repair mode may save an invalid file back only as itself (§6.4).
    const state = this.jsonState();
    if (this.editorMode() === 'json' && state && !state.valid) {
      if (!window.confirm('Save it broken? It will stay off the table until it validates.')) return;
    }

    const here = this.location();
    if (!here) {
      this.pickerOpen.set('save');
      return;
    }

    const name = this.currentName();
    const loaded = this.loadedName();
    const expectedFileName = name ? `${TOOLS_FOLDER}/${name}${TOOL_FILE_SUFFIX}` : null;
    if (name && loaded && name !== loaded && expectedFileName && here.path !== expectedFileName) {
      const renameFile = window.confirm(
        `The name is now "${name}". Also rename the file to ${expectedFileName}?\n\n` +
          'OK renames the file to match; Cancel keeps the current filename.',
      );
      if (renameFile) {
        try {
          if (await api.definitionFileExists(this.core, here.mountPointId, expectedFileName)) {
            this.toasts.showError(`${expectedFileName} already exists in this store; the file keeps its name.`);
          } else {
            const content = this.contentToWrite();
            if (content === null) return;
            // Write the new path FIRST, delete the old only once that landed — a
            // failure between the two leaves both copies, never neither.
            const result = await api.writeDefinitionFile(this.core, {
              mountPointId: here.mountPointId,
              path: expectedFileName,
              content,
            });
            await api.deleteDefinitionFile(this.core, here.mountPointId, here.path);
            this.location.set({ mountPointId: here.mountPointId, path: expectedFileName });
            this.mtime.set(result.mtime);
            this.loadedName.set(name);
            this.dirty.set(false);
            this.toasts.showSuccess('Filed under its new name.');
            this.saved.emit();
            return;
          }
        } catch (err) {
          this.toasts.showError(extractErrorMessage(err));
          return;
        }
      }
    }

    await this.save(here);
  }

  async reloadTheirs(): Promise<void> {
    const here = this.location();
    if (!here) return;
    try {
      const fresh = await api.readDefinitionFile(this.core, here.mountPointId, here.path);
      this.initializeFromContent(fresh.content, fresh.mtime);
      this.conflictContent.set(null);
    } catch (err) {
      this.toasts.showError(extractErrorMessage(err));
    }
  }

  // -- Plumbing -------------------------------------------------------------

  onDraftChange(next: ToolDraft): void {
    this.draft.set(next);
    this.dirty.set(true);
  }

  onMatched(outcomeId: string | null): void {
    this.flashOutcomeId.set(outcomeId);
    if (this.flashTimer) clearTimeout(this.flashTimer);
    this.flashTimer = setTimeout(() => this.flashOutcomeId.set(null), 1600);
  }

  protected onOpenExisting(mountPointId: string): void {
    const name = this.currentName();
    this.pickerOpen.set(null);
    if (name) {
      this.openOther.emit({
        mountPointId,
        path: `${TOOLS_FOLDER}/${name}${TOOL_FILE_SUFFIX}`,
      });
    }
  }

  handleBack(): void {
    if (this.dirty() && !window.confirm('Leave the workbench? Unsaved changes will be lost.')) {
      return;
    }
    this.back.emit();
  }
}

/**
 * v4 keys the conflict flow off HTTP 409. The dispatch envelope carries the same
 * distinction as a TYPED kind (`ErrorKind::Conflict`, kebab-cased on the wire),
 * which `store_mount_file` raises on an `expected_mtime` mismatch — so the
 * conflict is recognised structurally rather than by sniffing the message text.
 */
function isConflict(err: unknown): boolean {
  return err instanceof CoreDispatchError && err.kind === 'conflict';
}
