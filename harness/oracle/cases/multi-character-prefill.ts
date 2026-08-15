/**
 * P4.D79 unit 2 — the multi-character `[Name]` prefill resolver (v4
 * `23af7146`).
 *
 * Drives v4's REAL module:
 *   defaultMultiCharacterPrefill, profileUsesNamePrefill
 *     (lib/llm/multi-character-prefill.ts)
 *
 * The corpus is providers × {true, false, null, absent, non-boolean}. It pins
 * three things a hand-read would miss:
 *
 *   - `!provider` is JS falsiness, so the EMPTY STRING takes the "true" branch
 *     alongside null/undefined;
 *   - the hostile-provider test is `provider.toUpperCase()`, so casing and
 *     whitespace both matter (`' ANTHROPIC'` is NOT hostile);
 *   - the stored-choice test is `typeof … === 'boolean'`, so `0`/`1`/`'true'`
 *     fall THROUGH to the provider default rather than coercing.
 *
 * Run from inside the pinned v4 worktree:
 *   cd /tmp/qt-v4-pin-p4d79-aa464abf
 *   ~/.nvm/versions/node/v24.13.1/bin/npx tsx \
 *     ~/source/quilltap-v5/harness/oracle/cases/multi-character-prefill.ts \
 *     > /tmp/oracle-multi-character-prefill.ndjson
 */

import {
  defaultMultiCharacterPrefill,
  profileUsesNamePrefill,
} from '@/lib/llm/multi-character-prefill';

const rows: unknown[] = [];

/** Providers, including the falsy and casing/whitespace boundaries. */
const PROVIDERS: Array<[string, string | null | undefined]> = [
  ['anthropic-upper', 'ANTHROPIC'],
  ['anthropic-lower', 'anthropic'],
  ['anthropic-mixed', 'AnThRoPiC'],
  ['anthropic-leading-space', ' ANTHROPIC'],
  ['anthropic-trailing-space', 'ANTHROPIC '],
  // Dotless i: JS toUpperCase maps it to 'I', so this is NOT 'ANTHROPIC'.
  ['anthropic-dotless-i', 'anthropıc'],
  ['openai', 'OPENAI'],
  ['ollama', 'OLLAMA'],
  ['google', 'GOOGLE'],
  ['deepseek', 'DEEPSEEK'],
  ['openrouter', 'OPENROUTER'],
  ['grok', 'GROK'],
  ['custom-plugin', 'some-plugin-provider'],
  ['empty-string', ''],
  ['null', null],
  ['undefined', undefined],
];

for (const [id, provider] of PROVIDERS) {
  rows.push({
    kind: 'default',
    id,
    provider: provider === undefined ? '<undefined>' : provider,
    out: defaultMultiCharacterPrefill(provider),
  });
}

/** The stored column's states, including the non-boolean fall-throughs. */
const STORED: Array<[string, unknown]> = [
  ['true', true],
  ['false', false],
  ['null', null],
  ['absent', '<absent>'],
  ['number-1', 1],
  ['number-0', 0],
  ['string-true', 'true'],
  ['string-empty', ''],
];

for (const [pid, provider] of PROVIDERS) {
  for (const [sid, stored] of STORED) {
    const profile: Record<string, unknown> = {};
    if (provider !== undefined) profile.provider = provider;
    if (stored !== '<absent>') profile.multiCharacterPrefill = stored;
    rows.push({
      kind: 'resolve',
      id: `${pid}/${sid}`,
      provider: provider === undefined ? '<undefined>' : provider,
      stored: stored === '<absent>' ? '<absent>' : stored,
      out: profileUsesNamePrefill(
        profile as { provider?: string | null; multiCharacterPrefill?: boolean | null }
      ),
    });
  }
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
