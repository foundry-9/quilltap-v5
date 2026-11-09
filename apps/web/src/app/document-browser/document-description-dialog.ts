import {
  ChangeDetectionStrategy,
  Component,
  input,
  linkedSignal,
  output,
  signal,
} from '@angular/core';

import { Modal } from '../ui/modal';

/** The file being described (v4 `DocumentDescriptionDialogProps.file`). */
export interface DescribedFile {
  relativePath: string;
  fileName: string;
  description: string;
}

/**
 * "Describe <file>" — v4 `DocumentDescriptionDialog` + `DescriptionForm`
 * (`components/document-browser/DocumentFileDialogs.tsx`, `c34986421`;
 * P4.D387 item 10): the description or transcript a blob is searched by.
 * Save stays disabled while nothing changed; a save that took closes it.
 *
 * The draft seeds from the `file` input through a `linkedSignal` (v4 remounts
 * the form per `relativePath` — `key={file.relativePath}`), never inside a
 * query function.
 */
@Component({
  selector: 'qt-document-description-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    @if (file(); as f) {
      <qt-modal [title]="'Describe ' + f.fileName" maxWidth="lg" (close)="closed.emit()">
        <div class="flex flex-col gap-3">
          <label for="document-description" class="qt-label"
            >Description or transcript (searchable)</label
          >
          <textarea
            id="document-description"
            class="qt-input text-sm"
            rows="6"
            [value]="text()"
            (input)="text.set($any($event.target).value)"
          ></textarea>
          <div class="flex justify-end gap-2">
            <button
              type="button"
              class="qt-button-secondary qt-button-sm"
              (click)="closed.emit()"
              [disabled]="saving()"
            >
              Cancel
            </button>
            <button
              type="button"
              class="qt-button-primary qt-button-sm"
              [disabled]="saving() || text() === f.description"
              (click)="save()"
            >
              {{ saving() ? 'Saving…' : 'Save' }}
            </button>
          </div>
        </div>
      </qt-modal>
    }
  `,
})
export class DocumentDescriptionDialog {
  /** The file being described; null closes the dialog. */
  readonly file = input<DescribedFile | null>(null);
  /** Save the description; resolves whether it took. */
  readonly saveDescription = input.required<(description: string) => Promise<boolean>>();
  readonly closed = output<void>();

  protected readonly text = linkedSignal(() => this.file()?.description ?? '');
  protected readonly saving = signal(false);

  protected async save(): Promise<void> {
    this.saving.set(true);
    const ok = await this.saveDescription()(this.text());
    this.saving.set(false);
    if (ok) this.closed.emit();
  }
}
