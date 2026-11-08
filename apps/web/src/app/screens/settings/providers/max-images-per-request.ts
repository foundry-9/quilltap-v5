/**
 * The per-profile image cap — how many pictures a vision profile is sent in
 * one request (v4 `d439f6dc9`, at the pin `624d0d25c`).
 *
 * v5 port of v4 `lib/schemas/profile.types.ts:42-45` (the two constants) and
 * `lib/llm/connection-profile-utils.ts:146-174` (`resolveMaxImagesPerRequest`,
 * `parseMaxImagesPerRequest`), VERBATIM, docblocks included. Proven row for row
 * against v4's real modules by `max-images-per-request.corpus.spec.ts`
 * (`src/testing/fixtures/spa-smalls.oracle.ndjson`).
 */

/** Images sent to a vision profile in one request when it names no cap of its own. */
export const DEFAULT_MAX_IMAGES_PER_REQUEST = 4;
/** The highest per-request image cap a profile may store. */
export const MAX_IMAGES_PER_REQUEST_CEILING = 50;

/**
 * The most images a profile may be sent in one request.
 *
 * The one reader of `maxImagesPerRequest`: a missing, non-integer or
 * out-of-range value (a row or bundle older than the column, a hand-edited
 * archive) resolves to `DEFAULT_MAX_IMAGES_PER_REQUEST` rather than to "no
 * cap", since an uncapped request is what some models refuse.
 */
export function resolveMaxImagesPerRequest(
  profile: { maxImagesPerRequest?: number | null } | null | undefined,
): number {
  const raw = profile?.maxImagesPerRequest;
  if (
    typeof raw === 'number' &&
    Number.isInteger(raw) &&
    raw >= 1 &&
    raw <= MAX_IMAGES_PER_REQUEST_CEILING
  ) {
    return raw;
  }
  return DEFAULT_MAX_IMAGES_PER_REQUEST;
}

/**
 * Validate a `maxImagesPerRequest` value from a request body. Accepts an
 * integer or a numeric string; returns null for anything that isn't a whole
 * number between 1 and `MAX_IMAGES_PER_REQUEST_CEILING`.
 */
export function parseMaxImagesPerRequest(value: unknown): number | null {
  const parsed = typeof value === 'string' && value.trim() !== '' ? Number(value) : value;
  if (typeof parsed !== 'number' || !Number.isInteger(parsed)) return null;
  if (parsed < 1 || parsed > MAX_IMAGES_PER_REQUEST_CEILING) return null;
  return parsed;
}
