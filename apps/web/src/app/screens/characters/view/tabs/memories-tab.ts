import { ChangeDetectionStrategy, Component } from '@angular/core';

import { EmptyState } from '../../../../ui/empty-state';

/**
 * The Memories tab (v4 `components/MemoriesTab.tsx` → `MemoryList`): the
 * per-character Commonplace Book entries, search, and cascade actions. A full
 * port is its own future work order.
 */
@Component({
  selector: 'qt-character-memories-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [EmptyState],
  template: `
    <qt-empty-state
      title="No memories yet"
      description="The Commonplace Book vertical for this character lands later."
      variant="dashed"
    />
  `,
})
export class CharacterMemoriesTab {}
