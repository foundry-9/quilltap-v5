/**
 * Model classes — pure constant capability tiers (v4 `lib/llm/model-classes.ts`).
 * A connection profile may reference one by name.
 */
export interface ModelClass {
  name: string;
  tier: string;
  maxContext: number;
  maxOutput: number;
  tags: readonly string[];
  quality: number;
}

export const MODEL_CLASSES: readonly ModelClass[] = [
  {
    name: 'Compact',
    tier: 'A',
    maxContext: 32000,
    maxOutput: 4000,
    tags: ['SMALL', 'CHEAP', 'LOCAL'],
    quality: 0,
  },
  {
    name: 'Standard',
    tier: 'B',
    maxContext: 128000,
    maxOutput: 16000,
    tags: ['BUDGET'],
    quality: 1,
  },
  {
    name: 'Extended',
    tier: 'C',
    maxContext: 200000,
    maxOutput: 128000,
    tags: ['CREATIVE', 'THINKING'],
    quality: 2,
  },
  { name: 'Deep', tier: 'D', maxContext: 1000000, maxOutput: 128000, tags: ['MAX'], quality: 3 },
];

export function getModelClass(name: string): ModelClass | undefined {
  return MODEL_CLASSES.find((mc) => mc.name === name);
}

/** Normalize a profile name for uniqueness (v4 `normalizeProfileName`): trim + lower. */
export function normalizeProfileName(name: string): string {
  return name.trim().toLowerCase();
}

/** `desired`, else `desired (2)`, … until unique vs `taken` (v4 `makeUniqueProfileName`). */
export function makeUniqueProfileName(desired: string, taken: Set<string>): string {
  const base = desired.trim();
  if (!taken.has(normalizeProfileName(base))) {
    return base;
  }
  for (let n = 2; ; n++) {
    const candidate = `${base} (${n})`;
    if (!taken.has(normalizeProfileName(candidate))) {
      return candidate;
    }
  }
}
