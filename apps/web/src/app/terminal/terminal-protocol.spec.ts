import { describe, expect, it } from 'vitest';

import { TERMINAL_SESSION_ID_RE, extractTerminalSessionId } from './terminal-protocol';

/**
 * The chat⟷embed marker binding is v4's content marker; the extraction must
 * match v4 `MessageRow.tsx:25` byte-for-byte (a comment rendering as raw HTML,
 * or vice versa, is a port failure — quirk #5).
 */
describe('extractTerminalSessionId (v4 TERMINAL_SESSION_ID_RE)', () => {
  const uuid = '6a1e8e0c-6a5f-4d2a-9d5b-2f6f1a3b4c5d';

  it('pulls the session id out of the exact v4 marker', () => {
    expect(extractTerminalSessionId(`<!-- terminalSessionId:${uuid} -->`)).toBe(uuid);
  });

  it('tolerates surrounding prose and no inner spaces', () => {
    expect(extractTerminalSessionId(`Opened a terminal.\n<!--terminalSessionId:${uuid}-->`)).toBe(
      uuid,
    );
  });

  it('is case-insensitive on the marker keyword', () => {
    expect(extractTerminalSessionId(`<!-- TERMINALSESSIONID:${uuid} -->`)).toBe(uuid);
  });

  it('returns null for null / empty / unmarked content', () => {
    expect(extractTerminalSessionId(null)).toBeNull();
    expect(extractTerminalSessionId(undefined)).toBeNull();
    expect(extractTerminalSessionId('')).toBeNull();
    expect(extractTerminalSessionId('just a message')).toBeNull();
  });

  it('rejects a non-hex id', () => {
    expect(extractTerminalSessionId('<!-- terminalSessionId:not_a_uuid -->')).toBeNull();
  });

  it('pins the regex source against drift', () => {
    expect(TERMINAL_SESSION_ID_RE.source).toBe('<!--\\s*terminalSessionId:([0-9a-f-]+)\\s*-->');
    expect(TERMINAL_SESSION_ID_RE.flags).toBe('i');
  });
});
