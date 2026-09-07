import { ChangeDetectionStrategy, Component, inject, input, output, signal } from '@angular/core';

import { CoreClient, coreErrorMessage } from '../../../../core/core-client';
import { Icon } from '../../../../ui/icon';
import { ToastService } from '../../../../ui/toast.service';
import {
  dispatchCharacterRename,
  type RenamePair,
  type RenamePreviewResponse,
} from '../edit-generators.api';

interface ReplacementPairForm {
  id: string;
  oldValue: string;
  newValue: string;
  caseSensitive: boolean;
}

/**
 * The Rename & Replace tab — v4 `components/characters/RenameReplaceTab.tsx`
 * (455 lines): a primary name change plus additional nickname/alias
 * replacements, a dry-run **Preview** (`characterRename` §B.2, K1's verb —
 * a real `CoreRequest` member since the `2f4254b42` unification folded
 * it), grouped counts, then **Apply** with a confirmation and success toast.
 * Copy and the counts-table column order carry over verbatim.
 */
@Component({
  selector: 'qt-character-rename-replace-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="space-y-6">
      <div class="qt-bg-card rounded-lg border qt-border-default p-4">
        <h3 class="qt-text-section mb-4 text-foreground">Rename Character</h3>
        <p class="text-sm qt-text-muted mb-4">
          Change this character's name across all associated data including details, physical
          descriptions, memories, and chat conversations.
        </p>

        <div class="grid grid-cols-1 md:grid-cols-2 gap-4 mb-4">
          <div>
            <label class="block qt-text-label mb-2 text-foreground">Current Name</label>
            <input
              type="text"
              [value]="characterName()"
              disabled
              class="w-full px-3 py-2 border qt-border-default qt-bg-muted text-foreground rounded-lg"
            />
          </div>
          <div>
            <label class="block qt-text-label mb-2 text-foreground">New Name</label>
            <input
              type="text"
              placeholder="Enter new name"
              [value]="newName()"
              (input)="newName.set($any($event.target).value)"
              class="w-full px-3 py-2 border qt-border-default qt-bg-card text-foreground rounded-lg focus:outline-none focus:ring-2 focus:ring-ring"
            />
          </div>
        </div>

        <label class="flex items-center gap-2 text-sm text-foreground">
          <input
            type="checkbox"
            class="w-4 h-4 rounded qt-border-default text-primary focus:ring-ring"
            [checked]="caseSensitive()"
            (change)="caseSensitive.set($any($event.target).checked)"
          />
          Case sensitive matching
        </label>
      </div>

      <div class="qt-bg-card rounded-lg border qt-border-default p-4">
        <div class="flex items-center justify-between mb-4">
          <div>
            <h3 class="qt-text-section text-foreground">Additional Replacements</h3>
            <p class="text-sm qt-text-muted">
              Replace nicknames, aliases, or other terms associated with this character.
            </p>
          </div>
          <button
            type="button"
            class="px-3 py-1.5 text-sm qt-button-primary flex items-center gap-1"
            (click)="addReplacement()"
          >
            <qt-icon name="plus" class="w-4 h-4" />
            Add
          </button>
        </div>

        @if (additionalReplacements().length === 0) {
          <p class="text-sm qt-text-secondary italic py-4 text-center">
            No additional replacements. Click "Add" to add nicknames or aliases to replace.
          </p>
        } @else {
          <div class="space-y-3">
            @for (replacement of additionalReplacements(); track replacement.id) {
              <div class="flex items-start gap-3 p-3 qt-bg-muted rounded-lg">
                <div class="flex-1 grid grid-cols-1 md:grid-cols-2 gap-3">
                  <div>
                    <label class="block qt-text-label-xs mb-1 qt-text-muted">Find</label>
                    <input
                      type="text"
                      placeholder="e.g., Snips"
                      [value]="replacement.oldValue"
                      (input)="updateReplacement(replacement.id, 'oldValue', $any($event.target).value)"
                      class="w-full px-2 py-1.5 text-sm border qt-border-default qt-bg-card text-foreground rounded focus:outline-none focus:ring-1 focus:ring-ring"
                    />
                  </div>
                  <div>
                    <label class="block qt-text-label-xs mb-1 qt-text-muted">Replace with</label>
                    <input
                      type="text"
                      placeholder="e.g., Ace"
                      [value]="replacement.newValue"
                      (input)="updateReplacement(replacement.id, 'newValue', $any($event.target).value)"
                      class="w-full px-2 py-1.5 text-sm border qt-border-default qt-bg-card text-foreground rounded focus:outline-none focus:ring-1 focus:ring-ring"
                    />
                  </div>
                </div>
                <div class="flex flex-col items-center gap-2 pt-5">
                  <label class="flex items-center gap-1 text-xs qt-text-muted" title="Case sensitive">
                    <input
                      type="checkbox"
                      class="w-3 h-3 rounded qt-border-default text-primary"
                      [checked]="replacement.caseSensitive"
                      (change)="
                        updateReplacement(replacement.id, 'caseSensitive', $any($event.target).checked)
                      "
                    />
                    Aa
                  </label>
                  <button
                    type="button"
                    class="p-1 qt-text-destructive hover:qt-text-destructive/80"
                    title="Remove"
                    (click)="removeReplacement(replacement.id)"
                  >
                    <qt-icon name="close" class="w-4 h-4" />
                  </button>
                </div>
              </div>
            }
          </div>
        }
      </div>

      @if (error()) {
        <div class="qt-alert-error px-4 py-3 rounded-lg border">{{ error() }}</div>
      }

      <div class="flex gap-3">
        <button
          type="button"
          class="flex-1 px-4 py-2.5 qt-bg-muted text-foreground rounded-lg hover:qt-bg-muted disabled:opacity-50 disabled:cursor-not-allowed font-medium"
          [disabled]="!hasValidInput() || isLoading() || isExecuting()"
          (click)="handlePreview()"
        >
          {{ isLoading() ? 'Loading Preview...' : 'Preview Changes' }}
        </button>
      </div>

      @if (preview(); as p) {
        <div class="qt-bg-card rounded-lg border qt-border-default p-4">
          <h3 class="qt-text-section mb-4 text-foreground">Preview Results</h3>

          <div class="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-3 mb-4">
            <div class="qt-bg-muted rounded p-3 text-center">
              <div class="qt-heading-2 qt-text-info">{{ p.summary.characterFields }}</div>
              <div class="text-xs qt-text-muted">Character Fields</div>
            </div>
            <div class="qt-bg-muted rounded p-3 text-center">
              <div class="qt-heading-2 text-primary">{{ p.summary.physicalDescriptions }}</div>
              <div class="text-xs qt-text-muted">Descriptions</div>
            </div>
            <div class="qt-bg-muted rounded p-3 text-center">
              <div class="qt-heading-2 qt-text-success">{{ p.summary.memories }}</div>
              <div class="text-xs qt-text-muted">Memories</div>
            </div>
            <div class="qt-bg-muted rounded p-3 text-center">
              <div class="qt-heading-2 qt-text-warning">{{ p.summary.chatTitles }}</div>
              <div class="text-xs qt-text-muted">Chat Titles</div>
            </div>
            <div class="qt-bg-muted rounded p-3 text-center">
              <div class="qt-heading-2 qt-text-destructive">{{ p.summary.chatMessages }}</div>
              <div class="text-xs qt-text-muted">Messages</div>
            </div>
            <div class="qt-bg-info/10 rounded p-3 text-center border qt-border-info">
              <div class="qt-heading-2 qt-text-info">{{ p.summary.total }}</div>
              <div class="text-xs qt-text-info font-medium">Total</div>
            </div>
          </div>

          @if (p.replacements.length > 0) {
            <div class="mb-4">
              <h4 class="qt-text-label mb-2 text-foreground">Replacements ({{ p.replacements.length }})</h4>
              <div class="max-h-80 overflow-y-auto border qt-border-default rounded-lg">
                <table class="w-full text-sm">
                  <thead class="qt-bg-muted sticky top-0">
                    <tr>
                      <th class="text-left px-3 py-2 qt-text-muted">Location</th>
                      <th class="text-left px-3 py-2 qt-text-muted">Field</th>
                      <th class="text-left px-3 py-2 qt-text-muted">Change</th>
                      <th class="text-left px-3 py-2 qt-text-muted">Context</th>
                    </tr>
                  </thead>
                  <tbody class="divide-y divide-border">
                    @for (r of visibleReplacements(p); track $index) {
                      <tr class="hover:qt-bg-muted">
                        <td class="px-3 py-2 text-foreground">{{ r.location }}</td>
                        <td class="px-3 py-2 qt-text-secondary font-mono text-xs">{{ r.field }}</td>
                        <td class="px-3 py-2">
                          <span class="qt-text-destructive line-through">{{ r.oldText }}</span>
                          <span class="mx-1 qt-text-secondary">→</span>
                          <span class="qt-text-success">{{ r.newText }}</span>
                        </td>
                        <td class="px-3 py-2 qt-text-secondary text-xs max-w-xs truncate" [title]="r.context">
                          {{ r.context }}
                        </td>
                      </tr>
                    }
                  </tbody>
                </table>
                @if (p.replacements.length > 100) {
                  <div class="px-3 py-2 qt-bg-muted text-sm qt-text-muted text-center">
                    ...and {{ p.replacements.length - 100 }} more
                  </div>
                }
              </div>
            </div>
          } @else {
            <div class="text-center py-8 qt-text-secondary">
              No occurrences found for the specified replacements.
            </div>
          }

          @if (p.summary.total > 0) {
            <button
              type="button"
              class="w-full px-4 py-3 qt-button-primary rounded-lg disabled:opacity-50 disabled:cursor-not-allowed font-medium"
              [disabled]="isExecuting() || isLoading()"
              (click)="handleExecute()"
            >
              {{ isExecuting() ? 'Executing...' : 'Execute ' + p.summary.total + ' Replacements' }}
            </button>
          }
        </div>
      }

      <div class="qt-alert-info border rounded-lg p-4">
        <h4 class="qt-text-label mb-2">How this works</h4>
        <ul class="text-sm space-y-1 list-disc list-inside">
          <li>Enter a new name to rename the character across all associated data</li>
          <li>Add additional replacements for nicknames or aliases (e.g., "Snips" → "Ace")</li>
          <li>Click "Preview Changes" to see what will be affected before making changes</li>
          <li>
            All replacements are made within: character details, physical descriptions, memories, and
            chat conversations
          </li>
          <li>Only data directly associated with this character will be modified</li>
        </ul>
      </div>
    </div>
  `,
})
export class CharacterRenameReplaceTab {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly characterId = input.required<string>();
  readonly characterName = input.required<string>();
  readonly renameComplete = output<void>();

  protected readonly newName = signal('');
  protected readonly caseSensitive = signal(false);
  protected readonly additionalReplacements = signal<ReplacementPairForm[]>([]);
  protected readonly preview = signal<RenamePreviewResponse | null>(null);
  protected readonly isLoading = signal(false);
  protected readonly isExecuting = signal(false);
  protected readonly error = signal<string | null>(null);

  protected addReplacement(): void {
    this.additionalReplacements.update((prev) => [
      ...prev,
      { id: crypto.randomUUID(), oldValue: '', newValue: '', caseSensitive: false },
    ]);
  }

  protected removeReplacement(id: string): void {
    this.additionalReplacements.update((prev) => prev.filter((r) => r.id !== id));
  }

  protected updateReplacement(
    id: string,
    field: keyof Omit<ReplacementPairForm, 'id'>,
    value: string | boolean,
  ): void {
    this.additionalReplacements.update((prev) =>
      prev.map((r) => (r.id === id ? { ...r, [field]: value } : r)),
    );
  }

  protected hasValidInput(): boolean {
    return (
      !!this.newName().trim() ||
      this.additionalReplacements().some((r) => r.oldValue.trim() && r.newValue.trim())
    );
  }

  protected visibleReplacements(p: RenamePreviewResponse): RenamePreviewResponse['replacements'] {
    return p.replacements.slice(0, 100);
  }

  private buildAdditionalReplacements(): RenamePair[] {
    return this.additionalReplacements()
      .filter((r) => r.oldValue.trim() && r.newValue.trim())
      .map((r) => ({ oldValue: r.oldValue, newValue: r.newValue, caseSensitive: r.caseSensitive }));
  }

  private buildPrimaryRename(): RenamePair | undefined {
    return this.newName().trim()
      ? {
          oldValue: this.characterName(),
          newValue: this.newName().trim(),
          caseSensitive: this.caseSensitive(),
        }
      : undefined;
  }

  protected async handlePreview(): Promise<void> {
    this.isLoading.set(true);
    this.error.set(null);
    this.preview.set(null);
    try {
      const data = await dispatchCharacterRename(this.core, {
        type: 'characterRename',
        characterId: this.characterId(),
        dryRun: true,
        additionalReplacements: this.buildAdditionalReplacements(),
        primaryRename: this.buildPrimaryRename(),
      });
      this.preview.set(data);
    } catch (err) {
      this.error.set(coreErrorMessage(err, 'Failed to preview changes'));
    } finally {
      this.isLoading.set(false);
    }
  }

  protected async handleExecute(): Promise<void> {
    const p = this.preview();
    if (!p) return;

    const proceed = window.confirm(
      `Are you sure you want to rename this character and update ${p.summary.total} occurrences? This action cannot be undone.`,
    );
    if (!proceed) return;

    this.isExecuting.set(true);
    this.error.set(null);
    try {
      const data = await dispatchCharacterRename(this.core, {
        type: 'characterRename',
        characterId: this.characterId(),
        dryRun: false,
        additionalReplacements: this.buildAdditionalReplacements(),
        primaryRename: this.buildPrimaryRename(),
      });
      this.toasts.showSuccess(`Successfully updated ${data.summary.total} occurrences!`);

      this.newName.set('');
      this.additionalReplacements.set([]);
      this.preview.set(null);

      this.renameComplete.emit();
    } catch (err) {
      const message = coreErrorMessage(err, 'Failed to execute rename');
      this.error.set(message);
      this.toasts.showError(message);
    } finally {
      this.isExecuting.set(false);
    }
  }
}
