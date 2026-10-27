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
 * One Commonplace Book card (v4 `components/memory/memory-card.tsx` at
 * `01a83539d`): summary, an importance bucket (Low/Medium/High) with a
 * %-tooltip, an `Archived` chip on a cold row (dimmed `opacity-75`), an AUTO
 * (blue) / CONSOLIDATED `Digest` (amber) / MANUAL (green) source badge, the
 * digest provenance line (`Digested from N notes.` and, on an archived row, the
 * `Superseded by a digest` link or text), expandable content (>150 chars),
 * keyword chips, read-only tag badges, and a footer with the created date, an
 * AUTO "Source" link (navigates to the source chat — deep message anchoring is
 * a P4.6t deferral), and Edit / Delete actions.
 *
 * **v4's handler-presence gates, translated (P4.D276 R-D).** v4 renders Edit
 * only with `onEdit`, Delete only with `onDelete`, Source only with
 * `onNavigateToSource`, and the superseded BUTTON only with `onViewDigest` (the
 * plain text otherwise). An Angular `output` is always present, so two boolean
 * inputs carry the gates: `showActions` (default true) gates Edit, Delete AND
 * Source; `canViewDigest` (default false) picks the button over the text. The
 * digest viewer binds `[showActions]="false"` — v4's handler-less
 * `<MemoryCard memory={viewedDigest} />`.
 *
 * `canNavigateToSource` (default true) is v4's `onNavigateToSource` presence:
 * v4's list passes none (`memory-list.tsx:335-342`), so it binds false.
 */
@Component({
  selector: 'qt-memory-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="qt-card" [class.opacity-75]="archived()">
      <!-- Header: summary + importance + archived + source -->
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
          @if (archived()) {
            <span
              class="text-xs px-2 py-0.5 rounded-full qt-bg-muted qt-text-secondary"
              title="Archived: out of everyday recall, still on the shelf"
              >Archived</span
            >
          }
          <span class="text-xs px-2 py-0.5 rounded-full" [class]="sourceClass()">{{ sourceText() }}</span>
        </div>
      </div>

      <!-- Provenance (v4 :99-116). One line, no template whitespace: the lead
           carries v4's trailing space inside the string. -->
      @if (showProvenance()) {
        <p class="qt-text-xs qt-text-muted mb-2">{{ digestLead() }}@if (supersededId(); as digestId) {@if (canViewDigest()) {<button type="button" class="qt-link" (click)="viewDigest.emit(digestId)">Superseded by a digest</button>} @else {Superseded by a digest.}}</p>
      }

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
          @if (showActions() && showSource()) {
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
          @if (showActions()) {
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
          }
        </div>
      </div>
    </div>
  `,
})
export class MemoryCard {
  readonly memory = input.required<MemoryDto>();
  readonly isDeleting = input(false);
  /** R-D: v4's `onEdit` / `onDelete` / `onNavigateToSource` presence. */
  readonly showActions = input(true);
  /** R-D: v4's `onViewDigest` presence — the button over the plain text. */
  readonly canViewDigest = input(false);
  /** R-D: v4's `onNavigateToSource` presence — the Source link needs it (v4's list passes none). */
  readonly canNavigateToSource = input(true);

  readonly edit = output<MemoryDto>();
  readonly delete = output<string>();
  readonly navigateToSource = output<{ chatId: string; messageId: string }>();
  /** v4 `onViewDigest(supersededById)`. */
  readonly viewDigest = output<string>();

  protected readonly expanded = signal(false);

  protected readonly importanceText = computed(() => importanceLabel(this.memory().importance));
  protected readonly importanceColor = computed(() =>
    importanceColorClass(this.memory().importance),
  );
  protected readonly importancePct = computed(() => importancePercent(this.memory().importance));
  protected readonly createdLabel = computed(() => formatMemoryDate(this.memory().createdAt));

  /** v4 `:63` — `memory.tier === 'cold'` (an absent tier is not archived). */
  protected readonly archived = computed(() => this.memory().tier === 'cold');
  /** v4 `:64` — the member count, digests only. */
  protected readonly digestSize = computed(() => {
    const m = this.memory();
    return m.source === 'CONSOLIDATED' ? (m.consolidatedFrom?.length ?? 0) : 0;
  });
  /** v4 `:87-93`. */
  protected readonly sourceClass = computed(() => {
    const source = this.memory().source;
    return source === 'AUTO'
      ? 'qt-bg-info/10 qt-text-info'
      : source === 'CONSOLIDATED'
        ? 'qt-bg-warning/10 qt-text-warning'
        : 'qt-bg-success/10 qt-text-success';
  });
  /** v4 `:94`. */
  protected readonly sourceText = computed(() => {
    const source = this.memory().source;
    return source === 'AUTO' ? 'Auto' : source === 'CONSOLIDATED' ? 'Digest' : 'Manual';
  });
  /** v4 `:103-104` — the digest's superseder, only on an archived row. */
  protected readonly supersededId = computed(() =>
    this.archived() ? this.memory().supersededById || null : null,
  );
  /** v4 `:99` — `digestSize > 0 || (archived && supersededById)`. */
  protected readonly showProvenance = computed(
    () => this.digestSize() > 0 || this.supersededId() !== null,
  );
  /** v4 `:101` — the TRAILING space is v4's, kept inside the string. */
  protected readonly digestLead = computed(() => {
    const n = this.digestSize();
    return n > 0 ? `Digested from ${n} note${n === 1 ? '' : 's'}. ` : '';
  });

  /** v4: source link only for AUTO memories carrying both chatId + sourceMessageId. */
  protected readonly showSource = computed(() => {
    const m = this.memory();
    return this.canNavigateToSource() && m.source === 'AUTO' && !!m.sourceMessageId && !!m.chatId;
  });

  protected toggle(): void {
    this.expanded.update((v) => !v);
  }
}
