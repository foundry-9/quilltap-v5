import { ChangeDetectionStrategy, Component, inject, input, output, signal } from '@angular/core';

import { CoreClient } from '../../../core/core-client';
import { Modal } from '../../../ui/modal';
import { deleteArchiveBundle } from '../characters.api';

/**
 * Post-rehydrate bundle disposal (v4
 * `app/aurora/[id]/view/components/RehydrateBundleDialog.tsx`, its spec §6 step
 * 6). A successful rehydration deliberately LEAVES the sealed bundle on the
 * shelf as cheap insurance; this is where the operator decides whether it stays.
 *
 * The button polarity is v4's and is deliberate: the destructive act ("Discard
 * the Bundle") is the SECONDARY button and keeping it is the PRIMARY, because
 * the spare copy costs nothing but shelf space and discarding it is the only
 * irreversible half. Do not "tidy" this into the usual confirm/cancel order.
 */
@Component({
  selector: 'qt-rehydrate-bundle-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal
      title="The empty bundle remains on the shelf"
      maxWidth="lg"
      [closeOnBackdrop]="!working()"
      (close)="onKeep()"
    >
      <div class="space-y-4">
        <p class="qt-text-small">
          {{ characterName() }} is fully unpacked — memories, correspondence and photographs all back
          where they belong. The sealed bundle they travelled in still sits in the file library, and
          keeping it costs nothing but shelf space: it is a spare copy of everything the archive
          held, exactly as it was.
        </p>
        <p class="qt-text-small qt-text-secondary">
          Discard it and the spare copy is gone for good — though of course you can always archive
          {{ characterName() }} afresh, which packs a new bundle from their current state.
        </p>
        @if (error(); as msg) {
          <p class="qt-text-small qt-text-danger">{{ msg }}</p>
        }
      </div>

      <div qt-modal-footer class="flex gap-3 justify-end w-full">
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="working()"
          (click)="discard()"
        >
          {{ working() ? 'Clearing the shelf…' : 'Discard the Bundle' }}
        </button>
        <button
          type="button"
          class="qt-button qt-button-primary"
          [disabled]="working()"
          (click)="onKeep()"
        >
          Keep It
        </button>
      </div>
    </qt-modal>
  `,
})
export class RehydrateBundleDialog {
  private readonly core = inject(CoreClient);

  readonly characterName = input.required<string>();
  /** The ARCHIVE file left behind by the rehydration. */
  readonly bundleFileId = input.required<string>();

  readonly closed = output<void>();
  /** Emitted after a successful delete, IN ADDITION to `closed` (v4 `:37-38`). */
  readonly deleted = output<void>();

  protected readonly working = signal(false);
  protected readonly error = signal<string | null>(null);

  protected async discard(): Promise<void> {
    this.working.set(true);
    this.error.set(null);
    try {
      await deleteArchiveBundle(this.core, this.bundleFileId());
      this.deleted.emit();
      this.closed.emit();
    } catch (err) {
      // v4 keeps the dialog OPEN on failure with the message inline — the
      // bundle is still there, so there is still a decision to make.
      this.error.set(
        err instanceof Error && err.message ? err.message : 'Failed to discard the bundle',
      );
    } finally {
      this.working.set(false);
    }
  }

  protected onKeep(): void {
    if (this.working()) {
      return;
    }
    this.closed.emit();
  }
}
