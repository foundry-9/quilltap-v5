import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  ElementRef,
  input,
  output,
  signal,
  viewChild,
} from '@angular/core';

import { Icon } from '../ui/icon';
import type { DocumentMode, OpenDocEntry } from './document-mode';
import {
  countWords,
  extractFrontmatter,
  frontmatterRows,
  isMarkdownFile,
  type FrontmatterRow,
} from './frontmatter';
import { documentPaneUri } from './qtap-uri';

/**
 * `qt-document-pane` — the right-side document editor (v4 `DocumentPane`): a
 * header (click-to-rename title, focus/split toggle, delete, close), the
 * qtap:// URL row with copy, the frontmatter "Document Info" table (markdown
 * only), the editor, and a status bar.
 *
 * The D17 spike is RED (see `phase-4.md`), so EVERY file renders in the plain
 * `<textarea>` — byte-exact, no Markdown serializer. For markdown files the
 * frontmatter block is split off into the read-only table (v4 behaviour) and
 * only the BODY is edited; edits are recombined `rawBlock + body` on change so
 * the on-disk bytes stay faithful. Non-markdown files edit the whole content.
 *
 * A stretch host class keeps the pane bounded inside the split's flex height
 * cascade (the standing P4.6u #4 lesson).
 */
@Component({
  selector: 'qt-document-pane',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'flex flex-col flex-1 min-h-0 min-w-0' },
  imports: [Icon],
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

      <div class="flex-1 overflow-y-auto" (blur)="blur.emit()">
        <textarea
          class="qt-doc-editor-area w-full h-full p-4 qt-bg-input qt-text-primary font-mono text-sm resize-none outline-none"
          [attr.aria-label]="'Document editor'"
          [value]="editorValue()"
          (input)="onEditorInput($any($event.target).value)"
          [disabled]="entry().isLLMEditing"
          spellcheck="false"
          style="line-height: 1.6; min-height: 100%; background-color: var(--color-background)"
        ></textarea>
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
  readonly entry = input.required<OpenDocEntry>();
  readonly mode = input.required<DocumentMode>();

  readonly contentChange = output<string>();
  readonly blur = output<void>();
  readonly rename = output<string>();
  readonly close = output<void>();
  readonly delete = output<void>();
  readonly toggleFocus = output<void>();

  private readonly titleInput = viewChild<ElementRef<HTMLInputElement>>('titleInput');

  protected readonly document = computed(() => this.entry().document);
  protected readonly isMarkdown = computed(() => isMarkdownFile(this.document().filePath));

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
      this.uriCopied.set(true);
      if (this.copyResetTimer) clearTimeout(this.copyResetTimer);
      this.copyResetTimer = setTimeout(() => this.uriCopied.set(false), 1200);
    } catch {
      this.uriCopied.set(false);
    }
  }
}
