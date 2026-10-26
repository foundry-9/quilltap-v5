import { SALON_CHAT_PATH_RE } from '../wardrobe/default-character';

/** Where a rail entry that opens a scoped dialog (the Wardrobe, the Inventory) should go. */
export type RailTarget = { kind: 'tab' } | { kind: 'dialog' } | { kind: 'chat'; chatId: string };

/**
 * v4 `sidebar-footer.tsx:255-279` (the Inventory entry; the Wardrobe's at
 * `:227-253` reads the same): in the workspace, a browse/edit tab; on a Salon
 * chat path, the chat-scoped dialog (the caller resolves the default
 * character); anywhere else, the plain dialog.
 */
export function railTarget(url: string, workspaceTabs: boolean): RailTarget {
  if (workspaceTabs && url.split('?')[0] === '/workspace') return { kind: 'tab' };
  const chatMatch = url.match(SALON_CHAT_PATH_RE);
  return chatMatch ? { kind: 'chat', chatId: chatMatch[1] } : { kind: 'dialog' };
}
