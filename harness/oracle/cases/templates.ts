/**
 * Oracle case: template processing (chat-orchestration wave 1.1, Part A).
 *
 * Drives the REAL pure functions from v4's lib/templates/processor.ts:
 *   processTemplate, buildTemplateContext, processCharacterTemplates.
 * All three are side-effect-free; no injection needed.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/templates.ts \
 *     > /tmp/oracle-templates.ndjson
 */

import {
  processTemplate,
  buildTemplateContext,
  processCharacterTemplates,
  type TemplateContext,
} from '@/lib/templates/processor';

type ProcessRow = {
  kind: 'process';
  id: string;
  template: string;
  context: Record<string, string>;
  out: string;
};

type BuildRow = {
  kind: 'build';
  id: string;
  character: {
    name: string;
    description?: string | null;
    manifesto?: string | null;
    personality?: string | null;
    scenarios?: Array<{ id: string; title: string; content: string }> | null;
    exampleDialogues?: string | null;
  };
  userCharacter?: { name: string; description?: string | null } | null;
  scenario?: string | null;
  systemPrompt?: string | null;
  out: TemplateContext;
};

type CharRow = {
  kind: 'char';
  id: string;
  character: {
    name: string;
    description?: string | null;
    manifesto?: string | null;
    personality?: string | null;
    firstMessage?: string | null;
    exampleDialogues?: string | null;
  };
  userCharacter?: { name: string; description?: string | null } | null;
  scenario?: string | null;
  systemPrompt?: string | null;
  out: {
    description: string;
    manifesto: string;
    personality: string;
    scenario: string;
    firstMessage: string;
    exampleDialogues: string;
    systemPrompt: string;
  };
};

type Row = ProcessRow | BuildRow | CharRow;

const rows: Row[] = [];

// ---- processTemplate ------------------------------------------------------
// Each entry: id, template, context (only string keys — matches v4's runtime).
const processes: Array<[string, string, Record<string, string>]> = [
  ['empty-template', '', { char: 'Ada' }],
  ['no-placeholder', 'plain text, no vars', {}],
  ['char-only', '{{char}}', { char: 'Ada' }],
  ['char-and-user', '{{char}} meets {{user}}', { char: 'Ada', user: 'Bob' }],
  ['missing-var-empty', '[{{unknown}}]', { char: 'Ada' }],
  ['missing-all', '{{a}}{{b}}{{c}}', {}],
  ['repeated-placeholder', '{{char}} {{char}} {{char}}', { char: 'X' }],
  ['adjacent-placeholders', '{{char}}{{user}}', { char: 'A', user: 'B' }],
  ['value-contains-placeholder-text', '{{char}}', { char: '{{user}}' }], // no recursive re-scan
  ['value-with-braces', 'x{{char}}y', { char: '{{notreplaced}}' }],
  ['empty-string-value', '[{{char}}]', { char: '' }], // present-but-empty -> ''
  ['underscore-key', '{{my_var}}', { my_var: 'V' }],
  ['digit-key', '{{v1}} {{v2}}', { v1: 'one', v2: 'two' }],
  ['leading-digit-key', '{{2fast}}', { '2fast': 'ok' }],
  ['mixed-case-key', '{{CamelCase}}', { CamelCase: 'C' }],
  ['not-word-hyphen', '{{my-var}}', { 'my-var': 'never' }], // hyphen not \w -> literal
  ['not-word-dot', '{{a.b}}', { 'a.b': 'never' }],
  ['not-word-space', '{{a b}}', {}],
  ['single-brace', '{char} {{char}}', { char: 'X' }],
  ['triple-brace', '{{{char}}}', { char: 'X' }], // {{char}} matches inside -> {X}
  ['unbalanced-open', '{{char}', { char: 'X' }],
  ['unbalanced-close', 'char}}', { char: 'X' }],
  ['empty-braces', '{{}}', {}], // \w+ needs 1+, so no match
  ['trim-opener-consumed', '{{trim}}\nx\n{{/trim}}', {}], // opener replaced by '' -> quirk
  ['trim-key-present', '{{trim}}x{{/trim}}', { trim: 'T' }],
  ['trim-closer-literal', 'a {{/trim}} b', {}],
  ['trim-all-newlines', '{{trim}}\n\n{{/trim}}', {}],
  ['persona-alias', '{{persona}} / {{user}}', { persona: 'a wanderer', user: 'Nadia' }],
  ['newlines-preserved', 'line1\n{{char}}\nline3', { char: 'MID' }],
  ['nonascii-value', '{{char}}', { char: 'Élise Ångström' }],
  ['astral-value', '{{char}} says {{emote}}', { char: '𝕏', emote: '🎩🚂' }],
  ['astral-in-template', 'prefix 🎩 {{char}} 🚂 suffix', { char: 'Aurora' }],
  ['tab-and-cr', '{{char}}\t\r\n{{user}}', { char: 'A', user: 'B' }],
  ['many-vars', '{{char}}/{{description}}/{{manifesto}}/{{personality}}/{{scenario}}', {
    char: 'C',
    description: 'D',
    manifesto: 'M',
    personality: 'P',
    scenario: 'S',
  }],
  ['unicode-word-key-not-matched', '{{café}}', { café: 'never' }], // é not ASCII \w
  ['value-is-empty-vs-missing', '{{present}}{{absent}}', { present: '' }],
  ['long-repeat', '{{x}}'.repeat(10), { x: 'ab' }],
  ['placeholder-at-edges', '{{a}}middle{{b}}', { a: 'START', b: 'END' }],
];
for (const [id, template, context] of processes) {
  rows.push({ kind: 'process', id, template, context, out: processTemplate(template, context as TemplateContext) });
}

// ---- buildTemplateContext -------------------------------------------------
const builds: Array<[string, BuildRow['character'], BuildRow['userCharacter'], string | null | undefined, string | null | undefined]> = [
  ['minimal', { name: 'Ada' }, null, null, null],
  ['full', {
    name: 'Ada',
    description: 'desc',
    manifesto: 'mani',
    personality: 'pers',
    exampleDialogues: 'ex',
  }, { name: 'Bob', description: 'bob-desc' }, 'my scenario', 'sys prompt'],
  ['null-fields', {
    name: 'Ada',
    description: null,
    manifesto: null,
    personality: null,
    exampleDialogues: null,
  }, null, null, null],
  ['empty-user-name-falls-back', { name: 'Ada' }, { name: '' }, null, null],
  ['user-no-description', { name: 'Ada' }, { name: 'Zed' }, null, null],
  ['user-null-description', { name: 'Ada' }, { name: 'Zed', description: null }, null, null],
  ['empty-scenario-and-system', { name: 'Ada' }, null, '', ''],
  ['nonascii', { name: 'Élise', description: 'Ångström note 🎩' }, { name: 'Zoë' }, 'скеналио', 'prompt'],
];
for (const [id, character, userCharacter, scenario, systemPrompt] of builds) {
  const out = buildTemplateContext({ character, userCharacter, scenario, systemPrompt });
  rows.push({ kind: 'build', id, character, userCharacter, scenario, systemPrompt, out });
}

// ---- processCharacterTemplates --------------------------------------------
const chars: Array<[string, CharRow['character'], CharRow['userCharacter'], string | null | undefined, string | null | undefined]> = [
  ['plain', {
    name: 'Ada',
    description: 'A brilliant inventor.',
    manifesto: null,
    personality: 'Curious.',
    firstMessage: 'Hello!',
    exampleDialogues: null,
  }, null, null, null],
  ['with-placeholders', {
    name: 'Ada',
    description: '{{char}} is talking to {{user}}.',
    manifesto: 'Manifesto of {{char}}.',
    personality: '{{char}} is {{personality}}',
    firstMessage: 'Hi {{user}}, I am {{char}}.',
    exampleDialogues: '{{user}}: hi\n{{char}}: hello',
  }, { name: 'Nadia', description: 'a wanderer' }, 'A {{char}}-only scene', 'You are {{char}}. Persona: {{persona}}.'],
  ['self-reference-desc', {
    name: 'Ada',
    description: 'I am {{description}}',
  }, null, null, null], // {{description}} maps to context.description = own desc (recursive-ish, single pass)
  ['null-everything', {
    name: 'Nomen',
    description: null,
    manifesto: null,
    personality: null,
    firstMessage: null,
    exampleDialogues: null,
  }, null, null, null],
  ['scenario-override', {
    name: 'Ada',
    description: 'd',
  }, null, 'Custom {{char}} scenario', null],
  ['nonascii-fields', {
    name: 'Élise',
    description: '{{char}} habite à Genève.',
    firstMessage: 'Bonjour, {{user}}! 🎩',
    exampleDialogues: null,
  }, { name: 'Zoë', description: 'une voyageuse' }, null, null],
  ['persona-in-system', {
    name: 'Aurora',
    description: 'd',
  }, { name: 'Traveler', description: 'from afar' }, null, 'System: user is {{user}} ({{persona}})'],
];
for (const [id, character, userCharacter, scenario, systemPrompt] of chars) {
  const out = processCharacterTemplates({ character, userCharacter, scenario, systemPrompt });
  rows.push({ kind: 'char', id, character, userCharacter, scenario, systemPrompt, out });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
