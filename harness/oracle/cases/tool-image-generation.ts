/**
 * Tier-1 oracle case (P4.D262 item 16) — the wardrobe tools' picture decision
 * (v4 `lib/wardrobe/tool-image-generation.ts`, `b3f937076`) and the pure tool
 * formatters the wear ledger added (`3ee3b1342`).
 *
 * Drives v4's REAL code, no mocks of the units under test:
 *   - `wardrobeToolImagesEnabled` over the settings shapes that reach it (absent;
 *     no bag; the migration's ONE-key default; the repository's two-key seed;
 *     on; a string `"true"`);
 *   - `maybeQueueWardrobeToolImage` over the decision table (enabled ×
 *     requested ∈ {undefined, true, false} × defaultWhenEnabled), with STUB
 *     repositories as its inputs (the settings row; no usable image profile),
 *     so every arm short of the enqueue is reached: `undefined`, `not-enabled`,
 *     `no-image-profile` (the `queued` arm needs the job table — the
 *     wardrobe_tools family's switch-ON scenario);
 *   - `formatWardrobeToolImageLine` for every status and `undefined`;
 *     `formatWardrobeImageHandle`;
 *   - `formatRelativeDays` at the eight rungs' boundaries;
 *   - `formatWornRelative` (`ed8b15b50`, bug 183) at the same offsets;
 *   - `formatWardrobeListWearNote` (the caller's share + the household's,
 *     `ed8b15b50` bug 184 — four fields) and `formatWardrobeWearParagraph`
 *     (reader first) under ONE pinned `nowMs` (v4's second parameter), incl.
 *     the exact lines of v4's `wardrobe-wear-readout.test.ts`.
 *
 * Run (Node 24, from the v4 checkout or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   TZ=UTC $N/node --import tsx $V5/harness/oracle/cases/tool-image-generation.ts \
 *     > /tmp/oracle-tool-image-generation.ndjson
 */

process.env.LOG_LEVEL = 'error';

const NOW_MS = Date.parse('2026-06-15T12:00:00.000Z');
const DAY = 86400000;
const ago = (days: number): string => new Date(NOW_MS - days * DAY).toISOString();

async function main(): Promise<void> {
  const tig = await import('@/lib/wardrobe/tool-image-generation');
  const { formatRelativeDays } = await import('@/lib/format-time');
  const { formatWornRelative } = await import('@/lib/wardrobe/wear-display');
  const { formatWardrobeListWearNote } = await import('@/lib/tools/handlers/wardrobe-list-handler');
  const { formatWardrobeWearParagraph } = await import('@/lib/tools/handlers/wardrobe-read-handler');

  const rows: unknown[] = [];
  const emit = (row: Record<string, unknown>): void => {
    rows.push(row);
  };
  emit({ kind: 'meta', baseline: 'p4.d275-tool-image-generation', nowMs: NOW_MS });

  // ── wardrobeToolImagesEnabled ──
  const settingsShapes: Array<[string, unknown]> = [
    ['absent', null],
    ['no-bag', {}],
    ['null-bag', { wardrobeImageSettings: null }],
    ['one-key-migration-default', { wardrobeImageSettings: { imageProfileId: null } }],
    ['two-key-seed', { wardrobeImageSettings: { imageProfileId: null, generateFromTools: false } }],
    ['on', { wardrobeImageSettings: { imageProfileId: null, generateFromTools: true } }],
    ['string-true', { wardrobeImageSettings: { generateFromTools: 'true' } }],
    ['one', { wardrobeImageSettings: { generateFromTools: 1 } }],
  ];
  for (const [id, settings] of settingsShapes) {
    emit({ kind: 'enabled', id, settings, out: tig.wardrobeToolImagesEnabled(settings as never) });
  }

  // ── maybeQueueWardrobeToolImage: the decision table (no usable profile) ──
  for (const enabled of [false, true]) {
    for (const requested of [undefined, true, false]) {
      for (const defaultWhenEnabled of [false, true]) {
        const repos = {
          chatSettings: {
            findByUserId: async () => ({
              wardrobeImageSettings: { imageProfileId: null, generateFromTools: enabled },
            }),
          },
          imageProfiles: { findById: async () => null, findDefault: async () => null },
        };
        const out = await tig.maybeQueueWardrobeToolImage(repos as never, {
          userId: 'u',
          chatId: 'c',
          characterId: 'ch',
          itemId: 'i',
          requested,
          defaultWhenEnabled,
          callerContext: 'oracle',
        });
        emit({
          kind: 'decision',
          enabled,
          requested: requested ?? null,
          defaultWhenEnabled,
          out: out ?? null,
        });
      }
    }
  }

  // ── the line and the handle ──
  for (const status of ['queued', 'not-enabled', 'no-image-profile', 'failed'] as const) {
    const message = `the ${status} sentence`;
    emit({ kind: 'line', status, message, out: tig.formatWardrobeToolImageLine({ status, message }) });
  }
  emit({ kind: 'line', status: null, message: null, out: tig.formatWardrobeToolImageLine(undefined) });
  emit({ kind: 'handle', id: 'f1000000-0000-4000-8000-000000000001', out: tig.formatWardrobeImageHandle('f1000000-0000-4000-8000-000000000001') });

  // ── formatRelativeDays ──
  const OFFSETS = [-2, 0, 0.999, 1, 1.5, 2, 6.999, 7, 13.99, 14, 20, 29.99, 30, 59.99, 60, 89, 364.99, 365, 729.99, 730, 2000];
  for (const days of OFFSETS) {
    emit({ kind: 'relative', days, out: formatRelativeDays(NOW_MS - days * DAY, NOW_MS) });
  }

  // ── formatWornRelative (bug 183) ──
  for (const days of OFFSETS) {
    emit({ kind: 'worn_relative', days, out: formatWornRelative(NOW_MS - days * DAY, NOW_MS) });
  }

  // ── formatWardrobeListWearNote (bug 184: [wear_count, last_worn_at, worn_by_you, last_worn_by_you_at]) ──
  const notes: Array<[number, string | null, number, string | null]> = [
    [0, null, 0, null],
    [0, ago(3), 0, null],
    [115, ago(3), 0, null],
    [1, ago(3), 1, ago(3)],
    [4, ago(3), 4, ago(3)],
    [4, ago(3), 3, ago(3)],
    [4, ago(3), 2, null],
    [4, ago(3), 2, ''],
    [4, ago(3), 2, 'not a date'],
    [2, ago(9), 2, ago(9)],
    [2, ago(40), 2, ago(40)],
    [3, '2026-06-12', 3, '2026-06-12'],
    [3, ago(1), 1, '2026-06-12T12:00:00+02:00'],
    [2, ago(1), 1, ago(1)],
    [1, ago(0.5), 1, ago(0.5)],
    [12, ago(400), 5, ago(400)],
    [2, ago(3), 5, ago(3)],
    [9, ago(40), 1, ago(13.99)],
  ];
  for (const [wear_count, last_worn_at, worn_by_you, last_worn_by_you_at] of notes) {
    emit({
      kind: 'list_note',
      wear_count,
      last_worn_at,
      worn_by_you,
      last_worn_by_you_at,
      out: formatWardrobeListWearNote({ wear_count, last_worn_at, worn_by_you, last_worn_by_you_at } as never, NOW_MS),
    });
  }

  // ── formatWardrobeWearParagraph ──
  const YOU = 'a0000000-0000-4000-8000-000000000001';
  const MARGUERITE = 'a0000000-0000-4000-8000-000000000002';
  const GONE = 'a0000000-0000-4000-8000-000000000003';
  const w = (
    character_id: string | null,
    name: string,
    is_you: boolean,
    departed: boolean,
    wear_count: number,
    first: string,
    last: string,
  ) => ({ character_id, name, is_you, departed, wear_count, first_worn_at: first, last_worn_at: last });
  const paragraphs: Array<[string, unknown]> = [
    ['undefined', undefined],
    ['zero', { wear_count: 0, first_worn_at: null, last_worn_at: null, wearers: [] }],
    ['no-wearers', { wear_count: 2, first_worn_at: ago(9), last_worn_at: ago(3), wearers: [] }],
    ['no-last', { wear_count: 2, first_worn_at: ago(9), last_worn_at: null, wearers: [w(YOU, 'Rosalind', true, false, 2, ago(9), ago(3))] }],
    ['once-you', { wear_count: 1, first_worn_at: ago(3), last_worn_at: ago(3), wearers: [w(YOU, 'Rosalind', true, false, 1, ago(3), ago(3))] }],
    ['readout-4-you-and-marguerite', {
      wear_count: 4,
      first_worn_at: '2026-03-14T10:00:00.000Z',
      last_worn_at: ago(3),
      wearers: [w(YOU, 'Rosalind', true, false, 3, '2026-03-14T10:00:00.000Z', ago(3)), w(MARGUERITE, 'Marguerite', false, false, 1, ago(20), ago(20))],
    }],
    ['readout-3-marguerite-then-you', {
      wear_count: 3,
      first_worn_at: '2026-03-14T10:00:00.000Z',
      last_worn_at: ago(14),
      wearers: [w(MARGUERITE, 'Marguerite', false, false, 1, ago(14), ago(14)), w(YOU, 'Rosalind', true, false, 2, '2026-03-14T10:00:00.000Z', ago(30))],
    }],
    ['readout-5-departed', {
      wear_count: 5,
      first_worn_at: '2026-03-14T10:00:00.000Z',
      last_worn_at: ago(3),
      wearers: [w(GONE, 'a departed character', false, true, 2, ago(10), ago(3)), w(MARGUERITE, 'Marguerite', false, false, 1, ago(12), ago(12)), w(null, 'unattributed', false, true, 2, '2026-03-14T10:00:00.000Z', ago(50))],
    }],
    ['first-null-falls-to-last', { wear_count: 2, first_worn_at: null, last_worn_at: '2026-09-04T08:00:00.000Z', wearers: [w(YOU, 'Rosalind', true, false, 2, '2026-09-04T08:00:00.000Z', '2026-09-04T08:00:00.000Z')] }],
    ['unparseable-dates', { wear_count: 3, first_worn_at: 'whenever', last_worn_at: 'lately', wearers: [w(MARGUERITE, 'Marguerite', false, false, 3, 'whenever', 'lately')] }],
    ['four-wearers', {
      wear_count: 9,
      first_worn_at: '2025-12-31T23:59:59.999Z',
      last_worn_at: ago(0.2),
      wearers: [
        w(MARGUERITE, 'Marguerite', false, false, 1, ago(0.2), ago(0.2)),
        w(YOU, 'Rosalind', true, false, 2, ago(5), ago(5)),
        w(GONE, 'a departed character', false, true, 4, '2025-12-31T23:59:59.999Z', ago(70)),
        w('a0000000-0000-4000-8000-000000000004', 'Basil', false, false, 2, ago(100), ago(100)),
      ],
    }],
    // v4's `wardrobe-wear-readout.test.ts` shapes (`ed8b15b50`, bug 184).
    ['crowded-household-you-latest', {
      wear_count: 116,
      first_worn_at: '2026-06-13T10:00:00.000Z',
      last_worn_at: ago(0.1),
      wearers: [
        w(YOU, 'Vivienne', true, false, 13, '2026-06-13T10:00:00.000Z', ago(0.1)),
        w(MARGUERITE, 'Laura', false, false, 25, '2026-06-13T10:00:00.000Z', ago(3)),
        w(GONE, 'Gary', false, false, 78, '2026-06-13T10:00:00.000Z', ago(14)),
      ],
    }],
    ['single-other-wear', { wear_count: 1, first_worn_at: ago(3), last_worn_at: ago(3), wearers: [w(MARGUERITE, 'Marguerite', false, false, 1, ago(3), ago(3))] }],
    ['you-not-latest', {
      wear_count: 3,
      first_worn_at: '2026-03-14T10:00:00.000Z',
      last_worn_at: ago(14),
      wearers: [w(MARGUERITE, 'Marguerite', false, false, 1, ago(14), ago(14)), w(YOU, 'Rosalind', true, false, 2, '2026-03-14T10:00:00.000Z', '2026-04-01T10:00:00.000Z')],
    }],
    ['departed-only-repeat', {
      wear_count: 5,
      first_worn_at: '2026-03-14T10:00:00.000Z',
      last_worn_at: ago(3),
      wearers: [
        w(GONE, 'a departed character', false, true, 2, '2026-05-01T10:00:00.000Z', ago(3)),
        w(MARGUERITE, 'Marguerite', false, false, 1, '2026-04-01T10:00:00.000Z', '2026-04-01T10:00:00.000Z'),
        w(null, 'unattributed', false, true, 2, '2026-03-14T10:00:00.000Z', '2026-03-20T10:00:00.000Z'),
      ],
    }],
    ['exactly-one-other', { wear_count: 3, first_worn_at: ago(30), last_worn_at: ago(9), wearers: [w(MARGUERITE, 'Marguerite', false, false, 3, ago(30), ago(9))] }],
    ['twice-by-others', {
      wear_count: 2,
      first_worn_at: null,
      last_worn_at: ago(40),
      wearers: [w(MARGUERITE, 'Marguerite', false, false, 1, ago(40), ago(40)), w('a0000000-0000-4000-8000-000000000004', 'Basil', false, false, 1, ago(50), ago(50))],
    }],
    ['you-once-with-others', {
      wear_count: 4,
      first_worn_at: ago(60),
      last_worn_at: ago(2),
      wearers: [w(MARGUERITE, 'Marguerite', false, false, 3, ago(60), ago(2)), w(YOU, 'Rosalind', true, false, 1, ago(9), ago(9))],
    }],
    ['two-is-you-first-wins', {
      wear_count: 6,
      first_worn_at: ago(100),
      last_worn_at: ago(1),
      wearers: [w(YOU, 'Rosalind', true, false, 2, ago(100), ago(1)), w(YOU, 'Rosalind again', true, false, 4, ago(90), ago(40))],
    }],
    ['you-week-rung', { wear_count: 2, first_worn_at: ago(9), last_worn_at: ago(9), wearers: [w(YOU, 'Rosalind', true, false, 2, ago(12), ago(9))] }],
    ['you-month-rung', { wear_count: 2, first_worn_at: ago(40), last_worn_at: ago(40), wearers: [w(YOU, 'Rosalind', true, false, 2, ago(45), ago(40))] }],
    ['you-empty-first', { wear_count: 2, first_worn_at: '', last_worn_at: ago(3), wearers: [w(YOU, 'Rosalind', true, false, 2, '', ago(3))] }],
    ['absent-empty-first', { wear_count: 3, first_worn_at: '', last_worn_at: ago(3), wearers: [w(MARGUERITE, 'Marguerite', false, false, 3, '', ago(3))] }],
  ];
  for (const [id, wear] of paragraphs) {
    emit({ kind: 'paragraph', id, wear: wear ?? null, out: formatWardrobeWearParagraph(wear as never, NOW_MS) });
  }

  process.stdout.write(rows.map((r) => JSON.stringify(r)).join('\n') + '\n');
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`tool-image-generation oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
