import { ChangeDetectionStrategy, Component, computed, input, output, signal } from '@angular/core';

import type { MemoryDto } from '../core/core-contract';
import { Icon } from '../ui/icon';
import {
  formatMemoryDate,
  importanceColorClass,
  importanceLabel,
  importancePercent,
} from './memory-format';

/**
 * One Commonplace Book card (v4 `components/memory/memory-card.tsx`): summary,
 * an importance bucket (Low/Medium/High) with a %-tooltip, an AUTO (blue) /
 * MANUAL (green) source badge, expandable content (>150 chars), keyword chips,
 * read-only tag badges, and a footer with the created date, an AUTO "Source"
 * link (navigates to the source chat — deep message anchoring is a P4.6t
 * deferral), and Edit / Delete actions.
 */
@Component({
  selector: 'qt-memory-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="qt-card">
      <!-- Header: summary + importance + source -->
      <div class="flex items-start justify-between gap-2 mb-2">
        <div class="flex-1 min-w-0">
          <p class="qt-text-label line-clamp-2">{{ memory().summary }}</p>
        </div>
        <div class="flex items-center gap-2 flex-shrink-0">
          <span
            class="qt-text-label-xs"
            [class]="importanceColor()"
            [title]="'Importance: ' + importancePct() + '%'"
          >
            {{ importanceText() }}
          </span>
          <span
            class="text-xs px-2 py-0.5 rounded-full"
            [class]="
              memory().source === 'AUTO' ? 'qt-bg-info/10 qt-text-info' : 'qt-bg-success/10 qt-text-success'
            "
          >
            {{ memory().source === 'AUTO' ? 'Auto' : 'Manual' }}
          </span>
        </div>
      </div>

      <!-- Content preview / full -->
      <div class="mb-3">
        <p class="qt-text-small" [class.line-clamp-3]="!expanded()">{{ memory().content }}</p>
        @if (memory().content.length > 150) {
          <button type="button" class="text-xs text-primary hover:underline mt-1" (click)="toggle()">
            {{ expanded() ? 'Show less' : 'Show more' }}
          </button>
        }
      </div>

      <!-- Keywords -->
      @if (memory().keywords.length > 0) {
        <div class="flex flex-wrap gap-1 mb-3">
          @for (keyword of memory().keywords; track keyword) {
            <span class="qt-text-xs px-2 py-0.5 qt-bg-accent qt-text-on-accent rounded">{{
              keyword
            }}</span>
          }
        </div>
      }

      <!-- Tags (read-only) -->
      @if (memory().tagDetails; as tagDetails) {
        @if (tagDetails.length > 0) {
          <div class="flex flex-wrap gap-1 mb-3">
            @for (tag of tagDetails; track tag.id) {
              <span class="qt-text-xs px-2 py-0.5 qt-bg-muted qt-text-secondary rounded-full"
                >{{ tag.name }}</span
              >
            }
          </div>
        }
      }

      <!-- Footer -->
      <div class="flex items-center justify-between pt-2 border-t qt-border-default">
        <div class="flex items-center gap-2">
          <span class="qt-text-xs">{{ createdLabel() }}</span>
          @if (showSource()) {
            <button
              type="button"
              class="text-xs text-primary hover:underline flex items-center gap-1"
              title="View source message"
              (click)="navigateToSource.emit({ chatId: memory().chatId!, messageId: memory().sourceMessageId! })"
            >
              <qt-icon name="external-link" class="w-3 h-3" />
              Source
            </button>
          }
        </div>
        <div class="flex gap-2">
          <button type="button" class="text-xs text-primary hover:underline" (click)="edit.emit(memory())">
            Edit
          </button>
          <button
            type="button"
            class="text-xs qt-text-destructive hover:underline disabled:opacity-50"
            [disabled]="isDeleting()"
            (click)="delete.emit(memory().id)"
          >
            {{ isDeleting() ? 'Deleting...' : 'Delete' }}
          </button>
        </div>
      </div>
    </div>
  `,
})
export class MemoryCard {
  readonly memory = input.required<MemoryDto>();
  readonly isDeleting = input(false);

  readonly edit = output<MemoryDto>();
  readonly delete = output<string>();
  readonly navigateToSource = output<{ chatId: string; messageId: string }>();

  protected readonly expanded = signal(false);

  protected readonly importanceText = computed(() => importanceLabel(this.memory().importance));
  protected readonly importanceColor = computed(() =>
    importanceColorClass(this.memory().importance),
  );
  protected readonly importancePct = computed(() => importancePercent(this.memory().importance));
  protected readonly createdLabel = computed(() => formatMemoryDate(this.memory().createdAt));

  /** v4: source link only for AUTO memories carrying both chatId + sourceMessageId. */
  protected readonly showSource = computed(() => {
    const m = this.memory();
    return m.source === 'AUTO' && !!m.sourceMessageId && !!m.chatId;
  });

  protected toggle(): void {
    this.expanded.update((v) => !v);
  }
}
