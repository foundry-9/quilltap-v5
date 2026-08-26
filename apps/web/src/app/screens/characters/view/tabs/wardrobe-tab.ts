import { ChangeDetectionStrategy, Component, computed, inject, input } from '@angular/core';

import type { WardrobeContainer } from '../../../../wardrobe/wardrobe-container';

import { CharacterChooseOutfitCard } from '../../choose-outfit-card';
import { WardrobeDialogService } from '../../../../wardrobe/wardrobe-dialog.service';
import { WardrobeInstructionsSection } from '../../../../wardrobe/wardrobe-instructions-section';

/**
 * The Wardrobe tab (v4 `CharacterDetailView.tsx` inline `'wardrobe'` case,
 * `8bf3cb5f`): the intro prose, the `canChooseOutfit` checkbox card, a button
 * that opens the global wardrobe dialog with `{characterId}`, and — since v4
 * `b86bb1a5` — the character's own dressing instructions, the same
 * `Wardrobe/instructions.md` the wardrobe dialog edits. v4's
 * `disabled={!wardrobeDialog}` arm collapses here — the root-provided service is
 * always present.
 */
@Component({
  selector: 'qt-character-wardrobe-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CharacterChooseOutfitCard, WardrobeInstructionsSection],
  template: `
    <div class="space-y-4">
      <p class="qt-text-small qt-text-secondary">
        The wardrobe lives in a global dialog so it travels with you — edit, layer, and save outfits
        from anywhere, including from inside a chat.
      </p>
      <qt-character-choose-outfit-card
        [characterId]="characterId()"
        [canChooseOutfit]="canChooseOutfit()"
      />
      <button
        type="button"
        class="qt-button-primary"
        (click)="wardrobeDialog.open({ characterId: characterId() })"
      >
        Open wardrobe for {{ characterName() }}
      </button>
      <!-- The character's own dressing instructions — the same
           Wardrobe/instructions.md the wardrobe dialog edits (v4 :330-335). -->
      <qt-wardrobe-instructions-section [container]="container()" />
    </div>
  `,
})
export class CharacterWardrobeTab {
  protected readonly wardrobeDialog = inject(WardrobeDialogService);

  readonly characterId = input.required<string>();
  readonly characterName = input<string>('this character');
  readonly canChooseOutfit = input(false);

  /** v4 passes the literal `{ scope: 'character', id }` (`:335`). */
  protected readonly container = computed<WardrobeContainer>(() => ({
    scope: 'character',
    id: this.characterId(),
  }));
}
