/**
 * Oracle case: the per-profile image cap's two accessors (P4.D362, v4
 * `d439f6dc9`) — `resolveMaxImagesPerRequest(profile)` and
 * `parseMaxImagesPerRequest(value)` from v4's REAL
 * `lib/llm/connection-profile-utils.ts`, over one fixed corpus, one NDJSON
 * line per row. The Rust differential (`max_images_per_request_equivalence`)
 * feeds each row's `input` through `quilltap_core::services::
 * connection_profiles` and compares exactly (tier-1).
 *
 * `input` is the JSON value under test; `absent: true` means the profile has
 * no `maxImagesPerRequest` key at all (resolve) / the value is `undefined`
 * (parse). A row whose input is not JSON-representable (`NaN`) is built from
 * a string the Rust side parses the same way.
 *
 * Run (Node 24, from the v4 checkout so `@/` resolves):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/max-images-per-request.ts \
 *     > /tmp/oracle-max-images-per-request.ndjson
 */

import {
  resolveMaxImagesPerRequest,
  parseMaxImagesPerRequest,
} from '@/lib/llm/connection-profile-utils';
import {
  DEFAULT_MAX_IMAGES_PER_REQUEST,
  MAX_IMAGES_PER_REQUEST_CEILING,
} from '@/lib/schemas/profile.types';

const CORPUS: Array<{ name: string; input?: unknown; absent?: true }> = [
  { name: 'absent', absent: true },
  { name: 'null', input: null },
  { name: 'zero', input: 0 },
  { name: 'one', input: 1 },
  { name: 'four', input: 4 },
  { name: 'fifty', input: 50 },
  { name: 'fifty-one', input: 51 },
  { name: 'fraction', input: 4.5 },
  { name: 'four-point-zero', input: 4.0 },
  { name: 'negative', input: -1 },
  { name: 'string-four', input: '4' },
  { name: 'string-padded', input: ' 4 ' },
  { name: 'string-hex', input: '0x10' },
  { name: 'string-exponent', input: '4e0' },
  { name: 'string-empty', input: '' },
  { name: 'string-blank', input: '  ' },
  { name: 'string-nan', input: 'x' },
  { name: 'string-six', input: '6' },
  { name: 'string-fraction', input: '4.5' },
  { name: 'string-fifty-one', input: '51' },
  { name: 'bool-true', input: true },
  { name: 'array', input: [4] },
  { name: 'object', input: {} },
  { name: 'huge', input: 1e21 },
  { name: 'string-tab-newline', input: '\t7\n' },
];

console.log(
  JSON.stringify({
    name: '__constants',
    DEFAULT_MAX_IMAGES_PER_REQUEST,
    MAX_IMAGES_PER_REQUEST_CEILING,
  })
);
for (const row of CORPUS) {
  const value = row.absent ? undefined : row.input;
  const profile = row.absent ? {} : { maxImagesPerRequest: value as number };
  console.log(
    JSON.stringify({
      name: row.name,
      absent: row.absent ?? false,
      input: row.absent ? null : row.input,
      resolved: resolveMaxImagesPerRequest(profile),
      parsed: parseMaxImagesPerRequest(value),
    })
  );
}
