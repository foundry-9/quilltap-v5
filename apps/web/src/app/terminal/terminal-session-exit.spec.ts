import { describe, expect, it } from 'vitest';

import { applySessionExit } from './terminal';

/**
 * v4 `components/terminal/Terminal.tsx`, the session-exit effect at `77ff4e2e`.
 */

function fakeTerm(withTextarea = true) {
  const written: string[] = [];
  const textarea = withTextarea ? document.createElement('textarea') : undefined;
  return {
    written,
    textarea,
    term: {
      write: (chunk: string) => void written.push(chunk),
      textarea,
    },
  };
}

describe('applySessionExit (v4 exit effect @ 77ff4e2e)', () => {
  it('writes the exit-code line', () => {
    const { term, written } = fakeTerm();

    applySessionExit(term, { code: 0 });

    expect(written).toEqual(['\r\n[session ended — exit code 0]\r\n']);
  });

  it('names the signal when the session died on one', () => {
    const { term, written } = fakeTerm();

    applySessionExit(term, { code: null, signal: 'SIGKILL' });

    expect(written).toEqual(['\r\n[session ended — signal SIGKILL]\r\n']);
  });

  it('says unknown when neither a code nor a signal came back', () => {
    const { term, written } = fakeTerm();

    applySessionExit(term, null);

    expect(written).toEqual(['\r\n[session ended — exit code unknown]\r\n']);
  });

  it('disables the input textarea — NEWLY LIVE in v4 at 77ff4e2e', () => {
    // v4 used to poke `term._input`, which is not an xterm field in any
    // version, so the guard always short-circuited and an exited session stayed
    // typeable. v5 never had the poke at all. `textarea` is the documented
    // handle, and this is the first build of either app that refuses the
    // keystrokes.
    const { term, textarea } = fakeTerm();
    expect(textarea!.disabled).toBe(false);

    applySessionExit(term, { code: 137 });

    expect(textarea!.disabled).toBe(true);
  });

  it('still writes the line when xterm has not attached a textarea', () => {
    const { term, written } = fakeTerm(false);

    expect(() => applySessionExit(term, { code: 1 })).not.toThrow();
    expect(written).toEqual(['\r\n[session ended — exit code 1]\r\n']);
  });
});
