import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  OnInit,
  computed,
  inject,
  input,
  output,
  signal,
  viewChild,
} from '@angular/core';

import { RichEditor } from '../editor/rich-editor';
import type { CompiledRules } from '../editor/text-replacement';
import { Icon } from '../ui/icon';
import {
  uploadChatFile,
  type ChatFileUploadResult,
  type ConflictResolution,
  type FileConflictInfo,
  type UploadedChatFile,
} from './chat-files.api';
import { FileConflictDialog } from './file-conflict-dialog';

/** What the composer emits on send — the text plus any attached file ids. */
export interface ComposerSend {
  content: string;
  fileIds: string[];
}

/**
 * The Salon composer. The D17 gate ran GREEN, so the message box is now the
 * `qt-rich-editor` (ProseMirror over the v4 composer dialect — byte-faithful,
 * proven by `markdown-round-trip.spec.ts`) rather than a plain textarea. Enter
 * sends, Shift+Enter inserts a line break (v4 `KeyboardPlugin` chat mode); the
 * send reads the markdown from the editor handle at submit time (v4's decoupled
 * `ComposerSyncPlugin` posture), and a user-typed `*narration*` stays literal.
 * The **attach** gutter tool (v4 `useFileAttachments` + the composer chips): a
 * hidden file input + a paste-image handler upload to the chat-files multipart
 * leg, showing attached-file chips and the duplicate-conflict resolver; the file
 * ids ride the send. The announcement/mail/RNG gutter tools + drag-and-drop
 * remain locked deferrals.
 */
@Component({
  selector: 'qt-chat-composer',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, FileConflictDialog, RichEditor],
  template: `
    <div class="qt-chat-composer">
      @if (attachedFiles().length > 0) {
        <div class="qt-chat-attachment-list mb-2">
          @for (file of attachedFiles(); track file.id) {
            <div class="qt-chat-attachment-chip">
              @if (file.mimeType.startsWith('image/')) {
                <qt-icon
                  name="image"
                  class="qt-chat-attachment-chip-icon qt-chat-attachment-chip-icon-success"
                />
              } @else {
                <qt-icon
                  name="file"
                  class="qt-chat-attachment-chip-icon qt-chat-attachment-chip-icon-info"
                />
              }
              <span class="text-foreground max-w-[150px] truncate">{{ file.filename }}</span>
              <button
                type="button"
                class="qt-chat-attachment-chip-remove"
                title="Remove attachment"
                aria-label="Remove attachment"
                (click)="removeAttachedFile(file.id)"
              >
                <qt-icon name="close" class="w-4 h-4" />
              </button>
            </div>
          }
        </div>
      }

      @if (uploadError(); as msg) {
        <div class="qt-alert-error text-sm mb-2" role="alert">{{ msg }}</div>
      }

      <form class="qt-chat-composer-inner" (submit)="onSubmit($event)">
        <qt-rich-editor
          #editor
          class="qt-chat-composer-input qt-lexical-contenteditable"
          [value]="restoredDraft()"
          [placeholder]="placeholder()"
          [disabled]="disabled()"
          [submitOnEnter]="!compositionMode()"
          [submitOnModEnter]="compositionMode()"
          [textReplacementRules]="effectiveTextReplacementRules()"
          ariaLabel="Message"
          (contentChange)="onContentChange($event)"
          (submit)="submit()"
          (imagePaste)="onEditorImagePaste($event)"
        />

        <input
          #fileInput
          type="file"
          class="hidden"
          aria-label="Attach a file"
          (change)="onPick($any($event.target))"
        />

        <div class="qt-chat-composer-actions">
          <button
            type="button"
            class="qt-chat-toolbar-button"
            [class.qt-chat-toolbar-button-active]="compositionMode()"
            [title]="compositionToggleTitle()"
            [attr.aria-pressed]="compositionMode()"
            aria-label="Toggle composition mode"
            (click)="toggleCompositionMode()"
          >
            <qt-icon name="file" class="w-5 h-5" />
          </button>

          <button
            type="button"
            class="qt-chat-toolbar-button"
            title="Attach a file"
            aria-label="Attach a file"
            [disabled]="disabled() || uploading()"
            (click)="fileInput.click()"
          >
            <qt-icon name="paperclip" class="w-5 h-5" />
          </button>

          <button
            type="button"
            class="qt-chat-toolbar-button"
            title="Generate an image"
            aria-label="Generate an image"
            (click)="openGenerate.emit()"
          >
            <qt-icon name="sparkles" class="w-5 h-5" />
          </button>

          @if (!documentActive()) {
            <button
              type="button"
              class="qt-chat-toolbar-button"
              title="Open document"
              aria-label="Open document"
              (click)="openDocument.emit()"
            >
              <qt-icon name="book" class="w-5 h-5" />
            </button>
          }

          @if (!terminalActive()) {
            <button
              type="button"
              class="qt-chat-toolbar-button"
              title="Open terminal"
              aria-label="Open terminal"
              (click)="openTerminal.emit()"
            >
              <qt-icon name="code" class="w-5 h-5" />
            </button>
          }

          <button
            type="button"
            class="qt-chat-toolbar-button qt-chat-continue-button"
            title="Continue — let the next character respond"
            aria-label="Continue"
            [disabled]="disabled() || busy() || !hasActiveCharacters()"
            (click)="continue.emit()"
          >
            <qt-icon name="arrow-right" class="w-5 h-5" />
          </button>

          @if (busy()) {
            <button
              type="button"
              class="qt-chat-composer-send qt-chat-stop-button"
              title="Stop generating"
              aria-label="Stop generating"
              (click)="stop.emit()"
            >
              <qt-icon name="stop" class="w-5 h-5" />
            </button>
          } @else {
            <button
              type="submit"
              class="qt-chat-composer-send"
              [title]="sendTitle()"
              aria-label="Send message"
              [disabled]="!canSend()"
            >
              <qt-icon name="send" class="w-5 h-5" />
            </button>
          }
        </div>
      </form>
    </div>

    @if (conflict(); as c) {
      <qt-file-conflict-dialog
        [conflict]="c"
        [resolving]="resolving()"
        (resolve)="onResolveConflict($event)"
        (cancel)="cancelConflict()"
      />
    }
  `,
})
export class ChatComposer implements OnInit {
  private readonly destroyRef = inject(DestroyRef);

  /** Streaming/awaiting in flight — swaps Send for Stop and blocks input. */
  readonly busy = input(false);
  readonly disabled = input(false);
  readonly hasActiveCharacters = input(true);
  /** Terminal Mode is showing a pane — hide the open-terminal button (v4). */
  readonly terminalActive = input(false);
  /** Document Mode is showing a pane — hide the open-document button (v4). */
  readonly documentActive = input(false);
  /** The chat whose files the attach affordance uploads to. */
  readonly chatId = input.required<string>();
  /**
   * Composition mode (v4 `documentEditingMode`): plain Enter inserts a
   * paragraph, Cmd/Ctrl+Enter sends. Bound from the chat's flag by the salon at
   * unification (§3); the composer itself does not persist the toggle.
   */
  readonly compositionMode = input(false);
  /**
   * Compiled text-replacement rules + the on/off gate (v4
   * `TextReplacementPlugin`, `chat_settings.textReplacementsEnabled`). The salon
   * fetches and compiles these at unification (§3); in-lane they are injected.
   */
  readonly textReplacementRules = input<CompiledRules | null>(null);
  readonly textReplacementsEnabled = input(true);

  readonly send = output<ComposerSend>();
  readonly stop = output<void>();
  readonly continue = output<void>();
  readonly openTerminal = output<void>();
  readonly openDocument = output<void>();
  readonly openGenerate = output<void>();
  /** The user flipped the mode via the toolbar toggle; the salon persists it. */
  readonly compositionModeChange = output<boolean>();

  private readonly editor = viewChild.required(RichEditor);

  /** The saved draft restored once on mount — seeds the editor's initial value. */
  protected readonly restoredDraft = signal('');
  /** v4's 800ms draft debounce (`ComposerSyncPlugin.tsx:65`). */
  private static readonly DRAFT_DEBOUNCE_MS = 800;
  private draftTimer: ReturnType<typeof setTimeout> | null = null;

  /** The live markdown from the editor — drives `hasContent` gating only. */
  protected readonly text = signal('');
  protected readonly attachedFiles = signal<UploadedChatFile[]>([]);
  protected readonly uploading = signal(false);
  protected readonly uploadError = signal<string | null>(null);

  // Conflict-resolution state (v4 useFileAttachments).
  protected readonly conflict = signal<FileConflictInfo | null>(null);
  protected readonly resolving = signal(false);
  private pendingFile: File | null = null;

  protected readonly canSend = computed(
    () =>
      !this.disabled() &&
      !this.busy() &&
      (this.text().trim().length > 0 || this.attachedFiles().length > 0) &&
      this.hasActiveCharacters(),
  );

  protected readonly placeholder = computed(() => {
    if (!this.hasActiveCharacters()) return 'Add a character to start chatting…';
    return this.attachedFiles().length > 0 ? 'Add a message (optional)…' : 'Type a message…';
  });

  protected readonly sendTitle = computed(() =>
    this.hasActiveCharacters() ? 'Send message' : 'Add a character to start chatting',
  );

  /** Rules only take effect when the feature gate is on (v4). */
  protected readonly effectiveTextReplacementRules = computed(() =>
    this.textReplacementsEnabled() ? this.textReplacementRules() : null,
  );

  /** Mac vs non-Mac decides the Cmd/Ctrl label (v4 `isMac`, ChatComposer.tsx). */
  private readonly isMac =
    typeof navigator !== 'undefined' && navigator.platform.toUpperCase().indexOf('MAC') >= 0;

  /** v4's two titles, keyed off which mode a click would switch you INTO. */
  protected readonly compositionToggleTitle = computed(() =>
    this.compositionMode()
      ? 'Switch to chat mode (Enter to send)'
      : `Switch to composition mode (${this.isMac ? 'Cmd' : 'Ctrl'}+Enter to send)`,
  );

  /** Flip the mode and let the salon persist it (the composer stays uncontrolled). */
  protected toggleCompositionMode(): void {
    this.compositionModeChange.emit(!this.compositionMode());
  }

  // --- draft persistence (v4 useDraftPersistence + ComposerSyncPlugin) -------

  ngOnInit(): void {
    // Restore a saved draft ONCE on mount; the editor mounts with it as its
    // initial value (v4 restores via setInput, which the editor then adopts).
    try {
      const saved = localStorage.getItem(this.draftKey());
      if (saved) this.restoredDraft.set(saved);
    } catch {
      /* localStorage unavailable (private mode) — no draft to restore */
    }
    this.destroyRef.onDestroy(() => {
      if (this.draftTimer) clearTimeout(this.draftTimer);
    });
  }

  /** The editor's live markdown: gate send + schedule the debounced draft save. */
  protected onContentChange(md: string): void {
    this.text.set(md);
    if (this.draftTimer) clearTimeout(this.draftTimer);
    this.draftTimer = setTimeout(() => this.persistDraft(md), ChatComposer.DRAFT_DEBOUNCE_MS);
  }

  private draftKey(): string {
    return `quilltap-draft-${this.chatId()}`;
  }

  /** Blank text removes the key; otherwise store the markdown (v4 persistDraft). */
  private persistDraft(text: string): void {
    try {
      if (text.trim()) localStorage.setItem(this.draftKey(), text);
      else localStorage.removeItem(this.draftKey());
    } catch {
      /* localStorage unavailable — drop the draft silently */
    }
  }

  /** A successful send clears the draft immediately (v4 useSSEStreaming). */
  private clearDraft(): void {
    if (this.draftTimer) {
      clearTimeout(this.draftTimer);
      this.draftTimer = null;
    }
    try {
      localStorage.removeItem(this.draftKey());
    } catch {
      /* localStorage unavailable */
    }
  }

  protected onSubmit(event: Event): void {
    event.preventDefault();
    this.submit();
  }

  /** Enter (via the editor's submit) or the Send button both land here. */
  protected submit(): void {
    if (!this.canSend()) {
      return;
    }
    // Send reads the markdown from the editor handle at submit time (v4's
    // decoupled ComposerSyncPlugin posture) — authoritative over the debounced
    // `text` signal, and byte-faithful to the v4 composer dialect.
    this.send.emit({
      content: this.editor().getMarkdown().trim(),
      fileIds: this.attachedFiles().map((f) => f.id),
    });
    this.editor().setMarkdown('');
    this.text.set('');
    this.attachedFiles.set([]);
    this.clearDraft();
  }

  // --- attach ---------------------------------------------------------------

  protected async onPick(input: HTMLInputElement): Promise<void> {
    const file = input.files?.[0];
    input.value = ''; // reset so re-picking the same file re-fires change
    if (file) await this.upload(file);
  }

  /** A pasted image (from the editor) uploads like a picked file (v4 paste). */
  protected onEditorImagePaste(file: File): void {
    void this.upload(file);
  }

  protected removeAttachedFile(fileId: string): void {
    this.attachedFiles.update((files) => files.filter((f) => f.id !== fileId));
  }

  private async upload(
    file: File,
    opts?: { resolution?: ConflictResolution; conflictingFileId?: string },
  ): Promise<void> {
    this.uploading.set(true);
    this.uploadError.set(null);
    try {
      const result = await uploadChatFile(this.chatId(), file, opts);
      this.handleUploadResult(result, file);
    } catch (err) {
      this.uploadError.set(err instanceof Error ? err.message : 'Failed to upload file');
    } finally {
      this.uploading.set(false);
    }
  }

  private handleUploadResult(result: ChatFileUploadResult, file: File): void {
    if (result.kind === 'duplicate') {
      this.pendingFile = file;
      this.conflict.set(result.conflict);
      return;
    }
    this.attachedFiles.update((files) => [...files, result.file]);
  }

  protected async onResolveConflict(resolution: ConflictResolution): Promise<void> {
    const file = this.pendingFile;
    const info = this.conflict();
    if (!file || !info) return;
    this.resolving.set(true);
    this.uploadError.set(null);
    try {
      const result = await uploadChatFile(this.chatId(), file, {
        resolution,
        conflictingFileId: info.existingFile.id,
      });
      // 'skip' returns a fresh file or nothing; only push a real file result.
      if (result.kind === 'file') {
        this.attachedFiles.update((files) => [...files, result.file]);
      }
      this.conflict.set(null);
      this.pendingFile = null;
    } catch (err) {
      this.uploadError.set(err instanceof Error ? err.message : 'Failed to resolve conflict');
    } finally {
      this.resolving.set(false);
    }
  }

  protected cancelConflict(): void {
    this.conflict.set(null);
    this.pendingFile = null;
  }
}
