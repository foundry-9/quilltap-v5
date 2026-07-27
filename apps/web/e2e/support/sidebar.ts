import { expect, type Page } from '@playwright/test';

/**
 * The Salon chat sidebar (P4.9H1, v4 `components/chat/ChatSidebar.tsx`) opens
 * collapsed to its mini strip unless the operator has expanded it before — the
 * preference lives in `quilltap.chat-sidebar.collapsed`, and a fresh e2e profile
 * has no such key. Its drawers are a single-open accordion.
 *
 * Beats that drive a control inside the sidebar go through here: expand if
 * collapsed, then open the named card if it isn't already open. Idempotent.
 */
export type SidebarSection =
  | 'Participants'
  | 'Chat'
  | 'Visibility'
  | 'Organize'
  | 'Edit Content';

export async function openSidebarSection(page: Page, section: SidebarSection): Promise<void> {
  // The sidebar only renders once the chat record has loaded, so a beat that
  // navigates and calls straight in would otherwise probe an empty page.
  const sidebar = page.locator('qt-chat-sidebar');
  await expect(sidebar).toBeVisible({ timeout: 15_000 });

  const expand = page.getByRole('button', { name: 'Expand chat sidebar' });
  if (await expand.count()) {
    await expand.click();
  }
  const collapse = page.getByRole('button', { name: 'Collapse chat sidebar' });
  await expect(collapse).toBeVisible({ timeout: 15_000 });

  const header = page
    .locator('qt-chat-sidebar .qt-collapsible-card-header')
    .filter({ hasText: section })
    .first();
  await expect(header).toBeVisible({ timeout: 10_000 });
  if ((await header.getAttribute('aria-expanded')) !== 'true') {
    await header.click();
    await expect(header).toHaveAttribute('aria-expanded', 'true');
  }
}
