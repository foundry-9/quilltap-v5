import { ChangeDetectionStrategy, Component } from '@angular/core';

import { EmptyState } from '../../../../ui/empty-state';

/**
 * The Conversations tab (v4 `components/ConversationsTab.tsx` →
 * `CharacterConversationsTab`): the per-character chat list, search, infinite
 * scroll, and archive refresh. A full port is its own future work order — for
 * now, chats featuring this character can be found from the Salon roster.
 */
@Component({
  selector: 'qt-character-conversations-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [EmptyState],
  template: `
    <qt-empty-state
      title="No conversations shown here yet"
      description="This vertical lands later — for now, find chats featuring this character from the Salon roster."
      variant="dashed"
    />
  `,
})
export class CharacterConversationsTab {}
