/**
 * The v4-side recorder behind `src/app/progressions/schema.oracle.spec.ts`.
 *
 * The SPA has no `zod`, so `src/app/progressions/schema.ts` reimplements the
 * slice of Zod 4.5.4 that v4's `ProgressionSchema` uses. That port cannot be
 * verified by inspection: the editor modal renders
 * `${issue.path.join('.') || 'This entry'}: ${issue.message}` straight to the
 * author, so a browser phrasing a rejection differently would disagree with the
 * server about the same entry.
 *
 * This drives v4's REAL `lib/progressions/schema.ts` over a fixed corpus and
 * emits NDJSON: the verdict, the whole joined rejection sentence (paths and
 * ORDER included) and, on an accept, `JSON.stringify` of the parsed data — so
 * key order and the omitted optionals are pinned too.
 *
 * Every specimen is expressed as the BYTES of a `metadata.json` fragment,
 * never a re-stringified structure, because that is how the value actually
 * reaches a validator. One consequence, recorded rather than worked around:
 * **`NaN` is not expressible in JSON**, so the `received NaN` arm is unreachable
 * from a real vault file and is not in the corpus; the SPA spec pins it by hand
 * against the measurement noted there. `Infinity` IS reachable — `1e999`
 * overflows on parse — and is in the corpus.
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/lib/...` and
 * would not compile in the SPA's own tsconfig.
 *
 * Run it from a pinned v4 worktree (Node 24 at `~/.nvm/versions/node/v24.13.1/bin`):
 *
 * ```bash
 * PIN=/tmp/qt-v4-pin-p4d170-25f534c0b
 * git -C ~/source/quilltap-server worktree add --detach "$PIN" 25f534c0b
 * ln -sfn ~/source/quilltap-server/node_modules "$PIN/node_modules"
 * cp <V5>/apps/web/oracle/progressions-schema.recorder.ts "$PIN/"
 * cd "$PIN" && npx tsx progressions-schema.recorder.ts \
 *   > <V5>/apps/web/src/testing/fixtures/progressions-schema.oracle.ndjson
 * ```
 *
 * Expect 71 lines; a shorter file means the recorder errored and the redirect
 * already truncated the old one (the empty-file trap).
 */

import { ProgressionSchema, ProgressionsSchema } from '@/lib/progressions/schema';

/** The narrowest progression the schema will accept — v4's own `BASE`. */
const BASE = `"name":"Cannon recharge","startTime":"2026-09-08T14:02:10Z","endTime":"2026-09-08T14:12:10Z","timeIncrement":"minute"`;
const BACKWARDS = `"endTime":"2026-09-08T14:00:00Z"`;
const x = (n: number) => 'x'.repeat(n);

/** `[id, the entry's JSON bytes]`, one progression each. */
const ENTRIES: Array<[string, string]> = [
  // --- accepted
  ['accept-narrowest', `{${BASE}}`],
  [
    'accept-furnished',
    `{${BASE},"description":"You are carrying a child.","percentageReport":false,"reportFrequency":"1h","quantity":{"total":1.0,"unit":"MJ","precision":1},"reportTemplate":"{{description}} You are {{elapsedWhole}} along; due in {{remaining}}.","onComplete":"once","updatedAt":"2026-09-08T14:02:10Z"}`,
  ],
  ['accept-quantity-default-precision', `{${BASE},"quantity":{"total":1,"unit":"MJ"}}`],
  ['accept-name-80', `{${BASE},"name":"${x(80)}"}`],
  ['accept-name-80-astral', `{${BASE},"name":"${'\\ud83d\\ude00'.repeat(80)}"}`],
  ['accept-template-500', `{${BASE},"reportTemplate":"${x(500)}"}`],
  ['accept-freq-90m', `{${BASE},"reportFrequency":"90m"}`],
  ['accept-iso-space-separator', `{${BASE},"startTime":"2026-09-08 14:02:10Z"}`],
  ['accept-iso-lowercase-t-z', `{${BASE},"startTime":"2026-09-08t14:02:10z"}`],
  ['accept-iso-offset-no-colon', `{${BASE},"startTime":"2026-09-08T14:02:10-0500"}`],
  ['accept-iso-minute-precision', `{${BASE},"startTime":"2026-09-08T14:02Z"}`],
  ['accept-iso-nine-fractional-digits', `{${BASE},"startTime":"2026-09-08T14:02:10.123456789Z"}`],
  ['accept-iso-surrounding-whitespace', `{${BASE},"startTime":"  2026-09-08T14:02:10Z  "}`],
  ['accept-precision-zero', `{${BASE},"quantity":{"total":1,"unit":"MJ","precision":0}}`],
  ['accept-precision-six', `{${BASE},"quantity":{"total":1,"unit":"MJ","precision":6}}`],

  // --- the shape
  ['reject-not-an-object', `"a string"`],
  ['reject-null', `null`],
  ['reject-array', `[]`],
  ['reject-unknown-key', `{${BASE},"stages":[]}`],
  ['reject-unknown-keys-two', `{${BASE},"stages":[],"phases":1}`],
  ['reject-unknown-key-and-backwards-span', `{${BASE},"stages":[],${BACKWARDS}}`],
  [
    'reject-missing-name',
    `{"startTime":"2026-09-08T14:02:10Z","endTime":"2026-09-08T14:12:10Z","timeIncrement":"minute"}`,
  ],
  ['reject-missing-everything', `{}`],

  // --- the fields
  ['reject-name-empty', `{${BASE},"name":""}`],
  ['reject-name-81', `{${BASE},"name":"${x(81)}"}`],
  ['reject-name-81-astral', `{${BASE},"name":"${'\\ud83d\\ude00'.repeat(81)}"}`],
  ['reject-name-wrong-type', `{${BASE},"name":5,${BACKWARDS}}`],
  ['reject-description-501', `{${BASE},"description":"${x(501)}"}`],
  ['reject-increment-unknown', `{${BASE},"timeIncrement":"fortnight"}`],
  ['reject-increment-wrong-type', `{${BASE},"timeIncrement":5}`],
  ['reject-oncomplete-unknown', `{${BASE},"onComplete":"delete"}`],
  ['reject-percentage-wrong-type', `{${BASE},"percentageReport":"yes"}`],
  ['reject-template-empty', `{${BASE},"reportTemplate":""}`],
  ['reject-template-501', `{${BASE},"reportTemplate":"${x(501)}"}`],

  // --- the cadence grammar
  ['reject-freq-cron', `{${BASE},"reportFrequency":"0 * * * *"}`],
  ['reject-freq-zero-count', `{${BASE},"reportFrequency":"0h"}`],
  ['reject-freq-leading-zero', `{${BASE},"reportFrequency":"01h"}`],
  ['reject-freq-bare-unit', `{${BASE},"reportFrequency":"h"}`],
  ['reject-freq-month', `{${BASE},"reportFrequency":"1M"}`],
  ['reject-freq-wrong-type', `{${BASE},"reportFrequency":5}`],
  ['reject-freq-and-backwards-span', `{${BASE},"reportFrequency":"nope",${BACKWARDS}}`],

  // --- the instants
  ['reject-start-zoneless', `{${BASE},"startTime":"2026-09-08T14:02:10"}`],
  [
    'reject-both-zoneless',
    `{${BASE},"startTime":"2026-09-08T14:02:10","endTime":"2026-09-08T14:00:00"}`,
  ],
  ['reject-start-wrong-type', `{${BASE},"startTime":12345}`],
  ['reject-start-bare-date', `{${BASE},"startTime":"2026-09-08"}`],
  ['reject-start-ten-fractional-digits', `{${BASE},"startTime":"2026-09-08T14:02:10.1234567890Z"}`],
  ['reject-start-impossible-day', `{${BASE},"startTime":"2026-13-45T00:00:00Z"}`],
  ['reject-updated-at-prose', `{${BASE},"updatedAt":"sometime last Tuesday"}`],
  ['reject-end-equals-start', `{${BASE},"endTime":"2026-09-08T14:02:10Z"}`],
  ['reject-end-before-start', `{${BASE},${BACKWARDS}}`],

  // --- the quantity block
  ['reject-quantity-total-zero', `{${BASE},"quantity":{"total":0,"unit":"MJ"},${BACKWARDS}}`],
  ['reject-quantity-total-negative', `{${BASE},"quantity":{"total":-1,"unit":"MJ"}}`],
  [
    'reject-quantity-total-overflows-to-infinity',
    `{${BASE},"quantity":{"total":1e999,"unit":"MJ"}}`,
  ],
  ['reject-quantity-total-overflows-negative', `{${BASE},"quantity":{"total":-1e999,"unit":"MJ"}}`],
  ['reject-quantity-unit-empty', `{${BASE},"quantity":{"total":1,"unit":""}}`],
  ['reject-quantity-unit-17', `{${BASE},"quantity":{"total":1,"unit":"${x(17)}"}}`],
  [
    'reject-quantity-precision-fractional',
    `{${BASE},"quantity":{"total":1,"unit":"MJ","precision":1.5},${BACKWARDS}}`,
  ],
  [
    'reject-quantity-precision-seven',
    `{${BASE},"quantity":{"total":1,"unit":"MJ","precision":9},${BACKWARDS}}`,
  ],
  [
    'reject-quantity-precision-negative',
    `{${BASE},"quantity":{"total":1,"unit":"MJ","precision":-1}}`,
  ],
  [
    'reject-quantity-precision-overflows',
    `{${BASE},"quantity":{"total":1,"unit":"MJ","precision":1e999}}`,
  ],
  [
    'reject-quantity-unknown-key',
    `{${BASE},"quantity":{"total":1,"unit":"MJ","zz":1},${BACKWARDS}}`,
  ],
  ['reject-quantity-wrong-type', `{${BASE},"quantity":"lots",${BACKWARDS}}`],
];

/** `[id, the whole reserved key's JSON bytes]`. */
const RECORDS: Array<[string, string]> = [
  ['record-accept-two', `{"cannon":{${BASE}},"pregnancy":{${BASE}}}`],
  ['record-accept-empty', `{}`],
  [
    'record-accept-32',
    JSON.stringify(
      Object.fromEntries(Array.from({ length: 32 }, (_, i) => [`p${i}`, JSON.parse(`{${BASE}}`)])),
    ),
  ],
  [
    'record-reject-33',
    JSON.stringify(
      Object.fromEntries(Array.from({ length: 33 }, (_, i) => [`p${i}`, JSON.parse(`{${BASE}}`)])),
    ),
  ],
  ['record-reject-bad-key', `{"Cannon Recharge":{${BASE}}}`],
  ['record-reject-bad-key-and-bad-value', `{"Cannon Recharge":{${BASE},"name":""}}`],
  ['record-reject-two-bad-keys', `{"A":{${BASE}},"b c":{${BASE}}}`],
  ['record-reject-bad-value', `{"cannon":{${BASE},"name":""}}`],
  ['record-reject-array', `[]`],
];

function sentence(issues: Array<{ path: PropertyKey[]; message: string }>): string {
  return issues.map((i) => `${i.path.join('.') || '(root)'}: ${i.message}`).join('; ');
}

for (const [id, inputJson] of ENTRIES) {
  const result = ProgressionSchema.safeParse(JSON.parse(inputJson));
  console.log(
    JSON.stringify({
      kind: 'progression',
      id,
      inputJson,
      success: result.success,
      reason: result.success ? null : sentence(result.error.issues),
      data: result.success ? JSON.stringify(result.data) : null,
    }),
  );
}

for (const [id, inputJson] of RECORDS) {
  const result = ProgressionsSchema.safeParse(JSON.parse(inputJson));
  console.log(
    JSON.stringify({
      kind: 'progressions',
      id,
      inputJson,
      success: result.success,
      reason: result.success ? null : sentence(result.error.issues),
      data: result.success ? JSON.stringify(result.data) : null,
    }),
  );
}
