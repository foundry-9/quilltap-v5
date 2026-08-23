import { describe, expect, it } from 'vitest';

import {
  getAttachmentSupportDescription,
  getSupportedMimeTypes,
  supportsMimeType,
} from './attachment-support';

/**
 * The static client capability table, transcribed from v4
 * `lib/llm/attachment-support.ts` at `a14a1811`. These cases pin the
 * transcription — including the ways it is deliberately imperfect, because v4
 * is imperfect the same way.
 *
 * The three rows v4's hand-kept table lacked until `a14a1811` (NANOGPT,
 * DEEPSEEK, Z_AI) landed with its bug-91 fix, so the P4.21-era "faithfully
 * stale" case below is now a case about what those rows SAY.
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

  it('gives the three non-sending providers no attachments at all', () => {
    // OLLAMA and OPENAI_COMPATIBLE strip attachments before the wire, and so
    // does DEEPSEEK — v4 `a14a1811` finally says so in the table rather than
    // leaving DeepSeek to fall through the unknown-provider branch.
    expect(getSupportedMimeTypes('OLLAMA')).toEqual([]);
    expect(getSupportedMimeTypes('OPENAI_COMPATIBLE')).toEqual([]);
    expect(getSupportedMimeTypes('DEEPSEEK')).toEqual([]);
  });

  it('carries the two rows bug 91 added for image-sending plugins', () => {
    // NanoGPT serialises `image_url` as of plugin 1.1.0; Z.AI does for its
    // vision models. Both were absent from the table until `a14a1811` — the
    // staleness this file recorded at P4.21, repaired upstream.
    expect(getSupportedMimeTypes('NANOGPT')).toEqual([
      'image/jpeg',
      'image/png',
      'image/gif',
      'image/webp',
    ]);
    expect(getSupportedMimeTypes('Z_AI')).toEqual([
      'image/jpeg',
      'image/png',
      'image/gif',
      'image/webp',
    ]);
  });

  it('treats a provider with no entry as supporting nothing (v4 `isKnownProvider`)', () => {
    expect(getSupportedMimeTypes('NOT_A_PROVIDER')).toEqual([]);
  });

  it('answers the one question the profile editor asks of it', () => {
    expect(supportsMimeType('OPENAI', 'image/jpeg')).toBe(true);
    expect(supportsMimeType('OLLAMA', 'image/jpeg')).toBe(false);
    expect(supportsMimeType('ANTHROPIC', 'application/pdf')).toBe(true);
    expect(supportsMimeType('OPENAI', 'application/pdf')).toBe(false);
  });

  /**
   * The seed `ProfileModal.onProviderChange` reads: `supportsMimeType(provider,
   * 'image/jpeg')` decides whether a NEW profile starts with the vision box
   * ticked. The bug-91 rows move it for two providers and pin it for a third.
   */
  it('seeds the vision box ON for the image-sending providers and OFF for the rest', () => {
    expect(supportsMimeType('NANOGPT', 'image/jpeg')).toBe(true);
    expect(supportsMimeType('Z_AI', 'image/jpeg')).toBe(true);
    expect(supportsMimeType('DEEPSEEK', 'image/jpeg')).toBe(false);
    // Unchanged by this round: an endpoint we know nothing about still seeds OFF.
    expect(supportsMimeType('NOT_A_PROVIDER', 'image/jpeg')).toBe(false);
  });

  /**
   * The sentence under the modal's provider select (v4
   * `getAttachmentSupportDescription`, `:189-216`) — three categories in a
   * fixed order, comma-joined, image subtypes upper-cased, `text/plain` spelled
   * TXT, and one fixed sentence for a provider that takes nothing.
   */
  it('renders v4’s attachment sentence for each provider', () => {
    expect(getAttachmentSupportDescription('OPENAI')).toBe('Images (JPEG, PNG, GIF, WEBP)');
    expect(getAttachmentSupportDescription('ANTHROPIC')).toBe(
      'Images (JPEG, PNG, GIF, WEBP), PDF documents, Text files (TXT)',
    );
    expect(getAttachmentSupportDescription('NANOGPT')).toBe('Images (JPEG, PNG, GIF, WEBP)');
    expect(getAttachmentSupportDescription('Z_AI')).toBe('Images (JPEG, PNG, GIF, WEBP)');
    expect(getAttachmentSupportDescription('OLLAMA')).toBe('No file attachments supported');
    expect(getAttachmentSupportDescription('DEEPSEEK')).toBe('No file attachments supported');
    expect(getAttachmentSupportDescription('OPENAI_COMPATIBLE')).toBe(
      'No file attachments supported',
    );
    expect(getAttachmentSupportDescription('NOT_A_PROVIDER')).toBe('No file attachments supported');
  });
});
