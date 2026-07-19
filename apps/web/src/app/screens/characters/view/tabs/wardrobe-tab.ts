import { ChangeDetectionStrategy, Component, inject, input } from '@angular/core';

import { WardrobeDialogService } from '../../../../wardrobe/wardrobe-dialog.service';

/**
 * The Wardrobe tab (v4 `CharacterDetailView.tsx` inline `'wardrobe'` case,
 * `:218-235`): v4 renders the intro prose plus a button that opens the global
 * wardrobe dialog with `{characterId}` (`:229`). v4's `disabled={!wardrobeDialog}`
 * arm collapses here — the root-provided service is always present.
 */
@Component({
  selector: 'qt-character-wardrobe-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="space-y-2">
      <p class="qt-text-small qt-text-secondary">
        The wardrobe lives in a global dialog so it travels with you — edit, layer, and save outfits
        from anywhere, including from inside a chat.
      </p>
      <button
        type="button"
        class="qt-button-primary"
        (click)="wardrobeDialog.open({ characterId: characterId() })"
      >
        Open wardrobe for {{ characterName() }}
      </button>
    </div>
  `,
})
export class CharacterWardrobeTab {
  protected readonly wardrobeDialog = inject(WardrobeDialogService);

  readonly characterId = input.required<string>();
  readonly characterName = input<string>('this character');
}
