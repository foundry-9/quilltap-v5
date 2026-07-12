import { computed, DestroyRef, inject, Injectable, signal } from '@angular/core';

import type { MessageDto } from '../core/core-contract';
import { DocumentApi, type ActiveDocument } from './document-api';
import { isMarkdownFile } from './frontmatter';
import { formatAutosaveNotification } from './unified-diff';

export type DocumentMode = 'normal' | 'split' | 'focus';

/** An LLM `doc_focus` instruction routed to the pane that owns the target doc. */
export interface DocFocusTarget {
  anchor?: string;
  highlight?: string;
  line?: number;
  clear_focus?: boolean;
  chatDocumentId?: string;
  filePath?: string;
  scope?: string;
  mountPoint?: string | null;
}

export interface FocusRequest {
  anchor?: string;
  highlight?: string;
  line?: number;
}

/** One open document plus its per-pane editor state (v4 `OpenDocEntry`). */
export interface OpenDocEntry {
  document: ActiveDocument;
  isDirty: boolean;
  isSaving: boolean;
  isLLMEditing: boolean;
  contentVersion: number;
  attentionTop: number | null;
  focusRequest: FocusRequest | null;
}

/** The chat fields the document store hydrates from (a subset of ChatDetail). */
export interface ChatDocumentFields {
  documentMode?: DocumentMode | null;
  dividerPosition?: number | null;
}

const AUTOSAVE_DEBOUNCE_MS = 30000;

/**
 * Whether markdown files render in the rich (re-serializing) Lexical editor.
 * The D17 spike decides this: GREEN → true (markdown gets Lexical, so the
 * absorb-first-serialization baseline logic engages); RED → false (markdown
 * ships in the same plain textarea as everything else, so there is no
 * post-remount re-serialization to absorb). See the pane + the status log.
 */
export const USES_RICH_MARKDOWN_EDITOR = false;

/** v4 `getDocumentModeState`: mode defaults to 'normal', divider to 45. */
export function documentModeFromChat(source: ChatDocumentFields | null | undefined): DocumentMode {
  return (source?.documentMode as DocumentMode) || 'normal';
}

export function dividerPositionFromChat(source: ChatDocumentFields | null | undefined): number {
  return source?.dividerPosition ?? 45;
}

/**
 * The absorb-first-serialization flag on a fresh content load (v4
 * `applyDocContent`): only a re-serializing markdown editor with non-empty
 * content emits the post-remount change we adopt as baseline.
 */
export function computeAbsorbNext(
  filePath: string,
  content: string,
  incrementVersion: boolean,
): boolean {
  if (!incrementVersion) return false;
  return USES_RICH_MARKDOWN_EDITOR && isMarkdownFile(filePath) && content !== '';
}

/** Repair the focused-doc pointer after `removedId` leaves `order` (v4 `removeEntry`). */
export function nextFocusedAfterRemove(
  order: OpenDocEntry[],
  removedId: string,
  focusedId: string | null,
): string | null {
  if (focusedId !== removedId) return focusedId;
  const idx = order.findIndex((e) => e.document.id === removedId);
  const remaining = order.filter((e) => e.document.id !== removedId);
  const neighbor = remaining[Math.min(Math.max(idx, 0), remaining.length - 1)];
  return neighbor ? neighbor.document.id : null;
}

/**
 * Map an open-document error into a single-line user-facing message (v4
 * `friendlyOpenDocumentError`): a File-not-found gets a friendlier note.
 */
export function friendlyOpenDocumentError(
  rawMessage: string,
  filePath: string | undefined,
  title: string | undefined,
): string {
  const serverMessage = rawMessage;
  if (/^File not found/i.test(serverMessage)) {
    const target = filePath || title;
    return target
      ? `Couldn't open "${target}" — file not found. It may have been deleted or renamed.`
      : "Couldn't open document — file not found.";
  }
  return serverMessage || "Couldn't open document.";
}

/**
 * Document Mode state for one Salon conversation (v4 `useDocumentMode` +
 * `documentModeApi`). Tracks the chat's set of open documents, per-document
 * autosave + baseline tracking, 409-reload, and Librarian-announcement append.
 *
 * The legacy single-pane Salon route (all v5 has this round) shows the FOCUSED
 * document; the open-SET model still tracks several (the server contract
 * supports it) so LLM-driven multi-doc opens reconcile correctly.
 *
 * OWNS the horizontal `dividerPosition` (v4: the document hook owns it, the
 * terminal hook owns only `rightPaneVerticalSplit`) — this is the P4.6x
 * ownership move noted in the round plan.
 *
 * Not `providedIn: 'root'` — one instance per conversation screen.
 */
@Injectable()
export class DocumentModeController {
  private readonly api = inject(DocumentApi);
  private readonly destroyRef = inject(DestroyRef);

  private chatId: string | null = null;
  private onLibrarianMessage: ((message: MessageDto) => void) | null = null;

  readonly openDocs = signal<OpenDocEntry[]>([]);
  readonly focusedDocId = signal<string | null>(null);
  readonly documentMode = signal<DocumentMode>('normal');
  readonly dividerPosition = signal<number>(45);

  /** True while any document is open (v4 `documentActive`). */
  readonly documentActive = computed(() => this.openDocs().length > 0);

  /** The focused open-doc entry, or the first one (drives the single pane). */
  readonly activeEntry = computed<OpenDocEntry | null>(() => {
    const docs = this.openDocs();
    const focused = this.focusedDocId();
    return docs.find((e) => e.document.id === focused) ?? docs[0] ?? null;
  });

  /** The focused document, or the first open one (v4 `activeDocument`). */
  readonly activeDocument = computed<ActiveDocument | null>(
    () => this.activeEntry()?.document ?? null,
  );

  // Per-document refs keyed by chat_documents row id (v4's useRef maps).
  private readonly contentRefs = new Map<string, string>();
  private readonly savedContentRefs = new Map<string, string>();
  private readonly autosaveTimers = new Map<string, ReturnType<typeof setTimeout>>();
  private readonly absorbNextRefs = new Map<string, boolean>();
  private readonly scrollPositions = new Map<string, number>();

  /** Guards the async hydrate/reconcile against a chat change mid-flight. */
  private hydrateToken = 0;
  /** Tracks the last hydrated (mode, divider) so a re-hydrate is idempotent. */
  private lastHydratedKey: string | null = null;

  constructor() {
    this.destroyRef.onDestroy(() => {
      for (const t of this.autosaveTimers.values()) clearTimeout(t);
      this.autosaveTimers.clear();
    });
  }

  /** Bind the controller to a conversation (chat id + the Librarian-append sink). */
  configure(chatId: string, onLibrarianMessage: (message: MessageDto) => void): void {
    this.chatId = chatId;
    this.onLibrarianMessage = onLibrarianMessage;
  }

  // -------------------------------------------------------------------------
  // Hydration (v4's init effect over chat.documentMode / chat.dividerPosition)
  // -------------------------------------------------------------------------

  /**
   * Initialize from the chat record on load, and re-reconcile when the server's
   * documentMode changes (e.g. an LLM open/close persisted the flag). Idempotent
   * across identical (mode, divider) so a chat refetch doesn't churn.
   */
  hydrate(chat: ChatDocumentFields | null): void {
    if (!chat) return;
    const mode = documentModeFromChat(chat);
    const divider = dividerPositionFromChat(chat);
    const key = `${mode}:${divider}`;
    if (key === this.lastHydratedKey) return;
    this.lastHydratedKey = key;

    this.documentMode.set(mode);
    this.dividerPosition.set(divider);

    if (mode !== 'normal') {
      void this.reconcileOpenDocuments();
    } else {
      this.clearAllEntries();
    }
  }

  // -------------------------------------------------------------------------
  // Content load / baseline
  // -------------------------------------------------------------------------

  /**
   * Load (or refresh) a document's content into its entry (v4 `applyDocContent`).
   * Resets the dirty baseline and, when `incrementVersion`, bumps
   * `contentVersion` so a rich editor remounts and absorbs its first
   * re-serialization as baseline.
   */
  private applyDocContent(doc: ActiveDocument, incrementVersion = true): void {
    const content = doc.content || '';
    this.contentRefs.set(doc.id, content);
    this.savedContentRefs.set(doc.id, content);
    this.absorbNextRefs.set(doc.id, computeAbsorbNext(doc.filePath, content, incrementVersion));

    this.openDocs.update((prev) => {
      const build = (existing?: OpenDocEntry): OpenDocEntry => ({
        document: doc,
        isDirty: false,
        isSaving: existing?.isSaving ?? false,
        isLLMEditing: existing?.isLLMEditing ?? false,
        contentVersion: incrementVersion
          ? (existing?.contentVersion ?? 0) + 1
          : (existing?.contentVersion ?? 0),
        attentionTop: existing?.attentionTop ?? null,
        focusRequest: existing?.focusRequest ?? null,
      });
      if (prev.some((e) => e.document.id === doc.id)) {
        return prev.map((e) => (e.document.id === doc.id ? build(e) : e));
      }
      return [...prev, build()];
    });
  }

  private updateEntry(docId: string, updater: (e: OpenDocEntry) => OpenDocEntry): void {
    this.openDocs.update((prev) => prev.map((e) => (e.document.id === docId ? updater(e) : e)));
  }

  private clearTimerFor(docId: string): void {
    const t = this.autosaveTimers.get(docId);
    if (t) {
      clearTimeout(t);
      this.autosaveTimers.delete(docId);
    }
  }

  /** Remove a document's entry + refs and repair the focused pointer (v4 `removeEntry`). */
  private removeEntry(docId: string): void {
    this.clearTimerFor(docId);
    this.contentRefs.delete(docId);
    this.savedContentRefs.delete(docId);
    this.absorbNextRefs.delete(docId);

    const order = this.openDocs();
    const nextFocused = nextFocusedAfterRemove(order, docId, this.focusedDocId());
    this.openDocs.set(order.filter((e) => e.document.id !== docId));
    if (this.focusedDocId() === docId) {
      this.focusedDocId.set(nextFocused);
    }
  }

  private clearAllEntries(): void {
    for (const t of this.autosaveTimers.values()) clearTimeout(t);
    this.autosaveTimers.clear();
    this.contentRefs.clear();
    this.savedContentRefs.clear();
    this.absorbNextRefs.clear();
    this.openDocs.set([]);
    this.focusedDocId.set(null);
  }

  // -------------------------------------------------------------------------
  // Save / edit (per document)
  // -------------------------------------------------------------------------

  async saveDocument(docId: string): Promise<void> {
    const entry = this.openDocs().find((e) => e.document.id === docId);
    if (!entry || !entry.isDirty || !this.chatId) return;
    const doc = entry.document;

    this.updateEntry(docId, (e) => ({ ...e, isSaving: true }));
    try {
      const oldContent = this.savedContentRefs.get(docId) ?? '';
      const newContent = this.contentRefs.get(docId) ?? '';
      const diffContent =
        oldContent !== newContent
          ? formatAutosaveNotification(oldContent, newContent, doc.displayTitle || doc.filePath) ||
            undefined
          : undefined;

      const result = await this.api.writeDocument(this.chatId, {
        filePath: doc.filePath,
        scope: doc.scope,
        mountPoint: doc.mountPoint,
        content: newContent,
        mtime: doc.mtime,
        diffContent,
      });

      if (result.kind === 'conflict') {
        // The file was written elsewhere (likely the LLM). Re-read; if the user
        // has no pending edits, adopt the server version, else keep the unsaved
        // content and just refresh mtime so the next save can succeed.
        const localContent = this.contentRefs.get(docId) ?? '';
        const hadLocalEdits = localContent !== (this.savedContentRefs.get(docId) ?? '');
        try {
          const latest = await this.api.readDocument(this.chatId, {
            filePath: doc.filePath,
            scope: doc.scope,
            mountPoint: doc.mountPoint,
          });
          if (!hadLocalEdits) {
            this.applyDocContent({ ...doc, content: latest.content, mtime: latest.mtime });
          } else {
            this.updateEntry(docId, (e) => ({
              ...e,
              document: { ...e.document, mtime: latest.mtime },
            }));
          }
        } catch {
          // Reload failed; leave the pane as-is (the next save retries).
        }
        return;
      }

      if (result.kind === 'error') {
        return;
      }

      this.savedContentRefs.set(docId, newContent);
      this.updateEntry(docId, (e) => ({
        ...e,
        isDirty: false,
        document: { ...e.document, mtime: result.mtime },
      }));
      if (result.librarianMessage) {
        this.onLibrarianMessage?.(result.librarianMessage);
      }
    } finally {
      this.updateEntry(docId, (e) => ({ ...e, isSaving: false }));
    }
  }

  handleContentChange(docId: string, content: string): void {
    this.contentRefs.set(docId, content);
    this.updateEntry(docId, (e) => ({ ...e, document: { ...e.document, content } }));

    // First change after an external load is the rich editor's normalized
    // re-serialization of the just-loaded content — adopt it as baseline.
    if (this.absorbNextRefs.get(docId)) {
      this.absorbNextRefs.set(docId, false);
      this.savedContentRefs.set(docId, content);
      this.updateEntry(docId, (e) => ({ ...e, isDirty: false }));
      return;
    }

    if (content === (this.savedContentRefs.get(docId) ?? '')) {
      this.updateEntry(docId, (e) => ({ ...e, isDirty: false }));
      return;
    }

    this.updateEntry(docId, (e) => ({ ...e, isDirty: true }));

    this.clearTimerFor(docId);
    this.autosaveTimers.set(
      docId,
      setTimeout(() => {
        void this.saveDocument(docId);
      }, AUTOSAVE_DEBOUNCE_MS),
    );
  }

  flushSave(docId: string): void {
    this.clearTimerFor(docId);
    const entry = this.openDocs().find((e) => e.document.id === docId);
    if (entry?.isDirty) {
      void this.saveDocument(docId);
    }
  }

  // -------------------------------------------------------------------------
  // Open / close / rename / delete
  // -------------------------------------------------------------------------

  async openDocument(params: {
    filePath?: string;
    title?: string;
    scope?: 'project' | 'document_store' | 'general';
    mountPoint?: string;
    mode?: 'split' | 'focus';
    targetFolder?: string;
  }): Promise<ActiveDocument | null> {
    if (!this.chatId) return null;
    const targetMode = params.mode || 'split';

    try {
      const data = await this.api.openDocument(this.chatId, {
        filePath: params.filePath,
        title: params.title,
        scope: params.scope || 'project',
        mountPoint: params.mountPoint,
        mode: targetMode,
        targetFolder: params.targetFolder,
      });

      const doc = this.api.toActiveDocument(data.document, data.content || '', data.mtime);
      this.applyDocContent(doc);
      this.focusedDocId.set(doc.id);
      this.documentMode.set(targetMode);

      if (data.librarianMessage) {
        this.onLibrarianMessage?.(data.librarianMessage);
      }
      return doc;
    } catch (error) {
      // A recoverable miss (e.g. the picker pointed at a since-deleted file):
      // the pane just doesn't open. The SPA has no toast bus yet — swallow.
      const raw = error instanceof Error ? error.message : String(error);
      this.openError.set(friendlyOpenDocumentError(raw, params.filePath, params.title));
      return null;
    }
  }

  /** The most recent open failure, surfaced for the picker/pane (no toast bus yet). */
  readonly openError = signal<string | null>(null);

  async closeDocument(docId?: string): Promise<void> {
    const targetId = docId ?? this.focusedDocId();
    if (!targetId || !this.chatId) return;

    const entry = this.openDocs().find((e) => e.document.id === targetId);
    if (entry?.isDirty) {
      await this.saveDocument(targetId);
    }

    await this.api.closeDocument(this.chatId, targetId);
    this.removeEntry(targetId);

    // documentMode only returns to 'normal' once the last document closes.
    if (this.openDocs().length === 0) {
      this.documentMode.set('normal');
      await this.persistMode('normal');
    }
  }

  async renameDocument(docId: string, newTitle: string): Promise<void> {
    const entry = this.openDocs().find((e) => e.document.id === docId);
    if (!entry || !this.chatId) return;
    const trimmed = newTitle.trim();
    if (!trimmed || trimmed === entry.document.displayTitle) return;

    if (entry.isDirty) {
      await this.saveDocument(docId);
    }

    const data = await this.api.renameDocument(this.chatId, trimmed, docId);
    this.updateEntry(docId, (e) => ({
      ...e,
      document: {
        ...e.document,
        filePath: data.document.filePath,
        displayTitle: data.document.displayTitle || data.document.filePath,
      },
    }));
    if (data.librarianMessage) {
      this.onLibrarianMessage?.(data.librarianMessage);
    }
  }

  async deleteDocument(docId: string): Promise<void> {
    const entry = this.openDocs().find((e) => e.document.id === docId);
    if (!entry || !this.chatId) return;

    this.clearTimerFor(docId);
    const data = await this.api.deleteDocument(this.chatId, docId);
    this.removeEntry(docId);
    if (this.openDocs().length === 0) {
      this.documentMode.set('normal');
    }
    if (data.librarianMessage) {
      this.onLibrarianMessage?.(data.librarianMessage);
    }
  }

  // -------------------------------------------------------------------------
  // LLM / server reconciliation
  // -------------------------------------------------------------------------

  /**
   * Reconcile the open-document set against the server (v4
   * `reconcileOpenDocuments`): drop docs the server no longer reports open, add
   * new ones, refresh changed content. Dirty panes are left untouched.
   */
  private async reconcileOpenDocuments(): Promise<void> {
    if (!this.chatId) return;
    const token = ++this.hydrateToken;
    let serverDocs;
    try {
      serverDocs = await this.api.fetchOpenDocuments(this.chatId);
    } catch {
      return;
    }
    if (token !== this.hydrateToken) return;
    const serverIds = new Set(serverDocs.map((d) => d.id));

    for (const e of this.openDocs()) {
      if (!serverIds.has(e.document.id)) this.removeEntry(e.document.id);
    }

    for (const rec of serverDocs) {
      const existing = this.openDocs().find((e) => e.document.id === rec.id);
      if (existing?.isDirty) continue;
      let contentData;
      try {
        contentData = await this.api.readDocument(this.chatId, {
          filePath: rec.filePath,
          scope: rec.scope,
          mountPoint: rec.mountPoint,
        });
      } catch {
        continue;
      }
      const fresh = contentData.content;
      if (existing && fresh === (this.savedContentRefs.get(rec.id) ?? '')) {
        this.updateEntry(rec.id, (e) => ({
          ...e,
          document: {
            ...e.document,
            mtime: contentData.mtime,
            displayTitle: rec.displayTitle || e.document.displayTitle,
          },
        }));
        continue;
      }
      this.applyDocContent(this.api.toActiveDocument(rec, fresh, contentData.mtime));
    }

    const focused = this.focusedDocId();
    if (!focused || !serverIds.has(focused)) {
      this.focusedDocId.set(serverDocs.length ? serverDocs[serverDocs.length - 1].id : null);
    }
  }

  /** Reload the open-document set + each document's content from the server. */
  async reloadFromServer(): Promise<void> {
    if (!this.chatId) return;
    try {
      const state = await this.api.fetchChatDocumentState(this.chatId);
      this.documentMode.set(documentModeFromChat(state));
      this.lastHydratedKey = null;
    } catch {
      // fall through to reconcile with whatever mode we have
    }
    await this.reconcileOpenDocuments();
  }

  /**
   * Re-read every open document's content after the LLM finishes a turn (v4
   * `handleLLMEditEnd`). Skips dirty panes so pending edits aren't lost.
   */
  async handleLLMEditEnd(): Promise<void> {
    if (!this.chatId) return;
    for (const entry of this.openDocs()) {
      if (entry.isDirty) continue;
      const doc = entry.document;
      try {
        const data = await this.api.readDocument(this.chatId, {
          filePath: doc.filePath,
          scope: doc.scope,
          mountPoint: doc.mountPoint,
        });
        const fresh = data.content;
        if (fresh !== (this.savedContentRefs.get(doc.id) ?? '')) {
          this.applyDocContent({ ...doc, content: fresh, mtime: data.mtime });
        } else {
          this.updateEntry(doc.id, (e) => ({ ...e, document: { ...e.document, mtime: data.mtime } }));
        }
      } catch {
        // leave this pane as-is
      }
    }
  }

  // -------------------------------------------------------------------------
  // Focus / attention
  // -------------------------------------------------------------------------

  /** Resolve which open document a doc_focus targets (v4 `resolveTargetDocId`). */
  private resolveTargetDocId(target: DocFocusTarget): string | null {
    const docs = this.openDocs();
    if (target.chatDocumentId && docs.some((e) => e.document.id === target.chatDocumentId)) {
      return target.chatDocumentId;
    }
    if (target.filePath) {
      const match = docs.find(
        (e) =>
          e.document.filePath === target.filePath &&
          (target.scope === undefined || e.document.scope === target.scope) &&
          (target.mountPoint === undefined ||
            (e.document.mountPoint ?? null) === (target.mountPoint ?? null)),
      );
      if (match) return match.document.id;
    }
    return this.focusedDocId() ?? (docs.length ? docs[docs.length - 1].document.id : null);
  }

  handleDocFocus(target: DocFocusTarget): void {
    const docId = this.resolveTargetDocId(target);
    if (!docId) return;

    if (target.clear_focus) {
      this.updateEntry(docId, (e) => ({ ...e, attentionTop: null, focusRequest: null }));
      return;
    }

    this.updateEntry(docId, (e) => ({
      ...e,
      focusRequest: { anchor: target.anchor, highlight: target.highlight, line: target.line },
    }));
    this.focusedDocId.set(docId);
  }

  setDocAttentionTop(docId: string, top: number | null): void {
    this.updateEntry(docId, (e) => ({ ...e, attentionTop: top }));
  }

  clearDocFocusRequest(docId: string): void {
    this.updateEntry(docId, (e) => ({ ...e, focusRequest: null }));
  }

  // -------------------------------------------------------------------------
  // Scroll / baseline / layout
  // -------------------------------------------------------------------------

  getScrollPosition(filePath: string): number {
    return this.scrollPositions.get(filePath) ?? 0;
  }

  setScrollPosition(filePath: string, pos: number): void {
    this.scrollPositions.set(filePath, pos);
  }

  getBaselineContent(docId: string): string {
    return this.savedContentRefs.get(docId) ?? '';
  }

  setFocusedDoc(docId: string): void {
    this.focusedDocId.set(docId);
  }

  toggleFocusMode(): void {
    const next = this.documentMode() === 'focus' ? 'split' : 'focus';
    this.documentMode.set(next);
    void this.persistMode(next);
  }

  setDividerPosition(position: number): void {
    this.dividerPosition.set(position);
    void this.persistDivider(position);
  }

  private async persistMode(mode: DocumentMode): Promise<void> {
    if (!this.chatId) return;
    try {
      await this.api.persistChatDocumentState(this.chatId, { documentMode: mode });
    } catch (error) {
      console.error('[DocumentMode] Failed to persist document mode', error);
    }
  }

  private async persistDivider(position: number): Promise<void> {
    if (!this.chatId) return;
    try {
      await this.api.persistChatDocumentState(this.chatId, { dividerPosition: position });
    } catch (error) {
      console.error('[DocumentMode] Failed to persist divider position', error);
    }
  }
}
