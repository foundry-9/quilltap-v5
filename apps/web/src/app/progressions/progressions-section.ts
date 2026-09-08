import {
  ChangeDetectionStrategy,
  Component,
  computed,
  DestroyRef,
  inject,
  input,
  signal,
} from '@angular/core';

import { coreErrorMessage } from '../core/core-client';
import { Icon } from '../ui/icon';
import { ToastService } from '../ui/toast.service';
import { injectCharacterProgressions } from './character-progressions.api';
import { deriveProgression, renderProgressionReport } from './engine';
import { ProgressionEditorModal, type ProgressionEdit } from './progression-editor-modal';
import { MAX_PROGRESSIONS_PER_CHARACTER, type Progression } from './schema';

/** v4's `STATE_BADGE` — the pill beside each row's name. */
const STATE_BADGE: Record<string, { label: string; className: string }> = {
  pending: { label: 'not yet begun', className: 'qt-badge qt-badge-info' },
  active: { label: 'in progress', className: 'qt-badge qt-badge-success' },
  complete: { label: 'complete', className: 'qt-badge qt-badge-warning' },
};

/** One row, ready to render. */
interface Row {
  id: string;
  progression: Progression;
  badgeLabel: string;
  badgeClass: string;
  report: string;
}

/**
 * `qt-progressions-section` — a character's timed conditions, on the Aurora
 * "System Prompts" tab beside their subprompts (v4
 * `components/characters/progressions/ProgressionsSection.tsx` at
 * `25f534c0b`), whose header rides across:
 *
 * > A progression is a named span of time the character is carrying: a
 * > pregnancy, a recharging cannon, a fermentation. Every turn they take,
 * > Quilltap works out how far along it is and tells them, on whatever cadence
 * > the entry asks for.
 * >
 * > The live line under each row is rendered by the same client-safe engine the
 * > server prompts with, at `Date.now()` — so what the author reads here is
 * > exactly what the character will read, arithmetic and all. It ticks, because
 * > a "42% complete" frozen beside an advancing recharge would be a worse lie
 * > than no line at all.
 *
 * The save is a read-modify-write of the whole `metadata` object through the
 * ordinary character PUT — see `character-progressions.api.ts`. No new verb.
 */
@Component({
  selector: 'qt-progressions-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, ProgressionEditorModal],
  // v4's outer element is a `div` in a stack of siblings; an unstyled Angular
  // custom element is `display: inline`, which would drop the `pt-6 border-t`
  // rule below out of the flow (dogfood #97 / #107's class).
  host: { class: 'block' },
  template: `
    <div class="space-y-4 pt-6 border-t qt-border-default">
      <div class="flex justify-between items-center">
        <div>
          <h3 class="qt-heading-4 text-foreground">Progressions</h3>
          <p class="qt-text-small">
            Spans of time {{ characterName() }} is carrying — a gestation, a recharging weapon, a
            fermentation. Each turn, Quilltap works out how far along it is and tells them so,
            without the model having to do arithmetic or remember that time has passed. They live in
            the vault&rsquo;s <code>metadata.json</code>, where a custom tool can read them and
            adjust them; the model itself never sets one.
          </p>
        </div>
        <button
          type="button"
          class="qt-button-primary flex-shrink-0"
          [disabled]="isArchived() || atCeiling()"
          [attr.title]="addTitle()"
          (click)="openCreate()"
        >
          + Add Progression
        </button>
      </div>

      @if (isArchived()) {
        <p class="qt-text-small qt-text-secondary">
          {{ characterName() }} is archived, so this card is read-only. Rehydrate them to make
          changes.
        </p>
      }

      @if (invalidIds().length > 0) {
        <p class="qt-text-small qt-text-destructive">
          {{ invalidCountPhrase() }} this character&rsquo;s <code>metadata.json</code> could not be
          read and {{ invalidVerb() }} being skipped:
          @for (id of invalidIds(); track id; let last = $last) {
            <code>{{ id }}</code
            >{{ last ? '' : ', ' }}
          }
          . Editing the file directly is the way to mend {{ invalidPronoun() }}.
        </p>
      }

      @if (isLoading()) {
        <div class="text-center py-4 qt-text-secondary">Loading progressions...</div>
      } @else if (rows().length === 0) {
        <div class="qt-card text-center">
          <p class="qt-text-small mb-4">
            Nothing in progress. Add one and {{ characterName() }} will be told where it stands,
            every turn it matters.
          </p>
          <button
            type="button"
            class="qt-button-primary"
            [disabled]="isArchived()"
            (click)="openCreate()"
          >
            Create First Progression
          </button>
        </div>
      } @else {
        <div class="space-y-3">
          @for (row of rows(); track row.id) {
            <div class="qt-card hover:bg-accent/50 transition">
              <div class="flex justify-between items-start">
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2 mb-1 flex-wrap">
                    <h4 class="qt-text-primary truncate">{{ row.progression.name }}</h4>
                    <code class="text-xs qt-text-secondary">{{ row.id }}</code>
                    <span [class]="row.badgeClass">{{ row.badgeLabel }}</span>
                  </div>
                  <p class="qt-text-small italic">{{ row.report }}</p>
                </div>
                <div class="flex items-center gap-1 ml-4">
                  <button
                    type="button"
                    class="qt-button-icon qt-button-ghost"
                    title="Edit"
                    [disabled]="isArchived()"
                    [attr.aria-label]="'Edit progression ' + row.progression.name"
                    (click)="openEdit(row)"
                  >
                    <qt-icon name="pencil" class="w-4 h-4" />
                  </button>
                  <div class="relative">
                    <button
                      type="button"
                      class="qt-button-icon qt-button-ghost hover:qt-text-destructive"
                      title="Delete"
                      [disabled]="isArchived()"
                      [attr.aria-label]="'Delete progression ' + row.progression.name"
                      (click)="toggleConfirm(row.id)"
                    >
                      <qt-icon name="trash" class="w-4 h-4" />
                    </button>
                    @if (deleteConfirm() === row.id) {
                      <div
                        class="absolute right-0 top-full mt-1 p-3 qt-bg-card border qt-border-default rounded-lg qt-shadow-lg z-10 min-w-[240px]"
                      >
                        <p class="text-sm text-foreground mb-2">
                          Remove this progression? Any tool addressing <code>{{ row.id }}</code>
                          stops finding it.
                        </p>
                        <div class="flex gap-2">
                          <button
                            type="button"
                            class="qt-button-destructive qt-button-sm flex-1"
                            [disabled]="removePending()"
                            (click)="remove(row.id)"
                          >
                            Delete
                          </button>
                          <button
                            type="button"
                            class="qt-button-secondary qt-button-sm flex-1"
                            (click)="deleteConfirm.set(null)"
                          >
                            Cancel
                          </button>
                        </div>
                      </div>
                    }
                  </div>
                </div>
              </div>
            </div>
          }
        </div>
      }
    </div>

    @if (creating() || editing() !== null) {
      <!-- v4's key={editing?.id ?? '__new__'}: a one-element @for tracked by
           that string, so opening a different entry mounts a FRESH form rather
           than re-seating a half-typed one (the subprompts precedent). -->
      @for (key of [editorKey()]; track key) {
        <qt-progression-editor-modal
          [editing]="editing()"
          [existingIds]="existingIds()"
          [saving]="savePending()"
          (close)="closeEditor()"
          (save)="handleSave($event.id, $event.progression)"
        />
      }
    }
  `,
})
export class ProgressionsSection {
  private readonly toasts = inject(ToastService);

  readonly characterId = input.required<string>();
  readonly characterName = input.required<string>();

  private readonly api = injectCharacterProgressions(() => this.characterId());

  protected readonly invalidIds = this.api.invalidIds;
  protected readonly isArchived = this.api.isArchived;
  protected readonly isLoading = this.api.isLoading;
  protected readonly savePending = this.api.savePending;
  protected readonly removePending = this.api.removePending;

  protected readonly editing = signal<ProgressionEdit | null>(null);
  protected readonly creating = signal(false);
  protected readonly deleteConfirm = signal<string | null>(null);

  /**
   * The clock the live lines read. Advanced by an interval rather than read
   * during render — see the class header.
   */
  private readonly nowMs = signal(Date.now());

  constructor() {
    const timer = setInterval(() => this.nowMs.set(Date.now()), 1000);
    inject(DestroyRef).onDestroy(() => clearInterval(timer));
  }

  /** v4 `Object.entries(progressions).sort(([a], [b]) => a.localeCompare(b))`. */
  private readonly entries = computed(() =>
    Object.entries(this.api.progressions()).sort(([a], [b]) => a.localeCompare(b)),
  );

  protected readonly existingIds = computed(() => this.entries().map(([id]) => id));

  protected readonly atCeiling = computed(
    () => this.entries().length >= MAX_PROGRESSIONS_PER_CHARACTER,
  );

  protected readonly rows = computed<Row[]>(() =>
    this.entries().map(([id, progression]) => {
      const derived = deriveProgression(id, progression, this.nowMs());
      const badge = STATE_BADGE[derived.state];
      return {
        id,
        progression,
        badgeLabel: badge.label,
        badgeClass: badge.className,
        report: renderProgressionReport(progression, derived),
      };
    }),
  );

  /** v4's two disabled titles; `undefined` when the button is live. */
  protected readonly addTitle = computed(() => {
    if (this.isArchived()) return 'An archived character is a tombstone — rehydrate them first.';
    if (this.atCeiling())
      return `A character may carry at most ${MAX_PROGRESSIONS_PER_CHARACTER} progressions.`;
    return null;
  });

  /** v4's singular/plural triple on the unparseable-ids line. */
  protected readonly invalidCountPhrase = computed(() => {
    const n = this.invalidIds().length;
    return n === 1 ? 'One entry in' : `${n} entries in`;
  });
  protected readonly invalidVerb = computed(() => (this.invalidIds().length === 1 ? 'is' : 'are'));
  protected readonly invalidPronoun = computed(() =>
    this.invalidIds().length === 1 ? 'it' : 'them',
  );

  protected readonly editorKey = computed(() => this.editing()?.id ?? '__new__');

  protected openCreate(): void {
    this.creating.set(true);
  }

  protected openEdit(row: Row): void {
    this.editing.set({ id: row.id, progression: row.progression });
  }

  protected closeEditor(): void {
    this.creating.set(false);
    this.editing.set(null);
  }

  /** Clicking the trash toggles this row's confirm, closing any other. */
  protected toggleConfirm(id: string): void {
    this.deleteConfirm.set(this.deleteConfirm() === id ? null : id);
  }

  /**
   * v4 `handleSave`. The `updatedAt` stamp is the point: the character's very
   * next turn reports the change regardless of the entry's own cadence.
   */
  protected async handleSave(id: string, progression: Progression): Promise<void> {
    const wasEditing = this.editing() !== null;
    try {
      await this.api.save({
        ...this.api.progressions(),
        [id]: { ...progression, updatedAt: new Date().toISOString() },
      });
      this.toasts.showSuccess(wasEditing ? 'Progression updated' : 'Progression added');
      this.editing.set(null);
      this.creating.set(false);
    } catch (err) {
      this.toasts.showError(coreErrorMessage(err, 'The progression could not be saved.'));
    }
  }

  /** v4 `handleDelete` — the confirm clears in `finally`, either way. */
  protected async remove(id: string): Promise<void> {
    const next = { ...this.api.progressions() };
    delete next[id];
    try {
      await this.api.remove(next);
      this.toasts.showSuccess('Progression removed');
    } catch (err) {
      this.toasts.showError(coreErrorMessage(err, 'The progression could not be removed.'));
    } finally {
      this.deleteConfirm.set(null);
    }
  }
}
