import {
  expect,
  request as pwRequest,
  test,
  type APIRequestContext,
  type Page,
} from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * P4.D147 — the Move-to-Project folder picker, LIVE.
 *
 * v4 bug 113 (`a00e18f0d`): the Folder dropdown listed "/ (Root)" and nothing
 * else for every destination, because the derived list was mirrored into
 * component state behind an "only if empty" guard that the still-loading first
 * render satisfied. What this walk proves — against the REAL server, the REAL
 * dispatch verbs and the REAL DOM — is the re-derivation: a destination's own
 * folders appear, they change when the destination does, a folder created
 * inline appears at once (the refetch), and the folder the operator picked is
 * the one the file ends up in.
 *
 * ORDERING: rides the shared global-setup server, so the filename must sort
 * AFTER foundation.spec.ts ('m' > 'f'; workers: 1, alphabetical).
 */

const ESTATE = 'D147 Estate';
const PLANS = 'D147 Plans';
const FILE_NAME = 'd147-ledger.txt';

async function dispatchResp(
  ctx: APIRequestContext,
  req: unknown,
): Promise<{ type?: string; data?: Record<string, unknown> }> {
  const res = await ctx.post(`${BASE_URL}/api/dispatch`, { data: req });
  return (
    ((await res.json().catch(() => null)) as {
      type?: string;
      data?: Record<string, unknown>;
    } | null) ?? {}
  );
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

/** Create a project and return its id (idempotent across re-runs by name). */
async function ensureProject(ctx: APIRequestContext, name: string): Promise<string> {
  const listed = await dispatchResp(ctx, { type: 'projectList' });
  const existing = ((listed.data?.['projects'] as { id: string; name: string }[]) ?? []).find(
    (p) => p.name === name,
  );
  if (existing) return existing.id;
  const created = await dispatchResp(ctx, { type: 'projectCreate', project: { name } });
  const project = created.data?.['project'] as { id: string } | undefined;
  if (!project) throw new Error(`projectCreate answered ${JSON.stringify(created)}`);
  return project.id;
}

test.describe('P4.D147 — the Move-to-Project folder picker', () => {
  test("lists a destination's real folders, re-derives on a change, and moves the file into the folder picked", async ({
    page,
  }) => {
    // Unlock FIRST: the shared server is passphrase-locked until a spec opens it,
    // and the seeding dispatches below need it open. (Unlock is server-side, so
    // the API context sees it too.) Running alone, this beat is the opener.
    await page.goto('/salon');
    await maybeUnlock(page);

    const ctx = await pwRequest.newContext();
    let estateId = '';
    let plansId = '';
    let fileId = '';
    try {
      // Two destinations with DIFFERENT folders — the discriminator for the
      // re-derivation. `/Foundry-9/Quilltap/` is deliberately NOT seeded: the
      // walk creates it through the dialog's own Create affordance, which is
      // also the refetch proof (a snapshot copy could not contain it).
      estateId = await ensureProject(ctx, ESTATE);
      plansId = await ensureProject(ctx, PLANS);
      await dispatchResp(ctx, { type: 'filesFolderCreate', path: '/Gary/', projectId: estateId });
      await dispatchResp(ctx, {
        type: 'filesFolderCreate',
        path: '/Foundry-9/',
        projectId: plansId,
      });

      const uploadRes = await ctx.post(`${BASE_URL}/api/v1/files?action=upload`, {
        multipart: {
          file: {
            name: FILE_NAME,
            mimeType: 'text/plain',
            buffer: Buffer.from('A ledger, seeded for the Move-to-Project walk.'),
          },
          folderPath: '/',
        },
      });
      expect(uploadRes.ok()).toBe(true);
      // The REST leg answers v4's `{data: FileEntry}` envelope.
      const uploaded = (await uploadRes.json()) as { data?: { id?: string }; id?: string };
      fileId = uploaded.data?.id ?? uploaded.id ?? '';
      expect(fileId).not.toBe('');
    } finally {
      await ctx.dispose();
    }

    await page.goto('/files');
    await expect(page.getByRole('heading', { name: 'General Files', exact: true })).toBeVisible({
      timeout: 15_000,
    });

    // Open the seeded file's preview, then Move to Project from its toolbar.
    const fileCard = page.getByRole('button', { name: FILE_NAME }).first();
    await expect(fileCard).toBeVisible({ timeout: 15_000 });
    await fileCard.click();
    const preview = page.getByRole('dialog');
    await expect(preview).toBeVisible({ timeout: 10_000 });
    await preview.getByTitle('Move to Project', { exact: true }).click();

    const dialog = page.getByRole('dialog').filter({ hasText: 'Select Destination' });
    await expect(dialog).toBeVisible({ timeout: 10_000 });
    // No destination chosen yet — no folder control at all (v4's own gating).
    await expect(dialog.locator('#move-folder')).toHaveCount(0);

    // Destination 1: the Estate. Its OWN folder appears — the bug's signature
    // was "/ (Root)", alone, forever.
    await dialog.locator('#move-to-project').selectOption(estateId);
    const folder = dialog.locator('#move-folder');
    await expect(folder).toBeVisible({ timeout: 10_000 });
    await expect(folder.locator('option')).toHaveText([/\/ \(Root\)/, /└ Gary/], {
      timeout: 10_000,
    });

    // Destination 2: the Plans. The list re-derives — Gary is gone.
    await dialog.locator('#move-to-project').selectOption(plansId);
    await expect(folder.locator('option')).toHaveText([/\/ \(Root\)/, /└ Foundry-9/], {
      timeout: 10_000,
    });

    // Create a nested folder inline: it must appear at once (the refetch) and
    // indented by two NON-BREAKING spaces, since an <option> collapses ordinary
    // ones.
    await dialog.getByTitle('Create new folder').click();
    await dialog.getByPlaceholder('/path/to/folder/').fill('/Foundry-9/Quilltap/');
    await dialog.getByRole('button', { name: 'Create', exact: true }).click();

    const nested = folder.locator('option', { hasText: 'Quilltap' });
    await expect(nested).toHaveCount(1, { timeout: 10_000 });
    expect(await nested.textContent()).toContain('\u00a0\u00a0\u2514 Quilltap');
    // The create moved the selection onto the new folder (v4 `:196`).
    await expect(folder).toHaveValue('/Foundry-9/Quilltap/');

    await dialog.getByRole('button', { name: 'Move to Project', exact: true }).click();
    await expect(page.getByText(`"${FILE_NAME}" moved to ${PLANS}`)).toBeVisible({
      timeout: 15_000,
    });

    // The persisted row carries the folder the picker selected.
    const verify = await pwRequest.newContext();
    try {
      const listed = await dispatchResp(verify, { type: 'filesList', projectId: plansId });
      const row = (
        (listed.data?.['files'] as { id: string; folderPath: string | null }[]) ?? []
      ).find((f) => f.id === fileId);
      expect(row?.folderPath).toBe('/Foundry-9/Quilltap/');
    } finally {
      await verify.dispose();
    }
  });
});
