import { expect, request as pwRequest, test, type Page } from './support/fixtures';

import { BASE_URL, E2E_PASSPHRASE } from './support/env';

/**
 * P4.D177 §C.4 — the `memories` realtime topic, live (v4 `4a9be9878`, bug 128).
 *
 * ACTIVATE-AT-UNIFY behind {@link P4D175_SERVER_LANDED}: the topic + its
 * publish sites are P4.D175's. Named constant per §C.6, never a capability
 * probe.
 *
 * The "P4.D125 pushed-invalidation discriminator idiom"
 * (`page-toolbar-flow.spec.ts`'s `jobs` hint beat): watch the REAL dispatch
 * traffic, fire a REAL job that publishes the topic, and assert a fresh
 * `listChats` read follows PROMPTLY — well inside the realtime hub's slow
 * fallback-poll ceiling — proving the refetch was PUSHED, not polled.
 *
 * `memoryHousekeepSweep` (v4 `?action=housekeep-sweep`) is the safe trigger:
 * it enqueues `MEMORY_HOUSEKEEPING`, one of the two named collection-wide
 * publish sites (§C.4), and the seeded instance's default housekeeping
 * config (`perCharacterCap: 2000`, `mergeSimilar: false`) makes the sweep a
 * genuine no-op against the fixture's modest memory counts — nothing is
 * deleted or merged, so no sibling spec's memory-count assumptions move.
 * v5 has no per-chat memory count to assert a NEW VALUE against (the same
 * measured gap `realtime-topic-map.ts`'s `memories` case records), so this
 * beat proves the REFETCH happened rather than a badge number changing.
 */

const P4D175_SERVER_LANDED = true;

async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  const chats = page.getByRole('heading', { name: 'Chats', exact: true });
  await expect(passphrase.or(chats).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
  }
}

test.describe('P4.D177 §C.4 — the memories realtime topic', () => {
  test('a memory-housekeeping sweep completing pushes a fresh listChats read, polling parked', async ({
    page,
  }) => {
    test.skip(
      !P4D175_SERVER_LANDED,
      'awaits P4.D175: the memories topic + its publish sites (job completion, the delete gate twins).',
    );
    test.setTimeout(30_000);

    await page.goto('/salon');
    await maybeUnlock(page);
    await expect(page.getByRole('heading', { name: 'Chats', exact: true })).toBeVisible();

    // Count `listChats` dispatch calls as they happen — the discriminator.
    let listChatsCalls = 0;
    page.on('request', (req) => {
      if (!req.url().endsWith('/api/dispatch') || req.method() !== 'POST') return;
      const body = req.postDataJSON() as { type?: string } | null;
      if (body?.type === 'listChats') listChatsCalls += 1;
    });

    // Let the initial load's own listChats settle before taking the baseline.
    await page.waitForTimeout(500);
    const baseline = listChatsCalls;

    // Enqueue the sweep against the REAL server, outside the page's own fetch
    // machinery (a raw request context, matching the `jobs` beat's precedent).
    const ctx = await pwRequest.newContext();
    const posted = await ctx.post(`${BASE_URL}/api/dispatch`, {
      data: { type: 'memoryHousekeepSweep' },
    });
    expect(posted.ok(), `memoryHousekeepSweep → ${posted.status()}`).toBe(true);
    const { data } = (await posted.json()) as { data?: { jobId?: string } };
    const jobId = data?.jobId;
    expect(jobId).toBeTruthy();

    // Poll the job to completion — a no-op sweep over the fixture's modest
    // memory counts finishes in well under a second.
    let completed = false;
    for (let i = 0; i < 20; i += 1) {
      const statusResp = await ctx.post(`${BASE_URL}/api/dispatch`, {
        data: { type: 'systemJobGet', jobId },
      });
      const statusBody = (await statusResp.json()) as { data?: { job?: { status?: string } } };
      if (statusBody.data?.job?.status === 'COMPLETED') {
        completed = true;
        break;
      }
      await page.waitForTimeout(250);
    }
    expect(completed, 'the housekeeping sweep never reached COMPLETED').toBe(true);
    await ctx.dispose();

    // The push arrives promptly — well inside the realtime hub's slow
    // fallback-poll ceiling (the adaptive 1.5 s/8 s cadence P4.D123 records),
    // so a fresh listChats this soon can only be the pushed invalidation.
    await expect
      .poll(() => listChatsCalls, { timeout: 5_000, message: 'no fresh listChats after the sweep' })
      .toBeGreaterThan(baseline);
  });
});
