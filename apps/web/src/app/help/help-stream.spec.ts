/**
 * `help-stream.ts` parity — v4 `components/help-chat/hooks/useHelpChatStreaming.ts`
 * at `d883a5ee1`.
 *
 * Three proofs:
 *
 *  1. **`labelFromUrl` against a 35-vector capture** of v4's REAL function
 *     (`__fixtures__/label-from-url-vectors.json`), which reaches the quirks
 *     v4's own suite never asks about — `+` decoding to a space, a leading
 *     hyphen surviving in `tab` but yielding a DOUBLE space in `section`, an
 *     empty `tab` skipped as falsy, `/setupwizard` prefix-matching `/setup`.
 *  2. **v4's own 127-line jest suite**
 *     (`__tests__/unit/components/help-chat/hooks/labelFromUrl.test.ts`) ported
 *     case-for-case; it was run green at the pin before transcription.
 *  3. **The frame fold**, case by case against the hook's read loop, including
 *     the two branches that halt it and the buffer-clearing `status` rule that
 *     is the whole reason this is not the Salon reducer.
 */

import { describe, expect, it } from 'vitest';

import type { ChatStreamFrame } from '../core/core-contract';
import { initialHelpStreamState, labelFromUrl, reduceHelpFrame } from './help-stream';
import vectors from './__fixtures__/label-from-url-vectors.json';

function fold(frames: ChatStreamFrame[]) {
  return frames.reduce(reduceHelpFrame, initialHelpStreamState());
}

describe('labelFromUrl — the recorded-vector corpus (from v4 itself)', () => {
  it('covers at least 20 urls', () => {
    expect(vectors.labelFromUrl.length).toBeGreaterThanOrEqual(20);
  });

  for (const row of vectors.labelFromUrl) {
    it(`${JSON.stringify(row.url)} → ${JSON.stringify(row.label)}`, () => {
      expect(labelFromUrl(row.url)).toBe(row.label);
    });
  }
});

// ---------------------------------------------------------------------------
// v4's own suite, ported case-for-case (`labelFromUrl.test.ts`).
// ---------------------------------------------------------------------------

describe('labelFromUrl', () => {
  it('converts /settings to Settings', () => {
    expect(labelFromUrl('/settings')).toBe('Settings');
  });
  it('converts /aurora to Characters', () => {
    expect(labelFromUrl('/aurora')).toBe('Characters');
  });
  it('converts /salon to Chats', () => {
    expect(labelFromUrl('/salon')).toBe('Chats');
  });
  it('converts /prospero to Projects', () => {
    expect(labelFromUrl('/prospero')).toBe('Projects');
  });
  it('converts /profile to Profile', () => {
    expect(labelFromUrl('/profile')).toBe('Profile');
  });
  it('converts /files to Files', () => {
    expect(labelFromUrl('/files')).toBe('Files');
  });
  it('converts /setup to Setup', () => {
    expect(labelFromUrl('/setup')).toBe('Setup');
  });
  it('passes through unknown paths', () => {
    expect(labelFromUrl('/unknown')).toBe('/unknown');
  });
  it('adds tab to settings path', () => {
    expect(labelFromUrl('/settings?tab=chat')).toBe('Settings → Chat');
  });
  it('adds tab to appearance', () => {
    expect(labelFromUrl('/settings?tab=appearance')).toBe('Settings → Appearance');
  });
  it('capitalizes tab name', () => {
    expect(labelFromUrl('/settings?tab=system')).toBe('Settings → System');
  });
  it('converts hyphens to spaces in tab', () => {
    expect(labelFromUrl('/settings?tab=my-tab')).toBe('Settings → My tab');
  });
  it('adds section after tab', () => {
    expect(labelFromUrl('/settings?tab=chat&section=dangerous-content')).toBe(
      'Settings → Chat → Dangerous Content',
    );
  });
  it('capitalizes section words', () => {
    expect(labelFromUrl('/settings?tab=system&section=data-management')).toBe(
      'Settings → System → Data Management',
    );
  });
  it('handles multiple hyphens in section', () => {
    expect(labelFromUrl('/settings?tab=appearance&section=theme-color-palette')).toBe(
      'Settings → Appearance → Theme Color Palette',
    );
  });
  it('adds section without tab', () => {
    expect(labelFromUrl('/settings?section=general')).toBe('Settings → General');
  });
  it('capitalizes single-word section', () => {
    expect(labelFromUrl('/settings?section=appearance')).toBe('Settings → Appearance');
  });
  it('handles paths with no query string', () => {
    expect(labelFromUrl('/settings')).toBe('Settings');
  });
  it('handles empty query string', () => {
    expect(labelFromUrl('/settings?')).toBe('Settings');
  });
  it('ignores unrelated query parameters', () => {
    expect(labelFromUrl('/settings?other=value&tab=chat')).toBe('Settings → Chat');
  });
  it('handles query parameters in different order', () => {
    expect(labelFromUrl('/settings?section=test&tab=chat')).toBe('Settings → Chat → Test');
  });
  it('aurora with tab and section', () => {
    expect(labelFromUrl('/aurora?tab=browse&section=sort-options')).toBe(
      'Characters → Browse → Sort Options',
    );
  });
  it('salon with settings tab', () => {
    expect(labelFromUrl('/salon?tab=settings')).toBe('Chats → Settings');
  });
  it('profile with complex section', () => {
    expect(labelFromUrl('/profile?section=account-security')).toBe('Profile → Account Security');
  });
  it('lowercases query parameters for processing', () => {
    expect(labelFromUrl('/settings?tab=Chat')).toBe('Settings → Chat');
  });
  it('handles single-letter words in hyphens', () => {
    expect(labelFromUrl('/settings?tab=a-b-c')).toBe('Settings → A b c');
  });
  it('preserves tab name format with underscores if present', () => {
    expect(labelFromUrl('/settings?tab=my_tab')).toBe('Settings → My_tab');
  });
});

// ---------------------------------------------------------------------------
// The frame fold (v4's read loop).
// ---------------------------------------------------------------------------

describe('reduceHelpFrame — content and turns', () => {
  it('appends content chunks and clears the tool indicator', () => {
    const s = fold([{ status: 'thinking' }, { content: 'Hel' }, { content: 'lo' }]);
    expect(s.streamingContent).toBe('Hello');
    expect(s.isExecutingTools).toBe(false);
  });

  it('turnStart resets the buffer and switches the participant', () => {
    const s = fold([
      { content: 'first speaker' },
      { turnStart: true, participantId: 'p-2' },
      { content: 'second' },
    ]);
    expect(s.streamingContent).toBe('second');
    expect(s.streamingParticipantId).toBe('p-2');
  });

  it('done clears the buffer and records the message id', () => {
    const s = fold([{ content: 'answer' }, { done: true, messageId: 'm-1' }]);
    expect(s.streamingContent).toBe('');
    expect(s.completedMessageIds).toEqual(['m-1']);
  });

  it('a done without a messageId records nothing (v4 `if (messageId)`)', () => {
    const s = fold([{ done: true }]);
    expect(s.completedMessageIds).toEqual([]);
  });

  it('status clears the buffer and raises the tool indicator', () => {
    // This is the branch that makes the help fold NOT the Salon reducer: v4
    // wipes stale prose so a tool pass reads as "working", not as a truncated
    // answer.
    const s = fold([{ content: 'partial answer' }, { status: 'executing_tools' }]);
    expect(s.streamingContent).toBe('');
    expect(s.isExecutingTools).toBe(true);
  });

  it('status carries a participant id when it has one, else keeps the current', () => {
    const withId = fold([{ status: 'thinking', participantId: 'p-9' }]);
    expect(withId.streamingParticipantId).toBe('p-9');
    const without = fold([
      { turnStart: true, participantId: 'p-1' },
      { status: 'thinking' },
    ]);
    expect(without.streamingParticipantId).toBe('p-1');
  });
});

describe('reduceHelpFrame — the two halting branches', () => {
  it('an error frame records the sentence BARE and halts', () => {
    // Deliberate divergence from the shared reducer, which joins
    // `${error}: ${details}`. §B pins `details: ''` on help error frames.
    const s = fold([
      { error: 'The archives are shut', errorType: 'processing_error', details: '' },
      { content: 'never read' },
    ]);
    expect(s.error).toBe('The archives are shut');
    expect(s.isStreaming).toBe(false);
    expect(s.streamingContent).toBe('');
  });

  it('records the sentence bare even when `details` is NOT empty', () => {
    // The arm that makes the divergence measurable. §B pins `details: ''` on
    // the help error frame, so the production shape cannot tell the two
    // spellings apart — measured: a mutation adopting the shared reducer's
    // `${error}: ${details}` join survived the whole suite until this case
    // existed. v4's hook reads `event.error` and nothing else.
    const s = fold([
      { error: 'The archives are shut', errorType: 'fatal_error', details: 'ECONNRESET' },
    ]);
    expect(s.error).toBe('The archives are shut');
  });

  it('chainComplete ends the run and halts', () => {
    const s = fold([
      { content: 'last words' },
      { chainComplete: true, reason: 'cycle_complete', chainDepth: 1 },
      { content: 'never read' },
    ]);
    expect(s.isStreaming).toBe(false);
    expect(s.streamingContent).toBe('');
    expect(s.halted).toBe(true);
  });
});

describe('reduceHelpFrame — help_navigate → navigation links', () => {
  const nav = (result: unknown, success = true): ChatStreamFrame => ({
    toolResult: { name: 'help_navigate', success, result },
  });

  it('reads `url` off an object result', () => {
    const s = fold([nav({ url: '/settings?tab=chat' })]);
    expect(s.streamingNavigationLinks).toEqual([
      { url: '/settings?tab=chat', label: 'Settings → Chat' },
    ]);
  });

  it('reads a JSON STRING result (v4 parses it)', () => {
    const s = fold([nav(JSON.stringify({ url: '/aurora' }))]);
    expect(s.streamingNavigationLinks).toEqual([{ url: '/aurora', label: 'Characters' }]);
  });

  it('falls back to `navigationUrl`', () => {
    const s = fold([nav({ navigationUrl: '/files' })]);
    expect(s.streamingNavigationLinks).toEqual([{ url: '/files', label: 'Files' }]);
  });

  it('dedupes by url', () => {
    const s = fold([nav({ url: '/salon' }), nav({ url: '/salon' })]);
    expect(s.streamingNavigationLinks).toHaveLength(1);
  });

  it('ignores an unsuccessful result', () => {
    const s = fold([nav({ url: '/salon' }, false)]);
    expect(s.streamingNavigationLinks).toEqual([]);
  });

  it('swallows an unparseable result rather than breaking the stream', () => {
    const s = fold([nav('{not json'), { content: 'still flowing' }]);
    expect(s.streamingNavigationLinks).toEqual([]);
    expect(s.streamingContent).toBe('still flowing');
  });

  it('ignores a result with no url at all', () => {
    const s = fold([nav({ other: 'thing' })]);
    expect(s.streamingNavigationLinks).toEqual([]);
  });
});

describe('reduceHelpFrame — help_search → suggested links', () => {
  const search = (result: unknown, success = true): ChatStreamFrame => ({
    toolResult: { name: 'help_search', success, result },
  });

  it('takes `title` when present and falls back to labelFromUrl', () => {
    const s = fold([
      search({
        results: [
          { url: '/settings?tab=memory', title: 'Commonplace Book' },
          { url: '/prospero' },
        ],
      }),
    ]);
    expect(s.suggestedLinks).toEqual([
      { url: '/settings?tab=memory', label: 'Commonplace Book' },
      { url: '/prospero', label: 'Projects' },
    ]);
  });

  it('accepts a BARE array result (v4 `result?.results || result`)', () => {
    const s = fold([search([{ url: '/files', title: 'Files' }])]);
    expect(s.suggestedLinks).toEqual([{ url: '/files', label: 'Files' }]);
  });

  it('skips urls that are not app-internal', () => {
    const s = fold([
      search({ results: [{ url: 'https://example.com/x' }, { url: 'relative' }, {}] }),
    ]);
    expect(s.suggestedLinks).toEqual([]);
  });

  it('skips a url already offered as a navigation link', () => {
    const s = fold([
      { toolResult: { name: 'help_navigate', success: true, result: { url: '/aurora' } } },
      search({ results: [{ url: '/aurora' }, { url: '/files' }] }),
    ]);
    expect(s.suggestedLinks).toEqual([{ url: '/files', label: 'Files' }]);
  });

  it('dedupes within the suggestions', () => {
    const s = fold([search({ results: [{ url: '/files' }, { url: '/files' }] })]);
    expect(s.suggestedLinks).toHaveLength(1);
  });

  it('ignores a non-array payload', () => {
    const s = fold([search({ results: { url: '/files' } })]);
    expect(s.suggestedLinks).toEqual([]);
  });
});

describe('reduceHelpFrame — one frame, several markers', () => {
  it('applies the branches in the v4 order: content then status wins the buffer', () => {
    // v4's read loop uses independent `if`s, so a frame carrying both leaves
    // the LATER branch's clear in place.
    const s = fold([{ content: 'text', status: 'executing_tools' }]);
    expect(s.streamingContent).toBe('');
    expect(s.isExecutingTools).toBe(true);
  });

  it('a terminal frame carrying a done and a chainComplete records both', () => {
    const s = fold([{ done: true, messageId: 'm-9', chainComplete: true }]);
    expect(s.completedMessageIds).toEqual(['m-9']);
    expect(s.isStreaming).toBe(false);
  });
});
