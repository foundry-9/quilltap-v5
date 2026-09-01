/**
 * @jest-environment node
 *
 * Differential ORACLE (tier-1, DB-free) for the W4.1d5 image-generation pure
 * leaves: `parsePlaceholders` (v4 `lib/image-gen/prompt-expansion.ts`),
 * `resolveOrientation` (v4 `lib/image-gen/orientation.ts`), and — since
 * P4.D138 (`84f33ce94`) — the model matchers (`lib/plugins/model-matchers.ts`)
 * and the whole LoRA support resolver (`lib/image-gen/lora-support.ts`).
 * The plugin registry
 * (`getImageGenerationModels` / `getImageProviderConstraints`) is jest-mocked to
 * canned declarations (the external boundary). Emits one NDJSON line per case:
 *   { label, kind: 'placeholders'|'orientation', json }
 *
 * Run (Node 24, from the v4 checkout; STAGE outside any .claude path):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; STAGE=/tmp/qt-oracle-stage
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-image-gen-leaves.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- image-gen-leaves
 */

import * as fs from 'fs';

// Mutable holders the registry mock reads (set per orientation case).
let cannedModels: unknown = null;
let cannedConstraints: unknown = undefined;

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const lines: string[] = [];

  // ---- parsePlaceholders (no mocks needed; pure) ----
  {
    jest.resetModules();
    const { parsePlaceholders } = await import('@/lib/image-gen/prompt-expansion');
    const placeholderCases: Array<[string, string]> = [
      ['p_two', 'A scene with {{me}} and {{Aurora}}.'],
      ['p_none', 'no placeholders here'],
      ['p_ws', '{{ me }}'],
      ['p_adjacent', '{{a}}{{b}}'],
      ['p_empty', '{{}}'],
      ['p_multi', 'before {{x}} middle {{y}} after {{z}}'],
      ['p_unclosed', 'a {{ dangling name and {{good}}'],
    ];
    for (const [label, prompt] of placeholderCases) {
      lines.push(JSON.stringify({ label, kind: 'placeholders', json: JSON.stringify(parsePlaceholders(prompt)) }));
    }
  }

  // ---- resolveOrientation (registry mocked) ----
  interface Mapping { size?: string; aspectRatio?: string; promptHint?: string; nominalWidth?: number; nominalHeight?: number }
  interface Support { strategy: 'size' | 'aspectRatio' | 'prompt'; portrait: Mapping; landscape: Mapping; square?: Mapping }
  interface OCase {
    label: string;
    provider: string;
    model?: string;
    orientation: 'portrait' | 'landscape' | 'square';
    models: Array<{ id: string; orientationSupport?: Support }> | null;
    constraints?: Support;
  }
  const sizeSupport: Support = {
    strategy: 'size',
    portrait: { size: '1024x1792', nominalWidth: 1024, nominalHeight: 1792 },
    landscape: { size: '1792x1024', nominalWidth: 1792, nominalHeight: 1024 },
    square: { size: '1024x1024' },
  };
  const aspectSupport: Support = {
    strategy: 'aspectRatio',
    portrait: { aspectRatio: '3:4' },
    landscape: { aspectRatio: '4:3' },
  };
  const promptSupport: Support = {
    strategy: 'prompt',
    portrait: { promptHint: 'a tall vertical framing' },
    landscape: { promptHint: 'a wide framing' },
  };
  const degradeSupport: Support = {
    // 'size' strategy but portrait carries only a hint → degrade to prompt hint.
    strategy: 'size',
    portrait: { promptHint: 'custom portrait hint' },
    landscape: { size: '1792x1024' },
  };
  const noSquareSupport: Support = {
    strategy: 'size',
    portrait: { size: '1024x1792' },
    landscape: { size: '1792x1024' },
    // no square
  };

  const orientationCases: OCase[] = [
    { label: 'o_fallback_portrait', provider: 'unknown', orientation: 'portrait', models: null },
    { label: 'o_fallback_square', provider: 'unknown', orientation: 'square', models: null },
    { label: 'o_size_portrait', provider: 'openai', model: 'dall-e-3', orientation: 'portrait', models: [{ id: 'dall-e-3', orientationSupport: sizeSupport }] },
    { label: 'o_size_landscape', provider: 'openai', model: 'dall-e-3', orientation: 'landscape', models: [{ id: 'dall-e-3', orientationSupport: sizeSupport }] },
    { label: 'o_prefix_match', provider: 'openai', model: 'dall-e-3-mini', orientation: 'portrait', models: [{ id: 'dall-e-3', orientationSupport: sizeSupport }, { id: 'dall-e-2', orientationSupport: aspectSupport }] },
    { label: 'o_aspect', provider: 'flux', model: 'flux-pro', orientation: 'landscape', models: [{ id: 'flux-pro', orientationSupport: aspectSupport }] },
    { label: 'o_prompt_strategy', provider: 'p', model: 'm', orientation: 'portrait', models: [{ id: 'm', orientationSupport: promptSupport }] },
    { label: 'o_degrade_to_hint', provider: 'p', model: 'm', orientation: 'portrait', models: [{ id: 'm', orientationSupport: degradeSupport }] },
    { label: 'o_declared_absent_square', provider: 'p', model: 'm', orientation: 'square', models: [{ id: 'm', orientationSupport: noSquareSupport }] },
    { label: 'o_provider_level', provider: 'p', orientation: 'portrait', models: null, constraints: sizeSupport },
    { label: 'o_no_match_falls_provider', provider: 'p', model: 'unknown-model', orientation: 'landscape', models: [{ id: 'other', orientationSupport: aspectSupport }], constraints: promptSupport },
  ];

  for (const c of orientationCases) {
    jest.resetModules();
    cannedModels = c.models;
    cannedConstraints = c.constraints ? { orientationSupport: c.constraints } : undefined;
    jest.doMock('@/lib/plugins/provider-registry', () => ({
      getImageGenerationModels: () => cannedModels,
      getImageProviderConstraints: () => cannedConstraints,
    }));
    const { resolveOrientation } = await import('@/lib/image-gen/orientation');
    const result = resolveOrientation(c.provider, c.model, c.orientation as never);
    lines.push(JSON.stringify({ label: c.label, kind: 'orientation', json: JSON.stringify(result) }));
  }


  // ---- model-matchers (pure, no mocks) -------------------------------------
  {
    jest.resetModules();
    const { modelMatchesPattern, fieldAppliesToModel } = await import('@/lib/plugins/model-matchers');
    const patternCases: Array<[string, string, string]> = [
      ['mm_empty_pattern', 'flux-lora', ''],
      ['mm_empty_both', '', ''],
      ['mm_exact', 'flux-lora', 'flux-lora'],
      ['mm_prefix', 'flux-lora/inpainting', 'flux-lora'],
      ['mm_prefix_no', 'flux', 'flux-lora'],
      ['mm_glob_trailing', 'wavespeed-ai/krea-v2/turbo-lora', 'wavespeed-ai/*'],
      ['mm_glob_leading', 'z-image-turbo-lora', '*-lora'],
      ['mm_glob_leading_no', 'z-image-turbo', '*-lora'],
      ['mm_glob_middle', 'flux-2-klein-4b', 'flux-2-*-4b'],
      ['mm_glob_double', 'a-b-c', '*-b-*'],
      ['mm_glob_bare', 'anything', '*'],
      ['mm_meta_dot_escaped', 'gpt-image-125', 'gpt-image-1.5*'],
      ['mm_meta_dot_matches', 'gpt-image-1.5-mini', 'gpt-image-1.5*'],
      ['mm_meta_plus', 'a+b', 'a+*'],
      ['mm_meta_paren', 'x(y)z', 'x(y)*'],
      ['mm_meta_bracket', 'a[b]c', 'a[b]*'],
      ['mm_meta_backslash', 'a\\b', 'a\\*'],
      ['mm_meta_dollar', 'a$b', 'a$*'],
      ['mm_case_sensitive', 'FLUX-LORA', 'flux-lora'],
      ['mm_newline_star', 'a\nb', 'a*b'],
      ['mm_cr_star', 'a\rb', 'a*b'],
      ['mm_ls_star', 'a b', 'a*b'],
      ['mm_unicode_prefix', 'flüx-lora-x', 'flüx-lora'],
    ];
    for (const [label, model, pattern] of patternCases) {
      lines.push(JSON.stringify({ label, kind: 'matcher_pattern', model, pattern, json: JSON.stringify(modelMatchesPattern(model, pattern)) }));
    }

    const fieldCases: Array<[string, string[] | undefined, string | undefined]> = [
      ['mf_absent', undefined, 'hidream'],
      ['mf_empty', [], 'hidream'],
      ['mf_no_model', ['flux-lora'], undefined],
      ['mf_blank_model', ['flux-lora'], ''],
      ['mf_hit', ['hidream', 'flux-lora'], 'flux-lora'],
      ['mf_miss', ['flux-lora'], 'hidream'],
      ['mf_glob_hit', ['wavespeed-ai/*', 'pruna-ai/*'], 'pruna-ai/p-image/edit-lora'],
    ];
    for (const [label, list, model] of fieldCases) {
      lines.push(JSON.stringify({ label, kind: 'matcher_field', list: list ?? null, model: model ?? null, json: JSON.stringify(fieldAppliesToModel(list, model)) }));
    }
  }

  // ---- lora-support (registry mocked, per case) ----------------------------
  interface LSupport { maxLoras: number; scale?: { min: number; max: number; default: number; step?: number }; sourceKinds: string[]; supportsPrivateWeightsToken?: boolean }
  const indexedScale = { min: 0, max: 4, default: 1, step: 0.1 };
  const fluxDevLora: LSupport = { maxLoras: 4, scale: indexedScale, sourceKinds: ['url', 'hf-repo'] };
  const kleinLora: LSupport = { maxLoras: 3, scale: indexedScale, sourceKinds: ['url', 'hf-repo'] };
  const prunaLora: LSupport = { maxLoras: 1, scale: { min: 0, max: 4, default: 0.5, step: 0.05 }, sourceKinds: ['url', 'hf-repo'], supportsPrivateWeightsToken: true };
  const noScaleLora: LSupport = { maxLoras: 2, sourceKinds: ['provider-id'] };

  interface LCase {
    label: string;
    provider: string;
    model?: string;
    models: Array<{ id: string; loraSupport?: LSupport }> | null;
    constraints?: LSupport;
  }
  const nanoModels = [
    { id: 'hidream' },
    { id: 'flux-2-dev' },
    { id: 'flux-2-dev-lora', loraSupport: fluxDevLora },
    { id: 'flux-2-klein-4b', loraSupport: kleinLora },
    { id: 'pruna-ai/p-image/edit-lora', loraSupport: prunaLora },
  ];
  const supportCases: LCase[] = [
    { label: 'ls_none_anywhere', provider: 'OPENAI', model: 'dall-e-3', models: null },
    { label: 'ls_exact', provider: 'NANOGPT', model: 'flux-2-dev-lora', models: nanoModels },
    { label: 'ls_prefix_longest', provider: 'NANOGPT', model: 'flux-2-dev-lora-image-to-image', models: nanoModels },
    { label: 'ls_flagship_no_support', provider: 'NANOGPT', model: 'hidream', models: nanoModels },
    { label: 'ls_prefix_hits_non_lora_entry', provider: 'NANOGPT', model: 'flux-2-dev-x', models: nanoModels },
    { label: 'ls_unknown_model', provider: 'NANOGPT', model: 'who-knows', models: nanoModels },
    { label: 'ls_no_model', provider: 'NANOGPT', models: nanoModels },
    { label: 'ls_provider_level', provider: 'P', model: 'anything', models: null, constraints: noScaleLora },
    { label: 'ls_model_beats_provider', provider: 'NANOGPT', model: 'flux-2-klein-4b', models: nanoModels, constraints: noScaleLora },
    { label: 'ls_falls_to_provider', provider: 'NANOGPT', model: 'nope', models: nanoModels, constraints: noScaleLora },
    { label: 'ls_token_family', provider: 'NANOGPT', model: 'pruna-ai/p-image/edit-lora', models: nanoModels },
  ];
  for (const c of supportCases) {
    jest.resetModules();
    cannedModels = c.models;
    cannedConstraints = c.constraints ? { loraSupport: c.constraints } : undefined;
    jest.doMock('@/lib/plugins/provider-registry', () => ({
      getImageGenerationModels: () => cannedModels,
      getImageProviderConstraints: () => cannedConstraints,
    }));
    const { resolveLoraSupport, resolveLoraScaleBounds } = await import('@/lib/image-gen/lora-support');
    const support = resolveLoraSupport(c.provider, c.model);
    lines.push(JSON.stringify({
      label: c.label,
      kind: 'lora_support',
      models: c.models,
      constraints: c.constraints ?? null,
      model: c.model ?? null,
      json: JSON.stringify({ support: support ?? null, bounds: support ? resolveLoraScaleBounds(support) : null }),
    }));
  }

  // ---- readLorasFromParameters / capLoras / trigger phrases ----------------
  {
    jest.resetModules();
    jest.doMock('@/lib/plugins/provider-registry', () => ({
      getImageGenerationModels: () => null,
      getImageProviderConstraints: () => undefined,
    }));
    const { readLorasFromParameters, capLoras, loraTriggerPhrases, joinLoraTriggerPhrases, DEFAULT_LORA_SCALE } =
      await import('@/lib/image-gen/lora-support');

    lines.push(JSON.stringify({ label: 'lora_default_scale', kind: 'lora_const', json: JSON.stringify(DEFAULT_LORA_SCALE) }));

    const ctx = { provider: 'NANOGPT', model: 'flux-2-dev-lora' };
    const readCases: Array<[string, unknown]> = [
      ['lr_absent_bag', null],
      ['lr_empty_bag', {}],
      ['lr_null_key', { loras: null }],
      ['lr_not_a_list', { loras: { source: 'a/b' } }],
      ['lr_not_a_list_string', { loras: 'a/b' }],
      ['lr_not_a_list_number', { loras: 3 }],
      ['lr_empty_list', { loras: [] }],
      ['lr_plain', { loras: [{ source: 'owner/name' }] }],
      ['lr_trimmed', { loras: [{ source: '  owner/name  ', triggerPhrase: '  magic  ', label: '  L  ' }] }],
      ['lr_blank_source', { loras: [{ source: '   ' }, { source: 'ok/one' }] }],
      ['lr_missing_source', { loras: [{ scale: 1 }] }],
      ['lr_non_object_entries', { loras: ['a/b', 5, null, true, ['x'], { source: 'ok/two' }] }],
      ['lr_scale_ok', { loras: [{ source: 'a/b', scale: 0 }, { source: 'c/d', scale: 10 }, { source: 'e/f', scale: 2.5 }] }],
      ['lr_scale_out_of_range', { loras: [{ source: 'a/b', scale: -1 }, { source: 'c/d', scale: 10.5 }] }],
      ['lr_scale_nan', { loras: [{ source: 'a/b', scale: 'not-a-number' }] }],
      ['lr_scale_numeric_string', { loras: [{ source: 'a/b', scale: '1.5' }] }],
      ['lr_scale_bool', { loras: [{ source: 'a/b', scale: true }] }],
      ['lr_scale_null', { loras: [{ source: 'a/b', scale: null }] }],
      ['lr_scale_array_one', { loras: [{ source: 'a/b', scale: [2] }] }],
      ['lr_scale_array_two', { loras: [{ source: 'a/b', scale: [1, 2] }] }],
      ['lr_scale_object', { loras: [{ source: 'a/b', scale: {} }] }],
      ['lr_blank_trigger_and_label', { loras: [{ source: 'a/b', triggerPhrase: '   ', label: '' }] }],
      ['lr_non_string_trigger', { loras: [{ source: 'a/b', triggerPhrase: 5, label: false }] }],
      ['lr_full', { loras: [{ source: 'a/b', scale: 0.8, triggerPhrase: 'shou_xin', label: 'Shou Xin' }] }],
    ];
    for (const [label, bag] of readCases) {
      lines.push(JSON.stringify({ label, kind: 'lora_read', bag: bag ?? null, json: JSON.stringify(readLorasFromParameters(bag as never, ctx)) }));
    }

    const four = [
      { source: 'a/1', scale: 1 },
      { source: 'a/2' },
      { source: 'a/3', scale: 2 },
      { source: 'a/4' },
    ];
    const capCases: Array<[string, unknown[], LSupport | null]> = [
      ['lc_empty_no_support', [], null],
      ['lc_no_support_strips', four, null],
      ['lc_under_cap', four.slice(0, 2), kleinLora],
      ['lc_at_cap', four.slice(0, 3), kleinLora],
      ['lc_over_cap_keeps_leading', four, kleinLora],
      ['lc_cap_one', four, prunaLora],
      ['lc_cap_zero', four, { maxLoras: 0, sourceKinds: ['url'] }],
      ['lc_cap_fractional', four, { maxLoras: 2.9, sourceKinds: ['url'] }],
      ['lc_cap_negative', four, { maxLoras: -3, sourceKinds: ['url'] }],
    ];
    for (const [label, loras, support] of capCases) {
      lines.push(JSON.stringify({ label, kind: 'lora_cap', loras, support: support ?? null, json: JSON.stringify(capLoras(loras as never, support as never, ctx)) }));
    }

    const phraseCases: Array<[string, unknown[]]> = [
      ['lp_empty', []],
      ['lp_none_declared', [{ source: 'a/1' }, { source: 'a/2' }]],
      ['lp_one', [{ source: 'a/1', triggerPhrase: 'magic' }]],
      ['lp_dedupe_case_insensitive', [{ source: 'a/1', triggerPhrase: 'Magic' }, { source: 'a/2', triggerPhrase: 'magic' }, { source: 'a/3', triggerPhrase: 'MAGIC' }]],
      ['lp_order_preserved', [{ source: 'a/1', triggerPhrase: 'beta' }, { source: 'a/2', triggerPhrase: 'alpha' }]],
      ['lp_blank_skipped', [{ source: 'a/1', triggerPhrase: '   ' }, { source: 'a/2', triggerPhrase: 'kept' }]],
      ['lp_trimmed', [{ source: 'a/1', triggerPhrase: '  spaced  ' }]],
      ['lp_unicode_fold', [{ source: 'a/1', triggerPhrase: 'STRASSE' }, { source: 'a/2', triggerPhrase: 'strasse' }]],
    ];
    for (const [label, loras] of phraseCases) {
      lines.push(JSON.stringify({
        label,
        kind: 'lora_phrases',
        loras,
        json: JSON.stringify({ phrases: loraTriggerPhrases(loras as never), joined: joinLoraTriggerPhrases(loras as never) }),
      }));
    }
  }


  // ---- params-builder (registry mocked, per case) --------------------------
  interface PCase {
    label: string;
    profile: { provider: string; modelName?: string | null; parameters?: Record<string, unknown> | null };
    prompt: string;
    overrides?: Record<string, unknown>;
    orientation?: 'portrait' | 'landscape' | 'square';
    fallbackModel?: string;
    models: Array<{ id: string; orientationSupport?: Support; loraSupport?: LSupport }> | null;
    constraints?: { orientationSupport?: Support; loraSupport?: LSupport };
  }
  const pbSize: Support = {
    strategy: 'size',
    portrait: { size: '1024x1792', nominalWidth: 1024, nominalHeight: 1792 },
    landscape: { size: '1792x1024' },
    square: { size: '1024x1024' },
  };
  const pbHintOnly: Support = {
    strategy: 'prompt',
    portrait: { promptHint: 'a tall vertical framing' },
    landscape: { promptHint: 'a wide framing' },
  };
  const pbModels = [
    { id: 'dall-e-3', orientationSupport: pbSize },
    { id: 'flux-2-dev-lora', loraSupport: fluxDevLora },
    { id: 'flux-lora', loraSupport: { maxLoras: 1, scale: { min: 0.1, max: 4, default: 1, step: 0.1 }, sourceKinds: ['url', 'hf-repo'] } as LSupport },
    { id: 'hinted', orientationSupport: pbHintOnly, loraSupport: kleinLora },
  ];
  const paramsCases: PCase[] = [
    { label: 'pb_minimal', profile: { provider: 'OPENAI' }, prompt: 'a cat', models: null },
    { label: 'pb_fallback_model_override', profile: { provider: 'OPENAI' }, prompt: 'a cat', fallbackModel: 'my-fallback', models: null },
    { label: 'pb_model_from_profile', profile: { provider: 'OPENAI', modelName: 'dall-e-3' }, prompt: 'a cat', models: pbModels },
    { label: 'pb_model_from_defaults', profile: { provider: 'OPENAI', modelName: null, parameters: { model: 'from-bag' } }, prompt: 'a cat', models: null },
    { label: 'pb_model_override_wins', profile: { provider: 'OPENAI', modelName: 'dall-e-3', parameters: { model: 'from-bag' } }, prompt: 'a cat', overrides: { model: 'override' }, models: null },
    { label: 'pb_blank_override_falls_through', profile: { provider: 'OPENAI', modelName: 'dall-e-3' }, prompt: 'a cat', overrides: { model: '', size: '', quality: '' }, models: null },
    { label: 'pb_n_from_defaults', profile: { provider: 'OPENAI', modelName: 'm', parameters: { n: 3 } }, prompt: 'a cat', models: null },
    { label: 'pb_n_override_wins', profile: { provider: 'OPENAI', modelName: 'm', parameters: { n: 3 } }, prompt: 'a cat', overrides: { n: 1 }, models: null },
    { label: 'pb_n_non_number_default', profile: { provider: 'OPENAI', modelName: 'm', parameters: { n: '3' } }, prompt: 'a cat', models: null },
    {
      label: 'pb_all_string_defaults',
      profile: {
        provider: 'OPENAI', modelName: 'm',
        parameters: { negativePrompt: 'blurry', size: '512x512', aspectRatio: '3:4', quality: 'hd', style: 'vivid', responseFormat: 'url' },
      },
      prompt: 'a cat', models: null,
    },
    {
      label: 'pb_numeric_defaults',
      profile: { provider: 'OPENAI', modelName: 'm', parameters: { seed: 42, guidanceScale: 3.5, steps: 28 } },
      prompt: 'a cat', models: null,
    },
    {
      // `asNumber` takes real finite numbers only — a quoted default is ignored
      // where the OLD `mergeParameters` cast it straight through.
      label: 'pb_numeric_string_defaults_ignored',
      profile: { provider: 'OPENAI', modelName: 'm', parameters: { seed: '42', guidanceScale: null, steps: true } },
      prompt: 'a cat', models: null,
    },
    {
      label: 'pb_numeric_overrides_win',
      profile: { provider: 'OPENAI', modelName: 'm', parameters: { seed: 42, steps: 28 } },
      prompt: 'a cat', overrides: { seed: 7, guidanceScale: 1.5 }, models: null,
    },
    {
      label: 'pb_orientation_overwrites_size',
      profile: { provider: 'OPENAI', modelName: 'dall-e-3', parameters: { size: '512x512', aspectRatio: '1:1' } },
      prompt: 'a cat', orientation: 'portrait', models: pbModels,
    },
    {
      label: 'pb_orientation_prompt_hint_appended',
      profile: { provider: 'P', modelName: 'hinted' },
      prompt: 'a cat', orientation: 'landscape', models: pbModels,
    },
    {
      // The `POST /api/v1/images` shape: no orientation, so the explicit size stands.
      label: 'pb_no_orientation_keeps_size',
      profile: { provider: 'OPENAI', modelName: 'dall-e-3', parameters: { size: '512x512' } },
      prompt: 'a cat', models: pbModels,
    },
    {
      label: 'pb_residual_bag',
      profile: { provider: 'NANOGPT', modelName: 'hidream', parameters: { quality: 'hd', num_inference_steps: 20, guidance_scale: 2, zzz: 'x', nulled: null } },
      prompt: 'a cat', models: null,
    },
    {
      // Every host-owned key is withheld from the bag — and the bag is OMITTED
      // when nothing is left.
      label: 'pb_residual_bag_empty_omitted',
      profile: { provider: 'OPENAI', modelName: 'm', parameters: { prompt: 'x', negativePrompt: 'y', model: 'z', size: 's', aspectRatio: 'a', orientation: 'o', quality: 'q', style: 'st', n: 2, responseFormat: 'rf', seed: 1, guidanceScale: 2, steps: 3, loras: [] } },
      prompt: 'a cat', models: null,
    },
    {
      // The two LoRA-scoped keys are deliberately OFF the host-owned list: they
      // ride the residual bag so the plugin can scope them by dialect.
      label: 'pb_scoped_keys_ride_the_bag',
      profile: { provider: 'NANOGPT', modelName: 'flux-lora', parameters: { hf_api_token: 'tok', lora_preset: 'anime' } },
      prompt: 'a cat', models: pbModels,
    },
    {
      label: 'pb_loras_capped_and_appended',
      profile: {
        provider: 'NANOGPT', modelName: 'flux-lora',
        parameters: { loras: [{ source: 'a/1', scale: 1, triggerPhrase: 'shou_xin' }, { source: 'a/2', triggerPhrase: 'dropped' }] },
      },
      prompt: 'a cat', models: pbModels,
    },
    {
      label: 'pb_loras_trigger_already_present',
      profile: { provider: 'NANOGPT', modelName: 'flux-lora', parameters: { loras: [{ source: 'a/1', triggerPhrase: 'Shou_Xin' }] } },
      prompt: 'a cat wearing shou_xin', models: pbModels,
    },
    {
      label: 'pb_loras_no_support_stripped',
      profile: { provider: 'NANOGPT', modelName: 'hidream', parameters: { loras: [{ source: 'a/1', triggerPhrase: 'magic' }] } },
      prompt: 'a cat', models: pbModels,
    },
    {
      // Order matters: the orientation hint lands first, the trigger phrases
      // after it, and the `includes` test runs against the hinted prompt.
      label: 'pb_loras_after_orientation_hint',
      profile: { provider: 'P', modelName: 'hinted', parameters: { loras: [{ source: 'a/1', triggerPhrase: 'alpha' }, { source: 'a/2', triggerPhrase: 'beta' }] } },
      prompt: 'a cat', orientation: 'portrait', models: pbModels,
    },
    {
      label: 'pb_loras_provider_level_support',
      profile: { provider: 'P', modelName: 'unknown-model', parameters: { loras: [{ source: 'a/1' }, { source: 'a/2' }, { source: 'a/3' }] } },
      prompt: 'a cat', models: pbModels, constraints: { loraSupport: noScaleLora },
    },
  ];
  for (const c of paramsCases) {
    jest.resetModules();
    cannedModels = c.models;
    cannedConstraints = c.constraints;
    jest.doMock('@/lib/plugins/provider-registry', () => ({
      getImageGenerationModels: () => cannedModels,
      getImageProviderConstraints: () => cannedConstraints,
    }));
    const { buildImageGenParams } = await import('@/lib/image-gen/params-builder');
    const built = buildImageGenParams({
      profile: c.profile as never,
      prompt: c.prompt,
      overrides: (c.overrides ?? {}) as never,
      orientation: c.orientation as never,
      ...(c.fallbackModel ? { fallbackModel: c.fallbackModel } : {}),
    });
    lines.push(JSON.stringify({
      label: c.label,
      kind: 'params_builder',
      profile: c.profile,
      prompt: c.prompt,
      overrides: c.overrides ?? null,
      orientation: c.orientation ?? null,
      fallbackModel: c.fallbackModel ?? null,
      models: c.models,
      constraints: c.constraints ?? null,
      json: JSON.stringify({
        params: built.params,
        loraSupport: built.loraSupport ?? null,
        loras: built.loras,
        loraTriggerPhrase: built.loraTriggerPhrase,
        appendedTriggerPhrases: built.appendedTriggerPhrases,
        orientation: built.orientation ?? null,
      }),
    }));
  }

  // The HOST_OWNED_PARAMETER_KEYS set itself, in declaration order.
  {
    jest.resetModules();
    jest.doMock('@/lib/plugins/provider-registry', () => ({
      getImageGenerationModels: () => null,
      getImageProviderConstraints: () => undefined,
    }));
    const { HOST_OWNED_PARAMETER_KEYS } = await import('@/lib/image-gen/params-builder');
    lines.push(JSON.stringify({
      label: 'pb_host_owned_keys',
      kind: 'params_const',
      json: JSON.stringify([...HOST_OWNED_PARAMETER_KEYS]),
    }));
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`image-gen-leaves oracle wrote ${outPath} (${lines.length} cases)\n`);
}

test('image-gen-leaves oracle', async () => {
  await main();
});
