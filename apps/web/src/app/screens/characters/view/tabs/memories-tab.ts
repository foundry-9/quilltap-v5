import { ChangeDetectionStrategy, Component, input } from '@angular/core';

import { MemoryList } from '../../../../memory/memory-list';

/**
 * The Memories tab (v4 `components/MemoriesTab.tsx` → `MemoryList`): the
 * per-character Commonplace Book — a 12-line wrapper around {@link MemoryList},
 * exactly as v4's `MemoriesTab` wraps `MemoryList`.
 */
@Component({
  selector: 'qt-character-memories-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [MemoryList],
  template: `<qt-memory-list [characterId]="characterId()" />`,
})
export class CharacterMemoriesTab {
  readonly characterId = input.required<string>();
}
