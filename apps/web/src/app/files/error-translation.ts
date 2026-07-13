/**
 * Translate a failed mount-point op into a user-facing verdict (pure).
 *
 * A ported-forward copy of v4 `components/files/svar/error-translation.ts`. The
 * steampunk-voiced `code`-keyed table ports verbatim (UI strings keep v4's
 * register). The FALLBACK is re-targeted: v4 fell back on the HTTP status
 * (409/404/400) when a response carried no `code`; the v5 dispatch envelope
 * carries no HTTP status — it carries an `ErrorKind` (`conflict` / `not-found`
 * / `bad-request` / …). So we key on the body `code` FIRST (the
 * `FileOpError ∪ DatabaseStoreError` union — always present on a coded file-op
 * failure) and fall back on the dispatch `kind`.
 *
 * At the dispatch envelope, `CoreError.code` is skip-serialized when absent
 * (the P4.6y pin) — the wiring core reads it defensively off `resp.data`.
 *
 * Cases derived from v4 `components/files/svar/error-translation.ts` (the
 * `code` table ported verbatim; the kind-fallback cases are v5-derived).
 *
 * @module files/error-translation
 */

/** Codes the v1 routes / dispatch verbs emit (FileOpError ∪ DatabaseStoreError). */
export type MountOpErrorCode =
  | 'SOURCE_NOT_FOUND'
  | 'DEST_EXISTS'
  | 'MOUNT_NOT_FOUND'
  | 'INVALID_PATH'
  | 'UNSUPPORTED'
  | 'VERIFY_FAILED'
  | 'CONFLICT'
  | 'NOT_FOUND'
  | 'INVALID'
  | 'NOT_EMPTY';

export interface ErrorVerdict {
  message: string;
  /** Revert the optimistic tree change that triggered this call. */
  rollback: boolean;
  /** Offer "copy instead" — set for the cross-storage link/move refusal. */
  suggestCopy: boolean;
  /** The change-on-disk conflict — prompt the user before overwriting. */
  conflict: boolean;
}

const BY_CODE: Record<MountOpErrorCode, Omit<ErrorVerdict, 'rollback'>> = {
  UNSUPPORTED: {
    message:
      'These two repositories keep their ledgers differently, so a direct link won’t hold. Shall I copy it across instead?',
    suggestCopy: true,
    conflict: false,
  },
  DEST_EXISTS: {
    message: 'A document already keeps that desk. Choose another name, or overwrite the incumbent.',
    suggestCopy: false,
    conflict: true,
  },
  CONFLICT: {
    message:
      'That file has changed on disk since you opened it. Reload before saving, lest you overwrite a newer hand.',
    suggestCopy: false,
    conflict: true,
  },
  NOT_EMPTY: {
    message: 'That drawer isn’t empty — clear its contents before discarding the folder.',
    suggestCopy: false,
    conflict: false,
  },
  SOURCE_NOT_FOUND: {
    message:
      'I can’t lay hands on that item any longer — it may have been moved or removed. Refreshing the shelves.',
    suggestCopy: false,
    conflict: false,
  },
  NOT_FOUND: {
    message:
      'I can’t lay hands on that item any longer — it may have been moved or removed. Refreshing the shelves.',
    suggestCopy: false,
    conflict: false,
  },
  MOUNT_NOT_FOUND: {
    message: 'That repository has gone missing from the catalogue. Refreshing the shelves.',
    suggestCopy: false,
    conflict: false,
  },
  INVALID_PATH: {
    message: 'That name won’t do — kindly avoid slashes and other untoward marks.',
    suggestCopy: false,
    conflict: false,
  },
  INVALID: {
    message: 'That request didn’t sit right with the archive. Do try again.',
    suggestCopy: false,
    conflict: false,
  },
  VERIFY_FAILED: {
    message:
      'Something went awry mid-transit and the archive couldn’t vouch for the result. Nothing was changed.',
    suggestCopy: false,
    conflict: false,
  },
};

const FALLBACK: Omit<ErrorVerdict, 'rollback'> = {
  message: 'The archive declined that request for reasons it didn’t care to share. Do try again.',
  suggestCopy: false,
  conflict: false,
};

/**
 * Map an error response to a verdict. Prefer the body `code`; fall back to the
 * dispatch `kind` for responses without one. Every recognized failure rolls
 * back the optimistic change.
 */
export function translateMountOpError(input: { kind?: string; code?: string }): ErrorVerdict {
  const byCode = input.code && (BY_CODE as Record<string, Omit<ErrorVerdict, 'rollback'>>)[input.code];
  if (byCode) return { ...byCode, rollback: true };

  // No recognized code — infer the gist from the dispatch ErrorKind (kebab-case
  // per the Rust `ErrorKind` serialization).
  if (input.kind === 'conflict') return { ...BY_CODE.CONFLICT, rollback: true };
  if (input.kind === 'not-found') return { ...BY_CODE.NOT_FOUND, rollback: true };
  if (input.kind === 'bad-request') return { ...BY_CODE.INVALID, rollback: true };
  return { ...FALLBACK, rollback: true };
}
