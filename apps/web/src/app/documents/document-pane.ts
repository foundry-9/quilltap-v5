import {
  ChangeDetectionStrategy,
  Component,
  inject,
  computed,
  effect,
  ElementRef,
  input,
  output,
  signal,
  viewChild,
} from '@angular/core';

import { injectCharInsertSettings } from '../editor/char-insert/char-insert-settings';
import { recordRecent } from '../editor/char-insert/recents-storage';
import { applyDelimiterCommand, applySourceDelimiter } from '../editor/delimiter-transforms';
import { formatCommand } from '../editor/format-commands';
import {
  FormattingToolbar,
  type CharInsert,
  type FormatAction,
} from '../editor/formatting-toolbar';
import { RichEditor } from '../editor/rich-editor';
import { applySourceFormat, type TextResult, type TextState } from '../editor/source-transforms';
import { SmartTypographySettings } from '../smart-typography/settings';
import type { TemplateDelimiter } from '../core/core-contract';
import { Icon } from '../ui/icon';
import { USES_RICH_MARKDOWN_EDITOR, type DocumentMode, type OpenDocEntry } from './document-mode';
import {
  countWords,
  extractFrontmatter,
  frontmatterRows,
  isMarkdownFile,
  type FrontmatterRow,
} from './frontmatter';
import { documentPaneUri } from './qtap-uri';
import { ToastService } from '../ui/toast.service';

/**
 * `qt-document-pane` — the right-side document editor (v4 `DocumentPane`): a
 * header (click-to-rename title, source toggle, focus/split toggle, delete,
 * close), the qtap:// URL row with copy, the frontmatter "Document Info" table
 * (markdown only), the editor, and a status bar.
 *
 * The D17 gate is GREEN (see the status log / `phase-4.md`), so markdown files
 * render in the `qt-rich-editor` (ProseMirror over the v4 composer dialect —
 * byte-faithful, proven by `markdown-round-trip.spec.ts`). For markdown the
 * frontmatter block is split off into the read-only table (v4 behaviour) and
 * only the BODY is edited; edits are recombined `rawBlock + body` on change so
 * the on-disk bytes stay faithful. Everything else — non-markdown files, and
 * the raw-source toggle (v4 `showSource`) — renders in a plain `<textarea>`
 * (byte-exact, no serializer). When `USES_RICH_MARKDOWN_EDITOR` is off markdown
 * falls back to the same textarea path.
 *
 * A stretch host class keeps the pane bounded inside the split's flex height
 * cascade (the standing P4.6u #4 lesson).
 *
 * The FORMATTING TOOLBAR (P4.9L2, closing the gap P4.9L named) sits in a
 * `.qt-doc-toolbar` row above the editor, exactly where v4 mounts its own
 * `FormattingToolbar` (`DocumentPane.tsx:44, :323-355, :686-693`): the markdown
 * buttons, the indent controls, the code-block toggle, both character pickers,
 * the source toggle, and the active roleplay template's delimiter buttons. v4's
 * `DocToolbar` passes NO `narrationDelimiters`, so — unlike the composer — this
 * toolbar shows no synthesized "Nar" button; the binding is deliberately absent
 * here for that reason. Every branch it drives is shared with the composer
 * (`formatCommand`, `applyDelimiterCommand`, `applySourceFormat`,
 * `applySourceDelimiter`), so the two hosts of one toolbar cannot disagree.
 *
 * Two shapes are worth naming:
 *
 * - **The template arrives RESOLVED, not as an id.** v4 hands the toolbar a
 *   `roleplayTemplateId` and the toolbar fetches the row itself; v5 takes the
 *   resolved `delimiters` because the Salon already fetches that same row for
 *   its rendering patterns (the P4.9L divergence, recorded on the toolbar). The
 *   standalone view passes nothing — v4's `StandaloneDocumentView.tsx:381`
 *   passes no `roleplayTemplateId` either, so neither shows delimiter buttons.
 * - **DIVERGENCE (pre-existing, named here because the toolbar makes it
 *   reachable): source mode shows the BODY, not the whole file.** v4's
 *   raw-source branch renders `document.content` — frontmatter block included —
 *   and hides the frontmatter table while it is showing
 *   (`DocumentPane.tsx:695-720`). v5 has split the frontmatter off since P4.6ag,
 *   so the table stays up and the textarea holds the body alone. Nothing about
 *   the saved bytes moves (the recombine seam puts `rawBlock` back), but a
 *   toolbar transform at offset 0 now lands at the body's start where v4's
 *   would land inside the `---` fence. Changing the shape is its own slice with
 *   its own beats (word count, the table's visibility, the recombine seam), not
 *   a rider on the toolbar mount.
 * - **DIVERGENCE (v5-invented, kept): the header source button.** v4's document
 *   header carries three buttons (focus/split, delete, close) and its ONLY
 *   source toggle is the one inside the toolbar. v5 grew a header toggle in
 *   P4.6ag, when this pane had no toolbar to hold one. Both now drive the SAME
 *   `showSource` signal through the same `toggleSourceMode()` — one signal, two
 *   controls, never out of step — but the header control has no v4 counterpart
 *   and is a candidate for retirement.
 */
@Component({
  selector: 'qt-document-pane',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'flex flex-col flex-1 min-h-0 min-w-0' },
  imports: [FormattingToolbar, Icon, RichEditor],
  template: `
    <div class="flex flex-col h-full">
      <div class="qt-doc-header">
        <span class="qt-doc-badge">Document</span>

        @if (isEditingTitle()) {
          <input
            #titleInput
            class="qt-doc-title-input"
            [value]="editTitle()"
            (input)="editTitle.set($any($event.target).value)"
            (blur)="submitTitle()"
            (keydown)="onTitleKeyDown($event)"
          />
        } @else {
          <span class="qt-doc-title cursor-pointer" title="Click to rename" (click)="startTitleEdit()">
            {{ document().displayTitle }}
          </span>
        }

        <div class="flex items-center gap-1">
          @if (usesRichEditor()) {
            <button
              type="button"
              class="qt-doc-header-button"
              [class.qt-doc-header-button-active]="showSource()"
              [title]="showSource() ? 'Show the formatted editor' : 'Show the Markdown source'"
              [attr.aria-label]="showSource() ? 'Show formatted editor' : 'Show markdown source'"
              [attr.aria-pressed]="showSource()"
              (click)="toggleSourceMode()"
            >
              <qt-icon name="code" class="w-4 h-4" />
            </button>
          }

          <button
            type="button"
            class="qt-doc-header-button"
            [title]="mode() === 'focus' ? 'Show chat' : 'Maximize'"
            [attr.aria-label]="mode() === 'focus' ? 'Show chat' : 'Maximize document'"
            (click)="toggleFocus.emit()"
          >
            <qt-icon [name]="mode() === 'focus' ? 'compress' : 'expand'" class="w-4 h-4" />
          </button>

          <button
            type="button"
            class="qt-doc-header-button"
            title="Delete document"
            aria-label="Delete document"
            (click)="onDeleteClick()"
          >
            <qt-icon name="trash" class="w-4 h-4" />
          </button>

          <button
            type="button"
            class="qt-doc-header-button"
            title="Exit document mode"
            aria-label="Exit document mode"
            (click)="close.emit()"
          >
            <qt-icon name="close" class="w-4 h-4" />
          </button>
        </div>
      </div>

      <div class="qt-doc-uri-row">
        <span class="qt-doc-uri-label">URL</span>
        <span class="qt-doc-uri-value" [title]="documentUri()">{{ documentUri() }}</span>
        <button
          type="button"
          class="qt-doc-uri-copy-button"
          [title]="uriCopied() ? 'Copied' : 'Copy URL'"
          [attr.aria-label]="uriCopied() ? 'Copied URL' : 'Copy URL'"
          (click)="copyUri()"
        >
          <qt-icon name="copy" class="w-3.5 h-3.5" />
        </button>
      </div>

      <!-- v4 \`DocumentToolbarWrapper\` (\`DocumentPane.tsx:323-355\`): the same
           toolbar the composer wears, in its own row above the editor, and
           rendered for the very files v4 wraps in a Lexical composer (markdown).
           NO \`narrationDelimiters\` binding — v4's \`DocToolbar\` passes none, so
           this toolbar shows no synthesized "Nar" button. -->
      @if (usesRichEditor()) {
        <div class="qt-doc-toolbar">
          <qt-formatting-toolbar
            [disabled]="entry().isLLMEditing"
            [inCodeBlock]="inCodeBlock()"
            [showSource]="showSource()"
            [showSourceToggle]="true"
            [delimiters]="delimiters()"
            (action)="onFormatAction($event)"
            (applyDelimiter)="onApplyDelimiter($event)"
            (insertChar)="onInsertChar($event)"
            (toggleSource)="toggleSourceMode()"
          />
        </div>
      }

      @if (hasFrontmatter()) {
        <div class="qt-frontmatter">
          <div class="qt-frontmatter-title">Document Info</div>
          <table class="qt-frontmatter-table">
            <tbody>
              @for (row of frontmatter(); track row.key) {
                <tr>
                  <th>{{ row.key }}</th>
                  <td>
                    @if (row.chips; as chips) {
                      <div class="qt-frontmatter-chip-list">
                        @for (chip of chips; track $index) {
                          <span class="qt-badge qt-badge-tag">{{ chip }}</span>
                        }
                      </div>
                    } @else {
                      <span>{{ row.text }}</span>
                    }
                  </td>
                </tr>
              }
            </tbody>
          </table>
        </div>
      }

      <!-- focusout, not blur: blur does not bubble, so a container listener
           never fires when the textarea (or the ProseMirror contenteditable)
           loses focus. v4's React onBlur works because React delegates blur as
           focusout — focusout is the faithful Angular equivalent (the
           flush-on-blur save rides this). -->
      <div class="flex-1 overflow-y-auto flex flex-col min-h-0" (focusout)="blur.emit()">
        @if (useRichEditorNow()) {
          <qt-rich-editor
            class="qt-doc-editor-area h-full p-4"
            [value]="editorValue()"
            [ariaLabel]="'Document editor'"
            [smartTypography]="smartTypography()"
            [disabled]="entry().isLLMEditing"
            [composerEmoji]="charInsert.emoji()"
            [composerUnicode]="charInsert.unicode()"
            (contentChange)="onEditorInput($event)"
            style="line-height: 1.6; min-height: 100%; background-color: var(--color-background)"
          />
        } @else {
          <textarea
            #sourceArea
            class="qt-doc-editor-area w-full h-full p-4 qt-bg-input qt-text-primary font-mono text-sm resize-none outline-none"
            [attr.aria-label]="'Document editor'"
            [value]="editorValue()"
            (input)="onEditorInput($any($event.target).value)"
            [disabled]="entry().isLLMEditing"
            spellcheck="false"
            style="line-height: 1.6; min-height: 100%; background-color: var(--color-background)"
          ></textarea>
        }
      </div>

      <div class="qt-doc-status-bar">
        <span class="qt-doc-status-item">{{ isMarkdown() ? 'Markdown' : 'Plain text' }}</span>
        <span class="qt-doc-status-item"
          >{{ wordCount().toLocaleString() }} word{{ wordCount() === 1 ? '' : 's' }}</span
        >
        <span class="qt-doc-status-item">
          <span [class]="saveDotClass()"></span>
          {{ saveLabel() }}
        </span>
        @if (entry().isLLMEditing) {
          <span class="qt-doc-status-item ml-auto">AI editing...</span>
        }
      </div>
    </div>
  `,
})
export class DocumentPane {
  private readonly toasts = inject(ToastService);
  /**
   * Type-time dash/ellipsis substitution, the Document Mode half (v4 mounts
   * `SmartTypographyPlugin` in both the composer and this editor). The
   * raw-source `<textarea>` path below is deliberately untouched: v4 bails in
   * source-mode editors, and v5 gets that structurally because the textarea is
   * not the ProseMirror component at all.
   */
  protected readonly smartTypography = inject(SmartTypographySettings).typing;
  /**
   * The `:` / `\` typeahead toggles (v4 mounts `CharTypeaheadPlugin` in the
   * Document pane as well as the composer). Read here for the same reason v4's
   * plugin reads them itself — this pane's hosts pass no settings.
   */
  protected readonly charInsert = injectCharInsertSettings();
  readonly entry = input.required<OpenDocEntry>();
  readonly mode = input.required<DocumentMode>();
  /**
   * The active roleplay template's delimiter entries — v4's `roleplayTemplateId`
   * prop, already resolved (see the class doc). The Salon host passes what it
   * fetched; the standalone view passes nothing, exactly as v4 does.
   */
  readonly delimiters = input<readonly TemplateDelimiter[]>([]);

  readonly contentChange = output<string>();
  readonly blur = output<void>();
  readonly rename = output<string>();
  readonly close = output<void>();
  readonly delete = output<void>();
  readonly toggleFocus = output<void>();

  private readonly titleInput = viewChild<ElementRef<HTMLInputElement>>('titleInput');
  /**
   * Optional rather than required: the toolbar binds `inCodeBlock` in the
   * TEMPLATE, and a required query read during the first change-detection pass
   * throws before the view exists (the composer's lesson).
   */
  private readonly editorView = viewChild(RichEditor);
  private readonly sourceArea = viewChild<ElementRef<HTMLTextAreaElement>>('sourceArea');

  /** Flips the toolbar's code-block button (v4 tracks Lexical's selection). */
  protected readonly inCodeBlock = computed(() => this.editorView()?.inCodeBlock() ?? false);

  protected readonly document = computed(() => this.entry().document);
  protected readonly isMarkdown = computed(() => isMarkdownFile(this.document().filePath));

  /** Raw-source view of a markdown file (v4 `showSource`) — a plain textarea. */
  protected readonly showSource = signal(false);
  /** Markdown files use the rich editor when the D17 flag is on (else textarea). */
  protected readonly usesRichEditor = computed(
    () => USES_RICH_MARKDOWN_EDITOR && this.isMarkdown(),
  );
  /** The rich editor is shown when eligible AND not toggled to raw source. */
  protected readonly useRichEditorNow = computed(() => this.usesRichEditor() && !this.showSource());

  /** Frontmatter split (markdown only): the table shows it, the editor edits the body. */
  private readonly extracted = computed(() => {
    const content = this.document().content;
    if (!this.isMarkdown()) return { rawBlock: null as string | null, body: content, data: {} };
    return extractFrontmatter(content);
  });

  protected readonly hasFrontmatter = computed(() => this.extracted().rawBlock !== null);
  protected readonly frontmatter = computed<FrontmatterRow[]>(() =>
    frontmatterRows(this.extracted().data),
  );

  /** The textarea's value: the body for markdown (frontmatter is in the table), else the whole file. */
  protected readonly editorValue = computed(() =>
    this.isMarkdown() ? this.extracted().body : this.document().content,
  );

  protected readonly wordCount = computed(() =>
    countWords(this.isMarkdown() ? this.extracted().body : this.document().content),
  );

  protected readonly documentUri = computed(() => documentPaneUri(this.document()));

  protected readonly saveLabel = computed(() => {
    if (this.entry().isSaving) return 'Saving...';
    if (this.entry().isDirty) return 'Unsaved';
    return 'Saved';
  });
  protected readonly saveDotClass = computed(() =>
    this.entry().isSaving || this.entry().isDirty ? 'qt-doc-status-saving' : 'qt-doc-status-saved',
  );

  // --- title editing ---
  protected readonly isEditingTitle = signal(false);
  protected readonly editTitle = signal('');
  protected readonly uriCopied = signal(false);
  private copyResetTimer: ReturnType<typeof setTimeout> | null = null;

  constructor() {
    // Focus the title input when editing opens (v4's requestAnimationFrame focus).
    effect(() => {
      if (this.isEditingTitle()) {
        requestAnimationFrame(() => this.titleInput()?.nativeElement.focus());
      }
    });

    // Reset the raw-source toggle when the pane switches to a different document.
    let lastDocId: string | null = null;
    effect(() => {
      const id = this.document().id;
      if (id !== lastDocId) {
        lastDocId = id;
        this.showSource.set(false);
      }
    });
  }

  /** Recombine `rawBlock + body` for markdown so the on-disk bytes stay faithful. */
  protected onEditorInput(value: string): void {
    if (this.isMarkdown()) {
      const raw = this.extracted().rawBlock;
      this.contentChange.emit(raw ? `${raw}${value}` : value);
      return;
    }
    this.contentChange.emit(value);
  }

  // --- the formatting toolbar (v4 `DocumentToolbarWrapper`) -----------------

  /**
   * Enter/leave raw-source mode — the ONE handler behind both controls (the
   * header button and the toolbar's toggle), so the shared `showSource` signal
   * can never be driven two different ways.
   *
   * Leaving source mode flushes first, v4's `toggleSourceMode`
   * (`DocumentPane.tsx:487-494`): "flush any pending edits before Lexical
   * re-parses the markdown, so raw source bytes are persisted before
   * serialization." v5 flushes by emitting `blur` — v4's `onFlushSave` IS its
   * `onBlur` at both call sites (`DocumentPaneBinding.tsx:54-55`,
   * `StandaloneDocumentView.tsx:392-393`), so a second output would be the same
   * callback under a second name.
   */
  protected toggleSourceMode(): void {
    if (this.showSource()) this.blur.emit();
    this.showSource.set(!this.showSource());
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
   * textarea's caret rather than in the unmounted editor — the P4.D75 ruled
   * divergence the composer and the markdown-field already carry, for the same
   * reason: v4 hands the picker its Lexical instance unconditionally, so the
   * pick is silently lost.
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
   * Run a source-mode transform over THIS pane's textarea and restore the caret
   * after the re-render — v4 does the same inside a `requestAnimationFrame`
   * (`sourceApply`, `FormattingToolbar.tsx:123-134`).
   *
   * Unlike the composer and the markdown-field, this pane keeps no local
   * `sourceText` signal: the document's content IS the source of truth and the
   * textarea is bound straight to it. So the new bytes are written onto the
   * element the way a keystroke would write them, and then emitted through
   * `onEditorInput` — the very seam the textarea's own `(input)` uses, which is
   * what recombines the frontmatter block for a markdown file. The host's echo
   * then re-affirms the same string (v4 relies on exactly that echo, its
   * `setInput` being the pane's `onContentChange`).
   */
  private applyToSource(transform: (state: TextState) => TextResult): void {
    const textarea = this.sourceArea()?.nativeElement;
    if (!textarea) return;

    const result = transform({
      value: textarea.value,
      start: textarea.selectionStart,
      end: textarea.selectionEnd,
    });
    textarea.value = result.value;
    this.onEditorInput(result.value);
    requestAnimationFrame(() => {
      textarea.selectionStart = textarea.selectionEnd = result.cursor;
      textarea.focus();
    });
  }

  protected startTitleEdit(): void {
    this.editTitle.set(this.document().displayTitle);
    this.isEditingTitle.set(true);
  }

  protected submitTitle(): void {
    this.isEditingTitle.set(false);
    const trimmed = this.editTitle().trim();
    if (trimmed && trimmed !== this.document().displayTitle) {
      this.rename.emit(trimmed);
    }
  }

  protected onTitleKeyDown(event: KeyboardEvent): void {
    if (event.key === 'Enter') {
      this.submitTitle();
    } else if (event.key === 'Escape') {
      this.isEditingTitle.set(false);
      this.editTitle.set(this.document().displayTitle);
    }
  }

  protected onDeleteClick(): void {
    const msg = `Delete "${this.document().displayTitle}"? This removes the underlying file and cannot be undone.`;
    if (typeof window !== 'undefined' && !window.confirm(msg)) return;
    this.delete.emit();
  }

  protected async copyUri(): Promise<void> {
    try {
      await navigator.clipboard?.writeText(this.documentUri());
      // v4 `DocumentPane.tsx:553` keeps BOTH the 1.2s tick and the toast.
      this.toasts.showSuccess('Document URL copied');
      this.uriCopied.set(true);
      if (this.copyResetTimer) clearTimeout(this.copyResetTimer);
      this.copyResetTimer = setTimeout(() => this.uriCopied.set(false), 1200);
    } catch {
      this.uriCopied.set(false);
    }
  }
}
