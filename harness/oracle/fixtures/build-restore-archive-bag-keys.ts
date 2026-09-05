/**
 * P4.D158 restore-fixture builder — the archive for the four additions that
 * ride INSIDE an existing column.
 *
 * ── WHY A NEW ARCHIVE ────────────────────────────────────────────────────────
 * v4 `2edd823c0` ("pin every 4.9/4.10 data-model addition in the restore
 * guard") names the class in one sentence, and it is the sentence this fixture
 * exists to answer:
 *
 *   *a new column announces itself with a migration; a new key in a JSON bag
 *   or a widened enum domain is invisible to every schema check.*
 *
 * v4 pinned its four with jest mocks over `restore.ts`. v5's restore proof is a
 * tier-2 DB-state diff, which cannot see a key no committed archive carries —
 * and **none of the thirteen carries any of these four** (measured 2026-09-05;
 * `conciergeOverride`, `allowCheapFallback`, `loras` and
 * `perTurnConversationSummaries` appear in no restore/backup family, oracle case
 * or fixture builder in the tree). So the cross-side dump was green on all four
 * for the same reason v4's schema checks were: nothing asked.
 *
 * ── THE FOUR, AND WHY EACH ONE HIDES ─────────────────────────────────────────
 *  1 `chats.conciergeOverride = 'UNCENSORED'` — a WIDENED ENUM DOMAIN. The
 *    column is old; 4.9 added a fourth state. A restore that narrowed it back
 *    to `'OFF'` would silently re-arm the classifier on a chat whose operator
 *    had already ruled on it (P4.D148's surface). Carried on TWO chats so a
 *    narrowing is distinguishable from a blanket drop: chat 1 takes
 *    `'UNCENSORED'`, chat 2 takes `'OFF'`. Narrow the first and only the first
 *    cell moves; drop both and both move.
 *  2 `chat_settings.cheapLLMSettings.allowCheapFallback = true` — a JSON-BAG
 *    key whose schema default is `false` (`settings.types.ts:73`), so losing it
 *    in transit reads as the operator having DECLINED a stand-in they opted
 *    into (P4.D138's remainder).
 *  3 `image_profiles.parameters.loras` — an OPEN bag: nothing downstream
 *    validates its shape, so nothing downstream would notice the reserved key
 *    going missing (P4.D138's LoRA train). Carried beside a pre-existing
 *    sibling key so a bag-level replacement is distinguishable from a key-level
 *    drop.
 *  4 the `memoryRecall` instance-settings row carrying
 *    `perTurnConversationSummaries` — upserted by RAW SQL rather than through a
 *    repository (`restore.ts:879`), so the value travels as an opaque string
 *    and the guard is that the row is written at all, and written verbatim
 *    (P4.D95).
 *
 * ── WHY IT IS A DERIVATION, NOT A `createBackup` ─────────────────────────────
 * The same reasoning as `build-restore-archive-legacy-profiles.ts`: the rest of
 * the family is written by v4's REAL backup writer, and this shape is a
 * *modern* archive rather than an old one — v4's writer would happily produce
 * it from an instance whose chats had been ruled on, whose operator had opted
 * into a stand-in, whose image profile carried adapters and whose recall knob
 * was set. Reproducing that instance to re-dump it would be a much larger
 * fixture for the same bytes. So this is a JSON rewrite of the full archive:
 * `unzip`, four edits, `zip`. Every other byte in the zip — the layout, the
 * manifest, every other data file — is v4's own.
 *
 * Both engines then read the SAME derived bytes, which is the claim the restore
 * family exists to make.
 *
 * Run (Node 24, from the PINNED v4 worktree — see the lane record):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   PIN=/tmp/qt-v4-pin-p4d158-d883a5ee1
 *   cd "$PIN"
 *   QT_ARCHIVE_DIR=$V5W/crates/quilltap-web/tests/fixtures/restore-archives \
 *     $N/npx tsx $V5W/harness/oracle/fixtures/build-restore-archive-bag-keys.ts
 *
 * (The transform needs no v4 code at all — it is `unzip`, a JSON rewrite and
 * `zip`. The pinned cwd is kept for consistency with the rest of the family.)
 */

import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync, rmSync, readdirSync, copyFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

const BASE_ARCHIVE = 'restore-archive.zip';
const OUT_ARCHIVE = 'restore-archive-bag-keys.zip';

/**
 * The adapters arm 3 carries. Shaped like v4's own
 * `restore-field-fidelity.test.ts` block so the two fixtures describe the same
 * key, and carried BESIDE the base archive's existing `steps` so a bag-level
 * replacement and a key-level drop are different failures.
 */
const LORAS = [
  {
    source: 'author/some-lora',
    scale: 0.8,
    triggerPhrase: 'in the style of',
    label: 'Some LoRA',
  },
];

/** Arm 4's row. Stored as a JSON *string*, which is what the column holds. */
const MEMORY_RECALL_VALUE = JSON.stringify({
  scopePolicy: 'down-weight',
  expandRelated: false,
  perTurnConversationSummaries: true,
});

function readJson<T>(path: string): T {
  return JSON.parse(readFileSync(path, 'utf8')) as T;
}

function writeJson(path: string, value: unknown): void {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

function main(): void {
  const dir = process.env.QT_ARCHIVE_DIR;
  if (!dir) throw new Error('set QT_ARCHIVE_DIR to the committed restore-archives directory');

  const base = join(dir, BASE_ARCHIVE);
  const scratch = mkdtempSync(join(tmpdir(), 'qt-bag-keys-'));
  try {
    execFileSync('unzip', ['-q', '-o', base, '-d', scratch]);
    const roots = readdirSync(scratch).filter((n) => n.startsWith('quilltap-backup-'));
    if (roots.length !== 1) {
      throw new Error(`expected exactly one backup root in ${BASE_ARCHIVE}, saw ${roots.length}`);
    }
    const data = join(scratch, roots[0], 'data');

    // ── 1. the widened enum domain ──────────────────────────────────────────
    const chatsPath = join(data, 'chats.json');
    const chats = readJson<Array<Record<string, unknown>>>(chatsPath);
    if (chats.length !== 2) {
      throw new Error(`expected 2 chats in ${BASE_ARCHIVE}, saw ${chats.length}`);
    }
    for (const c of chats) {
      if ('conciergeOverride' in c) {
        throw new Error('base archive already carries conciergeOverride — rewrite this builder');
      }
    }
    chats[0].conciergeOverride = 'UNCENSORED';
    chats[1].conciergeOverride = 'OFF';
    writeJson(chatsPath, chats);

    // ── 2. the JSON-bag key with a falsy default ────────────────────────────
    const settingsPath = join(data, 'chat-settings.json');
    const settings = readJson<Array<Record<string, unknown>>>(settingsPath);
    if (settings.length !== 1) {
      throw new Error(`expected 1 chat-settings row in ${BASE_ARCHIVE}, saw ${settings.length}`);
    }
    const cheap = settings[0].cheapLLMSettings as Record<string, unknown> | undefined;
    if (!cheap || 'allowCheapFallback' in cheap) {
      throw new Error('base cheapLLMSettings is not the shape this builder assumes');
    }
    cheap.allowCheapFallback = true;
    writeJson(settingsPath, settings);

    // ── 3. the reserved key in an open bag ──────────────────────────────────
    const profilesPath = join(data, 'image-profiles.json');
    const profiles = readJson<Array<Record<string, unknown>>>(profilesPath);
    if (profiles.length !== 1) {
      throw new Error(`expected 1 image profile in ${BASE_ARCHIVE}, saw ${profiles.length}`);
    }
    const params = profiles[0].parameters as Record<string, unknown> | undefined;
    if (!params || !('steps' in params) || 'loras' in params) {
      throw new Error('base image-profile parameters is not the shape this builder assumes');
    }
    params.loras = LORAS;
    writeJson(profilesPath, profiles);

    // ── 4. the raw-SQL instance-settings row ────────────────────────────────
    const instancePath = join(data, 'instance-settings.json');
    const instance = readJson<Array<{ key: string; value: string }>>(instancePath);
    if (instance.some((r) => r.key === 'memoryRecall')) {
      throw new Error('base archive already carries a memoryRecall row');
    }
    instance.push({ key: 'memoryRecall', value: MEMORY_RECALL_VALUE });
    writeJson(instancePath, instance);

    const manifestPath = join(scratch, roots[0], 'manifest.json');
    const manifest = readJson<{ counts: Record<string, number> }>(manifestPath);
    manifest.counts.instanceSettings = instance.length;
    writeJson(manifestPath, manifest);

    // The same call `backup-service.ts:800` makes, from the same kind of cwd.
    const staged = join(scratch, `${roots[0]}.zip`);
    execFileSync('zip', ['-q', '-r', staged, roots[0]], { cwd: scratch });
    const out = join(dir, OUT_ARCHIVE);
    copyFileSync(staged, out);
    process.stderr.write(`  ${OUT_ARCHIVE}  ${readFileSync(out).length} bytes\n`);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

main();
