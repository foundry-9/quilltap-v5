import {
  expect,
  request as pwRequest,
  test,
  type APIRequestContext,
  type Page,
  type Route,
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
        // The leg is P4.6ae unit 4 (OPEN at the P4.6ae∥af∥ag unification) — the
        // guard covers it too, so this beat self-activates when the leg lands
        // rather than failing on a 404 seed.
        const uploadRes = await ctx.post(`${BASE_URL}/api/v1/files?action=upload`, {
          multipart: {
            file: {
              name: FILE_NAME,
              mimeType: 'text/plain',
              buffer: Buffer.from('A note, seeded for the e2e walk.'),
            },
            folderPath: FOLDER,
          },
        });
        live = uploadRes.ok();
      }
    } finally {
      await ctx.dispose();
    }
    test.skip(!live, 'lane A files-family variants + upload REST leg not live — self-activates when P4.6ae unit 4 lands');

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

/**
 * P4.6aj — the delete-associations dialog end-to-end.
 *
 * WHY ROUTE-MOCKED (not lane A's fixture): v4's `serializeFileEntry`
 * (app/api/v1/files/shared.ts) omits `linkedTo`, so the general-files LIST never
 * reveals which file is linked — a self-activating beat could not find the
 * seeded associations-arm file without a DESTRUCTIVE delete-probe over the shared
 * fixture. The order explicitly sanctions mocking the FILE_HAS_ASSOCIATIONS
 * response, so this beat intercepts the `/api/dispatch` files reads + the
 * `fileDelete` legs and drives the SPA's real HTTP client + real dialog DOM +
 * real dissociate resend against the pinned envelope shape. It runs GREEN in-lane
 * (no dependency on lanes A/B), verifying the wiring the unit specs assert, but in
 * a real browser over the wire. The unlock still hits the real server; the mock
 * is installed AFTER unlock, scoped to the files dispatches only.
 */
const LINKED_FILE = {
  id: 'aj-linked-1',
  userId: 'u',
  originalFilename: 'shared-portrait.txt',
  filename: 'shared-portrait.txt',
  mimeType: 'text/plain', // text, so the thumbnail-batch effect stays silent
  size: 24,
  category: 'general',
  description: null,
  projectId: null,
  folderPath: '/',
  filepath: '/api/v1/files/aj-linked-1',
  fileStatus: 'ok',
  createdAt: '2026-01-01T00:00:00.000Z',
  updatedAt: '2026-01-01T00:00:00.000Z',
};

const ASSOCIATIONS = {
  characters: [{ id: 'c1', name: 'Lorian', usage: 'avatar' }],
  messages: [{ chatId: 'ch1', chatName: 'A Midnight Correspondence', messageId: 'm1' }],
};

test.describe('P4.6aj — the delete-associations dialog', () => {
  test('bare delete surfaces the itemized dialog; "Delete Anyway" dissociates', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);

    // Auto-accept the browser confirm() the bare delete opens.
    page.on('dialog', (d) => void d.accept());

    // Closure state: the file vanishes only after the dissociate leg succeeds.
    let dissociated = false;
    await page.route('**/api/dispatch', async (route: Route) => {
      const req = (route.request().postDataJSON() ?? {}) as { type?: string; dissociate?: boolean };
      const json = (body: unknown) =>
        route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
      switch (req.type) {
        case 'filesList':
          return json({ type: 'data', data: { files: dissociated ? [] : [LINKED_FILE] } });
        case 'filesFoldersList':
          return json({ type: 'data', data: { folders: [] } });
        case 'fileDelete':
          if (req.dissociate) {
            dissociated = true;
            return json({ type: 'ack', data: {} });
          }
          // The pinned FILE_HAS_ASSOCIATIONS 400 envelope (top-level associations).
          return json({
            type: 'error',
            data: {
              kind: 'bad-request',
              message: 'File is linked to other items',
              associations: ASSOCIATIONS,
            },
          });
        default:
          return route.continue();
      }
    });

    await page.goto('/files');
    await expect(page.getByRole('heading', { name: 'General Files', exact: true })).toBeVisible({
      timeout: 15_000,
    });

    // Open the seeded file's preview (root folder → the card shows immediately).
    const fileCard = page.getByRole('button', { name: LINKED_FILE.filename }).first();
    await expect(fileCard).toBeVisible({ timeout: 15_000 });
    await fileCard.click();

    // Delete from the preview's stable (always-visible) Delete button.
    const preview = page.getByRole('dialog');
    await expect(preview).toBeVisible();
    await preview.getByTitle('Delete', { exact: true }).click();

    // The associations dialog appears with the itemized characters + chats.
    const assoc = page.getByRole('dialog').filter({ hasText: 'This file is in use' });
    await expect(assoc).toBeVisible({ timeout: 10_000 });
    await expect(assoc).toContainText('Lorian');
    await expect(assoc).toContainText('A Midnight Correspondence');

    // "Delete Anyway" fires the dissociate resend; the dialog closes and the file
    // disappears from the refetched list.
    await assoc.getByRole('button', { name: 'Delete Anyway' }).click();
    await expect(page.getByRole('dialog').filter({ hasText: 'This file is in use' })).toHaveCount(0, {
      timeout: 10_000,
    });
    await expect(page.getByRole('button', { name: LINKED_FILE.filename })).toHaveCount(0, {
      timeout: 10_000,
    });
  });
});
