import type { CharacterDetail, CharacterScenario, Pronouns } from '../../../core/core-contract';

/**
 * The Details-tab editable form (v4 `app/aurora/[id]/edit/types.ts`
 * `CharacterFormData`). Everything here round-trips through ONE
 * `characterUpdate` dispatch on "Save Character" — the physical-description /
 * depiction-guidelines slices (Appearance tab) and the avatar are saved
 * through their own, separate dispatches (v4-faithful: those are independent
 * PUT/POST calls in the original hooks too).
 *
 * Deviation from v4 worth flagging: v4's `TagEditor` persists add/remove
 * immediately (its own `?action=add-tag|remove-tag` calls). This port stages
 * tag membership locally in the form (an array of tag ids) and folds it into
 * the one `characterUpdate` bag on save, per the work order.
 */
export interface CharacterFormData {
  name: string;
  aliases: string[];
  pronouns: Pronouns | null;
  title: string;
  identity: string;
  description: string;
  manifesto: string;
  personality: string;
  scenarios: CharacterScenario[];
  firstMessage: string;
  exampleDialogues: string;
  systemPrompt: string;
  /** Tag ids currently attached to the character (staged, saved on submit). */
  tags: string[];
  systemTransparency: boolean;
  /** Tri-state per-character Core-whisper override: null = inherit. */
  coreWhisperEnabled: boolean | null;
  canBeCarina: boolean;
}

export const INITIAL_CHARACTER_FORM_DATA: CharacterFormData = {
  name: '',
  aliases: [],
  pronouns: null,
  title: '',
  identity: '',
  description: '',
  manifesto: '',
  personality: '',
  scenarios: [],
  firstMessage: '',
  exampleDialogues: '',
  systemPrompt: '',
  tags: [],
  systemTransparency: false,
  coreWhisperEnabled: null,
  canBeCarina: false,
};

/** Seed the form from the loaded `CharacterDetail` (v4 `fetchCharacter`'s `setState`). */
export function loadCharacterIntoForm(char: CharacterDetail): CharacterFormData {
  // `systemTransparency` / `coreWhisperEnabled` aren't in the narrow DTO surface
  // (they ride the `[key: string]: unknown` catch-all — the projection is a
  // hand-assembled whitelist in v4 too), so read them defensively.
  const rawTransparency = char['systemTransparency'];
  const rawCoreWhisper = char['coreWhisperEnabled'];
  return {
    name: char.name,
    aliases: char.aliases ?? [],
    pronouns: char.pronouns ?? null,
    title: char.title ?? '',
    identity: char.identity ?? '',
    description: char.description ?? '',
    manifesto: char.manifesto ?? '',
    personality: char.personality ?? '',
    scenarios: char.scenarios ?? [],
    firstMessage: char.firstMessage ?? '',
    exampleDialogues: char.exampleDialogues ?? '',
    systemPrompt: char.systemPrompt ?? '',
    tags: char.tags ?? [],
    systemTransparency: rawTransparency === true,
    coreWhisperEnabled: rawCoreWhisper === true || rawCoreWhisper === false ? rawCoreWhisper : null,
    canBeCarina: char.canBeCarina === true,
  };
}

/** Build the `characterUpdate` PUT bag from the form (v4's whole-`formData` PUT body). */
export function buildCharacterUpdateBag(form: CharacterFormData): Record<string, unknown> {
  return {
    name: form.name,
    aliases: form.aliases,
    pronouns: form.pronouns,
    title: form.title,
    identity: form.identity,
    description: form.description,
    manifesto: form.manifesto,
    personality: form.personality,
    scenarios: form.scenarios,
    firstMessage: form.firstMessage,
    exampleDialogues: form.exampleDialogues,
    systemPrompt: form.systemPrompt,
    tags: form.tags,
    systemTransparency: form.systemTransparency,
    coreWhisperEnabled: form.coreWhisperEnabled,
    canBeCarina: form.canBeCarina,
  };
}

/** The pronoun-preset dropdown options (v4 `CharacterBasicInfo.tsx` `PRONOUN_PRESETS`). */
export const PRONOUN_PRESETS: ReadonlyArray<{ label: string; value: Pronouns | 'custom' | null }> =
  [
    { label: 'Not set', value: null },
    { label: 'He/Him/His', value: { subject: 'he', object: 'him', possessive: 'his' } },
    { label: 'She/Her/Her', value: { subject: 'she', object: 'her', possessive: 'her' } },
    { label: 'They/Them/Their', value: { subject: 'they', object: 'them', possessive: 'their' } },
    { label: 'It/It/Its', value: { subject: 'it', object: 'it', possessive: 'its' } },
    { label: 'Custom', value: 'custom' },
  ];

/** Resolve which preset label matches the current pronouns (v4 `getPronounPreset`). */
export function getPronounPreset(pronouns: Pronouns | null): string {
  if (!pronouns) {
    return 'Not set';
  }
  for (const preset of PRONOUN_PRESETS) {
    if (
      preset.value &&
      preset.value !== 'custom' &&
      preset.value.subject === pronouns.subject &&
      preset.value.object === pronouns.object &&
      preset.value.possessive === pronouns.possessive
    ) {
      return preset.label;
    }
  }
  return 'Custom';
}

/** A fresh scenario row (v4 inline `newScenario` builder — client-minted id + timestamps). */
export function newScenario(): CharacterScenario {
  const now = new Date().toISOString();
  return { id: crypto.randomUUID(), title: '', content: '', createdAt: now, updatedAt: now };
}
