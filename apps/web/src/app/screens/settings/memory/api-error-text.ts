/**
 * The error sentences the memory settings cards show (v4 `components/tools/
 * hooks/api-error-text.ts:17-32` at `01a83539d`), derived from a thrown
 * dispatch failure exactly as v4 derives them from a thrown `apiFetch`
 * failure. v4's `ApiFetchError` (an HTTP failure, `lib/query/fetcher.ts`) is
 * v5's {@link CoreDispatchError} (a `{ type: "error" }` envelope); anything
 * else is a network or other throw.
 *
 * The two NEW cards (Consolidation, Extraction Grain) use these; the older
 * cards keep their `err.message || fallback` idiom (behaviourally identical for
 * a write).
 */

import { CoreDispatchError } from '../../../core/core-contract';
import { getErrorMessage } from '../../../core/error-utils';

/**
 * For a read: any server failure reads as `fallback` (the body is not
 * consulted); a network or other error keeps its own message.
 */
export function readErrorText(err: unknown, fallback: string): string {
  if (err instanceof CoreDispatchError) return fallback;
  return getErrorMessage(err, fallback);
}

/**
 * For a write: a server failure shows the server's sentence when it carries
 * one, otherwise `fallback`; a network or other error keeps its own message.
 */
export function writeErrorText(err: unknown, fallback: string): string {
  if (err instanceof CoreDispatchError) return err.message || fallback;
  return getErrorMessage(err, fallback);
}
