import { ChangeDetectionStrategy, Component, inject, input, signal } from '@angular/core';
import { injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import { characterKeys } from './characters.api';
import { ToastService } from '../../ui/toast.service';

/**
 * The "let this character choose their opening outfit" checkbox card (v4
 * `8bf3cb5f` — the Wardrobe-tab block shared byte-for-byte between
 * `CharacterEditView.tsx` and `CharacterDetailView.tsx`). Persists the vault
 * `properties.json` `canChooseOutfit` flag with an immediate PUT, mirroring the
 * other per-character wardrobe toggles.
 *
 * v4's edit view re-fetches on failure to snap the checkbox back to server
 * truth; v4's detail view updates optimistically and shows a toast. v5 collapses
 * both to the house style (v5 `defaults-tab.ts` `save()`): dispatch the
 * `characterUpdate`, then invalidate the character-detail query so the refetched
 * value drives the checkbox. A failed save surfaces inline (v4's toast affordance
 * has no v5 analogue here) and, because the invalidation refetches unconditionally,
 * the control snaps back to server truth.
 */
@Component({
  selector: 'qt-character-choose-outfit-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="rounded-lg border qt-border-default qt-bg-card p-4">
      <label class="flex items-start gap-3 cursor-pointer">
        <input
          type="checkbox"
          class="mt-1 accent-[var(--primary)]"
          [checked]="canChooseOutfit()"
          [disabled]="saving() || characterId() === null"
          (change)="onChange($any($event.target).checked)"
        />
        <span class="flex-1 min-w-0">
          <span class="qt-text-label block">Let this character choose their opening outfit</span>
          <span class="qt-text-small qt-text-secondary block mt-0.5">
            When enabled, a new chat with this character defaults its Starting Outfit to “Let
            character choose” instead of their default wardrobe. You can still overrule it per chat.
          </span>
        </span>
        @if (saving()) {
          <span class="h-4 w-4 mt-1 animate-spin rounded-full qt-spinner shrink-0"></span>
        }
      </label>
    </div>
  `,
})
export class CharacterChooseOutfitCard {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly queryClient = injectQueryClient();

  /** Null while the character is still loading (v4's `disabled={... || !character}`). */
  readonly characterId = input.required<string | null>();
  readonly canChooseOutfit = input(false);

  protected readonly saving = signal(false);

  protected async onChange(enabled: boolean): Promise<void> {
    const id = this.characterId();
    if (id === null) return;
    this.saving.set(true);
    try {
      await this.core.dispatchData({
        type: 'characterUpdate',
        characterId: id,
        character: { canChooseOutfit: enabled },
      });
      // v4 `handleSaveCanChooseOutfit` (`useCharacterView.ts:513-517`).
      this.toasts.showSuccess(
        enabled
          ? 'New chats will let this character choose their own opening outfit'
          : 'New chats will use this character’s default opening outfit',
      );
    } catch (err) {
      this.toasts.showError(
        err instanceof Error && err.message ? err.message : 'Failed to update outfit-choice setting',
      );
    } finally {
      this.saving.set(false);
      await this.queryClient.invalidateQueries({ queryKey: characterKeys.detail(id) });
    }
  }
}
