import { ChangeDetectionStrategy, Component, input } from '@angular/core';

/**
 * The Wardrobe tab (v4 `CharacterDetailView.tsx` inline `'wardrobe'` case): v4
 * renders the intro prose plus a button that opens the global wardrobe dialog.
 * That dialog is its own future work order here, so the button renders
 * disabled with the same v4 copy.
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
        disabled
        class="qt-button-primary"
        title="The wardrobe dialog is not yet available in this vertical"
      >
        Open wardrobe for {{ characterName() }}
      </button>
    </div>
  `,
})
export class CharacterWardrobeTab {
  readonly characterName = input<string>('this character');
}
