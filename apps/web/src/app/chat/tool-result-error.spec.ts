import { describe, expect, it } from 'vitest';

import { resolveToolResultErrorText } from './tool-result-error';

/**
 * Parity with v4's own suite, case for case: v4's client is the SPA's oracle,
 * and the case table below is transcribed from
 * `__tests__/unit/hooks/useSSEStreaming-tool-error.test.ts` (v4 `d9c98cf2`,
 * Bug 84 — "the tool-result error sentence is carried to the client and then
 * ignored").
 *
 * The SSE emitter puts a failing tool's human-readable text in `error`, a
 * SIBLING of `result`, precisely because `result` is null on failure. The Salon
 * used to look for it at `result.error` — one level too deep — so every failure
 * fell back to "Failed to generate image" / "Unknown error".
 */
describe('resolveToolResultErrorText', () => {
  it('reads the sibling `error` field when `result` is null (the real failure shape)', () => {
    expect(
      resolveToolResultErrorText({
        result: null,
        error: 'Error: Image generation is not enabled for this chat',
      }),
    ).toBe('Image generation is not enabled for this chat');
  });

  it("strips the executor's own `Error: ` wrapper so the toast doesn't double it", () => {
    expect(resolveToolResultErrorText({ error: 'Error: something went wrong' })).toBe(
      'something went wrong',
    );
    expect(resolveToolResultErrorText({ error: 'Error:   padded' })).toBe('padded');
  });

  it('leaves a sentence without the wrapper alone', () => {
    expect(resolveToolResultErrorText({ error: 'No image profile resolved' })).toBe(
      'No image profile resolved',
    );
  });

  it('falls back to a nested `result.error` if a provider ever puts it there', () => {
    expect(resolveToolResultErrorText({ result: { error: 'Error: nested sentence' } })).toBe(
      'nested sentence',
    );
  });

  it('prefers the sibling field over the nested one', () => {
    expect(resolveToolResultErrorText({ result: { error: 'nested' }, error: 'sibling' })).toBe(
      'sibling',
    );
  });

  it('returns undefined when there is nothing worth showing, so callers use their generic text', () => {
    expect(resolveToolResultErrorText(undefined)).toBeUndefined();
    expect(resolveToolResultErrorText({})).toBeUndefined();
    expect(resolveToolResultErrorText({ result: null })).toBeUndefined();
    expect(resolveToolResultErrorText({ result: 'a plain string' })).toBeUndefined();
    expect(resolveToolResultErrorText({ error: '' })).toBeUndefined();
    expect(resolveToolResultErrorText({ error: 'Error: ' })).toBeUndefined();
  });
});
