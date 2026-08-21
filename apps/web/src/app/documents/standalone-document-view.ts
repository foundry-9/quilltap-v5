/**
 * `qt-standalone-document-view` — the chat-less Document Mode workspace tab
 * (port of v4 `components/workspace/StandaloneDocumentView.tsx`, baseline
 * `7e6d13e5`).
 *
 * Renders one {@link DocumentPane} bound to the standalone documents surface
 * ({@link StandaloneDocumentApi}) instead of a chat's document actions. No
 * `chat_documents` row exists and no Librarian announcement is posted — the
 * tab's payload is the only record of the open, so saving or closing here never
 * notifies a Salon conversation.
 *
 * Re-implements the per-document mechanics inline (v4 does NOT reuse
 * `useDocumentMode` here): 30 s autosave debounce, flush-on-blur,
 * absorb-first-serialization baseline (the first re-serialization after a load
 * is adopted as the baseline, not treated as an edit — same trap as
 * `markdown-field-load-must-not-emit`), and 409-conflict reload. Rename updates
 * the payload's `filePath` but NOT the `docKey` (stable across renames); delete
 * closes the tab via the self-close seam.
 *
 * Hosted-only: this component is never routed — the workspace tab host always
 * provides `WORKSPACE_HANDLE` + `WORKSPACE_TAB_ID`.
 *
 * @module documents/standalone-document-view
 */

import {
  ChangeDetectionStrategy,
  Component,
  computed,
  DestroyRef,
  inject,
  input,
  OnInit,
  signal,
} from '@angular/core';

import {
  DocumentStandaloneTabPayload,
  WORKSPACE_HANDLE,
  WORKSPACE_TAB_ID,
} from '../workspace/workspace-contract';
import type { ActiveDocument } from './document-api';
import { DocumentPane } from './document-pane';
import { computeAbsorbNext, type OpenDocEntry } from './document-mode';
import { StandaloneDocumentApi, type StandaloneScope } from './standalone-wire';
import { ToastService } from '../ui/toast.service';

const AUTOSAVE_DEBOUNCE_MS = 30000;

@Component({
  selector: 'qt-standalone-document-view',
  changeDetection: ChangeDetectionStrategy.OnPush,
  // `h-full`, NOT `flex-1`. v4 renders `DocumentPane`'s `flex flex-col h-full`
  // root as a DIRECT child of `.qt-tab-pane` — `TabView` is context providers
  // only, no DOM box (`components/workspace/TabView.tsx:177-186`). Angular's
  // `qt-tab-view` element IS a box, and an unstyled custom element is
  // `display: inline`, so a `flex-1` host has no flex parent to grow in and
  // collapses to content height. `h-full` resolves against the nearest block
  // container — `.qt-tab-pane`, which is the grid cell v4 measures against too
  // — which is why the Salon's `block h-full` host fills correctly and this one
  // did not. Symptom when it was `flex-1`: source mode's `h-full` textarea got
  // 77px inside a 788px pane (dogfood #97).
  host: { class: 'flex flex-col h-full min-h-0 min-w-0' },
  imports: [DocumentPane],
  template: `
    @if (loadError(); as message) {
      <div class="flex flex-col items-center justify-center h-full gap-3 p-8 text-center">
        <p class="qt-text-primary font-medium">This document could not be opened.</p>
        <p class="qt-text-secondary text-sm max-w-md">{{ message }}</p>
        <button type="button" class="qt-button-secondary px-3 py-1.5 rounded-md text-sm" (click)="closeSelf()">
          Close tab
        </button>
      </div>
    } @else if (entry(); as e) {
      <!-- No \`delimiters\` binding, deliberately: v4's
           \`StandaloneDocumentView.tsx:381\` mounts \`DocumentPane\` with no
           \`roleplayTemplateId\`, so the chat-less pane's toolbar shows the
           markdown buttons and no delimiter rail. There is no chat here to
           carry a template in the first place. -->
      <qt-document-pane
        [entry]="e"
        [mode]="'split'"
        (contentChange)="onContentChange($event)"
        (blur)="flushSave()"
        (rename)="onRename($event)"
        (close)="onClose()"
        (delete)="onDelete()"
      />
    } @else {
      <div class="flex items-center justify-center h-full">
        <p class="qt-text-muted text-sm">Fetching the manuscript…</p>
      </div>
    }
  `,
})
export class StandaloneDocumentView implements OnInit {
  private readonly api = inject(StandaloneDocumentApi);
  private readonly toasts = inject(ToastService);
  private readonly handle = inject(WORKSPACE_HANDLE, { optional: true });
  private readonly tabId = inject(WORKSPACE_TAB_ID, { optional: true });
  private readonly destroyRef = inject(DestroyRef);

  readonly payload = input.required<DocumentStandaloneTabPayload>();

  private readonly doc = signal<ActiveDocument | null>(null);
  protected readonly loadError = signal<string | null>(null);
  private readonly isDirty = signal(false);
  private readonly isSaving = signal(false);
  private readonly contentVersion = signal(0);

  // Live mirrors read by the async save/close paths (v4's useRef fields).
  private contentRef = '';
  private savedContentRef = '';
  // When true, the next content change is the rich editor's post-load
  // re-serialization and is adopted as baseline, not flagged dirty.
  private absorbNext = false;
  private autosaveTimer: ReturnType<typeof setTimeout> | null = null;
  // Open (or create) exactly once — the tab is kept alive across tab switches,
  // and a payload refresh (a blank gaining its filePath) must not re-open.
  private opened = false;

  /** The single open-doc entry the pane renders (v4 builds `ActiveDocument` inline). */
  protected readonly entry = computed<OpenDocEntry | null>(() => {
    const document = this.doc();
    if (!document) return null;
    return {
      document,
      isDirty: this.isDirty(),
      isSaving: this.isSaving(),
      isLLMEditing: false,
      contentVersion: this.contentVersion(),
      attentionTop: null,
      focusRequest: null,
    };
  });

  constructor() {
    this.destroyRef.onDestroy(() => this.clearAutosaveTimer());
  }

  ngOnInit(): void {
    if (this.opened) return;
    this.opened = true;
    const p = this.payload();
    this.api
      .open({
        filePath: p.filePath,
        title: p.displayTitle,
        scope: p.scope,
        mountPoint: p.mountPoint,
        targetFolder: p.targetFolder,
      })
      .then((data) => {
        const displayTitle = data.document.displayTitle || data.document.filePath;
        this.adoptDocument({
          id: p.docKey,
          filePath: data.document.filePath,
          scope: data.document.scope,
          mountPoint: data.document.mountPoint,
          displayTitle,
          content: data.content || '',
          mtime: data.mtime,
        });
        // Refresh the tab so a blank document's persisted payload gains the
        // server-picked filePath (a reload reopens the real file, not a new blank).
        this.refreshTab(data.document.filePath, displayTitle);
      })
      .catch((error: unknown) => {
        this.loadError.set(error instanceof Error ? error.message : String(error));
      });
  }

  // --- content load / baseline ---------------------------------------------

  /** Load (or reload) content into the pane, resetting the dirty baseline. */
  private adoptDocument(next: ActiveDocument): void {
    const content = next.content || '';
    this.contentRef = content;
    this.savedContentRef = content;
    this.absorbNext = computeAbsorbNext(next.filePath, content, true);
    this.isDirty.set(false);
    this.doc.set(next);
    this.contentVersion.update((v) => v + 1);
  }

  /**
   * Refresh this tab's payload/title in the workspace store. Same-identity
   * (docKey) refresh updates in place — crucial for blank documents, whose
   * persisted payload must gain the server-picked filePath. The docKey is
   * stable across renames.
   */
  private refreshTab(filePath: string, displayTitle: string): void {
    this.handle?.refreshTab(
      'document-standalone',
      { ...this.payload(), filePath, displayTitle } satisfies DocumentStandaloneTabPayload,
      displayTitle,
    );
  }

  // --- save / edit ----------------------------------------------------------

  private async saveDocument(): Promise<void> {
    const current = this.doc();
    if (!current || !this.isDirty()) return;

    this.isSaving.set(true);
    try {
      const content = this.contentRef;
      const outcome = await this.api.write({
        filePath: current.filePath,
        scope: current.scope as StandaloneScope,
        mountPoint: current.mountPoint,
        content,
        mtime: current.mtime,
      });

      if (outcome.kind === 'conflict') {
        // The file was written elsewhere while we had it open. With no pending
        // edits, silently adopt the disk version; otherwise keep the unsaved
        // content and just refresh mtime so the next save can succeed.
        const hadLocalEdits = this.contentRef !== this.savedContentRef;
        try {
          const latest = await this.api.read({
            filePath: current.filePath,
            scope: current.scope as StandaloneScope,
            mountPoint: current.mountPoint,
          });
          if (!hadLocalEdits) {
            this.adoptDocument({ ...current, content: latest.content, mtime: latest.mtime });
          } else {
            this.doc.update((d) => (d ? { ...d, mtime: latest.mtime } : d));
          }
        } catch {
          // Reload failed; leave the pane as-is (the next save retries).
        }
        return;
      }

      if (outcome.kind === 'error') return;

      this.savedContentRef = content;
      this.isDirty.set(false);
      this.doc.update((d) => (d ? { ...d, mtime: outcome.mtime } : d));
    } finally {
      this.isSaving.set(false);
    }
  }

  protected onContentChange(content: string): void {
    this.contentRef = content;
    this.doc.update((d) => (d ? { ...d, content } : d));

    // First change after an external load is the rich editor's normalized
    // re-serialization of the just-loaded content — adopt it as baseline.
    if (this.absorbNext) {
      this.absorbNext = false;
      this.savedContentRef = content;
      this.isDirty.set(false);
      return;
    }

    if (content === this.savedContentRef) {
      this.isDirty.set(false);
      return;
    }

    this.isDirty.set(true);
    this.clearAutosaveTimer();
    this.autosaveTimer = setTimeout(() => void this.saveDocument(), AUTOSAVE_DEBOUNCE_MS);
  }

  protected flushSave(): void {
    this.clearAutosaveTimer();
    if (this.isDirty()) void this.saveDocument();
  }

  private clearAutosaveTimer(): void {
    if (this.autosaveTimer) {
      clearTimeout(this.autosaveTimer);
      this.autosaveTimer = null;
    }
  }

  // --- rename / close / delete ---------------------------------------------

  protected async onRename(newTitle: string): Promise<void> {
    const current = this.doc();
    if (!current) return;
    const trimmed = newTitle.trim();
    if (!trimmed || trimmed === current.displayTitle) return;

    if (this.isDirty()) await this.saveDocument();

    try {
      const result = await this.api.rename({
        filePath: current.filePath,
        scope: current.scope as StandaloneScope,
        mountPoint: current.mountPoint,
        newTitle: trimmed,
      });
      const filePath = result.document.filePath;
      const displayTitle = result.document.displayTitle || filePath;
      // The docKey does NOT change — only the payload's filePath/title.
      this.doc.update((d) => (d ? { ...d, filePath, displayTitle } : d));
      this.refreshTab(filePath, displayTitle);
    } catch (error) {
      // v4 `StandaloneDocumentView.tsx:288`.
      this.toasts.showError(
        error instanceof Error ? error.message : "Couldn't rename document.",
      );
    }
  }

  protected async onClose(): Promise<void> {
    this.clearAutosaveTimer();
    if (this.isDirty()) await this.saveDocument();
    this.closeSelf();
  }

  protected async onDelete(): Promise<void> {
    const current = this.doc();
    if (!current) return;
    this.clearAutosaveTimer();
    try {
      await this.api.remove({
        filePath: current.filePath,
        scope: current.scope as StandaloneScope,
        mountPoint: current.mountPoint,
      });
      this.closeSelf();
    } catch (error) {
      // v4 `StandaloneDocumentView.tsx:327`.
      this.toasts.showError(
        error instanceof Error ? error.message : "Couldn't delete document.",
      );
    }
  }

  protected closeSelf(): void {
    if (this.handle && this.tabId != null) this.handle.closeTab(this.tabId);
  }
}
