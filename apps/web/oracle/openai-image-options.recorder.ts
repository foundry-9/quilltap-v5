/**
 * The v4-side recorder behind
 * `src/app/screens/settings/images/openai-image-options.spec.ts`.
 *
 * v4's OpenAI plugin declares an image-profile options schema per selected
 * model (`plugins/dist/qtap-plugin-openai/image-options-schema.ts`, added by
 * `d8d2890ee` / PR #62). The host serves it verbatim through the
 * `options-schema` action, so the SPA renders bytes v5 never authors — which
 * makes a RECORDING, not a transcription, the honest client-side proof: the
 * spec mounts `ProviderOptionsPanel` over exactly what v4's function returns.
 *
 * `getOpenAIImageOptionsSchema` is pure and imports only its sibling
 * capability table, so it records straight from a pinned worktree with no
 * plugin runtime, no host and no network.
 *
 * The five inputs are the §R.10(c) contract set — one per distinct schema
 * shape the table can produce, plus the no-model call:
 *
 *   - `gpt-image-2.5-sunburst` — the six-tier quality list (`xhigh`/`max`
 *     carrying descriptions), the thirteen-entry wide size list, the
 *     arbitrary-size help text, and the `GPT Image Output` group;
 *   - `gpt-image-2`            — the same size list and group with the FOUR-tier
 *     quality list and no `xhigh`/`max` descriptions;
 *   - `dall-e-3`              — `standard`/`hd`, three sizes, `style`, no group;
 *   - `dall-e-2`              — `standard` alone, three small sizes, no style;
 *   - `undefined`             — the no-model call, which must equal sunburst's.
 *
 * P4.D196 records the SAME function's output at the SAME pin for its server
 * differential; the unifier `diff -q`s the two recordings (§R.10(c)).
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's plugin sources
 * and would not compile in the SPA's own tsconfig.
 *
 * Run it from a pinned v4 worktree (Node 24 at `~/.nvm/versions/node/v24.13.1/bin`):
 *
 * ```bash
 * PIN=/tmp/qt-v4-pin-p4d197-5f0a57dc4
 * git -C ~/source/quilltap-server worktree add --detach "$PIN" 5f0a57dc4
 * ln -sfn ~/source/quilltap-server/node_modules "$PIN/node_modules"
 * cp <V5>/apps/web/oracle/openai-image-options.recorder.ts "$PIN/"
 * cd "$PIN" && npx tsx openai-image-options.recorder.ts \
 *   > <V5>/apps/web/src/app/screens/settings/images/__fixtures__/openai-image-options-schemas.json
 * ```
 *
 * Expect a five-key object; a shorter file means the recorder errored and the
 * redirect already truncated the old one (the empty-file trap). Pin check: the
 * `gpt-image-2.5-sunburst` quality list must carry `xhigh` — the module does
 * not exist at the `1fefadb9a` baseline at all, so a baseline-pinned run
 * cannot even resolve the import.
 */

import { getOpenAIImageOptionsSchema } from './plugins/dist/qtap-plugin-openai/image-options-schema'

const MODELS: readonly (string | undefined)[] = [
  'gpt-image-2.5-sunburst',
  'gpt-image-2',
  'dall-e-3',
  'dall-e-2',
  undefined,
]

const out: Record<string, unknown> = {}
for (const model of MODELS) {
  out[model ?? '(no model)'] = getOpenAIImageOptionsSchema(model)
}

process.stdout.write(JSON.stringify(out, null, 2) + '\n')
