import {
  expect,
  request as pwRequest,
  test,
  type APIRequestContext,
  type Page,
} from '@playwright/test';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING + NAMING: this file rides the SHARED global-setup server and unlocks
 * it, so its filename must sort AFTER foundation.spec.ts (foundation walks the
 * locked→unlock gate and must reach the shared server first; workers: 1,
 * alphabetical). The P4.6af order names this `files-flow.spec.ts`, but "files"
 * sorts BEFORE "foundation" ('fi' < 'fo') — which would run this before the gate
 * walk and break it. Renamed to `general-files-flow.spec.ts` ('ge' > 'fo') to
 * satisfy the binding ordering rule; the rename is reported at unification.
 *
 * P4.6af lane B — the general Files walk. The screen-render beat is LIVE (the
 * /files screen is this lane's). The DATA beats depend on lane A's files-family
 * dispatch variants + the `?action=upload` REST leg, which are NOT live in this
 * worktree, so they PROBE-GUARD on `filesList` and skip cleanly in-lane — they
 * SELF-ACTIVATE at unification once lane A lands (the P4.6ac precedent).
 */

const FOLDER = '/e2e-files/';
const FILE_NAME = 'e2e-note.txt';

async function dispatchResp(
  ctx: APIRequestContext,
  req: unknown,
): Promise<{ type?: string; data?: Record<string, unknown> }> {
  const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: req });
  return ((await res.json().catch(() => null)) as {
    type?: string;
    data?: Record<string, unknown>;
  } | null) ?? {};
}

/** Whether lane A's files-family variants answer (else the data beats skip). */
async function filesVariantsLive(ctx: APIRequestContext): Promise<boolean> {
  const resp = await dispatchResp(ctx, { type: 'filesList', filter: 'general' });
  return resp.type !== 'error' && resp.type !== undefined;
}

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

test.describe('P4.6af — the general Files page', () => {
  test('the /files screen renders (General Files, no upload affordance)', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);

    await page.goto('/files');
    await expect(page.getByRole('heading', { name: 'General Files', exact: true })).toBeVisible({
      timeout: 15_000,
    });
    // v4 parity: NO upload affordance on the general Files page.
    await expect(page.locator('input[type=file]')).toHaveCount(0);
    await expect(page.getByTitle('Upload Files')).toHaveCount(0);
    // The toolbar + breadcrumb root are present (glyph buttons — locate by title).
    await expect(page.getByTitle('New Folder')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Root', exact: true })).toBeVisible();
  });

  test('browse a seeded folder + file, then open the preview', async ({ page }) => {
    const ctx = await pwRequest.newContext();
    let live = false;
    try {
      // Unlock via the API context path is implicit (the shared server is already
      // unlocked by the time this sorts in); probe the files variants.
      live = await filesVariantsLive(ctx);
      if (live) {
        await dispatchResp(ctx, { type: 'filesFolderCreate', path: FOLDER });
        // Seed a general file through lane A's REST upload leg (multipart).
        await ctx.post(`${BASE_URL}/api/v1/files?action=upload`, {
          multipart: {
            file: {
              name: FILE_NAME,
              mimeType: 'text/plain',
              buffer: Buffer.from('A note, seeded for the e2e walk.'),
            },
            folderPath: FOLDER,
          },
        });
      }
    } finally {
      await ctx.dispose();
    }
    test.skip(!live, 'lane A files-family variants not live in-worktree — self-activates at unification');

    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/files');
    await expect(page.getByRole('heading', { name: 'General Files', exact: true })).toBeVisible({
      timeout: 15_000,
    });

    // The seeded folder shows as a subfolder card; enter it.
    await page.getByRole('button', { name: /e2e-files/ }).first().click();
    // The seeded file appears inside; open its preview.
    const fileButton = page.getByRole('button', { name: FILE_NAME }).first();
    await expect(fileButton).toBeVisible({ timeout: 15_000 });
    await fileButton.click();

    // The preview lightbox opens (a text file → the copy button + content).
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible({ timeout: 10_000 });
    await expect(dialog.getByText(FILE_NAME)).toBeVisible();
    // Close via Escape (the keyboard handler).
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden({ timeout: 10_000 });
  });
});
