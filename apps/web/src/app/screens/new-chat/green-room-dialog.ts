import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';

import { GreenRoomStore } from './green-room.state';
import { OutfitSlotsPreview } from './outfit-slots-preview';

/**
 * The Green Room — the blocking chat-creation status dialog (v4
 * `ChatCreationProgressModal`). Non-dismissable while creation runs (no backdrop
 * / Escape / ✕ close); it closes on its own when the dispatch resolves. Only on
 * failure does it offer a Close button. Reads {@link GreenRoomStore}; mounted
 * once by the New-Chat page.
 */
@Component({
  selector: 'qt-green-room-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [OutfitSlotsPreview],
  template: `
    @if (state().open) {
      <div class="qt-dialog-overlay">
        <div
          class="qt-dialog flex flex-col max-h-[90vh]"
          style="max-width: 42rem;"
          role="dialog"
          aria-modal="true"
        >
          <div class="qt-dialog-header">
            <h2 class="qt-dialog-title">The Green Room</h2>
          </div>

          <div class="qt-dialog-body overflow-y-auto">
            <div class="flex flex-col gap-4">
              <!-- Headline -->
              <div class="flex items-center gap-2" role="status" aria-live="polite">
                @if (!isError() && !isDone()) {
                  <span
                    class="qt-bg-accent inline-block h-2 w-2 flex-shrink-0 animate-pulse rounded-full"
                  ></span>
                }
                <span class="qt-text-primary text-sm font-medium">{{
                  state().status || 'Setting the stage…'
                }}</span>
              </div>

              @if (isError() && state().errorMessage) {
                <div class="qt-text-danger text-sm">{{ state().errorMessage }}</div>
              }

              <!-- Per-character wardrobe consultations -->
              @if (state().wardrobe.length > 0) {
                <div class="flex flex-col gap-3">
                  @for (panel of state().wardrobe; track panel.characterId) {
                    <div class="flex flex-col gap-2">
                      <div class="flex items-center gap-2">
                        @if (panel.slots === null) {
                          <span
                            class="inline-block h-3 w-3 animate-spin rounded-full border-2 border-current border-t-transparent opacity-70"
                          ></span>
                        }
                        <span class="qt-text-primary text-sm font-semibold">{{
                          panel.characterName
                        }}</span>
                        <span class="qt-text-tertiary text-xs">{{
                          panel.slots === null ? 'consulting the wardrobe…' : 'is wearing'
                        }}</span>
                      </div>
                      @if (panel.slots !== null) {
                        <qt-outfit-slots-preview [slots]="panel.slots" />
                      }
                    </div>
                  }
                </div>
              }

              <!-- Scrolling activity log -->
              @if (state().logs.length > 0) {
                <div class="qt-divider mt-1 border-t pt-3">
                  <div class="qt-text-tertiary mb-2 text-xs uppercase tracking-wide">Activity</div>
                  <div class="max-h-40 overflow-y-auto pr-1">
                    <ul class="flex flex-col gap-1 text-sm">
                      @for (entry of state().logs; track $index) {
                        <li [class]="logColor(entry.level)">
                          <span class="qt-text-tertiary mr-2 tabular-nums">{{
                            time(entry.ts)
                          }}</span>
                          {{ entry.message }}
                        </li>
                      }
                    </ul>
                  </div>
                </div>
              }
            </div>
          </div>

          @if (isError()) {
            <div class="qt-dialog-footer flex justify-end">
              <button type="button" class="qt-button-primary" (click)="close()">Close</button>
            </div>
          }
        </div>
      </div>
    }
  `,
})
export class GreenRoomDialog {
  private readonly store = inject(GreenRoomStore);

  protected readonly state = this.store.state;
  protected readonly isError = computed(() => this.state().phase === 'error');
  protected readonly isDone = computed(() => this.state().phase === 'done');

  protected close(): void {
    this.store.close();
  }

  protected logColor(level: 'info' | 'warn' | 'error'): string {
    if (level === 'error') return 'qt-text-danger';
    if (level === 'warn') return 'qt-text-warning';
    return 'qt-text-secondary';
  }

  protected time(ts: number): string {
    if (!ts) return '';
    try {
      return new Date(ts).toLocaleTimeString();
    } catch {
      return '';
    }
  }
}
