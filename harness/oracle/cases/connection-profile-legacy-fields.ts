/**
 * P4.D126 unit 3 — the legacy connection-profile column seeding (v4
 * `e000d6bfc`, bug 103).
 *
 * Drives v4's REAL module:
 *   seedLegacyConnectionProfileFields (lib/llm/connection-profile-legacy-fields.ts)
 *
 * Tier-1 exact. Both restore and `.qtap` import call this one function, so
 * pinning it is what makes the shared-helper claim ("the two paths cannot
 * drift") a measurement rather than a reading.
 *
 * The corpus is providers × the two columns' stored states. What it pins that
 * a hand-read would miss:
 *
 *   - the condition is `=== undefined`, so a stored `false` and a stored `null`
 *     are BOTH left alone — a truthiness test would flip a GOOGLE profile whose
 *     owner deliberately turned vision off;
 *   - the provider lookup is `(profile.provider ?? '').toUpperCase()`, so
 *     casing folds AND a record with no provider at all resolves to the empty
 *     string rather than throwing (whitespace does not fold: `' OPENAI'` is not
 *     in the map);
 *   - `multiCharacterPrefill` is seeded as an explicit `null`, never as a
 *     boolean — it is the "never chosen" tri-state, not a default;
 *   - a NON-boolean stored value is left exactly as it was: the helper does not
 *     coerce, it only fills.
 *
 * Emitted per row: the two seeded values, whether each key ENDED UP present
 * (v4's return is a copy, so this catches a helper that dropped a key), and the
 * two `=== undefined` flags both call sites read for their debug lines.
 *
 * P4.D135 adds a SECOND corpus block (`kind: 'fallback'`) for the 4.10
 * fallback-chain pair (v4 `65f5021c8`). It is a separate cross product rather
 * than a widening of the first, so every pre-existing row stays byte-identical.
 * What it pins:
 *
 *   - both keys' `=== undefined` fill, and that a stored value — a `false`, an
 *     explicit `null` — is never touched;
 *   - the self-reference strip, whose gate is JS-TRUTHY
 *     (`seeded.fallbackProfileId && seeded.fallbackProfileId === seeded.id`):
 *     an empty-string id equal to an empty-string fallback is NOT stripped,
 *     which a `==`-style transcription would get wrong;
 *   - that `allowTierFallback` is filled with a literal `false`, never coerced
 *     from a non-boolean the archive carried.
 *
 * Run from the v4 checkout (pin a detached worktree via
 * `recipe_sweep.py --v4` when v4 HEAD has moved past the baseline):
 *   cd ~/source/quilltap-server
 *   ~/.nvm/versions/node/v24.13.1/bin/npx tsx \
 *     ~/source/quilltap-v5/harness/oracle/cases/connection-profile-legacy-fields.ts \
 *     > /tmp/oracle-cp-legacy-fields.ndjson
 */

import { seedLegacyConnectionProfileFields } from '@/lib/llm/connection-profile-legacy-fields';

const rows: unknown[] = [];

/** The stand-ins the NDJSON needs for values JSON cannot carry. */
const ABSENT = '<absent>';
const UNDEFINED = '<undefined>';

/** Providers: the whole historic map, the casing boundaries, and the misses. */
const PROVIDERS: Array<[string, string | null | undefined]> = [
  ['openai', 'OPENAI'],
  ['anthropic', 'ANTHROPIC'],
  ['google', 'GOOGLE'],
  ['grok', 'GROK'],
  ['openai-lower', 'openai'],
  ['anthropic-mixed', 'AnThRoPiC'],
  ['google-leading-space', ' GOOGLE'],
  ['grok-trailing-space', 'GROK '],
  ['ollama', 'OLLAMA'],
  ['openrouter', 'OPENROUTER'],
  ['openai-compatible', 'OPENAI_COMPATIBLE'],
  ['deepseek', 'DEEPSEEK'],
  ['zai', 'Z_AI'],
  ['nanogpt', 'NANOGPT'],
  ['empty', ''],
  ['null', null],
  ['undefined', undefined],
];

/** `supportsImageUpload`'s stored states. */
const IMAGE: Array<[string, unknown]> = [
  ['absent', ABSENT],
  ['true', true],
  ['false', false],
];

/** `multiCharacterPrefill`'s stored states, non-boolean fall-throughs included. */
const PREFILL: Array<[string, unknown]> = [
  ['absent', ABSENT],
  ['true', true],
  ['false', false],
  ['null', null],
  ['number-1', 1],
  ['string-true', 'true'],
];

for (const [pid, provider] of PROVIDERS) {
  for (const [iid, image] of IMAGE) {
    for (const [mid, prefill] of PREFILL) {
      const profile: Record<string, unknown> = { id: 'p1', name: 'A Profile' };
      if (provider !== undefined) profile.provider = provider;
      if (image !== ABSENT) profile.supportsImageUpload = image;
      if (prefill !== ABSENT) profile.multiCharacterPrefill = prefill;

      const seeded = seedLegacyConnectionProfileFields(profile) as Record<string, unknown>;

      rows.push({
        kind: 'seed',
        id: `${pid}/img-${iid}/pre-${mid}`,
        provider: provider === undefined ? UNDEFINED : provider,
        storedImage: image,
        storedPrefill: prefill,
        // What the helper decided.
        supportsImageUpload:
          'supportsImageUpload' in seeded ? seeded.supportsImageUpload : ABSENT,
        multiCharacterPrefill:
          'multiCharacterPrefill' in seeded ? seeded.multiCharacterPrefill : ABSENT,
        // The two `=== undefined` flags both call sites read for their debug line.
        seededSupportsImageUpload: profile.supportsImageUpload === undefined,
        seededMultiCharacterPrefill: profile.multiCharacterPrefill === undefined,
        // The helper returns a COPY: the input must be untouched.
        inputUntouched:
          ('supportsImageUpload' in profile ? profile.supportsImageUpload : ABSENT) === image &&
          ('multiCharacterPrefill' in profile ? profile.multiCharacterPrefill : ABSENT) ===
            prefill,
      });
    }
  }
}

// ── P4.D135: the 4.10 fallback-chain pair ───────────────────────────────────

/** `fallbackProfileId`'s stored states, including the self-reference. */
const FALLBACK: Array<[string, unknown]> = [
  ['absent', ABSENT],
  ['null', null],
  ['other', 'p-understudy'],
  ['self', 'p1'],
  ['empty', ''],
  ['number', 7],
];

/** `allowTierFallback`'s stored states, non-boolean fall-throughs included. */
const TIER: Array<[string, unknown]> = [
  ['absent', ABSENT],
  ['true', true],
  ['false', false],
  ['null', null],
  ['number-1', 1],
  ['string-true', 'true'],
];

/** The record's own `id` — the other half of the self-reference comparison. */
const IDS: Array<[string, unknown]> = [
  ['p1', 'p1'],
  ['empty-id', ''],
  ['absent-id', ABSENT],
];

for (const [idid, id] of IDS) {
  for (const [fid, fallback] of FALLBACK) {
    for (const [tid, tier] of TIER) {
      const profile: Record<string, unknown> = { name: 'A Profile', provider: 'OPENAI' };
      if (id !== ABSENT) profile.id = id;
      if (fallback !== ABSENT) profile.fallbackProfileId = fallback;
      if (tier !== ABSENT) profile.allowTierFallback = tier;

      const seeded = seedLegacyConnectionProfileFields(profile) as Record<string, unknown>;

      rows.push({
        kind: 'fallback',
        id: `${idid}/fb-${fid}/tier-${tid}`,
        recordId: id === ABSENT ? ABSENT : id,
        storedFallback: fallback,
        storedTier: tier,
        fallbackProfileId:
          'fallbackProfileId' in seeded ? seeded.fallbackProfileId : ABSENT,
        allowTierFallback:
          'allowTierFallback' in seeded ? seeded.allowTierFallback : ABSENT,
        seededFallbackProfileId: profile.fallbackProfileId === undefined,
        seededAllowTierFallback: profile.allowTierFallback === undefined,
      });
    }
  }
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
