/**
 * The v4-side recorder behind `src/app/chat/slash-commands/slash-commands.oracle.spec.ts`.
 *
 * v4 `4877fc2da` added the composer's `/` slash commands with their pure logic
 * in `lib/slash-commands/` (types, trigger, ranking, registry) — a CLIENT tier
 * despite its `lib/` home. v5's `chat/slash-commands/` is a character-for-
 * character transcription, and THIS corpus, recorded from v4's REAL module at
 * the round pin `25b8fab6a`, is what proves it: every exported function over
 * v4's own vectors (`lib/slash-commands/__tests__/slash-commands.test.ts`) plus
 * a wider corpus along each rule's edges (every opener context, the alphabet
 * and length edges, each `WORD_SPLIT` separator, kind-order and label ties,
 * `sensitivity: 'base'`, the limit, and the registry's skip rules).
 *
 * Row kinds (the `kind` key): `constants` (`SLASH_TRIGGER` with its two regexes
 * as `{ source, flags }`, `COMPOSER_ACTION_SPECS` whole), `trigger`
 * (`findSlashTrigger`), `rank` (`rankSlashCommands`, compared as the ordered
 * key list), `build` (`buildSlashCommands`, compared whole with key order —
 * `sources.actions` travels as an ARRAY and is rebuilt into a `Set`).
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/lib/...` and
 * would not compile in the SPA's own tsconfig.
 *
 * Run it from a pinned v4 worktree at `25b8fab6a` (Node 24.13.1 at
 * `~/.nvm/versions/node/v24.13.1/bin` — `localeCompare` /
 * `toLocaleLowerCase` are Node's ICU on both sides):
 *
 * ```bash
 * PIN=/tmp/qt-v4-pin-<order>-25b8fab6a
 * git -C ~/source/quilltap-server worktree add --detach "$PIN" 25b8fab6a
 * ln -sfn ~/source/quilltap-server/node_modules "$PIN/node_modules"
 * export PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH
 * cp <V5>/apps/web/oracle/slash-commands.recorder.ts "$PIN/"
 * rm -f <V5>/apps/web/src/testing/fixtures/slash-commands.ndjson
 * cd "$PIN" && npx tsx slash-commands.recorder.ts \
 *   > <V5>/apps/web/src/testing/fixtures/slash-commands.ndjson
 * wc -l <V5>/apps/web/src/testing/fixtures/slash-commands.ndjson
 * ```
 *
 * Expect the line count recorded in `src/testing/fixtures/README.md`'s
 * `slash-commands.ndjson` section; a shorter file means the recorder errored
 * and the redirect already truncated the old one (the empty-file trap). A
 * second run must reproduce the file byte for byte (`cmp`). Pin marker: the
 * module does not exist before `4877fc2da`, so an older tree cannot run this.
 */

import {
  COMPOSER_ACTION_SPECS,
  SLASH_TRIGGER,
  buildSlashCommands,
  findSlashTrigger,
  rankSlashCommands,
  type ComposerAction,
  type SlashBuiltInToolSource,
  type SlashCommand,
  type SlashCustomToolSource,
} from '@/lib/slash-commands';

const rows: unknown[] = [];
const seen = new Set<string>();
function emit(row: { id: string; kind: string } & Record<string, unknown>): void {
  if (seen.has(row.id)) throw new Error(`duplicate vector label: ${row.id}`);
  seen.add(row.id);
  rows.push(row);
}

// --- constants ----------------------------------------------------------------
emit({
  id: 'constants',
  kind: 'constants',
  trigger: {
    opener: SLASH_TRIGGER.opener,
    queryPattern: { source: SLASH_TRIGGER.queryPattern.source, flags: SLASH_TRIGGER.queryPattern.flags },
    queryStartPattern: SLASH_TRIGGER.queryStartPattern
      ? { source: SLASH_TRIGGER.queryStartPattern.source, flags: SLASH_TRIGGER.queryStartPattern.flags }
      : null,
    minQueryLength: SLASH_TRIGGER.minQueryLength,
    maxQueryLength: SLASH_TRIGGER.maxQueryLength,
    closer: SLASH_TRIGGER.closer,
    lowercaseQuery: SLASH_TRIGGER.lowercaseQuery,
  },
  specs: COMPOSER_ACTION_SPECS,
});

// --- findSlashTrigger -----------------------------------------------------------
const TRIGGERS: Array<[string, string]> = [
  // v4's vectors, verbatim — opens
  ['trigger: bare /', '/'],
  ['trigger: /un', '/un'],
  ['trigger: space /un', ' /un'],
  ['trigger: (/un', '(/un'],
  ['trigger: hello /Wear', 'hello /Wear'],
  ['trigger: line\\n/d20', 'line\n/d20'],
  ['trigger: /wardrobe_wear', '/wardrobe_wear'],
  // v4's vectors, verbatim — stays shut
  ['trigger: and/or', 'and/or'],
  ['trigger: 10/15', '10/15'],
  ['trigger: http://', 'http://'],
  ['trigger: http://x', 'http://x'],
  ['trigger: see /usr/bin', 'see /usr/bin'],
  ['trigger: a/b/c', 'a/b/c'],
  ['trigger: /1', '/1'],
  ['trigger: /-', '/-'],
  ['trigger: /un space', '/un '],
  ['trigger: word/', 'word/'],
  ['trigger: / + 65 a', `/${'a'.repeat(65)}`],
  // every opener context
  ['trigger: [/un', '[/un'],
  ['trigger: {/un', '{/un'],
  ['trigger: "/un', '"/un'],
  ["trigger: '/un", "'/un"],
  ['trigger: “/un', '“/un'],
  ['trigger: ‘/un', '‘/un'],
  ['trigger: —/un', '—/un'],
  ['trigger: –/un', '–/un'],
  ['trigger: tab /un', 'a\t/un'],
  ['trigger: soft break bare /', 'line\n/'],
  ['trigger: image leaf before /', '￼/un'],
  ['trigger: text image leaf /', 'see ￼/un'],
  ['trigger: ) before /', ')/un'],
  ['trigger: * before /', '*/un'],
  ['trigger: . before /', './un'],
  ['trigger: nbsp before /', 'a /un'],
  ['trigger: bare / after space', 'hello /'],
  ['trigger: double slash', '//'],
  ['trigger: double slash word', '//un'],
  // the alphabet and its edges
  ['trigger: /a-b_c9', '/a-b_c9'],
  ['trigger: /Ä rejected', '/Ä'],
  ['trigger: /aä dissolves', '/aä'],
  ['trigger: /_x', '/_x'],
  ['trigger: /a.b', '/a.b'],
  ['trigger: /a b', '/a b'],
  ['trigger: /a/b', '/a/b'],
  ['trigger: /WEAR folds', '/WEAR'],
  ['trigger: /Wardrobe_Wear folds', 'say /Wardrobe_Wear'],
  ['trigger: /Z9', '/Z9'],
  // the length edges
  ['trigger: / + 64 a', `/${'a'.repeat(64)}`],
  ['trigger: / + 63 a + b', `/${'a'.repeat(63)}b`],
  ['trigger: prose + / + 64 a', `go /${'a'.repeat(64)}`],
  // the last slash decides
  ['trigger: and/or /un', 'and/or /un'],
  ['trigger: /un /wear', '/un /wear'],
  ['trigger: /un/wear', '/un/wear'],
  ['trigger: empty', ''],
];
for (const [id, text] of TRIGGERS) {
  emit({ id, kind: 'trigger', text, out: findSlashTrigger(text) });
}

// --- rankSlashCommands ------------------------------------------------------------
function command(
  partial: Partial<SlashCommand> & Pick<SlashCommand, 'command' | 'label' | 'kind'>,
): SlashCommand {
  return {
    key: `${partial.kind}:${partial.command}`,
    description: '',
    keywords: [],
    source: '',
    needsDialog: false,
    payload: { kind: 'action', action: 'attach' },
    ...partial,
  };
}

function rank(id: string, commands: SlashCommand[], query: string, limit?: number): void {
  const out = (limit === undefined ? rankSlashCommands(commands, query) : rankSlashCommands(commands, query, limit)).map(
    (c) => c.key,
  );
  emit({ id, kind: 'rank', commands, query, limit: limit ?? null, out });
}

// v4's ranking fixtures, verbatim
const wear = command({ kind: 'builtin-tool', command: 'wardrobe_wear', label: 'Wear Wardrobe Items' });
const unlock = command({ kind: 'custom-tool', command: 'unlock', label: 'Force the Lock' });
const unlockBuiltIn = command({ kind: 'builtin-tool', command: 'unlock', label: 'Unlock' });
const attach = command({ kind: 'action', command: 'attach', label: 'Attach file' });
const announce = command({ kind: 'action', command: 'announce', label: 'Insert announcement' });
const undo = command({ kind: 'builtin-tool', command: 'redo_thing', label: 'Redo' });
const all = [wear, unlock, unlockBuiltIn, attach, announce, undo];
const library = command({ kind: 'action', command: 'library', label: 'Attach from library' });
const tagged = command({ kind: 'action', command: 'mail', label: 'Post a letter', keywords: ['office'] });
const many = Array.from({ length: 20 }, (_, i) =>
  command({ kind: 'builtin-tool', command: `tool_${i}`, label: `Tool ${String(i).padStart(2, '0')}` }),
);

rank('rank: v4 bare slash', all, '');
rank('rank: v4 un', all, 'un');
rank('rank: v4 exact unlock', [undo, unlockBuiltIn], 'unlock');
rank('rank: v4 wear', all, 'wear');
rank('rank: v4 lock', all, 'lock');
rank('rank: v4 keyword off', [tagged], 'off');
rank('rank: v4 nlo mid-word', all, 'nlo');
rank('rank: v4 att tier over kind', [library, attach], 'att');
rank('rank: v4 tool limit 5', many, 'tool', 5);
rank('rank: v4 bare limit 5', many, '', 5);
// more queries over v4's fixtures
for (const q of ['u', 'w', 'wardrobe', 'items', 'a', 'f', 'the', 'redo', 'thing', 'x', 'UN', 'Wear', 'force the']) {
  rank(`rank: all ${JSON.stringify(q)}`, all, q);
}
// each WORD_SPLIT separator, and a run of several
const SEPARATORS: Array<[string, string]> = [
  ['apostrophe', "Alpha'beta"],
  ['right single quote', 'Alpha’beta'],
  ['period', 'Alpha.beta'],
  ['middle dot', 'Alpha·beta'],
  ['hyphen', 'Alpha-beta'],
  ['underscore', 'Alpha_beta'],
  ['space', 'Alpha beta'],
  ['tab', 'Alpha\tbeta'],
  ['run', "Alpha .-_'’· beta"],
  ['none', 'Alphabeta'],
  ['slash (not a separator)', 'Alpha/beta'],
  ['colon (not a separator)', 'Alpha:beta'],
];
for (const [name, label] of SEPARATORS) {
  rank(`rank: split ${name} label`, [command({ kind: 'action', command: 'zz', label })], 'beta');
  rank(
    `rank: split ${name} keyword`,
    [command({ kind: 'action', command: 'zz', label: 'Zed', keywords: [label] })],
    'beta',
  );
  rank(
    `rank: split ${name} command`,
    [command({ kind: 'action', command: label.toLowerCase(), label: 'Zed' })],
    'beta',
  );
}
// kind-order and label ties
const sameAction = command({ kind: 'action', command: 'same', label: 'Same' });
const sameCustomB = command({ kind: 'custom-tool', command: 'same', label: 'Same', key: 'custom:same::b' });
const sameCustomA = command({ kind: 'custom-tool', command: 'same', label: 'Same', key: 'custom:same::a' });
const sameBuiltIn = command({ kind: 'builtin-tool', command: 'same', label: 'Same' });
const ties = [sameBuiltIn, sameCustomB, sameAction, sameCustomA];
rank('rank: ties bare', ties, '');
rank('rank: ties exact', ties, 'same');
rank('rank: ties prefix', ties, 'sa');
rank('rank: ties word', [...ties, command({ kind: 'action', command: 'other', label: 'The Same' })], 'same');
// sensitivity: 'base'
const base = [
  command({ kind: 'action', command: 'c3', label: 'Átach', key: 'k3' }),
  command({ kind: 'action', command: 'c1', label: 'attach', key: 'k2' }),
  command({ kind: 'action', command: 'c2', label: 'Attach', key: 'k1' }),
  command({ kind: 'action', command: 'c4', label: 'Attach', key: 'k0' }),
  command({ kind: 'action', command: 'c5', label: 'atTach', key: 'K0' }),
];
rank('rank: base sensitivity bare', base, '');
rank('rank: base sensitivity c', base, 'c');
rank('rank: base sensitivity at', base, 'at');
rank('rank: base mixed letters', [
  command({ kind: 'action', command: 'b', label: 'b' }),
  command({ kind: 'action', command: 'bb', label: 'B' }),
  command({ kind: 'action', command: 'a', label: 'a' }),
  command({ kind: 'action', command: 'aa', label: 'ä' }),
  command({ kind: 'action', command: 'z', label: 'Z' }),
], '');
// the limit
const thirteen = Array.from({ length: 13 }, (_, i) =>
  command({ kind: 'custom-tool', command: `dice_${i}`, label: `Dice ${String.fromCharCode(97 + i)}` }),
);
rank('rank: 13 match limit default', thirteen, 'dice');
rank('rank: 13 match limit 12', thirteen, 'dice', 12);
rank('rank: 13 match limit 13', thirteen, 'dice', 13);
rank('rank: 13 match limit 0', thirteen, 'dice', 0);
rank('rank: 13 bare limit 0', thirteen, '', 0);
rank('rank: 13 bare limit default', thirteen, '');
rank('rank: 12 match limit 12', thirteen.slice(0, 12), 'dice', 12);
// the /coin collision — exact tie, action before custom
const coinAction = command({ kind: 'action', command: 'coin', label: 'Flip a coin', keywords: ['flip', 'toss', 'heads', 'tails'] });
const coinCustom = command({ kind: 'custom-tool', command: 'coin', label: 'Flip A Coin', key: 'custom:coin::char-1' });
rank('rank: coin collision exact', [coinCustom, coinAction], 'coin');
rank('rank: coin collision prefix', [coinCustom, coinAction], 'co');
rank('rank: coin collision word flip', [coinCustom, coinAction], 'flip');

// --- buildSlashCommands -----------------------------------------------------------
interface SourcesRow {
  actions: ComposerAction[];
  builtInTools: SlashBuiltInToolSource[];
  customTools: SlashCustomToolSource[];
}
function build(id: string, sources: Partial<SourcesRow>): SlashCommand[] {
  const full: SourcesRow = { actions: [], builtInTools: [], customTools: [], ...sources };
  const out = buildSlashCommands({
    actions: new Set(full.actions),
    builtInTools: full.builtInTools,
    customTools: full.customTools,
  });
  emit({ id, kind: 'build', sources: full, out });
  return out;
}

const ALL_ACTIONS: ComposerAction[] = [
  'attach', 'library', 'image', 'announce', 'mail', 'inform',
  'tool', 'pascal', 'roll', 'd6', 'd20', 'coin', 'bottle',
];

// v4's vectors, verbatim
build('build: v4 wired only', { actions: ['attach', 'd20'] });
const allBuilt = build('build: v4 all actions', { actions: ALL_ACTIONS });
const V4_BUILT_INS: SlashBuiltInToolSource[] = [
  { id: 'self_inventory', name: 'Self Inventory', description: 'd', category: 'utility', parameters: { type: 'object', properties: {} } },
  { id: 'wardrobe_wear', name: 'Wear Wardrobe Items', description: 'd', category: 'wardrobe', parameters: { type: 'object', properties: { operations: {} } } },
  { id: 'internal', name: 'Internal', description: 'd', userInvocable: false },
  { id: 'generate_image', name: 'Generate Image', description: 'd', available: false, unavailableReason: 'no profile' },
  { id: 'mcp_x', name: 'MCP X', description: 'd', category: 'plugin', pluginName: 'mcp' },
];
const builtIns = build('build: v4 built-ins', { builtInTools: V4_BUILT_INS });
const V4_CUSTOMS: SlashCustomToolSource[] = [
  { name: 'unlock', title: 'Force the Lock', description: 'd', parameters: { bonus: { type: 'number' } } },
  { name: 'wear_regalia', title: 'Wear Regalia', description: 'd', parameters: {} },
  { name: 'wear_regalia', title: 'Wear Regalia', description: 'd', parameters: {}, characterLabel: 'Vivienne', asCharacterId: 'char-v' },
];
const customs = build('build: v4 customs', { customTools: V4_CUSTOMS });
build('build: v4 shared word', {
  builtInTools: [{ id: 'rng', name: 'Random Number Generator', description: 'd' }],
  customTools: [{ name: 'rng', title: 'House Dice', description: 'd', parameters: {} }],
});
// the actions: none, each alone, out of spec order
build('build: no actions', {});
for (const action of ALL_ACTIONS) build(`build: action ${action} alone`, { actions: [action] });
build('build: actions out of spec order', { actions: ['bottle', 'd20', 'attach', 'pascal', 'image'] });
build('build: actions repeated', { actions: ['attach', 'attach', 'mail'] });
// built-in edges
build('build: built-in edges', {
  builtInTools: [
    { id: 'listed_available_undefined', name: 'A', description: 'd' },
    { id: 'listed_user_invocable_undefined', name: 'B', description: 'd', available: true },
    { id: 'listed_user_invocable_true', name: 'C', description: 'd', userInvocable: true },
    { id: '', name: 'Empty Id', description: 'd' },
    { id: 'dup', name: 'Dup First', description: 'first', category: 'one' },
    { id: 'dup', name: 'Dup Second', description: 'second', category: 'two' },
    { id: 'props_empty', name: 'P0', description: 'd', parameters: { properties: {} } },
    { id: 'props_absent', name: 'P1', description: 'd' },
    { id: 'props_no_key', name: 'P2', description: 'd', parameters: { type: 'object' } },
    { id: 'props_one', name: 'P3', description: 'd', parameters: { properties: { a: 1 } } },
    { id: 'props_null', name: 'P4', description: 'd', parameters: { properties: null as unknown as Record<string, unknown> } },
    { id: 'props_string', name: 'P5', description: 'd', parameters: { properties: 'x' as unknown as Record<string, unknown> } },
    { id: 'params_string', name: 'P6', description: 'd', parameters: 'x' as unknown as Record<string, unknown> },
    { id: 'params_number', name: 'P7', description: 'd', parameters: 5 as unknown as Record<string, unknown> },
    { id: 'params_null', name: 'P8', description: 'd', parameters: null as unknown as Record<string, unknown> },
    { id: 'props_array', name: 'P9', description: 'd', parameters: { properties: ['a'] as unknown as Record<string, unknown> } },
    { id: 'plugin_over_category', name: 'Q', description: 'd', category: 'cat', pluginName: 'plug' },
    { id: 'category_only', name: 'R', description: 'd', category: 'cat' },
    { id: 'plugin_only', name: 'S', description: 'd', pluginName: 'plug' },
    { id: 'empty_qualifiers', name: 'T', description: 'd', category: '', pluginName: '' },
    { id: 'empty_plugin', name: 'U', description: 'd', category: 'cat', pluginName: '' },
    { id: 'gated', name: 'V', description: 'd', available: false },
    { id: 'hidden', name: 'W', description: 'd', userInvocable: false, available: true },
  ],
});
// custom edges
build('build: custom edges', {
  customTools: [
    { name: 'no_char', title: 'No Char', description: 'd', parameters: {} },
    { name: 'null_char', title: 'Null Char', description: 'd', parameters: {}, asCharacterId: null },
    { name: 'str_char', title: 'Str Char', description: 'd', parameters: {}, asCharacterId: 'c-1', characterLabel: 'Ada' },
    { name: 'no_char', title: 'No Char Again', description: 'd', parameters: { a: {} } },
    { name: 'str_char', title: 'Str Char Two', description: 'd', parameters: {}, asCharacterId: 'c-2', characterLabel: 'Bea' },
    { name: 'str_char', title: 'Str Char Dup', description: 'd', parameters: {}, asCharacterId: 'c-1', characterLabel: 'Ada' },
    { name: '', title: 'Empty Name', description: 'd', parameters: {} },
    { name: 'params_null', title: 'Params Null', description: 'd', parameters: null as unknown as Record<string, unknown> },
    { name: 'empty_label', title: 'Empty Label', description: 'd', parameters: {}, characterLabel: '', asCharacterId: 'c-3' },
    { name: 'label_no_id', title: 'Label No Id', description: 'd', parameters: { x: { type: 'string' } }, characterLabel: 'Cy' },
  ],
});
// a mixed build, then ranked through the real labels and keywords
const mixed = build('build: mixed', {
  actions: ALL_ACTIONS,
  builtInTools: V4_BUILT_INS,
  customTools: [
    ...V4_CUSTOMS,
    { name: 'coin', title: 'Flip A Coin', description: 'house coin', parameters: {}, asCharacterId: 'char-a', characterLabel: 'Aria' },
  ],
});
for (const q of ['', 'a', 'at', 'roll', 'r', 'd', 'd2', 'wear', 'w', 'co', 'coin', 'flip', 'pascal', 'p', 'tool', 't', 'mail', 'office', 'ooc', 'aria', 'vivienne', 'mcp', 'utility', 'die', 'prospero', 'custom']) {
  rank(`rank: mixed ${JSON.stringify(q)}`, mixed, q);
}
rank('rank: mixed "d" limit 3', mixed, 'd', 3);
rank('rank: all actions bare', allBuilt, '');
rank('rank: v4 built-ins bare', builtIns, '');
rank('rank: v4 customs wear', customs, 'wear');

for (const row of rows) process.stdout.write(`${JSON.stringify(row)}\n`);
