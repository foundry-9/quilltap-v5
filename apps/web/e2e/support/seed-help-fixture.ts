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

  // Prefer a character that already carries a connection profile — that is the
  // half of eligibility this seed cannot manufacture.
  const target = characters.find((c) => c.connectionProfileId) ?? characters[0];

  // `characterUpdate` takes the whole form bag under `character` (v4's
  // PUT /characters/:id) — the same door the characters Defaults tab uses.
  const update = await dispatch(ctx, baseUrl, {
    type: 'characterUpdate',
    characterId: target.id,
    character: { defaultHelpToolsEnabled: true },
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
