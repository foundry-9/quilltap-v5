/**
 * Tier-1 oracle case — the Post Office's letter-reference parser (v4
 * `lib/post-office/mailbox.ts` `resolveMailPath` + `letterFileName`, v4
 * `39bc98ffc`), P4.D234.
 *
 * Drives v4's REAL module over a fixed corpus and emits one NDJSON row per
 * input: `{ name, input, resolveMailPath, letterFileName }`, where
 * `resolveMailPath` is the resolved `Mail/…` path or `null`, and
 * `letterFileName` is the function applied to the raw input. The Rust side
 * (`mail_path_equivalence`) reads the input FROM the row, so the corpus lives
 * here alone and cannot drift between the two sides.
 *
 * The corpus pins the ORDER of `resolveMailPath`'s steps (the URI strip runs
 * BEFORE the leading-slash strip; exactly ONE `Mail/` strip), JS `trim`'s
 * whitespace set (NBSP, U+FEFF, U+2028 — not Rust's `str::trim`), the exact
 * `.` / `..` refusals (`...` and `.md` pass), the case-insensitive `.md` test
 * (`x.MD` keeps its extension), and non-ASCII names.
 *
 * Pure functions — no DB, no jest. The module imports the database store, so
 * the import alone must not open anything (it does not).
 *
 * Run (Node 24, from a v4 tree at or after `39bc98ffc` — the functions do not
 * exist before it, so a baseline run fails outright rather than passing stale):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/mail-path.ts \
 *     > /tmp/oracle-mail-path.ndjson
 */

import { letterFileName, resolveMailPath } from '@/lib/post-office/mailbox';

const CASES: Array<[string, string]> = [
  // v4's own mailbox.test.ts shapes
  ['bare_name', '111-from-ariadne.md'],
  ['bare_name_no_ext', '111-from-ariadne'],
  ['mail_path', 'Mail/111-from-ariadne.md'],
  ['self_uri', 'qtap://self/Mail/111-from-ariadne.md'],
  ['other_folder', 'Notes/secret.md'],
  ['dotdot_prefix', '../secret.md'],
  ['mail_dotdot', 'Mail/../secret.md'],
  ['dotdot', '..'],
  ['spaces_only', '   '],
  // the whitespace set JS trim covers
  ['ws_ascii', '  \t\n 111-from-ariadne.md \r\n'],
  ['ws_nbsp', ' 111-from-ariadne.md '],
  ['ws_bom', '﻿111-from-ariadne.md'],
  ['ws_line_sep', ' 111-from-ariadne.md '],
  ['ws_ideographic', '　111-from-ariadne.md'],
  ['ws_inside_kept', 'a b.md'],
  // the URI strip: case-insensitive, anchored, once
  ['uri_upper', 'QTAP://SELF/mail/x'],
  ['uri_double_slash', 'qtap://self//Mail/x'],
  ['uri_after_slash', '/qtap://self/Mail/x'],
  ['uri_twice', 'qtap://self/qtap://self/Mail/x'],
  ['uri_bare_name', 'qtap://self/x.md'],
  ['uri_other_authority', 'qtap://other/Mail/x'],
  ['uri_no_trailing_slash', 'qtap://self'],
  // leading slashes
  ['leading_slash_mail', '/Mail/x.md'],
  ['double_leading_slash', '//Mail/x'],
  ['leading_slash_bare', '/x.md'],
  // the Mail/ strip: exactly once, case-insensitive
  ['mail_lower', 'mail/X.MD'],
  ['mail_upper', 'MAIL/x'],
  ['mail_mail', 'Mail/Mail/x'],
  ['mail_folder_only', 'Mail/'],
  ['mail_word', 'Mail'],
  ['mailbox_prefix', 'Mailbox/x.md'],
  ['mail_no_slash_name', 'Mailx.md'],
  ['mail_sub', 'Mail/sub/x.md'],
  // the refusals
  ['empty', ''],
  ['dot', '.'],
  ['dot_ext', '.md'],
  ['three_dots', '...'],
  ['backslash', 'a\\b'],
  ['backslash_dotdot', '..\\secret.md'],
  ['mail_dot', 'Mail/.'],
  ['mail_dotdot_only', 'Mail/..'],
  ['trailing_slash', 'x.md/'],
  // the .md test
  ['ext_double', 'x.md.md'],
  ['ext_upper', 'x.MD'],
  ['ext_mixed', 'x.Md'],
  ['ext_other', 'x.txt'],
  ['ext_markdown', 'x.markdown'],
  ['ext_dot_only_tail', 'x.'],
  // non-ASCII names
  ['turkish_dotted_i', 'İ'],
  ['turkish_prefix', 'MAİL/x'],
  ['emoji', '🦚-from-ariadne'],
  ['accented', 'lettre-de-élodie.md'],
  ['cjk', '手紙'],
  // letterFileName-specific shapes (resolveMailPath rides along)
  ['lfn_non_mail', 'Notes/x.md'],
  ['lfn_mail_upper', 'MAIL/x'],
  ['lfn_mail_only', 'Mail/'],
  ['lfn_uri', 'qtap://self/Mail/x.md'],
  ['lfn_nested', 'Mail/Mail/x.md'],
];

for (const [name, input] of CASES) {
  process.stdout.write(
    JSON.stringify({
      name,
      input,
      resolveMailPath: resolveMailPath(input),
      letterFileName: letterFileName(input),
    }) + '\n',
  );
}
