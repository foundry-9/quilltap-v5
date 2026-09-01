/**
 * Model matchers for `ProviderOptionField.appliesToModels` (v4
 * `lib/plugins/model-matchers.ts` at `84f33ce94`).
 *
 * Pure string work with no imports, deliberately: the options panel is a
 * client component and cannot reach the server-side plugin registry, so the
 * matcher that decides whether a field applies has to be able to run in the
 * browser. The server-side `match_model` in `quilltap-core`'s `image_gen`
 * answers a different question (which declaration object applies) against the
 * registry; this one answers "does this matcher list cover this model?".
 *
 * A matcher is one of:
 *   - an exact model id (`flux-lora`)
 *   - a `*` glob (`wavespeed-ai/*`, `flux-2-*`, `*-lora`)
 *   - a family prefix (`flux-lora` also covers `flux-lora/inpainting`)
 *
 * A TS→TS transcription of v4's module, character for character — the names
 * match P4.D138's Rust twin so the two halves of the feature stay greppable
 * together.
 */

/** Does one matcher cover this model id? */
export function modelMatchesPattern(model: string, pattern: string): boolean {
  if (!pattern) return false;
  if (model === pattern) return true;

  if (pattern.includes('*')) {
    const escaped = pattern
      .split('*')
      .map((part) => part.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))
      .join('.*');
    return new RegExp(`^${escaped}$`).test(model);
  }

  // Plain prefix: a family entry covers the SKUs beneath it.
  return model.startsWith(pattern);
}

/**
 * Should a field with this `appliesToModels` list render for this model?
 *
 * Renders unconditionally when the list is absent or empty, and when the host
 * does not know which model is selected — a field the user cannot see is a
 * setting they cannot reach, so "unknown" resolves toward showing it rather
 * than hiding it.
 *
 * v5's panel types `modelName` as a non-nullable `string` defaulting to `''`;
 * the empty string is falsy exactly as v4's `undefined` is, so the
 * unknown-model arm is reached identically.
 */
export function fieldAppliesToModel(
  appliesToModels: string[] | undefined,
  model: string | undefined,
): boolean {
  if (!appliesToModels || appliesToModels.length === 0) return true;
  if (!model) return true;
  return appliesToModels.some((pattern) => modelMatchesPattern(model, pattern));
}
