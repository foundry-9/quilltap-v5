/**
 * The v4-side recorder behind `src/app/chat/carina-parser.oracle.spec.ts`.
 *
 * `parseCarinaQuery` (`lib/chat/carina-parser.ts`) is the one v4 module the SPA
 * gained a client twin of in P4.D181: In Their Own Words must NOT rehearse a
 * `@Name:` line, because a Carina address is machinery and has to survive
 * verbatim (`useImpersonationVoice.ts:66`). v4's client imports the server module
 * directly; v5 cannot, so `chat/carina-parser.ts` is a transcription and THIS
 * corpus is what proves it.
 *
 * The vectors were written against the real regex rather than the doc comment:
 * both separators, the four quote pairs, the `\w` ASCII boundary (an accented or
 * non-Latin name does NOT match), the one-word-name floor (the pattern needs a
 * word char at BOTH ends, so `@A:` cannot fire), the keep-scanning rule for an
 * empty question, CRLF, and the leading-whitespace refusal (`^@` is anchored).
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/lib/...` and
 * would not compile in the SPA's own tsconfig.
 *
 * Run it from a pinned v4 worktree (Node 24 at `~/.nvm/versions/node/v24.13.1/bin`):
 *
 * ```bash
 * PIN=/tmp/qt-v4-pin-<order>-<sha>
 * git -C ~/source/quilltap-server worktree add --detach "$PIN" <sha>
 * ln -sfn ~/source/quilltap-server/node_modules "$PIN/node_modules"
 * cp <V5>/apps/web/oracle/carina-parser.recorder.ts "$PIN/"
 * cd "$PIN" && npx tsx carina-parser.recorder.ts \
 *   > <V5>/apps/web/src/testing/fixtures/carina-parser.ndjson
 * ```
 *
 * Expect 49 lines; a shorter file means the recorder errored and the redirect
 * already truncated the old one (the empty-file trap).
 */

import { parseCarinaQuery } from '@/lib/chat/carina-parser';

/** `[label, content]` — the label is the spec's test name, so it must be unique. */
const VECTORS: Array<[string, string]> = [
  // --- the two separators -------------------------------------------------
  ['public address', '@Evangeline: what year is it?'],
  ['whispered address', '@Evangeline? what year is it?'],
  // --- the gate's own three cases, verbatim from v4's suite ----------------
  ['gate: mid-text @name is not an address', 'I look at @Evangeline and say nothing.'],
  ['gate: plain prose', 'I tell him I will take the job.'],
  ['gate: empty string', ''],
  // --- the `^@` anchor ----------------------------------------------------
  ['leading space defeats the anchor', ' @Evangeline: what year is it?'],
  ['leading tab defeats the anchor', '\t@Evangeline: what year is it?'],
  ['a second line CAN carry the address', 'Setting the scene.\n@Evangeline: what year is it?'],
  ['CRLF split is honoured', 'Setting the scene.\r\n@Evangeline: what year is it?'],
  // --- the name pattern: word char at BOTH ends, interior spaces allowed ---
  ['two-word name', '@Madame Evangeline: the hour, please.'],
  ['three-word name', '@The Right Honourable: your verdict?'],
  ['single character name cannot match (needs two word chars)', '@E: hello'],
  ['two character name is the floor', '@Ev: hello'],
  ['digits are word chars', '@Unit7: report'],
  ['underscores are word chars', '@brass_owl: report'],
  ['an apostrophe is NOT a word char', "@O'Malley: report"],
  ['a hyphen is NOT a word char', '@Jean-Luc: report'],
  ['an accented name does not match (ASCII \\w)', '@Zoé: bonjour'],
  ['a non-Latin name does not match (ASCII \\w)', '@Иван: привет'],
  ['an emoji name does not match', '@🦉: hoot'],
  ['a trailing space before the separator breaks the name', '@Evangeline : hello'],
  ['bare @ with no name', '@: hello'],
  ['bare @ alone', '@'],
  ['@ followed by the separator only', '@?'],
  // --- separators that are not separators ----------------------------------
  ['a comma is not a separator', '@Evangeline, what year is it?'],
  ['an exclamation is not a separator', '@Evangeline! what year is it?'],
  ['a dash is not a separator', '@Evangeline - what year is it?'],
  // --- whitespace after the separator --------------------------------------
  ['no space after the colon', '@Evangeline:what year is it?'],
  ['several spaces after the colon', '@Evangeline:     what year is it?'],
  ['a tab after the colon', '@Evangeline:\twhat year is it?'],
  // --- the empty-question keep-scanning rule --------------------------------
  ['an address with no question at all', '@Evangeline:'],
  ['an address with only whitespace after it', '@Evangeline:    '],
  ['an empty address then a real one on the next line', '@Evangeline:\n@Prosper: who is there?'],
  ['only the FIRST real question fires', '@Evangeline: first?\n@Prosper: second?'],
  // --- the four quote pairs -------------------------------------------------
  ['straight double quotes are stripped', '@Evangeline: "what year is it?"'],
  ['straight single quotes are stripped', "@Evangeline: 'what year is it?'"],
  ['smart double quotes are stripped', '@Evangeline: “what year is it?”'],
  ['smart single quotes are stripped', '@Evangeline: ‘what year is it?’'],
  ['an unmatched open quote keeps the quote', '@Evangeline: "what year is it?'],
  ['a mismatched close quote keeps the quote', '@Evangeline: “what year is it?"'],
  ['text after the close quote is dropped', '@Evangeline: "what year?" and be quick'],
  // An empty quoted span yields an EMPTY question, and an empty question makes
  // the line un-usable — the loop keeps scanning and the whole parse answers
  // null. Not "falls through to the remainder": `extractQuestion` already
  // returned, so the `(.*)` alternative never runs.
  ['an empty quoted span answers null, it does not fall through', '@Evangeline: ""'],
  // Same shape with text AFTER the empty span: `closeIdx` is 1, which IS > 0,
  // so the empty slice wins and ` really` is never seen.
  ['an empty span swallows the text after it', "@Evangeline: '' really"],
  ['inner quotes survive', '@Evangeline: "say \'hello\' twice"'],
  ['the quoted span is trimmed', '@Evangeline: "   spaced   "'],
  // --- odds and ends --------------------------------------------------------
  ['a markdown-ish line that merely starts with @', '@@Evangeline: hello'],
  ['an email-looking line', '@evangeline@example.com: ping'],
  ['whisper with a quoted question', "@Evangeline? 'is he still here'"],
  ['trailing newline after a valid address', '@Evangeline: what year is it?\n'],
];

const seen = new Set<string>();
for (const [label, content] of VECTORS) {
  if (seen.has(label)) throw new Error(`duplicate vector label: ${label}`);
  seen.add(label);
  console.log(JSON.stringify({ id: label, content, out: parseCarinaQuery(content) }));
}
