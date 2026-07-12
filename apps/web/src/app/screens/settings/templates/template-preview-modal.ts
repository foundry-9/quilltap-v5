import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import type { RoleplayTemplateDto, TemplateDelimiter } from '../../../core/core-contract';
import { Modal } from '../../../ui/modal';

interface DelimiterPreview {
  buttonName: string;
  name: string;
  sample: string;
}

/**
 * The read-only template Preview modal (v4 `roleplay-templates/index.tsx`
 * preview): name, description, a Built-in badge, the Formatting Delimiters
 * summary (each rendered as a code sample by kind), and the LLM Prompt in a
 * `<pre>`. Built-ins get a "Copy as New" action. Copy is v4-verbatim.
 */
@Component({
  selector: 'qt-template-preview-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal [title]="template().name" maxWidth="2xl" (close)="close.emit()">
      <div class="space-y-4">
        @if (template().isBuiltIn) {
          <span class="qt-badge-chat px-2 py-0.5 rounded qt-text-label-xs">Built-in</span>
        }
        @if (template().description) {
          <p class="qt-text-small qt-text-muted">{{ template().description }}</p>
        }

        @if (delimiterPreviews().length > 0) {
          <div class="p-3 rounded-lg qt-border qt-bg-surface">
            <h4 class="qt-text-label mb-2">Formatting Delimiters</h4>
            <ul class="space-y-1">
              @for (d of delimiterPreviews(); track $index) {
                <li class="flex items-center gap-2 qt-text-xs">
                  <span class="qt-badge-chat px-1.5 py-0.5 rounded font-medium">{{
                    d.buttonName
                  }}</span>
                  <span>{{ d.name }}</span>
                  <code class="qt-text-muted">{{ d.sample }}</code>
                </li>
              }
            </ul>
          </div>
        }

        <div>
          <h4 class="qt-text-label mb-1">LLM Prompt</h4>
          <pre
            class="qt-input text-sm whitespace-pre-wrap break-words max-h-80 overflow-y-auto"
          >{{ template().systemPrompt }}</pre>
        </div>
      </div>

      <div qt-modal-footer class="flex gap-2 justify-end">
        @if (template().isBuiltIn) {
          <button type="button" class="qt-button-secondary" (click)="copyAsNew.emit()">
            Copy as New
          </button>
        }
        <button type="button" class="qt-button-primary" (click)="close.emit()">Close</button>
      </div>
    </qt-modal>
  `,
})
export class TemplatePreviewModal {
  readonly template = input.required<RoleplayTemplateDto>();
  readonly close = output<void>();
  readonly copyAsNew = output<void>();

  protected readonly delimiterPreviews = computed<DelimiterPreview[]>(() =>
    (this.template().delimiters ?? []).map((d) => ({
      buttonName: d.buttonName,
      name: d.name,
      sample: sampleForDelimiter(d),
    })),
  );
}

/** v4 preview: a code sample string per delimiter kind. */
function sampleForDelimiter(d: TemplateDelimiter): string {
  if (d.kind === 'linePrefix') {
    return `${d.marker}…EOL`;
  }
  if (d.kind === 'tagPrefix') {
    return `${d.open}TOKEN${d.close}`;
  }
  if (Array.isArray(d.delimiters)) {
    return `${d.delimiters[0]}...${d.delimiters[1] || 'EOL'}`;
  }
  return `${d.delimiters}...${d.delimiters}`;
}
