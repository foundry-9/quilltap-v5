import { readFileSync } from 'node:fs';

import { expect, test, type Page } from './support/fixtures';

import { E2E_PASSPHRASE } from './support/env';

/**
 * ORDERING: this rides the SHARED global-setup server and only unlocks it if the
 * gate is showing, so its filename must sort AFTER foundation.spec.ts (which
 * walks the locked→unlock gate first; workers: 1, alphabetical).
 * "settings-data-system-flow" sorts after "foundation" ('se' > 'fo').
 *
 * P4.9G2 — a LIVE walk of the newly fitted-out Settings → Data & System tab.
 * The runnable beats use verbs already on main (changePassphrase / lock /
 * unlockState / chatSettingsUpdate): the eight-card tab walk + a `?section=`
 * deep-link, a passphrase round-trip (changed to a temp value and BACK, so the
 * shared server's passphrase is left exactly as found), and the app-wide
 * auto-lock provider's idle warning under a Playwright fake clock.
 *
 * ACTIVATED AT UNIFICATION (2026-07-24): P4.9G1 landed the tasks-queue family,
 * so the Tasks Queue beat now runs LIVE against `systemTasksQueue`, and the
 * Import/Export beat runs its client-side step-1 walk. The lane's original
 * `systemTasksQueue` probe was RETIRED as too coarse: G1 DEFINED all sixteen
 * §1 variants but only IMPLEMENTED the tasks-queue family, so the probe passes
 * for verbs that still answer the loud "recognized but not yet available"
 * refusal. The one genuinely-blocked beat is gated by name below instead.
 */

const TEMP_PASSPHRASE = 'a temporary interlude';

/**
 * The delete-all beat and its `DELETE_ALL_SERVER_LANDED` gate MOVED to
 * `zz-delete-all-destructive.spec.ts` when P4.9G3 landed the server family
 * (2026-07-24). It wipes the SHARED instance, and eight spec files sort after
 * this one — the beat's old "runs last" premise was never true. The `zz-`
 * filename is load-bearing. (P4.9G3 owned this block; nothing else moved.)
 */

/** Unlock only when the passphrase screen is showing (the shared server stays unlocked). */
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

test.describe('P4.9G2 — the Data & System tab', () => {
  test("the tab renders v4's card order (no Plugins) and honours a ?section= deep link", async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/settings?tab=system');

    for (const title of [
      'Encryption Passphrase',
      'Auto-Lock',
      'Backup & Restore',
      'Import / Export',
      'LLM Logging',
      'Tasks Queue',
      'LLM Logs',
      'Delete All Data',
    ]) {
      await expect(page.getByText(title, { exact: true }).first()).toBeVisible({ timeout: 15_000 });
    }

    // The WON'T-PORT Plugins card renders nothing.
    await expect(page.getByText('Plugins', { exact: true })).toHaveCount(0);
    // The placeholder is gone.
    await expect(page.getByText('not yet fitted out')).toHaveCount(0);

    // A ?section= deep link force-opens its card: the Auto-Lock body appears.
    await page.goto('/settings?tab=system&section=auto-lock');
    await expect(
      page
        .getByText('Automatically lock after idle period')
        .or(page.getByText('Auto-lock requires a passphrase')),
    ).toBeVisible({ timeout: 15_000 });
  });

  test('passphrase round-trip: change to a temp value and back (state restored)', async ({
    page,
  }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/settings?tab=system&section=encryption-passphrase');

    const current = page.locator('#cp-current');
    const next = page.locator('#cp-new');
    const confirm = page.locator('#cp-confirm');
    await expect(current).toBeVisible({ timeout: 15_000 });

    // E2E_PASSPHRASE → TEMP_PASSPHRASE.
    await current.fill(E2E_PASSPHRASE);
    await next.fill(TEMP_PASSPHRASE);
    await confirm.fill(TEMP_PASSPHRASE);
    await page.getByRole('button', { name: 'Change Passphrase' }).click();
    await expect(page.getByText('Passphrase changed successfully.')).toBeVisible({
      timeout: 15_000,
    });

    // TEMP_PASSPHRASE → E2E_PASSPHRASE (restore so later specs' unlock still works).
    await current.fill(TEMP_PASSPHRASE);
    await next.fill(E2E_PASSPHRASE);
    await confirm.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Change Passphrase' }).click();
    await expect(page.getByText('Passphrase changed successfully.')).toBeVisible({
      timeout: 15_000,
    });
  });

  test('auto-lock: the provider warns after the idle threshold (fake clock)', async ({ page }) => {
    // Install a controllable clock BEFORE the app boots so the provider's
    // setInterval + Date.now ride it.
    await page.clock.install();
    await page.goto('/salon');
    await maybeUnlock(page);

    // idleMinutes 2 ⇒ warning at 1 min, lock at 2 min. Enable via the card.
    //
    // ⚠ The provider learns each save via its OWN unlockState re-fetch over the
    // real network, while fastForward's jump reschedules the ONE due idle tick
    // to fire at the jump target — and after that tick no further fake time
    // ever advances. So the tick checks whatever config the provider holds at
    // that instant: if a re-fetch is still in flight, the tick sees a stale
    // config (disabled, or the wrong minutes), returns silently, and the
    // warning can never appear (the P4.26 / 2026-07-31 full-suite flake,
    // reproduced deterministically with a 1.5 s unlockState delay). Each save
    // below therefore awaits the provider's matching re-fetch RESPONSE before
    // the beat moves on — which also serializes the two saves' re-fetches, so
    // a late first response cannot overwrite the second's config.
    await page.goto('/settings?tab=system&section=auto-lock');
    const toggle = page.locator('qt-auto-lock-settings-card input[type="checkbox"]');
    await expect(toggle).toBeVisible({ timeout: 15_000 });
    if (!(await toggle.isChecked())) {
      // Enabling saves the bag; the provider re-fetches and, enabled, the
      // response carries a NUMERIC autoLockMinutes (null while disabled).
      const enabledSeen = page.waitForResponse(
        async (r) => {
          if (!r.url().includes('/api/dispatch')) return false;
          if (!(r.request().postData() ?? '').includes('unlockState')) return false;
          const text = await r.text().catch(() => '');
          return /"autoLockMinutes":\d/.test(text);
        },
        { timeout: 15_000 },
      );
      await toggle.check();
      await enabledSeen;
    }
    const minutes = page.locator('qt-auto-lock-settings-card input[type="number"]');
    await expect(minutes).toBeVisible({ timeout: 15_000 });
    const savedTwo = page.waitForResponse(
      (r) =>
        r.url().includes('/api/dispatch') &&
        (r.request().postData() ?? '').includes('autoLockSettings') &&
        (r.request().postData() ?? '').includes('"idleMinutes":2'),
    );
    const providerRefetched = page.waitForResponse(
      async (r) => {
        if (!r.url().includes('/api/dispatch')) return false;
        if (!(r.request().postData() ?? '').includes('unlockState')) return false;
        const text = await r.text().catch(() => '');
        return text.includes('"autoLockMinutes":2');
      },
      { timeout: 15_000 },
    );
    await minutes.fill('2');
    await minutes.blur();
    await savedTwo;
    await providerRefetched;

    // Advance past the 1-minute warning threshold (but short of the 2-minute
    // lock) with no interaction; the 60 s idle check fires the warning banner.
    await page.clock.fastForward('01:05');
    await expect(page.getByText('Auto-Lock Warning')).toBeVisible({ timeout: 15_000 });

    // Restore: disable auto-lock so no later spec inherits the idle timer.
    const savedOff = page.waitForResponse(
      (r) =>
        r.url().includes('/api/dispatch') &&
        (r.request().postData() ?? '').includes('autoLockSettings') &&
        (r.request().postData() ?? '').includes('"enabled":false'),
    );
    await page.getByRole('button', { name: 'Dismiss' }).click();
    await toggle.uncheck();
    await savedOff;
  });

  // === ACTIVATED AT UNIFICATION over P4.9G1's landed tasks-queue family ===

  test('tasks queue: the card renders live server state', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/settings?tab=system&section=tasks-queue');
    await expect(page.getByText('Simultaneous Labours', { exact: false })).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByText('Queue Items', { exact: true })).toBeVisible({ timeout: 15_000 });
    // The stat tiles and the processor pill render ONLY under `@if (data(); as d)`
    // — they appear only once the live `systemTasksQueue` verb has answered. That
    // makes these the assertions that prove P4.9G1's server surface rather than
    // static chrome. The tile labels are their own leaf elements (exact match);
    // the pill's text node shares a span with the status dot, so it is matched
    // UNANCHORED — Playwright's regex matching is not whitespace-normalized.
    await expect(page.getByText('Active Jobs', { exact: true })).toBeVisible({ timeout: 15_000 });
    await expect(page.getByText('Est. Tokens', { exact: true })).toBeVisible();
    await expect(page.getByText(/Queue (Running|Stopped)/).first()).toBeVisible();
  });

  /**
   * P4.9G4: a LIVE `.qtap` export download, then that same file fed straight
   * back into the Import dialog's preview.
   *
   * The export half drives the dialog end-to-end and captures the real browser
   * download, so the whole chain is under test: the SPA's `fetch` → the
   * `?action=export` web-edge leg → the NDJSON writer → the `Content-Disposition`
   * filename. The import half re-uploads the downloaded bytes and asserts the
   * live `?action=import-preview` leg answered — the reader (format sniff →
   * line parse → reassembly) and `previewImport` both ran on the server.
   *
   * ACTIVATE-AT-UNIFY (P4.9G4-resumed): the walk now goes all the way through
   * Import. The execute half below has **never been executed** — this lane
   * touched no other `apps/web` file and owed no Playwright run, so P4.9E2B's
   * run is its first. Two gesture traps this dialog has already sprung, both
   * already accounted for above: the header X also carries `aria-label="Close"`
   * (target `[qt-modal-footer]`), and step 1 only STAGES the file — `Next` is
   * what fires the request.
   */
  test('export → import round-trip', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/settings?tab=system&section=import-export');

    // ── Export ────────────────────────────────────────────────────────────────
    await page.getByRole('button', { name: 'Export Data' }).click();
    await expect(page.getByText('Select the type of data you want to export')).toBeVisible({
      timeout: 15_000,
    });
    // Tags: the smallest type, and the only one whose flow skips the options
    // step (`showExportButton()` is true on `select` for non-character/chat
    // types), so the walk is two clicks.
    await page.getByRole('radio', { name: 'Tags', exact: true }).check();
    await page.getByRole('button', { name: 'Next' }).click();
    await expect(page.getByText('Export All')).toBeVisible();

    const downloadPromise = page.waitForEvent('download');
    await page.getByRole('button', { name: 'Export', exact: true }).click();
    const download = await downloadPromise;
    await expect(page.getByText('Export Complete')).toBeVisible({ timeout: 15_000 });

    // The downloaded bytes ARE the server's NDJSON: line 1 must be the envelope.
    const qtapPath = await download.path();
    expect(qtapPath).toBeTruthy();
    const qtap = readFileSync(qtapPath!, 'utf8');
    const firstLine = JSON.parse(qtap.split('\n')[0]) as {
      kind: string;
      format: string;
      version: number;
      manifest: { exportType: string; format: string; version: string };
    };
    expect(firstLine.kind).toBe('__envelope__');
    expect(firstLine.format).toBe('qtap-ndjson');
    expect(firstLine.version).toBe(1);
    expect(firstLine.manifest.exportType).toBe('tags');
    expect(firstLine.manifest.format).toBe('quilltap-export');
    expect(firstLine.manifest.version).toBe('1.0');
    // The last non-empty line is the footer carrying the authoritative counts.
    const lines = qtap.split('\n').filter((l) => l.length > 0);
    const footer = JSON.parse(lines[lines.length - 1]) as { kind: string };
    expect(footer.kind).toBe('__footer__');

    // The dialog's header X carries `aria-label="Close"` too, so the role
    // locator is ambiguous — target the FOOTER button explicitly.
    await page.locator('[qt-modal-footer] button:has-text("Close")').click();

    // ── Import (preview only) ────────────────────────────────────────────────
    await page.getByRole('button', { name: 'Import Data' }).click();
    await page.locator('input[type="file"]').setInputFiles({
      name: download.suggestedFilename(),
      mimeType: 'application/x-ndjson',
      buffer: Buffer.from(qtap, 'utf8'),
    });

    // Step 1 only STAGES the file; `Next` is what fires the preview request.
    await expect(page.getByText(download.suggestedFilename(), { exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'Next' }).click();

    // The preview step renders only under `@if (preview(); as p)`, so these
    // assertions can only pass once the live `?action=import-preview` leg has
    // answered. `Export Type` carries the manifest the SERVER reassembled from
    // the uploaded bytes, and every tag in this file already exists here (same
    // instance) — so the `Exists` chip is the proof `previewImport` ran its
    // conflict test against the real database.
    await expect(page.getByText('Export Type', { exact: true })).toBeVisible({ timeout: 15_000 });
    await expect(page.getByText('tags', { exact: true })).toBeVisible();
    await expect(page.getByText('Exists', { exact: true }).first()).toBeVisible();

    // ── Import (execute) ─────────────────────────────────────────────────────
    // The options step, then the write. `skip` is the default strategy and the
    // safe one to walk here: every tag in this file already exists (the export
    // came from this same instance moments ago), so a successful import must
    // report them SKIPPED and create nothing — which is also what makes the
    // beat re-runnable without accumulating rows.
    await page.getByRole('button', { name: 'Next' }).click();
    await expect(page.getByText('When an item already exists:')).toBeVisible();

    await page.getByRole('button', { name: 'Import', exact: true }).click();

    // `Import Complete` renders only under `@case ('complete')` with a `result()`
    // present, so it can only appear once the live `?action=import-execute` leg
    // answered — the whole `executeImport` pipeline ran on the server. The
    // skipped row is the proof it ran the CONFLICT path rather than importing
    // duplicates.
    await expect(page.getByText('Import Complete')).toBeVisible({ timeout: 30_000 });
    await expect(page.getByText('tags (skipped)', { exact: true })).toBeVisible();
  });

  /**
   * ADDED AT UNIFICATION over P4.9G5's landed backup half.
   *
   * P4.9G5 deliberately shipped no e2e beat (writing one obliged a full
   * Playwright run the lane could not honestly afford), but its own status
   * header records that `backup create → download` would walk today. This is
   * that walk, and it is the only place the two halves meet: the SPA card that
   * has been sitting on main since P4.9G2 unit 7, and the server family that
   * landed this round.
   *
   * The chain under test is the whole thing — `systemBackupCreate` (collect →
   * manifest → 38-file staging tree → zip → the single-use temp store), then
   * `triggerUrlDownload` against the byte leg `GET /api/v1/system/backup/{id}`.
   * Capturing a real browser download is what proves the leg streamed actual
   * archive bytes rather than an error envelope.
   *
   * RESTORE is P4.9G5's open remainder (units 3–5), so this beat stops at the
   * backup half and never opens the Restore dialog.
   */
  test('backup: create → the single-use zip download streams real bytes', async ({ page }) => {
    await page.goto('/salon');
    await maybeUnlock(page);
    await page.goto('/settings?tab=system&section=backup-restore');

    await page.getByRole('button', { name: 'Create Backup', exact: true }).click();
    await expect(page.getByText('Create a complete backup of your Quilltap data')).toBeVisible({
      timeout: 15_000,
    });

    const downloadPromise = page.waitForEvent('download');
    await page.getByRole('button', { name: 'Download Backup', exact: true }).click();
    const download = await downloadPromise;

    // The create response carries no `filename`, so the client fallback always
    // runs — asserting the shape pins that documented P4.9G2 behaviour too.
    expect(download.suggestedFilename()).toMatch(/^quilltap-backup-.*\.zip$/);

    // A zip local-file-header magic ("PK\x03\x04") is the cheapest proof the
    // byte leg streamed the archive rather than a JSON error body.
    const zipPath = await download.path();
    expect(zipPath).toBeTruthy();
    const head = readFileSync(zipPath!).subarray(0, 4);
    expect([...head]).toEqual([0x50, 0x4b, 0x03, 0x04]);
  });
});
