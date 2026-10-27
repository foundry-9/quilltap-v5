import { describe, expect, it } from 'vitest';

import { CoreDispatchError } from '../../../core/core-contract';
import { getErrorMessage } from '../../../core/error-utils';
import { readErrorText, writeErrorText } from './api-error-text';

/**
 * P4.D276 R-G — v5's twin of v4 `components/tools/hooks/api-error-text.ts:17-32`
 * and `lib/error-utils.ts` (`getErrorMessage`), at `01a83539d`.
 */
describe('api-error-text (P4.D276 R-G)', () => {
  const server = (message: string) => new CoreDispatchError({ kind: 'internal', message });

  it('a server failure on a READ reads as the fallback — the server text is not shown', () => {
    expect(readErrorText(server('Disk on fire'), 'Failed to load')).toBe('Failed to load');
  });

  it('a server failure on a WRITE shows the server text, the fallback when it is empty', () => {
    expect(writeErrorText(server('Watermark must be at least 1'), 'Failed to save')).toBe(
      'Watermark must be at least 1',
    );
    expect(writeErrorText(server(''), 'Failed to save')).toBe('Failed to save');
  });

  it('a plain Error keeps its message on both', () => {
    expect(readErrorText(new Error('fetch failed'), 'F')).toBe('fetch failed');
    expect(writeErrorText(new Error('fetch failed'), 'F')).toBe('fetch failed');
  });

  it('a string throw is itself; {} and null are the fallback', () => {
    expect(readErrorText('boom', 'F')).toBe('boom');
    expect(writeErrorText('boom', 'F')).toBe('boom');
    expect(readErrorText({}, 'F')).toBe('F');
    expect(writeErrorText({}, 'F')).toBe('F');
    expect(readErrorText(null, 'F')).toBe('F');
    expect(writeErrorText(undefined, 'F')).toBe('F');
  });

  it('getErrorMessage transcribes v4 lib/error-utils.ts', () => {
    expect(getErrorMessage(new Error(''), 'F')).toBe('');
    expect(getErrorMessage(42, 'F')).toBe('42');
    expect(getErrorMessage({ toString: () => 'custom' }, 'F')).toBe('custom');
    expect(getErrorMessage(undefined)).toBe('Unknown error');
  });
});
