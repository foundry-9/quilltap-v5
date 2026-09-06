/**
 * P4.9I2B's e2e seed: make the shared instance's mock character HELP-ENABLED.
 *
 * A help chat needs a character with BOTH `defaultHelpToolsEnabled` and a
 * tool-capable connection profile — that pair is exactly what
 * `helpChatEligibility` answers on, and what the rail button and the Ask
 * launcher gate on. The shared salon fixture's character already has the mock
 * OPENAI_COMPATIBLE profile (tool-capable), so only the flag is missing.
 *
 * Deliberately driven through the RUNNING SERVER's own verbs rather than by
 * writing the DB: the flag lives on a store-backed character record, and the
 * standing note is that SQL-seeding a store-overlay property is invisible to the
 * app unless every sha/length sidecar moves with it. The API is the honest door,
 * and it also proves the surface the characters Defaults tab already writes.
 *
 * Idempotent: a re-run flips an already-set flag to the same value.
 */

import type { APIRequestContext } from '@playwright/test';

/** What the seed found and did — reported by the beats' guard, never thrown. */
export interface HelpSeedResult {
  /** True when at least one character now answers eligible with a tool-capable profile. */
  eligible: boolean;
  /** The character the seed flipped, when it flipped one. */
  characterId?: string;
  /** Why the seed could not make anything eligible (for a LOUD skip message). */
  reason?: string;
}

interface CharacterRow {
  id: string;
  name?: string;
  defaultHelpToolsEnabled?: boolean;
  /** The list DTO's spelling (`core-contract.ts` `defaultConnectionProfileId`);
   *  the eligibility payload's is `connectionProfileId`. */
  defaultConnectionProfileId?: string | null;
  connectionProfileId?: string | null;
}

async function dispatch(
  ctx: APIRequestContext,
  baseUrl: string,
  body: Record<string, unknown>,
): Promise<{ type?: string; data?: Record<string, unknown> } | null> {
  const res = await ctx.post(`${baseUrl}/api/dispatch`, { data: body });
  return (await res.json().catch(() => null)) as {
    type?: string;
    data?: Record<string, unknown>;
  } | null;
}

/**
 * Flip `defaultHelpToolsEnabled` on the first character that has a connection
 * profile, then confirm through `helpChatEligibility` that the server agrees.
 *
 * Returns rather than throws: in-lane the help verbs do not exist at all, and a
 * beat wants to SKIP loudly with the reason, not fail on a missing surface.
 */
export async function seedHelpFixture(
  ctx: APIRequestContext,
  baseUrl: string,
): Promise<HelpSeedResult> {
  const list = await dispatch(ctx, baseUrl, { type: 'characterList' });
  const characters = (list?.data?.['characters'] as CharacterRow[] | undefined) ?? [];
  if (characters.length === 0) {
    return { eligible: false, reason: 'the shared instance has no characters' };
  }

  // Prefer a character that already carries a DEFAULT connection profile:
  // v4's help create copies `char.defaultConnectionProfileId || null` onto the
  // participant and `processHelpResponse` throws `No connection profile for
  // help character` on null — eligibility's "any tool-capable profile exists"
  // arm does NOT make the send work (the `p4.9i2` unification's first live
  // run: the seat was eligible and every send died on that sentence). So when
  // no character has one, give the seat the instance's default profile.
  const target = characters.find((c) => c.defaultConnectionProfileId) ?? characters[0];
  const patch: Record<string, unknown> = { defaultHelpToolsEnabled: true };
  if (!target.defaultConnectionProfileId) {
    const profiles = await dispatch(ctx, baseUrl, { type: 'connectionProfileList' });
    const rows =
      (profiles?.data?.['profiles'] as Array<{ id: string; isDefault?: boolean }> | undefined) ??
      (Array.isArray(profiles?.data) ? (profiles.data as Array<{ id: string; isDefault?: boolean }>) : []);
    const chosen = rows.find((p) => p.isDefault) ?? rows[0];
    if (!chosen) return { eligible: false, characterId: target.id, reason: 'no connection profile to seat the help character on' };
    patch['defaultConnectionProfileId'] = chosen.id;
  }

  // `characterUpdate` merges the partial bag under `character` — the same door
  // the characters Defaults tab's per-field autosave uses.
  const update = await dispatch(ctx, baseUrl, {
    type: 'characterUpdate',
    characterId: target.id,
    character: patch,
  });
  if (update?.type === 'error') {
    return {
      eligible: false,
      characterId: target.id,
      reason: `characterUpdate refused: ${String(update.data?.['message'] ?? 'unknown')}`,
    };
  }

  // The server is the arbiter of eligibility, not this seed's arithmetic.
  const elig = await dispatch(ctx, baseUrl, { type: 'helpChatEligibility' });
  if (elig?.type === 'error') {
    const message = String(elig.data?.['message'] ?? '');
    return {
      eligible: false,
      characterId: target.id,
      reason: /unknown variant/i.test(message)
        ? 'the help verbs are not served — the sibling server lane (P4.9I2A) lands them'
        : `helpChatEligibility refused: ${message}`,
    };
  }

  const rows = (elig?.data?.['characters'] as CharacterRow[] | undefined) ?? [];
  const capable = rows.filter(
    (c) => (c as { hasToolCapableProfile?: boolean }).hasToolCapableProfile,
  );
  if (capable.length === 0) {
    const reasons = (elig?.data?.['reasons'] as string[] | undefined) ?? [];
    return {
      eligible: false,
      characterId: target.id,
      reason: `no tool-capable help character after the flip${
        reasons.length ? ` — server says: ${reasons.join('; ')}` : ''
      }`,
    };
  }

  return { eligible: true, characterId: target.id };
}
