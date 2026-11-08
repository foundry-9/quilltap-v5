/**
 * The v4-side recorder behind `src/app/chat/message-navigation.oracle.spec.ts`.
 *
 * `revealRequestFromSalonHref` (`lib/chat/message-navigation.ts`, v4
 * `4f5047241`) is the one pure function of v4's `?msg=` reveal: every way into
 * a `/salon/<id>?msg=<messageId>` link (the workspace link interceptor,
 * `useWorkspaceNavigate`) reads the chat and the message out of the href with
 * it before parking a reveal request. v5's `chat/message-navigation.ts` is a
 * transcription, and THIS corpus is what proves it.
 *
 * The vectors are v4's own test hrefs (`message-navigation.test.ts:80-94`) plus
 * the edges of the regex `/^\/salon\/([^/?#]+)\/?\?([^#]*)/`, the `new`
 * exclusion, `URLSearchParams` (`+`, an empty `msg`, two `msg`s, `&` noise) and
 * the chat id's `decodeURIComponent` (`%2F`, `%20`, an encoded `new`, and a
 * malformed escape, which THROWS — recorded as `threw`, never caught by v4).
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/lib/...` and
 * would not compile in the SPA's own tsconfig.
 *
 * Run it from the pinned v4 worktree, which carries its OWN `npm ci` (never a
 * symlink to a live checkout's `node_modules`), Node 24 at
 * `~/.nvm/versions/node/v24.13.1/bin`:
 *
 * ```bash
 * PIN='/tmp/qt-v4-pin-p4d384-624d0d25c'
 * ls -ld "$PIN" && git -C "$PIN" rev-parse HEAD   # 624d0d25c…
 * rm -f <V5>/apps/web/src/testing/fixtures/message-navigation.oracle.ndjson
 * cp <V5>/apps/web/oracle/message-navigation.recorder.ts "$PIN/"
 * cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   npx tsx message-navigation.recorder.ts \
 *   > <V5>/apps/web/src/testing/fixtures/message-navigation.oracle.ndjson
 * ```
 *
 * Expect EXACTLY `HREFS.length` = 42 lines; a shorter file means the recorder
 * errored and the redirect already truncated the old one (the empty-file trap).
 * `grep -c '"threw"'` must answer 1.
 *
 * Row shape: `{ id, href, out }` (`out` = the request or `null`) or
 * `{ id, href, threw }` (`threw` = `${error.name}: ${error.message}`).
 */

import { revealRequestFromSalonHref } from '@/lib/chat/message-navigation';

/** `[label, href]`. */
const HREFS: Array<[string, string]> = [
  // v4's own test hrefs (message-navigation.test.ts:80-94).
  ['v4: a Message search result link', '/salon/chat-a?msg=msg-1'],
  ['v4: a trailing slash and another param first', '/salon/chat-a/?x=1&msg=msg-1'],
  ['v4: no query', '/salon/chat-a'],
  ['v4: a query without msg', '/salon/chat-a?tab=x'],
  ['v4: /salon/new is excluded', '/salon/new?msg=m'],
  ['v4: a non-salon path', '/aurora/x?msg=m'],
  // `new`.
  ['new: with a trailing slash', '/salon/new/?msg=m'],
  ['new: upper case is a chat id', '/salon/NEW?msg=m'],
  ['new: percent-encoded decodes to "new" after the check', '/salon/%6Eew?msg=m'],
  ['new: a longer id starting with new', '/salon/newer?msg=m'],
  // The trailing slash.
  ['slash: trailing slash, no query', '/salon/chat-a/'],
  ['slash: two trailing slashes', '/salon/chat-a//?msg=m'],
  ['slash: a deeper path', '/salon/chat-a/terminal/s1?msg=m'],
  // The hash.
  ['hash: after the query', '/salon/chat-a?msg=m1#frag'],
  ['hash: inside the msg value position', '/salon/chat-a?x=1#msg=m1'],
  ['hash: before the query', '/salon/chat-a#frag?msg=m1'],
  ['hash: a hash-only tail', '/salon/chat-a#msg=m1'],
  // Encoded ids.
  ['encoded: %2F in the chat id', '/salon/a%2Fb?msg=m1'],
  ['encoded: %20 in the chat id', '/salon/a%20b?msg=m1'],
  ['encoded: a raw space in the chat id', '/salon/a b?msg=m1'],
  ['encoded: %2F in the msg', '/salon/chat-a?msg=m%2F1'],
  ['encoded: %20 in the msg', '/salon/chat-a?msg=m%201'],
  ['encoded: a malformed escape in the chat id', '/salon/a%ZZ?msg=m1'],
  ['encoded: a malformed escape in the msg', '/salon/chat-a?msg=m%ZZ'],
  ['encoded: a multi-byte chat id', '/salon/%C3%A9t%C3%A9?msg=m1'],
  // msg values.
  ['msg: empty', '/salon/chat-a?msg='],
  ['msg: bare key', '/salon/chat-a?msg'],
  ['msg: two msgs, the first wins', '/salon/chat-a?msg=a&msg=b'],
  ['msg: an empty first msg then a real one', '/salon/chat-a?msg=&msg=b'],
  ['msg: a plus is a space', '/salon/chat-a?msg=a+b'],
  ['msg: ampersand noise', '/salon/chat-a?&msg=m1&'],
  ['msg: upper-case key', '/salon/chat-a?MSG=m1'],
  ['msg: a uuid', '/salon/0b7c6f8e-1d2a-4f3b-9c8d-7e6f5a4b3c2d?msg=5f4e3d2c-1b0a-4987-8654-3210fedcba98'],
  // Non-salon paths.
  ['path: /salons', '/salons/x?msg=m'],
  ['path: /salon with no id', '/salon?msg=m'],
  ['path: /salon/ with no id', '/salon/?msg=m'],
  ['path: /workspace', '/workspace?open=salon&chatId=c&msg=m'],
  // Relative and absolute hrefs.
  ['relative: no leading slash', 'salon/chat-a?msg=m1'],
  ['relative: a query only', '?msg=m1'],
  ['absolute: an http URL', 'http://localhost:3000/salon/chat-a?msg=m1'],
  ['absolute: a protocol-relative URL', '//localhost/salon/chat-a?msg=m1'],
  ['empty: the empty string', ''],
];

const seen = new Set<string>();
for (const [label, href] of HREFS) {
  if (seen.has(label)) throw new Error(`duplicate vector label: ${label}`);
  seen.add(label);
  try {
    console.log(JSON.stringify({ id: label, href, out: revealRequestFromSalonHref(href) }));
  } catch (error) {
    const e = error as Error;
    console.log(JSON.stringify({ id: label, href, threw: `${e.name}: ${e.message}` }));
  }
}
