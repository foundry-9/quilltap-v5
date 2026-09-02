/**
 * Oracle case: the HuggingFace LoRA lookup (v4 `2ece98c90`, the LoRA train's
 * third commit).
 *
 * Drives the REAL `lib/image-gen/huggingface-repo-id.ts` +
 * `lib/image-gen/huggingface-lookup.ts` — `extractHuggingFaceRepoId`,
 * `huggingFaceCardUrl` and `lookupHuggingFaceLora` — over a fixed corpus, with
 * `global.fetch` replaced by a canned responder (the `record-web-search-wire`
 * precedent: v4's real module, a mocked transport underneath it). Each network
 * row records the REQUEST as well as the result, so v5's request building —
 * the URL and whether an `Authorization` header was attached — is a comparand
 * and not an assumption.
 *
 * IMPORTANT — this imports the actual app code, it does not reimplement it.
 * Run it from inside the server checkout so `@/` path aliases resolve:
 *
 *   cd ~/source/quilltap-server
 *   QT_ORACLE=~/source/quilltap-v5/harness/oracle \
 *   QT_ORACLE_OUT=/tmp/oracle-hf-lora.ndjson \
 *     npx tsx "$QT_ORACLE/cases/huggingface-lora-lookup.ts"
 *
 * `QT_ORACLE_OUT` is REQUIRED here, unlike the older pure cases: this module
 * logs (v4's `lookupHuggingFaceLora` narrates every declined lookup), and the
 * app logger writes JSON lines to stdout — so a `>` redirect would interleave
 * log lines with corpus rows. `LOG_LEVEL=error` is set below as well, but the
 * file sink is what makes the corpus independent of that.
 *
 * The corpus is fixed in code: no randomness, no clock, and every network row
 * names its canned status + body, so the oracle is reproducible and the Rust
 * side drives the identical corpus through `CannedWireTransport`.
 */

import { writeFileSync } from 'node:fs';

// Set before anything logs: v4's lookup narrates at debug/info/warn, and those
// lines share stdout with the corpus.
process.env.LOG_LEVEL = 'error';

import {
  extractHuggingFaceRepoId,
  huggingFaceCardUrl,
  lookupHuggingFaceLora,
} from '@/lib/image-gen/huggingface-lookup';

/** One recorded fetch, so the request is diffed and not assumed. */
interface SeenRequest {
  url: string;
  /** Sorted `name: value` pairs — the header ORDER is the transport's, not v4's. */
  headers: string[];
}

const rows: unknown[] = [];
function emit(row: Record<string, unknown>): void {
  rows.push(row);
}

// ---- the pure half: repo-id extraction + the card URL --------------------
//
// v4's own suite (`__tests__/unit/image-gen/huggingface-lookup.test.ts`) is the
// corpus SHAPE; these drive the real function rather than restating its table,
// and add the edges that suite leaves implicit (a port with a path, an uppercase
// host, a subdomain, a trailing slash, a dot-leading segment).
const REPO_ID_CASES: string[] = [
  'XLabs-AI/flux-RealismLora',
  '  Datou1111/shou_xin  ',
  'ostris/flux2_berthe_morisot',
  'https://huggingface.co/lovis93/Flux-2-Multi-Angles-LoRA-v2/resolve/main/weights-fal.safetensors',
  'https://huggingface.co/owner/name',
  'https://cdn.example.com/weights.safetensors',
  '',
  '   ',
  'justonesegment',
  'too/many/segments',
  'owner name/with space',
  'https://huggingface.co/owner',
  'https://nothuggingface.co/owner/name',
  // Beyond v4's suite — the arms its regex and URL parse decide silently.
  'HTTPS://HUGGINGFACE.CO/Owner/Name',
  'https://cdn-lfs.huggingface.co/owner/name',
  'http://huggingface.co/owner/name',
  'https://huggingface.co/owner/name/',
  'https://huggingface.co:443/owner/name',
  'https://huggingface.co/.hidden/name',
  '.hidden/name',
  'owner/.hidden',
  'owner/name.safetensors',
  'a/b',
  'owner//name',
  'not a url at all',
  'https://huggingface.co',
  // The `new URL()` throw arms — v4 catches and answers null.
  'https://',
  'http://[bad',
  'https:// huggingface.co/owner/name',
  // The prefix test is case-insensitive but anchored, so a scheme-ish prefix
  // inside the string is not a URL.
  'owner/https://name',
  // A percent-encoded segment: `pathname` keeps the encoding, so the candidate
  // is the encoded text and the pattern decides on THAT.
  'https://huggingface.co/ow%2Fner/name',
  'https://huggingface.co/owner/na%20me',
  // WHATWG special-scheme arms a hand-written stand-in gets wrong (the
  // P4.D138 follow-up review): dot segments resolve, `\\` separates, the host
  // percent-decodes, and a bad port throws.
  'https://huggingface.co/./owner/name',
  'https://huggingface.co/a/../owner/name',
  'https://huggingface.co\\owner\\name',
  'https://huggingface%2Eco/owner/name',
  'https://huggingface.co:abc/owner/name',
  'https://huggingface.co:99999/owner/name',
];

REPO_ID_CASES.forEach((source, index) => {
  const repoId = extractHuggingFaceRepoId(source);
  emit({
    kind: 'repo_id',
    name: `repo_id_${index}`,
    source,
    repoId,
    cardUrl: repoId === null ? null : huggingFaceCardUrl(repoId),
  });
});

// ---- the network half ----------------------------------------------------

const REALISM_PAYLOAD = {
  id: 'XLabs-AI/flux-RealismLora',
  tags: ['diffusers', 'lora', 'text-to-image', 'base_model:adapter:black-forest-labs/FLUX.1-dev'],
  pipeline_tag: 'text-to-image',
  cardData: { base_model: 'black-forest-labs/FLUX.1-dev' },
  siblings: [
    { rfilename: 'README.md' },
    { rfilename: 'lora.safetensors' },
    { rfilename: 'config.json' },
  ],
  downloads: 90210,
  likes: 1232,
  lastModified: '2026-05-01T12:00:00.000Z',
};

interface NetCase {
  name: string;
  source: string;
  token?: string;
  status: number;
  statusText?: string;
  /** The literal body text; `null` throws on the transport instead. */
  body: string | null;
  /** A transport-level throw with this `name`, when `body` is null. */
  throwName?: string;
  throwMessage?: string;
}

const NET_CASES: NetCase[] = [
  {
    name: 'facts_from_a_real_payload',
    source: 'XLabs-AI/flux-RealismLora',
    status: 200,
    body: JSON.stringify(REALISM_PAYLOAD),
  },
  {
    name: 'trigger_phrase_is_the_point_of_the_button',
    source: 'Datou1111/shou_xin',
    status: 200,
    body: JSON.stringify({
      id: 'Datou1111/shou_xin',
      tags: ['lora'],
      cardData: { instance_prompt: '  shou_xin, pencil sketch  ' },
      siblings: [{ rfilename: 'shou_xin.safetensors' }],
    }),
  },
  {
    name: 'instance_prompt_as_a_list_takes_the_first_non_blank',
    source: 'owner/list-prompt',
    status: 200,
    body: JSON.stringify({
      id: 'owner/list-prompt',
      cardData: { instance_prompt: ['', '   ', ' second one ', 'third'] },
    }),
  },
  {
    name: 'instance_prompt_of_the_wrong_type_is_absent',
    source: 'owner/number-prompt',
    status: 200,
    body: JSON.stringify({ id: 'owner/number-prompt', cardData: { instance_prompt: 42 } }),
  },
  {
    name: 'list_base_model_merges_with_adapter_tags_without_duplicating',
    source: 'owner/multi-base',
    status: 200,
    body: JSON.stringify({
      id: 'owner/multi-base',
      tags: [
        'base_model:adapter:black-forest-labs/FLUX.1-dev',
        'base_model:adapter:stabilityai/sd-3.5',
        'base_model:adapter:',
        'LoRA',
      ],
      cardData: { base_model: ['  black-forest-labs/FLUX.1-dev  ', '', 'other/base'] },
    }),
  },
  {
    name: 'lora_tag_is_matched_case_insensitively',
    source: 'owner/upper-lora',
    status: 200,
    body: JSON.stringify({ id: 'owner/upper-lora', tags: ['LoRA', 7, null] }),
  },
  {
    name: 'gated_repository_reports_its_gate_mode',
    source: 'owner/gated',
    status: 200,
    body: JSON.stringify({ id: 'owner/gated', gated: 'manual', tags: ['lora'] }),
  },
  {
    name: 'gated_false_stays_false',
    source: 'owner/ungated',
    status: 200,
    body: JSON.stringify({ id: 'owner/ungated', gated: false }),
  },
  {
    name: 'every_safetensors_is_named_so_ambiguity_shows',
    source: 'owner/ambiguous',
    status: 200,
    body: JSON.stringify({
      id: 'owner/ambiguous',
      siblings: [
        { rfilename: 'a.safetensors' },
        { rfilename: 'nested/b.safetensors' },
        { rfilename: 'c.bin' },
        { rfilename: 42 },
        null,
        { notafile: true },
      ],
    }),
  },
  {
    name: 'the_payloads_own_id_wins_over_the_queried_one',
    source: 'owner/renamed',
    status: 200,
    body: JSON.stringify({ id: 'canonical/elsewhere' }),
  },
  {
    name: 'a_non_string_id_falls_back_to_the_queried_one',
    source: 'owner/weird-id',
    status: 200,
    body: JSON.stringify({ id: 12345 }),
  },
  {
    name: 'card_data_that_is_an_array_is_not_card_data',
    source: 'owner/array-card',
    status: 200,
    body: JSON.stringify({ id: 'owner/array-card', cardData: ['base_model'] }),
  },
  {
    name: 'a_401_never_claims_the_repository_does_not_exist',
    source: 'owner/private',
    status: 401,
    body: JSON.stringify({ error: 'Invalid username or password.' }),
  },
  { name: 'a_403_is_also_missing_or_private', source: 'owner/forbidden', status: 403, body: '{}' },
  { name: 'a_404_is_a_genuine_absence', source: 'owner/gone', status: 404, body: '{}' },
  { name: 'a_429_is_rate_limiting', source: 'owner/busy', status: 429, body: '{}' },
  { name: 'any_other_status_is_http', source: 'owner/teapot', status: 418, body: '{}' },
  {
    name: 'the_token_rides_as_a_bearer_credential',
    source: 'owner/tokened',
    token: 'hf_secret_value',
    status: 200,
    body: JSON.stringify({ id: 'owner/tokened' }),
  },
  {
    name: 'a_source_with_no_repository_never_reaches_the_network',
    source: 'https://cdn.example.com/weights.safetensors',
    status: 200,
    body: JSON.stringify({ id: 'never/queried' }),
  },
  {
    name: 'a_body_that_is_not_json_is_http',
    source: 'owner/garbled',
    status: 200,
    body: 'this is not json',
  },
  {
    name: 'an_array_payload_is_an_unexpected_shape',
    source: 'owner/array-payload',
    status: 200,
    body: JSON.stringify([1, 2, 3]),
  },
  {
    name: 'a_null_payload_is_an_unexpected_shape',
    source: 'owner/null-payload',
    status: 200,
    body: 'null',
  },
  {
    name: 'a_timeout_is_a_timeout_not_a_missing_repository',
    source: 'owner/slow',
    status: 0,
    body: null,
    throwName: 'TimeoutError',
    throwMessage: 'The operation was aborted due to timeout',
  },
  {
    name: 'an_abort_is_also_a_timeout',
    source: 'owner/aborted',
    status: 0,
    body: null,
    throwName: 'AbortError',
    throwMessage: 'This operation was aborted',
  },
  {
    name: 'anything_else_thrown_is_a_network_failure',
    source: 'owner/unreachable',
    status: 0,
    body: null,
    throwName: 'TypeError',
    throwMessage: 'fetch failed',
  },
];

const realFetch = global.fetch;

async function run(): Promise<void> {
  for (const c of NET_CASES) {
    const seen: SeenRequest[] = [];
    global.fetch = (async (input: unknown, init?: { headers?: Record<string, string> }) => {
      seen.push({
        url: String(input),
        headers: Object.entries(init?.headers ?? {})
          .map(([k, v]) => `${k}: ${v}`)
          .sort(),
      });
      if (c.body === null) {
        const err = new Error(c.throwMessage ?? 'thrown');
        err.name = c.throwName ?? 'Error';
        throw err;
      }
      return new Response(c.body, {
        status: c.status,
        statusText: c.statusText ?? '',
        headers: { 'content-type': 'application/json' },
      });
    }) as unknown as typeof global.fetch;

    const result = await lookupHuggingFaceLora(c.source, c.token);
    // The canned wire rides WITH the row so the Rust side drives the identical
    // transport instead of restating this table (the standing corpus rule).
    emit({
      kind: 'lookup',
      name: c.name,
      source: c.source,
      token: c.token ?? null,
      wire:
        c.body === null
          ? { thrown: { name: c.throwName ?? 'Error', message: c.throwMessage ?? 'thrown' } }
          : { status: c.status, body: c.body },
      result,
      seen,
    });
  }
  global.fetch = realFetch;
  const text = rows.map((row) => `${JSON.stringify(row)}\n`).join('');
  const out = process.env.QT_ORACLE_OUT;
  if (out) {
    writeFileSync(out, text);
    process.stderr.write(`huggingface-lora-lookup oracle wrote ${out} (${rows.length} rows)\n`);
  } else {
    process.stdout.write(text);
  }
}

void run();
