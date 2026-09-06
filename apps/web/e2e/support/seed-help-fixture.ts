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

import { MOCK_LLM_PORT } from './env';

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
  /** The archive tombstone — an archived character refuses every write. */
  archivedAt?: string | null;
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
  // Only a LIVE character can be seated: the archived island global-setup
  // seeds refuses every write (the archive tombstone), and in the full suite
  // the list's order is whatever earlier specs left, so the pick must skip
  // `archivedAt` rows and fall through on a refusal (the `p4.9i2`
  // unification's gate-of-record run: the seed landed on a refusing row, every
  // Ask beat skipped and the Guide's entry stayed disabled).
  const live = characters.filter((c) => !c.archivedAt);
  if (live.length === 0) {
    return { eligible: false, reason: 'the shared instance has no live (un-archived) character' };
  }
  const candidates = [
    ...live.filter((c) => c.defaultConnectionProfileId),
    ...live.filter((c) => !c.defaultConnectionProfileId),
  ];
  let target = candidates[0];
  // ALWAYS seat the character on the profile that targets the spec's canned
  // LLM (global-setup rewrote the fixture's OPENAI_COMPATIBLE profile to
  // `http://127.0.0.1:${MOCK_LLM_PORT}/v1`). In the full suite an earlier spec
  // leaves other profiles behind — a dead-endpoint understudy at
  // `localhost:8080` was the seat's default on the `p4.9i2` unification's
  // full run, and every send died on `error sending request` while the file
  // passed alone. A beat that depends on state another spec leaves is the
  // standing trap; pinning the seat's profile here removes the dependency.
  const profiles = await dispatch(ctx, baseUrl, { type: 'connectionProfileList' });
  const profileRows =
    (profiles?.data?.['profiles'] as Array<{ id: string; baseUrl?: string; isDefault?: boolean }> | undefined) ??
    (Array.isArray(profiles?.data)
      ? (profiles.data as Array<{ id: string; baseUrl?: string; isDefault?: boolean }>)
      : []);
  const chosen =
    profileRows.find((p) => (p.baseUrl ?? '').includes(`127.0.0.1:${MOCK_LLM_PORT}`)) ??
    profileRows.find((p) => p.isDefault) ??
    profileRows[0];
  if (!chosen) {
    return { eligible: false, characterId: target.id, reason: 'no connection profile to seat the help character on' };
  }
  // Eligibility's other half is the PROFILE: v4 counts a profile tool-capable
  // only when `allowToolUse !== false`, and an earlier spec in the full suite
  // switches that flag off on this very profile (the gate-of-record run's
  // skip reason: `No tool-capable connection profiles available`). Restore
  // it on the seat's profile; the flag is what the send's tool loop needs
  // anyway.
  const profileUpdate = await dispatch(ctx, baseUrl, {
    type: 'connectionProfileUpdate',
    profileId: chosen.id,
    profile: { allowToolUse: true },
  });
  if (profileUpdate?.type === 'error') {
    return {
      eligible: false,
      reason: `connectionProfileUpdate refused: ${String(profileUpdate.data?.['message'] ?? 'unknown')}`,
    };
  }
  const patch: Record<string, unknown> = {
    defaultHelpToolsEnabled: true,
    defaultConnectionProfileId: chosen.id,
  };

  // `characterUpdate` merges the partial bag under `character` — the same door
  // the characters Defaults tab's per-field autosave uses. Try each live
  // candidate until one accepts.
  let refused = '';
  let seated = false;
  for (const candidate of candidates) {
    const update = await dispatch(ctx, baseUrl, {
      type: 'characterUpdate',
      characterId: candidate.id,
      character: patch,
    });
    if (update?.type === 'error') {
      refused = String(update.data?.['message'] ?? 'unknown');
      continue;
    }
    target = candidate;
    seated = true;
    break;
  }
  if (!seated) {
    return { eligible: false, characterId: target.id, reason: `characterUpdate refused: ${refused}` };
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
