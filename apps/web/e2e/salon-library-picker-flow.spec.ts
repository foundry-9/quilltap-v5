import { expect, request as pwRequest, test, type Page } from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * P4.9E4B — the Library file picker, opened from the composer gutter.
 *
 * ORDERING: rides the shared global-setup server, so the filename must sort after
 * `foundation.spec.ts` ('sa' > 'fo'), which walks the locked→unlock gate.
 *
 * **What runs live in-lane.** The LEGACY leg — `POST …/files?action=link` — has
 * been on main since P4.6ah, so the "General → pick a file → it lands in the
 * composer tray" walk runs live here, against the real binary.
 *
 * **What does not.** The store leg — `POST …/files?action=attach-mount-file` — is
 * the SIBLING lane P4.9E4A's deliverable and does not exist on main while this
 * lane runs. Its beat is gated behind the named constant below, `false` on the
 * branch: the unifier flips it to `true` and the walk runs with no other edit.
 */

/** ACTIVATED at the 2026-07-27 unification: P4.9E4A's attach-mount-file leg is on main. */
const ATTACH_MOUNT_FILE_LANDED = true;

const LIBRARY_FILE = 'e2e-library-note.txt';
const STORE_NAME = 'Library Picker E2E';
const STORE_FILE = 'picker-plan.md';

let storeId = '';

async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const chats = page.getByRole('heading', { name: 'Chats', exact: true });
  await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
    await expect(chats).toBeVisible({ timeout: 15_000 });
  }
}

async function openChat(page: Page, title: string): Promise<void> {
  await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();
  const card = page.locator('.chat-card-stack a.qt-entity-card', { hasText: title });
  await expect(card).toBeVisible();
  await card.click();
  await expect(page.locator('.qt-chat-messages-list')).toBeVisible();
}

/**
 * Seed one general-library file at the root, through the same REST upload leg
 * the Files page uses. Called AFTER the unlock — a dispatch against a locked
 * vault refuses, and a `beforeAll` seed would silently mis-skip in isolation.
 */
async function seedLibraryFile(): Promise<boolean> {
  const ctx = await pwRequest.newContext();
  try {
    const res = await ctx.post(`${BASE_URL}/api/v1/files?action=upload`, {
      multipart: {
        file: {
          name: LIBRARY_FILE,
          mimeType: 'text/plain',
          buffer: Buffer.from('A note the picker can reach for.'),
        },
        folderPath: '/',
      },
    });
    return res.ok();
  } catch {
    return false;
  } finally {
    await ctx.dispose();
  }
}

/** Seed a database document store with one markdown file in it. */
async function seedStore(): Promise<boolean> {
  if (storeId) return true;
  const ctx = await pwRequest.newContext();
  try {
    const res = await ctx.post(`${BASE_URL}/api/dispatch`, {
      data: {
        type: 'mountPointCreate',
        mountPoint: { name: STORE_NAME, mountType: 'database', storeType: 'documents' },
      },
    });
    const body = (await res.json().catch(() => null)) as {
      data?: { mountPoint?: { id?: string } };
    } | null;
    storeId = body?.data?.mountPoint?.id ?? '';
    if (!storeId) return false;
    const put = await ctx.put(`${BASE_URL}/api/v1/mount-points/${storeId}/files/${STORE_FILE}`, {
      multipart: {
        file: {
          name: STORE_FILE,
          mimeType: 'text/markdown',
          buffer: Buffer.from('# The plan\n\nAll of it, written down.\n'),
        },
      },
    });
    return put.ok();
  } catch {
    return false;
  } finally {
    await ctx.dispose();
  }
}

test.afterAll(async () => {
  // Leave the shared server's DB as we found it.
  if (!storeId) return;
  try {
    const ctx = await pwRequest.newContext();
    await ctx.post(`${BASE_URL}/api/dispatch`, {
      data: { type: 'mountPointDelete', mountPointId: storeId },
    });
    await ctx.dispose();
  } catch {
    /* best-effort cleanup */
  }
});

test.describe('P4.9E4B — the Library file picker', () => {
  test('the gutter offers "Attach file from library", and it opens the scope step', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await openChat(page, 'Solo Voyage');

    const actions = page.locator('.qt-chat-composer-actions');
    const entry = actions.getByRole('button', { name: 'Attach file from library' });
    await expect(entry).toBeVisible();
    await entry.click();

    // A dialog's component host is zero-sized — locate by role.
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible({ timeout: 10_000 });
    await expect(dialog.getByText('Choose File Source')).toBeVisible();
    await expect(dialog.getByText('Files not assigned to any project')).toBeVisible();
    // The gallery scope is always offered, named for the persona when there is one
    // (the fixture chat has one, so both the title and the subtitle say "Photos" —
    // locate the CARD, not the text, or strict mode sees two nodes).
    await expect(dialog.getByRole('button', { name: /Photos/ })).toBeVisible();

    await dialog.locator('[qt-modal-footer]').getByRole('button', { name: 'Cancel' }).click();
    await expect(dialog).toBeHidden({ timeout: 10_000 });
  });

  test('General → pick a file → it lands in the composer tray (the LIVE legacy leg)', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    const seeded = await seedLibraryFile();
    expect(seeded, 'the files upload leg is on main — the seed must succeed').toBe(true);

    await openChat(page, 'Solo Voyage');
    await page.getByRole('button', { name: 'Attach file from library' }).click();

    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible({ timeout: 10_000 });
    await dialog.getByText('Files not assigned to any project').click();
    await expect(dialog.getByText('Browse Files — General')).toBeVisible({ timeout: 15_000 });

    await dialog.getByRole('button', { name: new RegExp(LIBRARY_FILE) }).first().click();

    // The dialog closes and the file is a chip above the composer, ready to send.
    await expect(dialog).toBeHidden({ timeout: 15_000 });
    await expect(page.locator('.qt-chat-attachment-chip', { hasText: LIBRARY_FILE })).toBeVisible({
      timeout: 15_000,
    });
    // v4's toast sentence, as the Salon's chat flash.
    await expect(page.getByText(`Linked "${LIBRARY_FILE}" to chat`)).toBeVisible();
  });

  test('a document store → pick a file → the Librarian announces it (ACTIVATE-AT-UNIFY)', async ({
    page,
  }) => {
    test.skip(
      !ATTACH_MOUNT_FILE_LANDED,
      'P4.9E4A’s attach-mount-file leg is not on main — flip ATTACH_MOUNT_FILE_LANDED at unification',
    );
    await page.goto('/salon');
    await maybeUnlock(page);
    const seeded = await seedStore();
    expect(seeded, 'the store seed must succeed').toBe(true);

    await openChat(page, 'Solo Voyage');
    await page.getByRole('button', { name: 'Attach file from library' }).click();

    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible({ timeout: 10_000 });
    await expect(dialog.getByText('Document Stores')).toBeVisible({ timeout: 15_000 });
    await dialog.getByText(STORE_NAME, { exact: true }).click();
    await expect(dialog.getByText(`Browse Files — ${STORE_NAME}`)).toBeVisible({ timeout: 15_000 });

    await dialog.getByRole('button', { name: new RegExp(STORE_FILE) }).first().click();
    await expect(dialog).toBeHidden({ timeout: 15_000 });

    // No composer chip — the announcement IS the hand-off.
    await expect(page.locator('.qt-chat-attachment-chip')).toHaveCount(0);
    await expect(
      page.getByText(`Attached "${STORE_FILE}" — the Librarian has noted it`),
    ).toBeVisible();
    // The Librarian's announcement is in the transcript, exactly once.
    await expect(
      page.locator('.qt-chat-messages-list').getByText(STORE_FILE, { exact: false }),
    ).toHaveCount(1, { timeout: 15_000 });
  });
});
