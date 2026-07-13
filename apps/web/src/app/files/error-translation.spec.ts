// The `code` cases are ported verbatim from v4
// `components/files/svar/error-translation.ts`; the kind-fallback cases are
// v5-derived (the dispatch envelope carries an ErrorKind, not an HTTP status).
import { describe, expect, it } from 'vitest';

import { translateMountOpError } from './error-translation';

describe('translateMountOpError — code first', () => {
  it('UNSUPPORTED offers the cross-storage copy-instead', () => {
    const v = translateMountOpError({ code: 'UNSUPPORTED' });
    expect(v.suggestCopy).toBe(true);
    expect(v.conflict).toBe(false);
    expect(v.rollback).toBe(true);
    expect(v.message).toContain('copy it across');
  });

  it('DEST_EXISTS and CONFLICT set the conflict flag', () => {
    expect(translateMountOpError({ code: 'DEST_EXISTS' }).conflict).toBe(true);
    expect(translateMountOpError({ code: 'CONFLICT' }).conflict).toBe(true);
  });

  it('NOT_EMPTY / SOURCE_NOT_FOUND / MOUNT_NOT_FOUND / INVALID_PATH / VERIFY_FAILED are plain rollbacks', () => {
    for (const code of ['NOT_EMPTY', 'SOURCE_NOT_FOUND', 'MOUNT_NOT_FOUND', 'INVALID_PATH', 'VERIFY_FAILED']) {
      const v = translateMountOpError({ code });
      expect(v).toMatchObject({ rollback: true, suggestCopy: false, conflict: false });
    }
  });

  it('code wins over kind', () => {
    // A coded UNSUPPORTED still suggests copy even if the kind is bad-request.
    expect(translateMountOpError({ code: 'UNSUPPORTED', kind: 'bad-request' }).suggestCopy).toBe(true);
  });
});

describe('translateMountOpError — kind fallback', () => {
  it('maps the dispatch ErrorKind to the closest verdict', () => {
    expect(translateMountOpError({ kind: 'conflict' }).conflict).toBe(true);
    expect(translateMountOpError({ kind: 'not-found' }).message).toContain('can’t lay hands');
    expect(translateMountOpError({ kind: 'bad-request' }).message).toContain('didn’t sit right');
  });

  it('falls back to the generic decline for an unknown kind/code', () => {
    const v = translateMountOpError({ kind: 'internal' });
    expect(v.message).toContain('declined that request');
    expect(v).toMatchObject({ rollback: true, suggestCopy: false, conflict: false });
    expect(translateMountOpError({}).message).toContain('declined that request');
  });

  it('an unrecognized code is ignored, falling through to kind', () => {
    expect(translateMountOpError({ code: 'WAT', kind: 'conflict' }).conflict).toBe(true);
  });
});
