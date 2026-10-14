import { describe, expect, it } from 'vitest';

import {
  SEND_FAILURE_TOAST_MS,
  SUBMISSION_RESTORED_NOTE,
  sendFailureToastText,
} from './send-failure';

/** v4 `__tests__/unit/hooks/useSSEStreaming-send-failure.test.tsx:329-336`, verbatim. */
describe('sendFailureToastText (v4 bug 241)', () => {
  it('ends the reason as a sentence before the restored note', () => {
    expect(sendFailureToastText('Failed to generate response: No API key configured', true)).toBe(
      `Failed to generate response: No API key configured. ${SUBMISSION_RESTORED_NOTE}`,
    );
    expect(sendFailureToastText('Please try again.', true)).toBe(
      `Please try again. ${SUBMISSION_RESTORED_NOTE}`,
    );
    expect(sendFailureToastText('Nothing restored', false)).toBe('Nothing restored');
  });

  it("carries v4's bytes", () => {
    expect(SUBMISSION_RESTORED_NOTE).toBe('Your message is back in the composer.');
    expect(SEND_FAILURE_TOAST_MS).toBe(8000);
    // The other two terminal marks v4's pattern accepts, and a trailing space.
    expect(sendFailureToastText('Gone!  ', true)).toBe(`Gone! ${SUBMISSION_RESTORED_NOTE}`);
    expect(sendFailureToastText('Gone…', true)).toBe(`Gone… ${SUBMISSION_RESTORED_NOTE}`);
    expect(sendFailureToastText('Gone?', true)).toBe(`Gone? ${SUBMISSION_RESTORED_NOTE}`);
  });
});
