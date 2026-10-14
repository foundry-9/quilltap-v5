/**
 * Tier-1 oracle — v4's `memoryExtractionPassOf(payload)` (P4.D288 unit 3,
 * C1-6; `lib/background-jobs/queue-service.ts:116-121`, `ac6348c5f` +
 * `64743ee13`, at the `83334b9a5` pin) through its REAL export: every flag
 * combination, `pass: ''` (falsy — the flags decide), a truthy non-boolean
 * flag (`1`, `'yes'`, `{}`, `[]`), a falsy one (`0`, `''`, `null`), each known
 * pass, and an UNKNOWN pass returned as is (R-X) — a string and a truthy
 * non-string. One `{label, payload, pass}` line per row.
 *
 * Run (Node 24, from the pinned v4 worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   rm -f /tmp/oracle-memory-extraction-pass.ndjson
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/memory-extraction-pass.ts \
 *     > /tmp/oracle-memory-extraction-pass.ndjson
 */

const ROWS: Array<[string, Record<string, unknown>]> = [
  ['empty', {}],
  ['turn', { pass: 'turn' }],
  ['other-fold', { pass: 'other-fold' }],
  ['episode-fold', { pass: 'episode-fold' }],
  ['pass-wins-over-flags', { pass: 'turn', foldEpisodeCatchup: true, foldOtherCatchup: true }],
  ['empty-pass-flags-decide', { pass: '', foldOtherCatchup: true }],
  ['null-pass-episode', { pass: null, foldEpisodeCatchup: true }],
  ['episode-before-other', { foldEpisodeCatchup: true, foldOtherCatchup: true }],
  ['other-only', { foldOtherCatchup: true }],
  ['episode-only', { foldEpisodeCatchup: true }],
  ['flag-one', { foldOtherCatchup: 1 }],
  ['flag-string', { foldEpisodeCatchup: 'yes' }],
  ['flag-object', { foldOtherCatchup: {} }],
  ['flag-array', { foldEpisodeCatchup: [] }],
  ['flag-zero', { foldOtherCatchup: 0 }],
  ['flag-empty-string', { foldEpisodeCatchup: '' }],
  ['flag-null', { foldOtherCatchup: null, foldEpisodeCatchup: null }],
  ['flag-false', { foldOtherCatchup: false, foldEpisodeCatchup: false }],
  ['ignore-idle-irrelevant', { foldOtherIgnoreIdle: true, ignoreIdle: true }],
  ['unknown-pass', { pass: 'bogus' }],
  ['unknown-pass-case', { pass: 'Turn' }],
  ['unknown-pass-over-flags', { pass: 'other', foldEpisodeCatchup: true }],
  ['truthy-number-pass', { pass: 5 }],
  ['truthy-true-pass', { pass: true }],
  ['falsy-zero-pass', { pass: 0, foldOtherCatchup: true }],
];

async function main(): Promise<void> {
  process.env.LOG_LEVEL = 'error';
  const { memoryExtractionPassOf } = await import('@/lib/background-jobs/queue-service');
  for (const [label, payload] of ROWS) {
    process.stdout.write(
      JSON.stringify({ label, payload, pass: memoryExtractionPassOf(payload as never) }) + '\n',
    );
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`memory-extraction-pass oracle failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
