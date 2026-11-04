import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { copyFileSync, mkdirSync, openSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { expect, test, type Page } from './support/fixtures';

import { makeDbKeyFile } from './support/dbkey';
import {
  ARTIFACTS_DIR,
  cliBinary,
  E2E_PASSPHRASE,
  FIXTURE_USER,
  FIXTURES_DIR,
  REPO_ROOT,
  SINGLE_USER_ID,
  spaDir,
  TEST_PEPPER,
  webBinary,
} from './support/env';
import { startMockLlm, type MockLlm } from './support/mock-llm';

/**
 * P4.D359 — the SPA riders of the `236c04e35` round (item 18).
 *
 * Its OWN instance on port 4358 (mock LLM 45356 — the round's §R.9 ports for
 * this lane), over the `characters-main.db` / `characters-mount.db` pair with
 * `instance_settings`, `projects`, `groups`, `chat_settings` and
 * `group_doc_mount_links` materialized from the committed D23 dump (the
 * `inventory-flow.spec.ts` recipe, verbatim) and the shared llm-logs fixture
 * copied in for the Inspector beat. The mock answers with a line that NAMES
 * Bram twice — a workspace character not seated in Solo Voyage (Aria and Cleo
 * are) — for the chips beat (d).
 *
 * **LIVE beats** need only this branch: (a) the always-on launcher and the
 * floating console over a `qt-modal`; (b) the tab and the floating console on
 * two conversations; (e) the About credits; (f) the Inspector's derived Other
 * filter and the new badges; (j) the templates card's default surviving a
 * tab round trip with no reload.
 *
 * **GATED beats** — flipped at unification (§S.9). NAMED constants, never a
 * capability probe; each gated beat calls `test.skip(…)` first.
 */
// ACTIVATE-AT-UNIFY: P4.D341's keystone column / admission / API (C8).
const P4D341_LANDED = true;
// ACTIVATE-AT-UNIFY: P4.D347's `chatAbsentMentions` arm (C5) — AND P4.D344 applying this lane's
// `handoffs/p4.d359-salon-conversation.patch` (the Salon's AbsentMentionsSlot).
const P4D347_SERVER_LANDED = true;
// ACTIVATE-AT-UNIFY: P4.D349's vault keys (C6 — `humanoid`, `pronounGuidance`).
const P4D349_SERVER_LANDED = true;
// ACTIVATE-AT-UNIFY: P4.D350's R-1 location-level `inventory` publish.
const P4D350_PUBLISH_LANDED = true;
// ACTIVATE-AT-UNIFY: P4.D358's `qt-modal` Escape (R-N) carrying this lane's floating-layer guard.
const P4D358_MODAL_ESCAPE_LANDED = true;

const PORT = 4358;
const BASE_URL = `http://127.0.0.1:${PORT}`;
const MOCK_LLM_PORT = 45356;
const INSTANCE_DIR = resolve(ARTIFACTS_DIR, 'spa-riders-instance');
const DATA_DIR = resolve(INSTANCE_DIR, 'data');
const SERVER_LOG = resolve(ARTIFACTS_DIR, 'spa-riders-server.log');

const SOLO_VOYAGE_CHAT_ID = 'c1000000-0000-4000-8000-000000000001';
// The pair's characters: Aria (seated in Solo Voyage with Cleo), Bram and Dax (seated nowhere there).
const ARIA_ID = 'a1000000-0000-4000-8000-000000000001';
const BRAM_ID = 'a1000000-0000-4000-8000-000000000002';
const DAX_ID = 'a1000000-0000-4000-8000-000000000004';
/** The mock's reply: names Bram — absent from Solo Voyage — twice; only the first is chipped. */
const MOCK_REPLY = 'Bram would not approve. Bram never does.';
/** Beat (c)'s planted ordinary memories of Aria's (ids, summaries; content is the summary + '.'). */
const KEYSTONE_PLANTS = [
  ['e3590000-0000-4000-8000-0000000000c1', 'Aria was born aboard the airship Meridian'],
  ['e3590000-0000-4000-8000-0000000000c2', 'Aria cannot abide the smell of burnt toast'],
  ['e3590000-0000-4000-8000-0000000000c3', 'Aria keeps her grandmother compass'],
  ['e3590000-0000-4000-8000-0000000000c4', 'Aria swore never to fly over the Shattered Reach'],
] as const;

let server: ChildProcess | undefined;
let mock: MockLlm | undefined;

test.use({ baseURL: BASE_URL });

test.describe('P4.D359 — the SPA riders', () => {
  test.beforeAll(async () => {
    test.setTimeout(120_000);
    const web = webBinary();
    const cli = cliBinary();

    rmSync(INSTANCE_DIR, { recursive: true, force: true });
    mkdirSync(DATA_DIR, { recursive: true });
    mkdirSync(resolve(DATA_DIR, 'files'), { recursive: true });
    copyFileSync(resolve(FIXTURES_DIR, 'characters-main.db'), resolve(DATA_DIR, 'quilltap.db'));
    copyFileSync(
      resolve(FIXTURES_DIR, 'characters-mount.db'),
      resolve(DATA_DIR, 'quilltap-mount-index.db'),
    );
    copyFileSync(
      resolve(FIXTURES_DIR, 'salon-llm-logs.db'),
      resolve(DATA_DIR, 'quilltap-llm-logs.db'),
    );
    writeFileSync(resolve(DATA_DIR, 'quilltap.dbkey'), makeDbKeyFile(TEST_PEPPER, E2E_PASSPHRASE));

    const freshSchema = JSON.parse(
      readFileSync(
        resolve(REPO_ROOT, 'crates/quilltap-core/src/services/provisioning/fresh_schema.json'),
        'utf8',
      ),
    ) as { main: string[]; mountIndex: string[] };
    for (const [partition, table] of [
      ['main', 'instance_settings'],
      ['main', 'projects'],
      ['main', 'groups'],
      ['main', 'chat_settings'],
      // Beat (c)'s manual memories need a fitted BUILTIN vocabulary and its
      // embedding ledger; beat (j) needs templates to choose between (the boot
      // seeds the built-ins once the table exists). The pair predates all three.
      ['main', 'tfidf_vocabularies'],
      ['main', 'embedding_status'],
      ['main', 'roleplay_templates'],
      ['mountIndex', 'group_doc_mount_links'],
    ] as const) {
      const create = freshSchema[partition].find((sql) =>
        sql.startsWith(`CREATE TABLE "${table}"`),
      );
      if (!create) throw new Error(`fresh_schema.json carries no ${table} CREATE TABLE`);
      runCliWrite(cli, create.replace('CREATE TABLE', 'CREATE TABLE IF NOT EXISTS'), {
        mountPoints: partition === 'mountIndex',
      });
    }
    for (const table of [
      'characters',
      'chats',
      'tags',
      'api_keys',
      'connection_profiles',
      'image_profiles',
      'files',
    ]) {
      runCliWrite(
        cli,
        `UPDATE ${table} SET userId = '${SINGLE_USER_ID}' WHERE userId = '${FIXTURE_USER}';`,
      );
    }
    runCliWrite(
      cli,
      `UPDATE connection_profiles SET baseUrl = 'http://127.0.0.1:${MOCK_LLM_PORT}/v1' WHERE provider = 'OPENAI_COMPATIBLE';`,
    );
    // Beat (f): two of v4 `eb2d87788`'s newly-labelled types on Solo Voyage.
    for (const [id, type] of [
      ['e3590000-0000-4000-8000-000000000001', 'ANSWER_CONFIRMATION'],
      ['e3590000-0000-4000-8000-000000000002', 'IMAGE_GENERATION'],
    ] as const) {
      runCliWrite(
        cli,
        `INSERT OR REPLACE INTO llm_logs (id, userId, type, messageId, chatId, characterId, autonomousRunId, ` +
          `provider, modelName, request, response, usage, cacheUsage, rawProviderUsage, requestHashes, durationMs, ` +
          `createdAt, updatedAt) VALUES ('${id}', '${SINGLE_USER_ID}', '${type}', NULL, '${SOLO_VOYAGE_CHAT_ID}', ` +
          `NULL, NULL, 'OPENAI_COMPATIBLE', 'mock-model', '{"messageCount":0,"messages":[],"toolCount":0}', ` +
          `'{"content":"ok","contentLength":2}', NULL, NULL, NULL, NULL, 10, '2026-02-01T00:00:0${id.slice(-1)}.000Z', ` +
          `'2026-02-01T00:00:0${id.slice(-1)}.000Z');`,
        { llmLogs: true },
      );
    }

    // Beat (c): four more ordinary memories of Aria's, which the beat marks as
    // keystones through the edit route to fill the subject's limit of five.
    // Planted, not created: on this pair's tiny TF-IDF vocabulary admission
    // reads short new sentences as the same memory and REINFORCES instead of
    // inserting (measured: six creates left four rows).
    for (const [id, fact] of KEYSTONE_PLANTS) {
      runCliWrite(
        cli,
        `INSERT INTO memories (id, characterId, aboutCharacterId, chatId, projectId, content, summary, keywords, tags, ` +
          `importance, embedding, source, witnessedContext, sourceMessageId, lastAccessedAt, reinforcementCount, ` +
          `lastReinforcedAt, relatedMemoryIds, reinforcedImportance, createdAt, updatedAt, occurredAt, narrativeTime, ` +
          `entities, kind, tier, supersededById, consolidatedFrom, consolidatedAt, supersededAt, keystone) ` +
          `SELECT '${id}', characterId, aboutCharacterId, chatId, projectId, '${fact}.', '${fact}', keywords, tags, ` +
          `importance, embedding, source, witnessedContext, sourceMessageId, lastAccessedAt, reinforcementCount, ` +
          `lastReinforcedAt, relatedMemoryIds, reinforcedImportance, createdAt, updatedAt, occurredAt, narrativeTime, ` +
          `entities, kind, tier, supersededById, consolidatedFrom, consolidatedAt, supersededAt, 0 ` +
          `FROM memories WHERE id = '0e000000-0000-4000-8000-000000000001';`,
      );
    }

    mock = await startMockLlm(MOCK_REPLY, MOCK_LLM_PORT);
    const logFd = openSync(SERVER_LOG, 'w');
    server = spawn(
      web,
      [
        '--host',
        '127.0.0.1',
        '--port',
        String(PORT),
        '--data-dir',
        INSTANCE_DIR,
        '--spa-dir',
        spaDir(),
      ],
      { stdio: ['ignore', logFd, logFd], detached: true, env: withoutPepper() },
    );
    server.unref();
    await waitForHealth();
    await dispatch({ type: 'unlock', passphrase: E2E_PASSPHRASE });
    // The pair carries no connection profile, so the Brahma Console would be
    // ineligible; give it one on the mock (beat (b) opens two conversations).
    const profile = (await dispatch({
      type: 'connectionProfileCreate',
      profile: {
        name: 'P4D359 Engine',
        provider: 'OPENAI_COMPATIBLE',
        baseUrl: `http://127.0.0.1:${MOCK_LLM_PORT}/v1`,
        modelName: 'mock-model',
        isDefault: true,
      },
    })) as { type?: string };
    if (profile.type === 'error')
      throw new Error(`connectionProfileCreate: ${JSON.stringify(profile)}`);
    // Beat (c) writes manual memories, which admission refuses without an
    // embedding (`…could not be embedded`); the pair carries no embedding
    // profile, so give it the offline BUILTIN one as the default.
    const embedding = (await dispatch({
      type: 'embeddingProfileCreate',
      name: 'P4D359 Builtin',
      provider: 'BUILTIN',
      modelName: 'tfidf-bm25-v1',
      isDefault: true,
    })) as { type?: string };
    if (embedding.type === 'error')
      throw new Error(`embeddingProfileCreate: ${JSON.stringify(embedding)}`);
    // A BUILTIN profile embeds nothing until its TF-IDF vocabulary is fitted on
    // the instance's corpus: refit, and wait for the vocabulary to appear.
    const builtinId = (embedding as { data?: { id?: string } }).data?.id;
    if (!builtinId) throw new Error(`embeddingProfileCreate: ${JSON.stringify(embedding)}`);
    // `vocabularyStats` rides the LIST read (v4's GET by id carries none).
    const builtinOf = async () => {
      const listed = (await dispatch({ type: 'embeddingProfileList' })) as {
        data?: { profiles?: Array<{ id: string; vocabularyStats?: unknown }> };
      };
      return listed.data?.profiles?.find((p) => p.id === builtinId) ?? listed;
    };
    const refit = (await dispatch({ type: 'embeddingProfileRefit', profileId: builtinId })) as {
      type?: string;
    };
    if (refit.type === 'error') throw new Error(`embeddingProfileRefit: ${JSON.stringify(refit)}`);
    const fitBy = Date.now() + 60_000;
    while (!((await builtinOf()) as { vocabularyStats?: unknown }).vocabularyStats) {
      if (Date.now() > fitBy)
        throw new Error(`the BUILTIN vocabulary was never fitted: ${JSON.stringify(await builtinOf())}`);
      await new Promise((r) => setTimeout(r, 500));
    }
    // The Inspector reads logs only while logging is on (the global-setup recipe).
    await dispatch({
      type: 'chatSettingsUpdate',
      settings: { llmLoggingSettings: { enabled: true, verboseMode: false, retentionDays: 30 } },
    });
  });

  test.afterAll(async () => {
    if (server?.pid) {
      try {
        process.kill(-server.pid, 'SIGTERM');
      } catch {
        try {
          process.kill(server.pid, 'SIGTERM');
        } catch {
          // already gone
        }
      }
    }
    await mock?.close();
    rmSync(INSTANCE_DIR, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  });

  // -------------------------------------------------------------------------
  // LIVE
  // -------------------------------------------------------------------------
  test('(a) the launcher toggles the floating console, which sits over a qt-modal without dismissing it', async ({
    page,
  }) => {
    await page.goto('/characters');
    await maybeUnlock(page);
    const launcher = page.locator('button.qt-brahma-launcher');
    await expect(launcher).toBeVisible({ timeout: 15_000 });
    await expect(launcher).toHaveAttribute('aria-pressed', 'false');
    await expect(launcher).toHaveAttribute('aria-label', 'Open the Brahma Console');

    // A qt-modal first: the SillyTavern import dialog.
    await page.getByRole('button', { name: 'Import from SillyTavern' }).click();
    const modal = page
      .getByRole('dialog', { name: /Import from SillyTavern/ })
      .or(page.locator('.qt-dialog', { hasText: 'Import from SillyTavern' }));
    await expect(modal.first()).toBeVisible();

    // The launcher is reachable over the modal's scrim (the floating layer's z-index).
    await launcher.click();
    await expect(launcher).toHaveAttribute('aria-pressed', 'true');
    await expect(launcher).toHaveAttribute('aria-label', 'Hide the Brahma Console');
    const consoleDialog = page.locator('.qt-floating-dialog-topmost[data-qt-floating-layer]');
    await expect(consoleDialog).toBeVisible({ timeout: 15_000 });
    await expect(consoleDialog).toHaveAttribute('aria-modal', 'false');

    // A click inside the console's composer leaves the modal open.
    const composer = consoleDialog.locator('textarea');
    await composer.click();
    await expect(modal.first()).toBeVisible();

    // Escape in the composer closes the console only.
    await composer.press('Escape');
    await expect(consoleDialog).toBeHidden();
    await expect(modal.first()).toBeVisible();
    await expect(launcher).toHaveAttribute('aria-pressed', 'false');

    // The geometry persists under its key, across a reload.
    await launcher.click();
    await expect(consoleDialog).toBeVisible();
    const header = consoleDialog.locator('.qt-floating-dialog-header');
    const box = (await header.boundingBox())!;
    await page.mouse.move(box.x + 40, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + 10, box.y + box.height / 2 + 30, { steps: 5 });
    await page.mouse.up();
    const stored = await page.evaluate(() =>
      localStorage.getItem('quilltap:brahma-console-geometry'),
    );
    expect(stored, 'the drag persisted the geometry').toBeTruthy();
    const before = JSON.parse(stored!) as { x: number; y: number };
    await page.reload();
    await maybeUnlock(page);
    await page.locator('button.qt-brahma-launcher').click();
    await expect(consoleDialog).toBeVisible({ timeout: 15_000 });
    const after = await consoleDialog.evaluate((el) => ({
      x: parseFloat((el as HTMLElement).style.left),
      y: parseFloat((el as HTMLElement).style.top),
    }));
    expect(after).toEqual({ x: before.x, y: before.y });
  });

  test('(a′) Escape in the modal closes only the modal (P4.D358’s qt-modal Escape)', async ({
    page,
  }) => {
    test.skip(
      !P4D358_MODAL_ESCAPE_LANDED,
      'awaits P4.D358’s qt-modal Escape (R-N) with the floating-layer guard',
    );
    await page.goto('/characters');
    await maybeUnlock(page);
    await page.getByRole('button', { name: 'Import from SillyTavern' }).click();
    await page.locator('button.qt-brahma-launcher').click();
    const consoleDialog = page.locator('.qt-floating-dialog-topmost[data-qt-floating-layer]');
    await expect(consoleDialog).toBeVisible();
    await page
      .locator('.qt-dialog', { hasText: 'Import from SillyTavern' })
      .locator('button')
      .first()
      .focus();
    await page.keyboard.press('Escape');
    await expect(page.locator('.qt-dialog', { hasText: 'Import from SillyTavern' })).toBeHidden();
    await expect(consoleDialog).toBeVisible();
  });

  test('(b) the workspace tab and the floating console remember two different conversations', async ({
    page,
  }) => {
    const a = (await dispatch({ type: 'brahmaConsoleCreate' })) as {
      data?: { chat?: { id?: string } };
    };
    const b = (await dispatch({ type: 'brahmaConsoleCreate' })) as {
      data?: { chat?: { id?: string } };
    };
    const chatA = a.data?.chat?.id;
    const chatB = b.data?.chat?.id;
    expect(chatA && chatB, `brahmaConsoleCreate answered ${JSON.stringify(a)}`).toBeTruthy();
    await page.goto('/characters');
    await maybeUnlock(page);
    await page.evaluate(
      ([tab, floating]) => {
        localStorage.setItem('quilltap:brahma-console-last-id', tab!);
        localStorage.setItem('quilltap:brahma-floating-last-id', floating!);
      },
      [chatA, chatB],
    );
    await page.reload();
    await maybeUnlock(page);
    await page.locator('button.qt-brahma-launcher').click();
    await expect(
      page.locator('.qt-floating-dialog-topmost [title="Past conversations"]'),
    ).toBeVisible({
      timeout: 15_000,
    });
    // Each surface's key is its own: opening the floating one left the tab's alone.
    expect(await page.evaluate(() => localStorage.getItem('quilltap:brahma-console-last-id'))).toBe(
      chatA,
    );
    expect(
      await page.evaluate(() => localStorage.getItem('quilltap:brahma-floating-last-id')),
    ).toBe(chatB);
    // Past conversations returns the floating surface to the launcher and forgets ITS id only.
    await page.locator('.qt-floating-dialog-topmost [title="Past conversations"]').click();
    await expect(
      page.locator('.qt-floating-dialog-topmost').getByText('Recent Console Conversations'),
    ).toBeVisible();
    expect(
      await page.evaluate(() => localStorage.getItem('quilltap:brahma-floating-last-id')),
    ).toBeNull();
    expect(await page.evaluate(() => localStorage.getItem('quilltap:brahma-console-last-id'))).toBe(
      chatA,
    );
  });

  test('(e) the About page shows the Diagrams and Terminals & Realtime lines', async ({ page }) => {
    await page.goto('/about');
    await maybeUnlock(page);
    await expect(
      page.getByText('Mermaid, Graphviz (@hpcc-js/wasm-graphviz), svg2roughjs, DOMPurify'),
    ).toBeVisible({
      timeout: 15_000,
    });
    await expect(page.getByText('xterm.js, node-pty, ws, croner')).toBeVisible();
  });

  test('(f) the Inspector’s derived Other lists an ANSWER_CONFIRMATION row; IMAGE_GENERATION is badged Image', async ({
    page,
  }) => {
    await page.goto(`/salon/${SOLO_VOYAGE_CHAT_ID}`);
    await maybeUnlock(page);
    await page.getByRole('button', { name: 'Toggle LLM Inspector' }).click();
    const filter = page.getByLabel('Filter log entries');
    await expect(filter).toBeVisible({ timeout: 15_000 });
    await filter.selectOption('other');
    await expect(page.locator('[data-log-id="e3590000-0000-4000-8000-000000000001"]')).toBeVisible({
      timeout: 15_000,
    });
    await expect(
      page.locator('[data-log-id="e3590000-0000-4000-8000-000000000001"]'),
    ).toContainText('Confirm');
    await filter.selectOption('image');
    const image = page.locator('[data-log-id="e3590000-0000-4000-8000-000000000002"]');
    await expect(image).toBeVisible();
    await expect(image).toContainText('Image');
  });

  test('(j) a default-template change holds across a tab round trip with no reload', async ({
    page,
  }) => {
    await page.goto('/settings?tab=templates');
    await maybeUnlock(page);
    const select = page.locator('qt-roleplay-templates-card select').first();
    await expect(select).toBeVisible({ timeout: 15_000 });
    const values = await select
      .locator('option')
      .evaluateAll((os) => os.map((o) => (o as HTMLOptionElement).value));
    const target = values.find((v) => v !== '' && v !== (values[0] ?? ''));
    test.skip(!target, 'the fixture carries no template to choose');
    await select.selectOption(target!);
    await expect(select).toHaveValue(target!);
    // Another tab and back — the shared ['chatSettings'] row was seeded from the PUT.
    // The settings tabs are `EntityTabs` buttons in `nav.qt-tab-group` (no `tab` role).
    const tabs = page.locator('nav.qt-tab-group');
    await tabs.getByRole('button', { name: 'Chat', exact: true }).click();
    await expect(page.locator('qt-roleplay-templates-card')).toHaveCount(0);
    await tabs.getByRole('button', { name: 'Templates & Prompts', exact: true }).click();
    await expect(page.locator('qt-roleplay-templates-card select').first()).toHaveValue(target!);
  });

  // -------------------------------------------------------------------------
  // GATED
  // -------------------------------------------------------------------------
  test('(c) a keystone memory: the pill, Keystones only, the sixth refused', async ({ page }) => {
    test.skip(!P4D341_LANDED, 'awaits P4.D341’s keystone column, admission and API (C8)');
    test.setTimeout(90_000);
    await page.goto(`/characters/${ARIA_ID}`);
    await maybeUnlock(page);
    await page.locator('nav.qt-tab-group').getByRole('button', { name: 'Memories' }).click();
    const cards = page.locator('qt-memory-card');
    // The pair bakes two ordinary memories for Aria, so `Keystones only` has
    // something to leave out.
    await expect(cards.first()).toBeVisible({ timeout: 15_000 });
    const before = await cards.count();
    expect(before, 'Aria’s baked memories').toBeGreaterThan(0);

    // A keystone made through the editor carries the pill.
    const first = 'Aria keeps the ship’s log in green ink';
    await addMemory(page, first, 'She has kept it in green ink since her first voyage.', true);
    const card = cards.filter({ hasText: first });
    await expect(card).toBeVisible({ timeout: 15_000 });
    await expect(card.locator('span', { hasText: /^Keystone$/ })).toBeVisible();
    // An ordinary memory never does.
    await expect(cards.filter({ hasNotText: first }).locator('span', { hasText: /^Keystone$/ })).toHaveCount(0);

    // `Keystones only` lists it alone.
    await page.getByRole('checkbox', { name: 'Keystones only' }).check();
    await expect(cards).toHaveCount(1, { timeout: 15_000 });
    await expect(cards.first()).toContainText(first);
    await page.getByRole('checkbox', { name: 'Keystones only' }).uncheck();
    await expect(cards).toHaveCount(before + 1, { timeout: 15_000 });

    // Four more about the same subject (Aria herself) fill the limit of five:
    // the planted ordinary memories, marked through the edit route.
    for (const [id, fact] of KEYSTONE_PLANTS) {
      const marked = (await dispatch({
        type: 'memoryUpdate',
        memoryId: id,
        memory: {
          characterId: ARIA_ID,
          summary: fact,
          content: `${fact}.`,
          keywords: [],
          importance: 0.5,
          keystone: true,
        },
      })) as { type?: string; data?: { memory?: { keystone?: unknown } } };
      expect(marked.data?.memory?.keystone, `memoryUpdate ${id}: ${JSON.stringify(marked)}`).toBe(true);
    }
    await page.getByRole('checkbox', { name: 'Keystones only' }).check();
    await expect(cards).toHaveCount(5, { timeout: 15_000 });
    await page.getByRole('checkbox', { name: 'Keystones only' }).uncheck();

    // …so a sixth is refused with v4's sentence, in the editor, and nothing is written.
    const sixth = 'Aria takes her tea without sugar';
    await page.getByRole('button', { name: 'Add Memory', exact: true }).click();
    const dialog = page.getByRole('dialog');
    await dialog.getByPlaceholder('Brief summary of this memory').fill(sixth);
    await expect(dialog.getByPlaceholder('Brief summary of this memory')).toHaveValue(sixth);
    await dialog.getByRole('textbox', { name: 'Memory content' }).fill('No sugar, ever.');
    await dialog.getByRole('checkbox', { name: /Keystone — always remembered/ }).check();
    await dialog.getByRole('button', { name: 'Create Memory' }).click();
    await expect(dialog).toContainText(
      'A character may keep at most 5 keystones about one subject; unmark one first.',
      { timeout: 15_000 },
    );
    await expect(dialog).toBeVisible();
    await dialog.getByRole('button', { name: 'Cancel' }).click();
    await page.getByRole('checkbox', { name: 'Keystones only' }).check();
    await expect(cards).toHaveCount(5, { timeout: 15_000 });
    await expect(cards.filter({ hasText: sixth })).toHaveCount(0);
  });

  test('(d) a reply naming an absent character chips its first mention; the card pins and unpins', async ({
    page,
  }) => {
    test.skip(
      !P4D347_SERVER_LANDED,
      'awaits P4.D347’s chatAbsentMentions arm (C5) and P4.D344 applying handoffs/p4.d359-salon-conversation.patch',
    );
    test.setTimeout(90_000);
    // Solo Voyage seats Aria (the model) and Cleo (the operator); Bram is a
    // workspace character who is not in it. The mock's reply names him twice.
    await page.goto(`/salon/${SOLO_VOYAGE_CHAT_ID}`);
    await maybeUnlock(page);
    const composer = page.locator('.qt-chat-composer-input').first();
    await expect(composer).toBeVisible({ timeout: 15_000 });
    await composer.click();
    await page.keyboard.type('Shall we tell anyone of the plan?');
    await page.locator('button[aria-label="Send message"]').click();

    const reply = page.locator('.qt-chat-message-content').filter({ hasText: MOCK_REPLY }).last();
    await expect(reply).toBeVisible({ timeout: 30_000 });

    // ONE chip, on the FIRST mention only, with v4's bytes.
    const chips = page.locator('.qt-mention-chip');
    await expect(chips).toHaveCount(1, { timeout: 15_000 });
    const chip = reply.locator('.qt-mention-chip');
    await expect(chip).toHaveText('Bram');
    await expect(chip).toHaveAttribute('role', 'button');
    await expect(chip).toHaveAttribute('tabindex', '0');
    await expect(chip).toHaveAttribute('aria-label', 'Bram, not in this conversation');
    // Nobody seated is chipped.
    await expect(page.locator('.qt-mention-chip', { hasText: 'Aria' })).toHaveCount(0);

    // Hover → the card.
    const card = page.locator('qt-mention-card .qt-tooltip');
    await chip.hover();
    await expect(card).toBeVisible({ timeout: 5_000 });
    await expect(card.locator('.qt-tooltip-title')).toHaveText('Bram');
    await expect(card).toContainText('Not in this conversation · click to pin');
    await expect(card).not.toHaveAttribute('data-pinned', 'true');

    // Click → pinned: it outlives the pointer leaving.
    await chip.click();
    await expect(card).toHaveAttribute('data-pinned', 'true');
    await page.mouse.move(5, 5);
    await page.waitForTimeout(800);
    await expect(card).toBeVisible();

    // Escape → unpinned and gone.
    await page.keyboard.press('Escape');
    await expect(card).toHaveCount(0);
  });

  test('(g) untick Humanoid → the non-humanoid hint, round-tripped', async ({ page }) => {
    test.skip(!P4D349_SERVER_LANDED, 'awaits P4.D349’s physicalDescription.humanoid writer (C6)');
    const HS_HUMANOID =
      'Used for avatar generation — describe only the head, face, hair, expression and neckline; nothing below the shoulders.';
    const HS_NON_HUMANOID =
      'Not used for a non-humanoid character — its avatar is drawn from the Medium prompt, whole form in frame.';
    await page.goto(`/characters/${DAX_ID}/edit`);
    await maybeUnlock(page);
    await page.getByRole('button', { name: 'Appearance', exact: true }).click();
    const box = page.locator('#physical-humanoid');
    const hint = page.getByTestId('physical-headshoulders-hint');
    // The form is seeded from the stored description (the pair's is named
    // `default`); an unseeded form would Save nothing (`Name is required`).
    await expect(page.locator('#physical-name')).toHaveValue('default', { timeout: 15_000 });
    await expect(box).toBeChecked();
    await expect(hint).toHaveText(HS_HUMANOID);

    await box.uncheck();
    await expect(hint).toHaveText(HS_NON_HUMANOID);
    const saved = page.waitForResponse(
      (r) =>
        r.url().includes('/api/dispatch') &&
        (r.request().postData() ?? '').includes('"characterUpdate"'),
    );
    await page.getByRole('button', { name: 'Save Physical Descriptions' }).click();
    const reply = (await (await saved).json()) as { type?: string };
    expect(reply.type, JSON.stringify(reply)).not.toBe('error');

    // The server kept it: the character read carries `humanoid: false`…
    const read = (await dispatch({ type: 'characterGet', characterId: DAX_ID })) as {
      data?: { character?: { physicalDescription?: { humanoid?: unknown } } };
    };
    expect(read.data?.character?.physicalDescription?.humanoid, JSON.stringify(read)).toBe(false);

    // …and a reload reads it back unticked, with the non-humanoid sentence.
    await page.reload();
    await maybeUnlock(page);
    await page.getByRole('button', { name: 'Appearance', exact: true }).click();
    await expect(page.locator('#physical-humanoid')).not.toBeChecked({ timeout: 15_000 });
    await expect(page.getByTestId('physical-headshoulders-hint')).toHaveText(HS_NON_HUMANOID);
  });

  test('(h) a She/Her/Her garment hides from a he character’s pronoun-aware wardrobe', async ({
    page,
  }) => {
    test.skip(!P4D349_SERVER_LANDED, 'awaits P4.D349’s pronounGuidance writer and reader (C6)');
    test.setTimeout(90_000);
    // Bram is he/him/his (the pair stores no pronouns, so give him the standard set).
    const he = (await dispatch({
      type: 'characterUpdate',
      characterId: BRAM_ID,
      character: { pronouns: { subject: 'he', object: 'him', possessive: 'his' } },
    })) as { type?: string };
    expect(he.type, JSON.stringify(he)).not.toBe('error');

    const title = `Ball Gown ${Date.now()}`;
    await page.goto(`/characters/${BRAM_ID}`);
    await maybeUnlock(page);
    await page.locator('.character-view').getByRole('button', { name: 'Wardrobe', exact: true }).click();
    await page.getByRole('button', { name: 'Open wardrobe for Bram' }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog.locator('.qt-dialog-title')).toHaveText('Wardrobe', { timeout: 15_000 });
    const aware = dialog.getByRole('checkbox', { name: /Pronoun-aware/ });
    await expect(aware).toBeChecked({ timeout: 15_000 });

    await page.getByRole('button', { name: '+ New Item' }).click();
    await expect(page.getByRole('heading', { name: 'New Wardrobe Item' })).toBeVisible();
    await page.locator('#wardrobe-title').fill(title);
    await page.locator('label').filter({ hasText: 'Accessories' }).locator('input[type="checkbox"]').click();
    const guidance = page.getByRole('group', { name: 'Pronoun Guidance' });
    await guidance.getByRole('checkbox', { name: 'She/Her/Her' }).check();
    await page.getByRole('button', { name: 'Create', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'New Wardrobe Item' })).toBeHidden({ timeout: 15_000 });

    // The server stored the guidance (the list re-read counts it), and the
    // pronoun-aware list holds the gown back from Bram.
    const row = dialog.locator('qt-wardrobe-item-row').filter({ hasText: title });
    await expect(dialog.getByText('(1 hidden)')).toBeVisible({ timeout: 15_000 });
    await expect(row).toHaveCount(0);

    // Untick → it is offered after all, marked for whom it was cut.
    await aware.uncheck();
    await expect(row).toBeVisible({ timeout: 15_000 });
    await expect(row.getByTestId('wardrobe-pronoun-guidance')).toHaveText('For She/Her/Her');
    await expect(dialog.getByText('(1 hidden)')).toHaveCount(0);
    await page.getByRole('button', { name: 'Done' }).click();
  });

  test('(i) an operator Inventory create refreshes a second dialog’s archetype picker', async ({
    page,
    context,
  }) => {
    test.skip(!P4D350_PUBLISH_LANDED, 'awaits P4.D350’s R-1 location-level inventory publish');
    test.setTimeout(90_000);
    const name = `Orrery ${Date.now()}`;

    // Dialog one: Aria's inventory, the archetype picker open (`+ From archetype`).
    await openInventoryDialog(page);
    await page.getByRole('button', { name: '+ From archetype' }).click();
    const picker = page.getByRole('dialog', { name: 'Make an object from an archetype' });
    await expect(picker).toBeVisible({ timeout: 15_000 });
    await expect(picker.getByText('Looking for archetypes…')).toHaveCount(0, { timeout: 15_000 });
    const option = picker.getByRole('option', { name: new RegExp(name) });
    await expect(option).toHaveCount(0);

    // Dialog two, on a second page: an operator create of an archetype in the same vault.
    const other = await context.newPage();
    try {
      await openInventoryDialog(other);
      await other.getByRole('button', { name: '+ New Archetype' }).click();
      await other.locator('#inventory-name').fill(name);
      await other.getByRole('button', { name: 'Save', exact: true }).click();
      await expect(other.getByText(`"${name}" is laid on the table`)).toBeVisible({ timeout: 15_000 });
    } finally {
      await other.close();
    }

    // Dialog one's picker re-reads on the server's `inventory` publish — no reload.
    await expect(option).toBeVisible({ timeout: 15_000 });
  });
});

/** The memory editor, from the Memories tab: create one, optionally a keystone. */
async function addMemory(page: Page, summary: string, content: string, keystone: boolean): Promise<void> {
  await page.getByRole('button', { name: 'Add Memory', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  // Each fill asserted before the next (the characters-flow render-race idiom).
  await dialog.getByPlaceholder('Brief summary of this memory').fill(summary);
  await expect(dialog.getByPlaceholder('Brief summary of this memory')).toHaveValue(summary);
  await dialog.getByRole('textbox', { name: 'Memory content' }).fill(content);
  await expect(dialog.getByPlaceholder('Brief summary of this memory')).toHaveValue(summary);
  if (keystone) await dialog.getByRole('checkbox', { name: /Keystone — always remembered/ }).check();
  await dialog.getByRole('button', { name: 'Create Memory' }).click();
  await expect(dialog).toBeHidden({ timeout: 15_000 });
}

/** The Inventory dialog from the rail, on Aria's vault (`inventory-flow.spec.ts`'s helper). */
async function openInventoryDialog(page: Page): Promise<void> {
  await page.goto('/salon');
  await maybeUnlock(page);
  await page.getByTitle('Inventory', { exact: true }).click();
  await expect(page.getByRole('dialog').locator('.qt-dialog-title').first()).toHaveText('Inventory', {
    timeout: 15_000,
  });
  await page.locator('#inventory-container-select').selectOption({ label: 'Aria' });
}

async function dispatch(body: Record<string, unknown>): Promise<unknown> {
  const res = await fetch(`${BASE_URL}/api/dispatch`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  return res.json();
}

/** Unlock only when the passphrase screen is showing. */
async function maybeUnlock(page: Page): Promise<void> {
  const passphrase = page.locator('#qt-passphrase');
  await expect(passphrase.or(page.locator('qt-shell')).first()).toBeVisible({ timeout: 15_000 });
  if (await passphrase.count()) {
    await passphrase.fill(E2E_PASSPHRASE);
    await page.getByRole('button', { name: 'Unlock' }).click();
    await expect(page.locator('qt-shell')).toBeVisible({ timeout: 15_000 });
  }
}

function runCliWrite(
  cli: string,
  sql: string,
  opts: { mountPoints?: boolean; llmLogs?: boolean } = {},
): void {
  const args = ['db', '--data-dir', INSTANCE_DIR, '--write', sql];
  if (opts.mountPoints) args.splice(1, 0, '--mount-points');
  if (opts.llmLogs) args.splice(1, 0, '--llm-logs');
  const res = spawnSync(cli, args, {
    env: { ...withoutPepper(), QUILLTAP_DB_PASSPHRASE: E2E_PASSPHRASE, QUILLTAP_QUIET_HINTS: '1' },
    encoding: 'utf8',
  });
  if (res.status !== 0)
    throw new Error(`CLI write failed (${sql.slice(0, 80)}):\n${res.stdout}\n${res.stderr}`);
}

function withoutPepper(): NodeJS.ProcessEnv {
  const env = { ...process.env };
  delete env['ENCRYPTION_MASTER_PEPPER'];
  return env;
}

async function waitForHealth(): Promise<void> {
  const deadline = Date.now() + 30_000;
  let lastErr = '';
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${BASE_URL}/health`);
      // 423 = up, instance locked (the server gets no passphrase env).
      if (res.status === 423 || res.status === 200) return;
      lastErr = `status ${res.status}`;
    } catch (err) {
      lastErr = String(err);
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`quilltap-web did not become healthy on ${PORT}: ${lastErr}`);
}
