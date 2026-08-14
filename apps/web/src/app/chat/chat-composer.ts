import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  type ElementRef,
  OnInit,
  computed,
  inject,
  input,
  output,
  signal,
  viewChild,
} from '@angular/core';

import { injectCharInsertSettings } from '../editor/char-insert/char-insert-settings';
import { recordRecent } from '../editor/char-insert/recents-storage';
import {
  applyDelimiterCommand,
  applySourceDelimiter,
} from '../editor/delimiter-transforms';
import { formatCommand } from '../editor/format-commands';
import {
  FormattingToolbar,
  type CharInsert,
  type FormatAction,
} from '../editor/formatting-toolbar';
import { applySourceFormat } from '../editor/source-transforms';
import { RichEditor } from '../editor/rich-editor';
import { SmartTypographySettings } from '../smart-typography/settings';
import type { CompiledRules } from '../editor/text-replacement';
import type { NarrationDelimiters, TemplateDelimiter } from '../core/core-contract';
import { Icon } from '../ui/icon';
import {
  uploadChatFile,
  type ChatFileUploadResult,
  type ConflictResolution,
  type FileConflictInfo,
  type UploadedChatFile,
} from './chat-files.api';
import { CustomToolsPopup } from './custom-tools-popup';
import { RngDropdown, type RngPendingResult } from './rng-dropdown';
import { FileConflictDialog } from './file-conflict-dialog';
import { SpeakingAsAvatar } from './speaking-as-avatar';
import { ToastService } from '../ui/toast.service';

/** The character the human is currently speaking as (v4 `speakingAs` prop). */
export interface SpeakingAsSeat {
  name: string;
  avatarUrl: string | null;
}

/** What the composer emits on send — the text plus any attached file ids. */
export interface ComposerSend {
  content: string;
  fileIds: string[];
}

/**
 * One rolled-but-unsent tool result as the composer renders it (v4
 * `types.ts:340-360` `PendingToolResult` — the salon-side record, which adds the
 * `id` and `createdAt` the raw roll has no need of).
 */
export interface PendingToolResultChip extends RngPendingResult {
  id: string;
  createdAt: string;
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
 * ids ride the send. The **Generate Image** gutter tool (v4
 * `ComposerGutterTools:102-112`) raises `openGenerate`, which the Salon answers
 * with the standalone generate dialog — v4's only opener for it.
 *
 * **The gutter group follows v4's grid fill order** (`ComposerGutterTools.tsx
 * :36-46`: announcement, mail / library-file, camera / paperclip, RNG / wand),
 * flattened into v5's single action row. All six of v4's tools are present:
 * **Library file** filled the last empty slot in P4.9E4B, raising `openLibrary`,
 * which the Salon answers with `qt-library-file-picker-modal`.
 *
 * **RNG** filled its slot in P4.9E1B, over P4.9E3A's `chatRng`. It rolls in
 * PREVIEW mode, so the result arrives as a chip above the box rather than as a
 * message: the operator can discard it, and it only becomes a TOOL row when the
 * next send carries it as a `pendingToolResults` entry. The list itself belongs
 * to the Salon (as it does in v4), because it must outlive this component's own
 * post-send reset.
 *
 * The composition-mode toggle leads the row: it is v4's own composer-level
 * control (`ChatComposer.tsx`), not one of the gutter tools.
 *
 * ⚠ The row is now ELEVEN wide, where v4's grid spends only two columns on six
 * tools. That difference is load-bearing in a narrow pane: `_chat.css` gives the
 * message box a `min-width` floor and lets both the inner wrapper and this row
 * wrap, or the tools crowd the box to zero width and it stops being clickable at
 * all (see the P4.9E2B lane record). Check the split-pane width before adding a
 * twelfth.
 *
 * v4 has **no** composer drag-and-drop upload — the phrase appeared in v5's own
 * deferral note (P4.6ac's lane record) and propagated, but there is no drag
 * handler anywhere in v4's chat components, salon page, file-attachment hook or
 * markdown editor at `e646f58b`. Nothing is owed here.
 */
@Component({
  selector: 'qt-chat-composer',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    Icon,
    FileConflictDialog,
    FormattingToolbar,
    RichEditor,
    CustomToolsPopup,
    RngDropdown,
    SpeakingAsAvatar,
  ],
  template: `
    <div class="qt-chat-composer">
     <div class="qt-chat-composer-content">
      @if (attachedFiles().length > 0 || pendingToolResults().length > 0) {
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

          <!-- Pending tool results — a rolled-but-unsent RNG chip, tooltipped
               with v4's prompt + formatted result (v4 ChatComposer :284-302). -->
          @for (result of pendingToolResults(); track result.id) {
            <div
              class="qt-chat-tool-result-chip group relative"
              [title]="result.requestPrompt + '\n\n' + result.formattedResult"
            >
              <span class="text-base leading-none">{{ result.icon }}</span>
              <span class="text-foreground max-w-[200px] truncate">{{ result.summary }}</span>
              <button
                type="button"
                class="qt-chat-attachment-chip-remove"
                title="Remove tool result"
                aria-label="Remove tool result"
                (click)="removePendingToolResult.emit(result.id)"
              >
                <qt-icon name="close" class="w-4 h-4" />
              </button>
            </div>
          }
        </div>
      }


      <!-- The formatting toolbar — its own row above the form, shown only in
           composition mode (v4 ChatComposer :320-345, gated on its
           documentEditingMode prop). The Salon hands down the active roleplay
           template's delimiters; a chat with no template shows the markdown
           buttons and the pickers alone. -->
      @if (compositionMode()) {
        <qt-formatting-toolbar
          [disabled]="disabled() || !hasActiveCharacters()"
          [inCodeBlock]="inCodeBlock()"
          [showSource]="showSource()"
          [showSourceToggle]="true"
          [delimiters]="templateDelimiters()"
          [narrationDelimiters]="narrationDelimiters()"
          (action)="onFormatAction($event)"
          (applyDelimiter)="onApplyDelimiter($event)"
          (insertChar)="onInsertChar($event)"
          (toggleSource)="onToggleSource()"
        />
      }

      <form class="qt-chat-composer-inner" (submit)="onSubmit($event)">
        @if (showSource()) {
          <textarea
            #sourceArea
            class="qt-chat-composer-input qt-source-mode-textarea"
            [value]="sourceText()"
            [disabled]="disabled() || !hasActiveCharacters()"
            aria-label="Message (markdown source)"
            spellcheck="false"
            style="line-height: 1.5"
            (input)="onSourceInput($any($event.target).value)"
          ></textarea>
        }
        <qt-rich-editor
          #editor
          class="qt-chat-composer-input qt-lexical-contenteditable"
          [style.display]="showSource() ? 'none' : null"
          [value]="restoredDraft()"
          [placeholder]="placeholder()"
          [disabled]="disabled()"
          [submitOnEnter]="!compositionMode()"
          [submitOnModEnter]="compositionMode()"
          [textReplacementRules]="effectiveTextReplacementRules()"
          [smartTypography]="smartTypography()"
          [spellcheck]="composerSpellcheck()"
          [composerEmoji]="charInsert.emoji()"
          [composerUnicode]="charInsert.unicode()"
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

        <!-- "Speaking as" portrait — full-height, directly left of the toolbar
             cluster; hidden on the narrowest viewports where there's no room.
             The outer wrapper owns the responsive show/hide so it doesn't
             collide with the avatar's own layout (v4 ChatComposer :351-366). -->
        @if (speakingAs(); as seat) {
          <div class="hidden sm:flex self-stretch flex-shrink-0">
            <qt-speaking-as-avatar
              [name]="seat.name"
              [avatarUrl]="seat.avatarUrl"
              [canType]="canType()"
            />
          </div>
        }

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

          <!-- v4 gutter row 1, col 1: Insert Announcement (ComposerGutterTools
               :66-75), down to its title, aria-label and megaphone glyph. -->
          <button
            type="button"
            class="qt-chat-toolbar-button"
            title="Insert announcement"
            aria-label="Insert announcement"
            [disabled]="disabled()"
            (click)="openAnnouncement.emit()"
          >
            <qt-icon name="megaphone" class="w-5 h-5" />
          </button>

          <!-- v4 gutter row 1, col 2: Compose Mail — pairs with the megaphone as
               another "insert a special message" action (The Post Office). -->
          <button
            type="button"
            class="qt-chat-toolbar-button"
            title="Post a letter"
            aria-label="Post a letter"
            [disabled]="disabled()"
            (click)="openMail.emit()"
          >
            <qt-icon name="mail" class="w-5 h-5" />
          </button>

          <!-- v4 gutter row 2, col 1: Library file (ComposerGutterTools :90-100),
               down to its title, aria-label and file-plus glyph. -->
          <button
            type="button"
            class="qt-chat-toolbar-button"
            title="Attach file from library"
            aria-label="Attach file from library"
            [disabled]="disabled()"
            (click)="openLibrary.emit()"
          >
            <qt-icon name="file-plus" class="w-5 h-5" />
          </button>

          <!-- v4 gutter row 2, col 2: Generate Image (:102-112). It is v4's ONLY
               opener for the standalone generate dialog. -->
          <button
            type="button"
            class="qt-chat-toolbar-button"
            title="Generate image"
            aria-label="Generate image"
            (click)="openGenerate.emit()"
          >
            <qt-icon name="camera" class="w-5 h-5" />
          </button>

          <!-- v4 gutter row 3, col 1: Attach File (:114-130). -->
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

          <!-- v4 gutter row 3, col 2: RNG (ComposerGutterTools :132-139). Its
               rolls land as pending chips above the box, not as messages. -->
          <qt-rng-dropdown
            [chatId]="chatId()"
            [disabled]="disabled()"
            (pendingResult)="pendingToolResult.emit($event)"
          />

          <!-- v4 gutter row 4, col 1: Pascal's custom tools — opens a dialog
               (bespoke here: the button gates its own visibility on the roster
               rather than on a chat-payload flag). -->
          <qt-custom-tools-popup
            [chatId]="chatId()"
            [disabled]="disabled()"
            (ran)="customToolRan.emit()"
          />

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
  /**
   * Type-time dash/ellipsis substitution (v4 `SmartTypographyPlugin`). Read
   * from the shared settings singleton rather than passed down from the Salon,
   * because v4's plugin runs its OWN settings query for the same reason: the
   * composer is where the feature lives, and the rules must follow the toggle
   * without a rebuild. The two rules are gated independently.
   */
  protected readonly smartTypography = inject(SmartTypographySettings).typing;
  private readonly toasts = inject(ToastService);
  /**
   * The `:` / `\` typeahead toggles. Read HERE rather than taken as inputs,
   * because v4's plugin reads `chat_settings` itself and the feature's other
   * host (the Document-Mode pane) has no settings-passing parent at all. The
   * shared query key means this costs no extra GET.
   */
  protected readonly charInsert = injectCharInsertSettings();

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
  /**
   * Browser spellcheck in the composer (v4 `chat_settings.composerSpellcheck`,
   * applied at `LexicalComposerWrapper.tsx:107`). v4's default when unset is
   * true; the salon passes the setting.
   */
  readonly composerSpellcheck = input(true);
  /**
   * Rolled-but-unsent tool results, shown as chips above the box (v4
   * `SalonView.pendingToolResults` → `ChatComposer`). The Salon owns the list —
   * v4 does too — because the results outlive the composer's own reset and ride
   * the next send as `pendingToolResults`.
   */
  readonly pendingToolResults = input<PendingToolResultChip[]>([]);
  /**
   * The character the human is currently speaking as — rendered as a persistent
   * full-height portrait directly left of the toolbar cluster (v4 `speakingAs`).
   * Null when the human plays no character (e.g. an all-LLM room); the salon
   * computes it via `findActiveUserParticipant` so it matches server attribution.
   */
  readonly speakingAs = input<SpeakingAsSeat | null>(null);
  /**
   * The active roleplay template's delimiter entries and narration characters,
   * for the formatting toolbar's delimiter section (v4 passes
   * `roleplayTemplateId` + `narrationDelimiters` and the toolbar fetches the
   * rest; the Salon already holds the fetched row here — see the toolbar's
   * class doc). Empty while the template fetch is in flight, which is v4's
   * `loadingTemplate` gate.
   */
  readonly templateDelimiters = input<readonly TemplateDelimiter[]>([]);
  readonly narrationDelimiters = input<NarrationDelimiters | null>(null);

  readonly send = output<ComposerSend>();
  readonly stop = output<void>();
  readonly continue = output<void>();
  readonly openTerminal = output<void>();
  readonly openDocument = output<void>();
  readonly openGenerate = output<void>();
  /** The file-plus glyph — the Salon answers with the Library file picker. */
  readonly openLibrary = output<void>();
  /** The megaphone — the Salon answers with the Insert Announcement dialog. */
  readonly openAnnouncement = output<void>();
  /** The envelope — the Salon answers with the Compose Mail dialog. */
  readonly openMail = output<void>();
  /** A manual custom-tool run landed — the salon refetches the chat (v4 `onRan`). */
  readonly customToolRan = output<void>();
  /** The user flipped the mode via the toolbar toggle; the salon persists it. */
  readonly compositionModeChange = output<boolean>();
  /** The gutter's RNG rolled something (preview mode) — the Salon holds it. */
  readonly pendingToolResult = output<RngPendingResult>();
  /** The chip's ✕ (v4 `onRemovePendingToolResult`). */
  readonly removePendingToolResult = output<string>();

  /**
   * Optional rather than required because the toolbar binds `inCodeBlock` in
   * the TEMPLATE, and a required query read during the first change-detection
   * pass throws before the view exists.
   */
  private readonly editorView = viewChild(RichEditor);
  private readonly sourceArea = viewChild<ElementRef<HTMLTextAreaElement>>('sourceArea');

  /** Flips the toolbar's code-block button (v4 tracks Lexical's selection). */
  protected readonly inCodeBlock = computed(() => this.editorView()?.inCodeBlock() ?? false);

  /**
   * Raw-markdown source view (v4 `showSource`, held in `SalonView`'s modal
   * state as `showPreview`). v5 keeps it HERE: the only reader in v4 is
   * `ChatComposer` itself, so lifting it to the Salon would be state with no
   * second consumer.
   */
  protected readonly showSource = signal(false);
  /** The bytes the source textarea shows — seeded from the editor on entry. */
  protected readonly sourceText = signal('');

  /** The saved draft restored once on mount — seeds the editor's initial value. */
  protected readonly restoredDraft = signal('');
  /** v4's 800ms draft debounce (`ComposerSyncPlugin.tsx:65`). */
  private static readonly DRAFT_DEBOUNCE_MS = 800;
  private draftTimer: ReturnType<typeof setTimeout> | null = null;

  /** The live markdown from the editor — drives `hasContent` gating only. */
  protected readonly text = signal('');
  protected readonly attachedFiles = signal<UploadedChatFile[]>([]);
  protected readonly uploading = signal(false);

  // Conflict-resolution state (v4 useFileAttachments).
  protected readonly conflict = signal<FileConflictInfo | null>(null);
  protected readonly resolving = signal(false);
  private pendingFile: File | null = null;

  protected readonly canSend = computed(
    () =>
      !this.disabled() &&
      !this.busy() &&
      // v4 `:456`: a pending tool result is enough to send on its own — the
      // roll IS the message, and the text is optional commentary.
      (this.text().trim().length > 0 ||
        this.attachedFiles().length > 0 ||
        this.pendingToolResults().length > 0) &&
      this.hasActiveCharacters(),
  );

  /**
   * The floor is the human's — bright the "speaking as" cue; otherwise it dims
   * (v4 `hasActiveCharacters && !sending && !streaming && !waitingForResponse`;
   * v5's `busy` already folds streaming + waiting).
   */
  protected readonly canType = computed(
    () => this.hasActiveCharacters() && !this.busy() && !this.disabled(),
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

  // --- the formatting toolbar (v4 FormattingToolbar, composition mode) -------

  /**
   * Enter/leave raw-source mode (v4 `ChatComposer.tsx:330-341`): entering seeds
   * the textarea from the editor's OWN serialization — the page's `input` lags
   * while typing, which is the whole point of the decoupled sync — and leaving
   * hands the edited bytes back to the editor.
   *
   * v4 keeps Lexical mounted-but-hidden across the swap "to preserve undo
   * history"; v5 does the same with `display: none` rather than unmounting, so
   * the ProseMirror history plugin survives a round trip too. (The shared
   * markdown-field DOES unmount, because a form field has no such note and its
   * source textarea is the taller of the two views.)
   */
  protected onToggleSource(): void {
    if (this.showSource()) {
      this.editorView()?.setMarkdown(this.sourceText());
      this.showSource.set(false);
      return;
    }
    this.sourceText.set(this.editorView()?.getMarkdown() ?? '');
    this.showSource.set(true);
  }

  /** A textarea keystroke: keep the send gate and the draft in step. */
  protected onSourceInput(value: string): void {
    this.sourceText.set(value);
    this.onContentChange(value);
  }

  /** A markdown/indent/code-block button (v4 `handleMarkdownClick` & friends). */
  protected onFormatAction(action: FormatAction): void {
    if (this.showSource()) {
      this.applyToSource((state) => applySourceFormat(state, action));
      return;
    }
    this.editorView()?.runCommand(formatCommand(action, this.inCodeBlock()));
  }

  /** A roleplay-delimiter button (v4 `handleDelimiterClick`, both branches). */
  protected onApplyDelimiter(delimiter: TemplateDelimiter): void {
    if (this.showSource()) {
      this.applyToSource((state) => applySourceDelimiter(state, delimiter));
      return;
    }
    this.editorView()?.runCommand(applyDelimiterCommand(delimiter));
  }

  /**
   * A character picked from a toolbar picker. In source mode it lands at the
   * textarea's caret rather than in the hidden editor — the markdown-field's
   * ruled divergence (P4.D75), for the same reason: v4 hands the picker its
   * Lexical instance unconditionally, so the pick is silently lost.
   */
  protected onInsertChar({ profile, char }: CharInsert): void {
    if (this.showSource()) {
      this.applyToSource((state) => ({
        value: state.value.slice(0, state.start) + char + state.value.slice(state.end),
        cursor: state.start + char.length,
      }));
      recordRecent(profile, char);
      return;
    }
    this.editorView()?.insertChar(profile, char);
  }

  /**
   * Run a source-mode transform over the textarea and restore the caret after
   * the re-render — v4 does the same inside a `requestAnimationFrame`
   * (`sourceApply`, `FormattingToolbar.tsx:123-134`).
   */
  private applyToSource(
    transform: (state: { value: string; start: number; end: number }) => {
      value: string;
      cursor: number;
    },
  ): void {
    const textarea = this.sourceArea()?.nativeElement;
    if (!textarea) return;

    const result = transform({
      value: textarea.value,
      start: textarea.selectionStart,
      end: textarea.selectionEnd,
    });
    this.sourceText.set(result.value);
    this.onContentChange(result.value);
    requestAnimationFrame(() => {
      textarea.selectionStart = textarea.selectionEnd = result.cursor;
      textarea.focus();
    });
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
    //
    // DELIBERATE DIVERGENCE in source mode: v4 reads the LEXICAL handle here
    // too (`SalonView.tsx:1581`) while its bridge is `suspendSync`ed, so a send
    // made with the raw-source textarea open silently discards every source
    // edit — and since `hasContent` also comes from the editor, the Send button
    // does not even light for text typed there. v5 sends the bytes the writer
    // can see. Same shape as the markdown-field picker divergence (P4.D75): a
    // control that visibly does nothing is not worth reproducing. Queued as a
    // v4-side finding.
    const content = this.showSource() ? this.sourceText() : (this.editorView()?.getMarkdown() ?? '');
    this.send.emit({
      content: content.trim(),
      fileIds: this.attachedFiles().map((f) => f.id),
    });
    this.editorView()?.setMarkdown('');
    this.sourceText.set('');
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

  /**
   * Put an already-linked file in the tray, so the next send carries it (v4's
   * `onFileLinked` → `setAttachedFiles`, `ChatModals.tsx:250-261`).
   *
   * v4 keeps `attachedFiles` in SalonView and passes it down; v5 keeps the tray
   * here, beside the upload and conflict machinery that fills it. That leaves the
   * Library file picker — the one other producer — needing a way in, and this is
   * it: the Salon calls it on the picker's `fileLinked`. Ignores a file already
   * in the tray, so a double-link can't produce a double chip (and a duplicate
   * `fileIds` entry on the send).
   */
  addAttachedFile(file: UploadedChatFile): void {
    this.attachedFiles.update((files) =>
      files.some((f) => f.id === file.id) ? files : [...files, file],
    );
  }

  private async upload(
    file: File,
    opts?: { resolution?: ConflictResolution; conflictingFileId?: string },
  ): Promise<void> {
    this.uploading.set(true);
    try {
      const result = await uploadChatFile(this.chatId(), file, opts);
      this.handleUploadResult(result, file);
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to upload file');
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
    // v4 `handleFileSelect` (:126-129) reports only the non-conflict arm.
    this.toasts.showSuccess('File attached');
    this.attachedFiles.update((files) => [...files, result.file]);
  }

  protected async onResolveConflict(resolution: ConflictResolution): Promise<void> {
    const file = this.pendingFile;
    const info = this.conflict();
    if (!file || !info) return;
    this.resolving.set(true);
    try {
      const result = await uploadChatFile(this.chatId(), file, {
        resolution,
        conflictingFileId: info.existingFile.id,
      });
      // 'skip' returns a fresh file or nothing; only push a real file result.
      if (result.kind === 'file') {
        this.attachedFiles.update((files) => [...files, result.file]);
      }
      // v4 `handleConflictResolution` (:160-166) — one sentence per resolution.
      this.toasts.showSuccess(
        resolution === 'skip'
          ? 'Upload skipped'
          : resolution === 'keepBoth'
            ? 'File attached with new name'
            : 'File replaced',
      );
      this.conflict.set(null);
      this.pendingFile = null;
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to resolve conflict');
    } finally {
      this.resolving.set(false);
    }
  }

  protected cancelConflict(): void {
    this.conflict.set(null);
    this.pendingFile = null;
  }
}
