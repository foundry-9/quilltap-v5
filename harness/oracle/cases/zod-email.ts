/**
 * Oracle case: `z.email()` — the verdict v4's REAL zod returns for a fixed
 * corpus of address-shaped strings.
 *
 * v5 hand-rolls this validator once, at `api/user_profile.rs::is_zod_email`,
 * for v4's `email: z.email().optional()` on `PUT /api/v1/user/profile`
 * (`app/api/v1/user/profile/route.ts:32`) and on `lib/schemas/auth.types.ts:23`.
 * The verdict is user-visible: a rejected address answers 400 with v4's flat
 * `Validation error` envelope and the column is never written, so an
 * over-permissive port PERSISTS an address v4 refuses.
 *
 * This drives v4's real `zod` — the same install every other oracle resolves —
 * rather than transcribing `v4/core/regexes.js`, so the corpus re-records with
 * the dependency instead of rotting beside it (the `zod_version_guard`
 * obligation this case was written under: P4.D211, zod 4.5.4 -> 4.6.5, where
 * the `email` regex was rewritten from the lookahead form to a grouped one).
 *
 * The corpus is a deterministic cross product (local parts x domains) plus a
 * hand list of shapes a cross product cannot spell — no randomness, no clock.
 * Both sides read the SAME `input` strings, so the Rust side does not
 * re-derive them: it replays the recorded rows.
 *
 * Run (Node 24). Regenerate from the CHECKOUT: a `/tmp` pin never survives the
 * round that made it, which is the sweep driver's `stale_v4_pin_path` refusal —
 * when the round's baseline is behind v4 HEAD the pin comes from the driver's
 * `--v4 <pin>` instead, not from this header.
 *   cd ~/source/quilltap-server
 *   TZ=UTC npx tsx <V5>/harness/oracle/cases/zod-email.ts \
 *     > /tmp/oracle-zod-email.ndjson
 * Rust side:
 *   QT_ORACLE_ZOD_EMAIL=/tmp/oracle-zod-email.ndjson \
 *     cargo test -p quilltap-harness --test zod_email_equivalence
 */

import { pathToFileURL } from 'node:url';

import { UserSchema } from '@/lib/schemas/auth.types';

/**
 * v4's own `zod`, resolved from the CHECKOUT this case is run in rather than
 * from the case file's own directory — a bare `import { z } from 'zod'` here
 * resolves relative to the v5 worktree, where there is no `node_modules`, and
 * dies before the first row. Every other oracle case gets v4's zod for free by
 * importing a v4 module that imports it; this one needs `z.email()` itself,
 * which no v4 module exports.
 */
const { z } = (await import(
  new URL('node_modules/zod/index.js', pathToFileURL(`${process.cwd()}/`)).href
)) as typeof import('zod');

/**
 * Local parts, chosen to exercise every position of 4.6's grammar
 * `(?:[A-Za-z0-9_'+\-]+\.)*[A-Za-z0-9_'+\-]*[A-Za-z0-9_+-]` — the dot-separated
 * run, the optional tail, and the FINAL character class, which (unlike the
 * others) admits neither `'` nor `.`.
 */
const LOCALS = [
  'a',
  'abc',
  'charlie',
  'a.b',
  'a.b.c',
  'first.last',
  'a_b',
  'a+b',
  'a-b',
  "a'b",
  "o'brien",
  'A1',
  '1',
  '_',
  '+',
  '-',
  "'",
  "'''",
  "''.",
  '.a',
  'a.',
  'a..b',
  '..',
  '',
  'a b',
  ' a',
  'a ',
  'a"b',
  'a;b',
  'a,b',
  'a(b',
  'a:b',
  'a\\b',
  'ä',
  'aéb',
  'a.b.',
  '.a.b',
  "a.'",
  'a+b.c-d_e',
  'x'.repeat(65),
];

/**
 * Domains, exercising the label class `[A-Za-z0-9][A-Za-z0-9\-]*` (which 4.5's
 * form spelled identically) and the `[A-Za-z]{2,}$` last label.
 */
const DOMAINS = [
  'b.co',
  'foundry-9.com',
  'a.b.co',
  'sub.domain.example.org',
  'x.museum',
  'B.CO',
  'b.c',
  'b.1',
  'b.123',
  'b.co1',
  'b.c-o',
  'b',
  '',
  '.com',
  'b..co',
  '-b.co',
  'b-.co',
  'b.-co',
  'b .co',
  '9b.co',
  'b_c.co',
  "b'c.co",
  'b.co.',
  '[1.2.3.4]',
  'ä.co',
];

const rows: string[] = [];
for (const local of LOCALS) {
  for (const domain of DOMAINS) {
    rows.push(`${local}@${domain}`);
  }
}

/** Shapes the cross product cannot spell: no `@`, several `@`, stray control bytes. */
rows.push(
  '',
  'nope',
  '@',
  '@b.co',
  'a@',
  'a@@b.co',
  'a@b@c.co',
  'a@b.co@d.co',
  'charlie@foundry-9.com',
  'reginald@foundry-9.com',
  'not-an-email',
  'a@b.co ',
  ' a@b.co',
  'a@b.co\n',
  '\na@b.co',
  'a@b.co\t',
  'a\n@b.co',
  'a@b\n.co',
  'a@b.co\u0000',
);

/** The validator the profile route builds inline (`z.email().optional()`). */
const schema = z.email();
/**
 * v4's other live site, exported so it can be driven directly:
 * `UserSchema.shape.email` is `z.email().nullable().optional()` — the users
 * repo's own gate (`lib/schemas/auth.types.ts:23`), which `db/users.rs:52`
 * mirrors. Every corpus input is a STRING, so the nullable/optional wrapper is
 * transparent and the two must agree on every row; the case ASSERTS that
 * rather than assuming it, so a wrapper that started swallowing a verdict
 * fails here instead of quietly widening the comparand.
 */
const viaUserSchema = UserSchema.shape.email;

const seen = new Set<string>();
for (const input of rows) {
  if (seen.has(input)) continue;
  seen.add(input);
  const result = schema.safeParse(input);
  const wrapped = viaUserSchema.safeParse(input);
  if (wrapped.success !== result.success) {
    throw new Error(
      `UserSchema.shape.email disagreed with z.email() on ${JSON.stringify(input)}: ` +
        `${wrapped.success} vs ${result.success}`,
    );
  }
  process.stdout.write(
    JSON.stringify({
      input,
      valid: result.success,
      // Constant in practice, recorded so a sentence change is visible rather
      // than silent (the same reason the Pascal corpus carries its sentences).
      message: result.success ? null : (result.error.issues[0]?.message ?? null),
    }) + '\n',
  );
}

// The wrapper IS a wrapper: it accepts what `z.email()` alone refuses.
for (const nullish of [null, undefined]) {
  if (!viaUserSchema.safeParse(nullish).success) {
    throw new Error(`UserSchema.shape.email rejected ${String(nullish)} — not the expected wrapper`);
  }
  if (schema.safeParse(nullish).success) {
    throw new Error(`z.email() accepted ${String(nullish)} — the two comparands are the same schema`);
  }
}
