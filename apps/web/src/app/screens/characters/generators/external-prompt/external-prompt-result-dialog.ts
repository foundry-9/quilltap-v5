import { ChangeDetectionStrategy, Component, inject, input, output, signal } from '@angular/core';

import { MessageContent } from '../../../../chat/message-content';
import { Icon } from '../../../../ui/icon';
import { ToastService } from '../../../../ui/toast.service';

/**
 * `ExternalPromptResultDialog` — v4 `app/aurora/[id]/view/components/
 * ExternalPromptResultDialog.tsx` (95 lines): the generated prompt, rendered
 * as markdown, with copy-to-clipboard and download-as-`.md` actions.
 *
 * v4 renders via `<ReactMarkdown remarkPlugins={[remarkGfm]}>`; v5 reuses the
 * existing v4-parity markdown pipeline (`qt-message-content`) rather than
 * pulling in a second renderer for a read-only text view.
 *
 * v4's copy button rides `useCopyToClipboard` (a bare `navigator.clipboard.
 * writeText`); v5's `core/clipboard-utils.ts` (the P4.D114 transcription the
 * work order cites) carries only `copyImageToClipboard` — no text-copy twin
 * exists yet — so this dialog calls `navigator.clipboard.writeText` directly,
 * the same primitive v4's hook itself wraps.
 */
@Component({
  selector: 'qt-external-prompt-result-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, MessageContent],
  host: {
    '(document:keydown.escape)': 'onClose()',
  },
  template: `
    <div class="bg-background/80 fixed inset-0 z-50 flex items-center justify-center p-4 backdrop-blur-sm">
      <div
        class="qt-border-default qt-bg-card flex max-h-[90vh] w-full max-w-md flex-col rounded-2xl border p-6 shadow-2xl md:max-w-3xl"
      >
        <div class="mb-4 flex flex-shrink-0 items-center justify-between">
          <h3 class="qt-heading-4">
            Generated Prompt{{ characterName() ? ' for ' + characterName() : '' }}
          </h3>
          <div class="flex items-center gap-2">
            <button
              type="button"
              class="qt-border-default qt-bg-card qt-label qt-shadow-sm hover:qt-bg-muted inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-foreground"
              title="Copy to clipboard"
              (click)="handleCopy()"
            >
              @if (copied()) {
                <qt-icon name="check" class="qt-text-success w-4 h-4" />
                Copied
              } @else {
                <qt-icon name="copy" class="w-4 h-4" />
                Copy
              }
            </button>
            <button
              type="button"
              class="qt-border-default qt-bg-card qt-label qt-shadow-sm hover:qt-bg-muted inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-foreground"
              title="Download as Markdown file"
              (click)="handleDownload()"
            >
              <qt-icon name="download" class="w-4 h-4" />
              Download
            </button>
          </div>
        </div>

        <div class="qt-border-default bg-background/50 -mr-2 flex-1 overflow-y-auto rounded-lg border p-4 pr-2">
          <qt-message-content [content]="prompt()" />
        </div>

        <div class="mt-4 flex flex-shrink-0 justify-end">
          <button
            type="button"
            class="bg-primary hover:qt-bg-primary/90 rounded-lg px-4 py-2 text-sm font-semibold text-primary-foreground shadow"
            (click)="onClose()"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  `,
})
export class ExternalPromptResultDialog {
  private readonly toasts = inject(ToastService);

  readonly characterName = input<string | null>(null);
  readonly prompt = input.required<string>();
  readonly closed = output<void>();

  protected readonly copied = signal(false);
  private copiedTimeout: ReturnType<typeof setTimeout> | null = null;

  protected async handleCopy(): Promise<void> {
    try {
      await navigator.clipboard.writeText(this.prompt());
      this.copied.set(true);
      if (this.copiedTimeout) clearTimeout(this.copiedTimeout);
      this.copiedTimeout = setTimeout(() => this.copied.set(false), 2000);
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Could not copy to clipboard');
    }
  }

  /** v4 `:26-38` — a synthetic `<a download>` click, byte-identical mechanism. */
  protected handleDownload(): void {
    const safeName = (this.characterName() || 'character').replace(/[^a-zA-Z0-9-_ ]/g, '').trim();
    const filename = `${safeName}-external-prompt.md`;
    const blob = new Blob([this.prompt()], { type: 'text/markdown;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    try {
      const link = document.createElement('a');
      link.href = url;
      link.download = filename;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
    } finally {
      URL.revokeObjectURL(url);
    }
  }

  protected onClose(): void {
    this.closed.emit();
  }
}
