/**
 * @jest-environment node
 *
 * Differential ORACLE (tier-1, DB-free) for the W4.1d5 image-generation pure
 * leaves: `parsePlaceholders` (v4 `lib/image-gen/prompt-expansion.ts`) and
 * `resolveOrientation` (v4 `lib/image-gen/orientation.ts`). The plugin registry
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

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`image-gen-leaves oracle wrote ${outPath} (${lines.length} cases)\n`);
}

test('image-gen-leaves oracle', async () => {
  await main();
});
