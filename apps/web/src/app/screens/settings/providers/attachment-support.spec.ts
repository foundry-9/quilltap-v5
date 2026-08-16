import { describe, expect, it } from 'vitest';

import { getSupportedMimeTypes, supportsMimeType } from './attachment-support';

/**
 * The static client capability table, transcribed from v4
 * `lib/llm/attachment-support.ts` at `93ed8abf`. These cases pin the
 * transcription — including the two ways it is deliberately imperfect, because
 * v4 is imperfect the same way.
 */
describe('attachment support (v4 client table)', () => {
  it('carries each provider’s mime list verbatim', () => {
    expect(getSupportedMimeTypes('OPENAI')).toEqual([
      'image/jpeg',
      'image/png',
      'image/gif',
      'image/webp',
    ]);
    expect(getSupportedMimeTypes('ANTHROPIC')).toEqual([
      'image/jpeg',
      'image/png',
      'image/gif',
      'image/webp',
      'application/pdf',
      'text/plain',
    ]);
    expect(getSupportedMimeTypes('GOOGLE')).toEqual([
      'image/jpeg',
      'image/png',
      'image/gif',
      'image/webp',
    ]);
    expect(getSupportedMimeTypes('GROK')).toEqual([
      'image/jpeg',
      'image/png',
      'image/gif',
      'image/webp',
    ]);
    expect(getSupportedMimeTypes('OPENROUTER')).toEqual([
      'image/jpeg',
      'image/png',
      'image/gif',
      'image/webp',
    ]);
  });

  it('gives the two local-endpoint providers no attachments at all', () => {
    expect(getSupportedMimeTypes('OLLAMA')).toEqual([]);
    expect(getSupportedMimeTypes('OPENAI_COMPATIBLE')).toEqual([]);
  });

  it('treats a provider with no entry as supporting nothing (v4 `isKnownProvider`)', () => {
    // ⚠ Faithfully stale: Z_AI and DEEPSEEK ship as plugins at this baseline
    // and have no row in v4's hand-kept table, so both fall through here. A
    // new profile on either starts with the vision box unticked in BOTH apps.
    expect(getSupportedMimeTypes('Z_AI')).toEqual([]);
    expect(getSupportedMimeTypes('DEEPSEEK')).toEqual([]);
    expect(getSupportedMimeTypes('NOT_A_PROVIDER')).toEqual([]);
  });

  it('answers the one question the profile editor asks of it', () => {
    expect(supportsMimeType('OPENAI', 'image/jpeg')).toBe(true);
    expect(supportsMimeType('OLLAMA', 'image/jpeg')).toBe(false);
    expect(supportsMimeType('ANTHROPIC', 'application/pdf')).toBe(true);
    expect(supportsMimeType('OPENAI', 'application/pdf')).toBe(false);
  });
});
