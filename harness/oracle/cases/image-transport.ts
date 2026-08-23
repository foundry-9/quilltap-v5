/**
 * Oracle case: the image-transport predicate pair (P4.D106; v4 `a14a1811`
 * bug 91, `lib/llm/image-transport.ts` + `lib/llm/attachment-support.ts`).
 *
 * Drives v4's REAL code in three sections:
 *   - `kind: "static"` — `staticProviderCanTransportImages` (the client-safe
 *     map: keys off the `types` list after the `isKnownProvider` guard,
 *     unknown → true) over every §C1 provider, case variants, and unknowns;
 *   - `kind: "full_uninit"` — `providerCanTransportImages` BEFORE the plugin
 *     registry is initialized (v4's startup/tests/job-child arm: falls back to
 *     the static map). v5 has no uninitialized-registry state, so its twin for
 *     these rows is the static tier;
 *   - `kind: "full_init"` — `providerCanTransportImages` AFTER
 *     `initializeProviderRegistry` with all TEN real dist plugins (the
 *     production truth: `supportsAttachments === true` AND an `image/*` MIME
 *     type in the plugin's declared list). v5's baked-manifest registry tier
 *     is the twin.
 *
 * ⚠ Section order matters: the two uninitialized sections run BEFORE the
 * registry init (a registry cannot be un-initialized in-process).
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/image-transport.ts \
 *     > /tmp/oracle-image-transport.ndjson
 */

import { createRequire } from 'node:module';
import { join } from 'node:path';

import { staticProviderCanTransportImages } from '@/lib/llm/attachment-support';
import { providerCanTransportImages } from '@/lib/llm/image-transport';

const nodeRequire = createRequire(import.meta.url);

// The ten built-in providers (UPPERCASE registry keys), case variants, and
// names neither source knows.
const PROVIDERS = [
  'OPENAI',
  'ANTHROPIC',
  'GOOGLE',
  'GROK',
  'OLLAMA',
  'OPENROUTER',
  'OPENAI_COMPATIBLE',
  'NANOGPT',
  'DEEPSEEK',
  'Z_AI',
  // Case variants — the static fn uppercases its input; the registry lookup
  // in `providerCanTransportImages` also uppercases before `getProvider`.
  'openrouter',
  'NanoGPT',
  'z_ai',
  'deepseek',
  // Unknown providers — both sources answer "not crippled by our ignorance".
  'SOME_THIRD_PARTY_VISION',
  'MYSTERY',
  '',
];

function emit(kind: string, provider: string, result: boolean): void {
  process.stdout.write(JSON.stringify({ kind, provider, result }) + '\n');
}

async function main(): Promise<void> {
  // ---- pre-init sections -------------------------------------------------
  for (const p of PROVIDERS) emit('static', p, staticProviderCanTransportImages(p));
  for (const p of PROVIDERS) emit('full_uninit', p, providerCanTransportImages(p));

  // ---- initialize the registry with the real dist plugins ----------------
  const PLUGIN_DIRS = [
    'anthropic',
    'openai',
    'google',
    'grok',
    'deepseek',
    'z-ai',
    'openrouter',
    'ollama',
    'openai-compatible',
    'nanogpt',
  ];
  const { initializeProviderRegistry } = await import('@/lib/plugins/provider-registry');
  const providers = PLUGIN_DIRS.map((d) => {
    const m = nodeRequire(join(process.cwd(), 'plugins', 'dist', `qtap-plugin-${d}`, 'index.js'));
    return m.plugin || m.default?.plugin || m.default;
  });
  await initializeProviderRegistry(providers);

  for (const p of PROVIDERS) emit('full_init', p, providerCanTransportImages(p));
}

void main();
