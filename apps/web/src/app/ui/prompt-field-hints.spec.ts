import { describe, expect, it } from 'vitest';

import { PROMPT_FIELD_HINTS, type PromptFieldHintKey } from './prompt-field-hints';

/**
 * The v4-client-oracle parity pin for the prompt-field hint table (v4
 * `components/prompt-fields/field-hints.ts` at `a6870c5a`).
 *
 * The expectation rows below were EMITTED from v4's real module — a tsx script
 * imported `PROMPT_FIELD_HINTS` from the v4 checkout and printed each entry as
 * a quoted tuple — so the bytes here came out of v4, not out of a second pass
 * of human retyping. Any drift on either side (a swapped typographic
 * apostrophe, a re-worded helper, a re-ordered or renamed key) goes red.
 *
 * Regen recipe (from the v4 checkout, Node 24):
 *   npx tsx <script importing components/prompt-fields/field-hints>
 * emitting `[key, label, helper, example]` tuples; paste in place of V4_HINTS.
 */
type Row = [key: string, label: string, helper: string, example: string | undefined];

const V4_HINTS: Row[] = [
  [
    'identity',
    'Identity',
    'What strangers know about the character on sight or by reputation — name, station, occupation, public reputation. The shallow first impression, and read only by others.',
    'Ariadne is a research librarian at the Athenaeum, known for finding what others gave up on.',
  ],
  [
    'description',
    'Description',
    'How acquaintances perceive the character — behaviour, mannerisms, frequent verbal patterns. Not physical appearance (that lives in physical descriptions), and never the inner monologue.',
    'She finishes other people’s sentences, then apologises for it.',
  ],
  [
    'manifesto',
    'Manifesto',
    'The foundational tenets of this character — the basic truths that anchor everything else, delivered to the character alone. What this character is, at root.',
    'You do not lie to Charlie, not even kindly.',
  ],
  [
    'personality',
    'Personality',
    'What the character knows about themselves — the inner drivers of speech and behaviour. Other characters don’t see it unless the character shows them.',
    'You keep your worry behind your teeth. You have never once asked for help first.',
  ],
  [
    'scenario',
    'Scenario',
    'The setting and circumstances for conversations — the stage, never the actor.',
    'The reading room is empty at this hour, rain against the high windows.',
  ],
  [
    'firstMessage',
    'First Message',
    'The character’s opening message to start conversations, in their own voice.',
    undefined,
  ],
  [
    'exampleDialogues',
    'Example Dialogues',
    'Example conversations that model the character’s voice, as {{char}}: / {{user}}: exchanges — each line exactly as that speaker would say it.',
    undefined,
  ],
  [
    'systemPrompt',
    'System Prompt',
    'Stage direction for the model playing the character — voice, pacing, boundaries, interaction style — addressed to the character directly.',
    'You are Ariadne. You answer plainly and you never flatter.',
  ],
  [
    'physicalDescription',
    'Physical Description',
    'Descriptive phrases only — this text also drives image generation, so keep to what a lens would record, never a sentence addressed to anyone.',
    'auburn hair cut short; grey eyes; a scar across the left knuckle',
  ],
  [
    'projectInstructions',
    'Project Instructions',
    'Standing instructions folded into the prompt of every character working in this project, addressed to the character they reach.',
    'You are helping Charlie draft sermon material; cite chapter and verse.',
  ],
  [
    'groupInstructions',
    'Group Instructions',
    'Standing instructions folded into the prompt of every member of this group, addressed to the character they reach.',
    'You have known the others here for years; you do not explain yourselves to each other.',
  ],
  [
    'wardrobeInstructions',
    'Dressing Instructions',
    'Standing guidance for a character choosing their own opening outfit, addressed to the character in the second person. Consulted only when a chat begins with “Let character choose” — the nearest copy wins (a character’s own over their group’s, a group’s over the project’s, the project’s over Quilltap General) and the search stops there.',
    'You prefer practical tweeds for fieldwork, and reserve the brass-buttoned frock coat for occasions with an audience.',
  ],
  [
    'roleplayTemplatePrompt',
    'LLM Prompt',
    'Formatting instructions delivered with every character’s prompt in chats using this template.',
    'Wrap narration in asterisks; keep replies to three paragraphs or fewer.',
  ],
];

describe('PROMPT_FIELD_HINTS — v4 parity', () => {
  it('carries exactly v4’s thirteen keys, in v4’s order', () => {
    expect(Object.keys(PROMPT_FIELD_HINTS)).toEqual(V4_HINTS.map(([key]) => key));
  });

  for (const [key, label, helper, example] of V4_HINTS) {
    it(`${key}: label, helper, and example match v4 byte-for-byte`, () => {
      const hint = PROMPT_FIELD_HINTS[key as PromptFieldHintKey] as {
        label: string;
        helper: string;
        example?: string;
      };
      expect(hint.label).toBe(label);
      expect(hint.helper).toBe(helper);
      expect(hint.example).toBe(example);
    });
  }

  it('uses typographic apostrophes, never ASCII ones, in every string', () => {
    const all = Object.values(PROMPT_FIELD_HINTS).flatMap((h) => [
      h.label,
      h.helper,
      (h as { example?: string }).example ?? '',
    ]);
    expect(all.filter((s) => s.includes("'"))).toEqual([]);
    // …and the six strings that DO carry one carry the typographic form.
    expect(all.filter((s) => s.includes('’')).length).toBe(6);
  });
});
