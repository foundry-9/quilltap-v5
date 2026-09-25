/**
 * Oracle case: the Concierge refusal classifier (P4.D225; v4 `8bd080267` #73,
 * `lib/services/dangerous-content/refusal.ts` — unchanged by `49059fb14`).
 *
 * Drives v4's REAL `classifyRefusal` (and `isModerationRefusal`) over a fixed
 * corpus that reaches all six exits — typed-error (by `code` AND by `name`
 * alone), provider-code (`record.code` THEN `record.error.code`, lower-cased,
 * the original case in the detail; finite and non-finite numeric codes),
 * finish-reason (with and without an error, BEFORE the message patterns, the
 * raw reason untruncated), message-pattern (each of the eleven, the four
 * `messageOf` shapes, the empty-lowered short-circuit, a 201+-unit message),
 * inferred, and the bare `{ refused: false }` — and records the verdict AND
 * every line the `ConciergeRefusal` logger wrote (level, message, bag), so the
 * DEBUG-on-every-call / INFO-on-refusal pair is a comparand, not a
 * transcription.
 *
 * Each row carries its INPUT as a JSON description of the JS value (the error
 * shape, its props, a `{"$num": "NaN"}` marker where JSON cannot carry a
 * non-finite number) so the Rust side can build the same `RefusalError`.
 *
 * Regenerate (Node 24, from the v4 checkout; stage the case OUTSIDE any
 * `.claude/` path — v4's jest ignores `/\.claude/`). While v4 HEAD is past the
 * oracle baseline this needs a PINNED worktree; that is the sweep driver's
 * `--v4`, never a path in this header.
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5} ; TMPO=/tmp/qt-oracle-run-refusal-classify
 *   cd ~/source/quilltap-server
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/refusal-classify.test.ts" "$TMPO/cases/"
 *   rm -f /tmp/oracle-refusal-classify.ndjson
 *   QT_ORACLE_OUT=/tmp/oracle-refusal-classify.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "cases/refusal-classify\.test\.ts$"
 */

import * as fs from 'node:fs';

type ErrorSpec =
  | null
  | { shape: 'error'; message: string; props?: Record<string, unknown> }
  | { shape: 'object'; value: Record<string, unknown> }
  | { shape: 'string'; value: string }
  | { shape: 'number'; value: number };

interface Case {
  label: string;
  error: ErrorSpec;
  finishReason?: string | null;
  emptyBody?: boolean;
  contentWasFlagged?: boolean;
}

interface RecordedLog {
  service: string | null;
  level: string;
  message: string;
  bag: Record<string, unknown>;
}

const LONG = 'x'.repeat(240);
const PAT_LONG = `The request was rejected: content policy. ${'y'.repeat(260)}`;
// 199 units exactly after the pattern prefix lands the cut on an ASCII
// boundary even though astral characters sit earlier — UTF-16 counting, not
// char counting, decides where the ellipsis goes.
const ASTRAL_EARLY = `😀😀😀 blocked by safety ${'z'.repeat(250)}`;

const CASES: Case[] = [
  // --- 1. typed-error ---
  { label: 't_code_only', error: { shape: 'error', message: 'OpenAI refused this image request on content grounds', props: { code: 'MODERATION_REJECTED' } } },
  { label: 't_name_only', error: { shape: 'error', message: 'nope', props: { name: 'ModerationRejectionError' } } },
  { label: 't_reason', error: { shape: 'error', message: '400 Your request was rejected by the safety system.', props: { code: 'MODERATION_REJECTED', providerReason: 'moderation_blocked', statusCode: 400 } } },
  { label: 't_reason_empty', error: { shape: 'error', message: 'refused', props: { code: 'MODERATION_REJECTED', providerReason: '' } } },
  { label: 't_reason_nonstring', error: { shape: 'error', message: 'refused', props: { code: 'MODERATION_REJECTED', providerReason: 42 } } },
  { label: 't_no_message', error: { shape: 'error', message: '', props: { code: 'MODERATION_REJECTED' } } },
  { label: 't_no_message_reason', error: { shape: 'error', message: '', props: { code: 'MODERATION_REJECTED', providerReason: 'IMAGE_SAFETY' } } },
  { label: 't_ws_message', error: { shape: 'error', message: '   ', props: { code: 'MODERATION_REJECTED' } } },
  { label: 't_long', error: { shape: 'error', message: LONG, props: { code: 'MODERATION_REJECTED', providerReason: 'content refusal' } } },
  { label: 't_code_wrong_case', error: { shape: 'error', message: 'plain failure', props: { code: 'moderation_rejected' } } },
  { label: 't_object_shape', error: { shape: 'object', value: { code: 'MODERATION_REJECTED', message: 'Gemini declined to generate this image (IMAGE_SAFETY)', providerReason: 'IMAGE_SAFETY' } } },
  { label: 't_object_nonstring_message', error: { shape: 'object', value: { code: 'MODERATION_REJECTED', message: 5 } } },
  { label: 't_object_name_only', error: { shape: 'object', value: { name: 'ModerationRejectionError', message: 'from a copy' } } },
  { label: 't_beats_code_and_finish', error: { shape: 'error', message: 'safety system', props: { code: 'MODERATION_REJECTED', error: { code: 'content_filter' } } }, finishReason: 'content_filter', emptyBody: true, contentWasFlagged: true },
  // --- 2. provider-code ---
  { label: 'pc_own', error: { shape: 'error', message: 'bad prompt', props: { code: 'content_policy_violation' } } },
  { label: 'pc_nested', error: { shape: 'error', message: '400 Your request was rejected as a result of our safety system.', props: { error: { code: 'moderation_blocked', message: 'x' } } } },
  { label: 'pc_both_own_first', error: { shape: 'error', message: 'm', props: { code: 'safety', error: { code: 'content_filter' } } } },
  { label: 'pc_own_other_nested_mod', error: { shape: 'error', message: 'm', props: { code: 'invalid_request_error', error: { code: 'content_filter' } } } },
  { label: 'pc_mixed_case', error: { shape: 'error', message: 'm', props: { code: 'Moderation_Blocked' } } },
  { label: 'pc_upper_nested', error: { shape: 'object', value: { error: { code: 'SAFETY' }, message: 'blocked' } } },
  { label: 'pc_numeric_1301', error: { shape: 'error', message: 'Contains sensitive content', props: { code: 1301 } } },
  { label: 'pc_numeric_nested_1301', error: { shape: 'object', value: { error: { code: 1301 } } } },
  { label: 'pc_string_1301', error: { shape: 'error', message: '', props: { code: '1301' } } },
  { label: 'pc_float_code', error: { shape: 'error', message: 'm', props: { code: 1301.5 } } },
  { label: 'pc_nan_code', error: { shape: 'error', message: 'plain', props: { code: { $num: 'NaN' } } } },
  { label: 'pc_infinity_nested', error: { shape: 'error', message: 'plain', props: { error: { code: { $num: 'Infinity' } } } } },
  { label: 'pc_empty_own_nested_mod', error: { shape: 'error', message: 'm', props: { code: '', error: { code: 'content_filter' } } } },
  { label: 'pc_no_message', error: { shape: 'error', message: '', props: { code: 'safety' } } },
  { label: 'pc_long', error: { shape: 'error', message: LONG, props: { code: 'content_filter' } } },
  { label: 'pc_beats_finish', error: { shape: 'error', message: 'm', props: { code: 'content_filter' } }, finishReason: 'SAFETY' },
  { label: 'pc_string_error_is_not_a_record', error: { shape: 'string', value: 'content_filter' } },
  { label: 'pc_nested_not_object', error: { shape: 'error', message: 'plain', props: { error: 'content_filter' } } },
  // --- 3. finish-reason ---
  { label: 'fr_no_error', error: null, finishReason: 'content_filter' },
  { label: 'fr_upper', error: null, finishReason: 'SAFETY' },
  { label: 'fr_padded_raw_detail', error: null, finishReason: '  refusal  ' },
  { label: 'fr_beats_pattern', error: { shape: 'error', message: 'Your request was rejected by the safety system' }, finishReason: 'content_filter' },
  { label: 'fr_beats_inferred', error: null, finishReason: 'prohibited_content', emptyBody: true, contentWasFlagged: true },
  { label: 'fr_stop', error: null, finishReason: 'stop' },
  { label: 'fr_null', error: null, finishReason: null },
  { label: 'fr_empty', error: null, finishReason: '' },
  { label: 'fr_sensitive_with_plain_error', error: { shape: 'error', message: 'ETIMEDOUT' }, finishReason: 'sensitive' },
  // --- 4. message-pattern (each of the eleven, mixed case) ---
  { label: 'mp_content_moderation', error: { shape: 'error', message: '400 Your request was rejected by Content Moderation.' } },
  { label: 'mp_content_policy_snake', error: { shape: 'error', message: 'error: CONTENT_POLICY triggered' } },
  { label: 'mp_content_policy', error: { shape: 'error', message: 'This prompt violates our content policy' } },
  { label: 'mp_safety_system', error: { shape: 'error', message: 'Rejected as a result of our Safety System' } },
  { label: 'mp_rejected_by_content', error: { shape: 'error', message: 'prompt Rejected By Content filters' } },
  { label: 'mp_moderation_blocked', error: { shape: 'error', message: 'moderation_blocked: no' } },
  { label: 'mp_responsible_ai', error: { shape: 'error', message: 'Filtered by Responsible AI practices' } },
  { label: 'mp_declined_to_generate', error: { shape: 'error', message: 'Model declined to generate an image: I cannot draw that' } },
  { label: 'mp_blocked_by_safety', error: { shape: 'error', message: 'Output blocked by safety settings' } },
  { label: 'mp_prompt_blocked', error: { shape: 'error', message: 'PROMPT_BLOCKED' } },
  { label: 'mp_image_safety', error: { shape: 'error', message: 'finish IMAGE_SAFETY' } },
  { label: 'mp_string_error', error: { shape: 'string', value: 'Your request was rejected by content moderation' } },
  { label: 'mp_object_message', error: { shape: 'object', value: { message: 'Blocked by Safety filters' } } },
  { label: 'mp_object_nonstring_message', error: { shape: 'object', value: { message: 12, detail: 'content policy' } } },
  { label: 'mp_number_error', error: { shape: 'number', value: 42 } },
  { label: 'mp_empty_message', error: { shape: 'error', message: '' } },
  { label: 'mp_long', error: { shape: 'error', message: PAT_LONG } },
  { label: 'mp_astral_early', error: { shape: 'error', message: ASTRAL_EARLY } },
  { label: 'mp_trimmed', error: { shape: 'error', message: '  content policy violated  ' } },
  { label: 'mp_bare_400', error: { shape: 'error', message: '400 Bad Request' } },
  { label: 'mp_try_different', error: { shape: 'error', message: 'Please try a different prompt' } },
  { label: 'mp_bare_safety', error: { shape: 'error', message: 'safety' } },
  { label: 'mp_with_empty_flagged', error: { shape: 'error', message: 'content policy' }, emptyBody: true, contentWasFlagged: true },
  // --- 5. inferred ---
  { label: 'inf_both', error: null, emptyBody: true, contentWasFlagged: true },
  { label: 'inf_with_plain_error', error: { shape: 'error', message: 'ETIMEDOUT' }, emptyBody: true, contentWasFlagged: true },
  { label: 'inf_with_stop', error: null, finishReason: 'stop', emptyBody: true, contentWasFlagged: true },
  { label: 'inf_empty_only', error: null, emptyBody: true },
  { label: 'inf_flag_only', error: null, contentWasFlagged: true },
  { label: 'inf_false_body', error: null, emptyBody: false, contentWasFlagged: true },
  { label: 'inf_false_flag', error: null, emptyBody: true, contentWasFlagged: false },
  // --- 6. nothing ---
  { label: 'none_empty_input', error: null },
  { label: 'none_timeout', error: { shape: 'error', message: 'ETIMEDOUT' } },
  { label: 'none_rate_limit', error: { shape: 'error', message: '429 Too Many Requests', props: { code: 'rate_limit_exceeded', status: 429 } } },
];

function resolveNum(v: unknown): unknown {
  if (v && typeof v === 'object' && !Array.isArray(v)) {
    const rec = v as Record<string, unknown>;
    if (typeof rec.$num === 'string' && Object.keys(rec).length === 1) return Number(rec.$num);
    const out: Record<string, unknown> = {};
    for (const [k, val] of Object.entries(rec)) out[k] = resolveNum(val);
    return out;
  }
  return v;
}

function buildError(spec: ErrorSpec): unknown {
  if (spec === null) return undefined;
  switch (spec.shape) {
    case 'error': {
      const e = new Error(spec.message) as Error & Record<string, unknown>;
      for (const [k, v] of Object.entries(spec.props ?? {})) e[k] = resolveNum(v);
      return e;
    }
    case 'object':
      return resolveNum(spec.value);
    case 'string':
      return spec.value;
    case 'number':
      return spec.value;
  }
}

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const logs: RecordedLog[] = [];
  jest.resetModules();
  jest.doMock('@/lib/logger', () => {
    const recorder = (service: string | null) => {
      const record = (level: string) => (message: string, bag?: Record<string, unknown>) =>
        logs.push({ service, level, message, bag: JSON.parse(JSON.stringify(bag ?? {})) });
      const self: Record<string, unknown> = {
        debug: record('debug'),
        info: record('info'),
        warn: record('warn'),
        error: record('error'),
        trace: record('trace'),
      };
      self.child = (ctx: Record<string, unknown>) =>
        recorder(typeof ctx?.service === 'string' ? ctx.service : null);
      return self;
    };
    return {
      __esModule: true,
      LogLevel: { ERROR: 'error', WARN: 'warn', INFO: 'info', DEBUG: 'debug', TRACE: 'trace' },
      logger: recorder(null),
    };
  });

  const { classifyRefusal, isModerationRefusal } = await import('@/lib/services/dangerous-content/refusal');

  const lines: string[] = [];
  for (const c of CASES) {
    const input: Record<string, unknown> = {};
    const error = buildError(c.error);
    if (c.error !== null) input.error = error;
    if ('finishReason' in c) input.finishReason = c.finishReason;
    if ('emptyBody' in c) input.emptyBody = c.emptyBody;
    if ('contentWasFlagged' in c) input.contentWasFlagged = c.contentWasFlagged;

    logs.length = 0;
    const verdict = classifyRefusal(input as never);
    const classifyLogs = logs.filter((l) => l.service === 'ConciergeRefusal').slice();
    if (classifyLogs.length !== logs.length) {
      throw new Error(`${c.label}: a line from outside ConciergeRefusal: ${JSON.stringify(logs)}`);
    }
    logs.length = 0;
    const isRefusal = c.error === null ? null : isModerationRefusal(error);

    lines.push(
      JSON.stringify({
        kind: 'case',
        label: c.label,
        input: {
          error: c.error,
          ...('finishReason' in c ? { finishReason: c.finishReason } : {}),
          ...('emptyBody' in c ? { emptyBody: c.emptyBody } : {}),
          ...('contentWasFlagged' in c ? { contentWasFlagged: c.contentWasFlagged } : {}),
        },
        verdict,
        isModerationRefusal: isRefusal,
        logs: classifyLogs.map(({ level, message, bag }) => ({ level, message, bag })),
      }),
    );
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`refusal-classify oracle wrote ${outPath} (${lines.length} cases)\n`);
}

test('refusal-classify oracle', async () => {
  await main();
});
