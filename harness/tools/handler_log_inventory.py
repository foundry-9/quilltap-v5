#!/usr/bin/env python3
"""Regenerate `docs/developer/porting/handler-logging-inventory.md` (P4.74).

Every v4 `logger.*` call in the surveyed areas, mechanically dispositioned
against the v5 tree. The inventory is DERIVED, never hand-maintained: run this
after porting a log line and commit the result.

    python3 harness/tools/handler_log_inventory.py --v4 ~/source/quilltap-server

Dispositions:
  PORTED-PINNED     a v5 tracing site carries the sentence AND a test asserts it
  PORTED-UNPINNED   a v5 tracing site carries it; nothing asserts it
  NO-PORT-RECORDED  no v5 tracing site, but a v5 comment quotes the sentence to
                    record that it was deliberately not ported
  NO-SITE           neither

A match inside a v5 comment never counts as a port — several sites quote a v4
sentence precisely to say they did NOT port it, and an earlier version of this
script was fooled by exactly that.

Run from the repo root.
"""
import argparse, re, subprocess, pathlib, os

ap = argparse.ArgumentParser()
ap.add_argument('--v4', default=os.path.expanduser('~/source/quilltap-server'),
                help='the v4 checkout (or a pinned detached worktree)')
ap.add_argument('--out', default='docs/developer/porting/handler-logging-inventory.md')
args = ap.parse_args()
PIN = pathlib.Path(args.v4)
V5  = pathlib.Path('crates/quilltap-core/src')

# The seed list. `token-tracking.service.ts` joined it at the follow-ups-round-2
# unification: its `updateChatTokenAggregates` catch logs `Failed to update chat
# token aggregates` — the sibling of `create_system_event`'s row, three lines
# below it in v5's `cost_events.rs`, and invisible to the inventory until seeded.
# P4.76 seeded the images COLLECTION route — the FIRST `app/api/**` file in this
# inventory. Its eight lines belong here because `?action=generate` is a whole
# synchronous pipeline living in a route handler rather than a job handler, and
# because the P4.73 unification review found two of them (the upload / import
# receipts) unported with nothing to say so. Seeding the file is also the first
# repayment of the scope gap dogfood finding #110 recorded: this survey used to
# reach `lib/background-jobs/handlers/*.ts` and three named services only.
files = (sorted(PIN.glob('lib/background-jobs/handlers/*.ts'))
         + [PIN/'lib/services/system-events.service.ts']
         + [PIN/'lib/services/token-tracking.service.ts']
         + [PIN/'lib/chat/file-attachment-fallback.ts']
         # P4.9I2A: the help/HelpChat server family — the two route families the
         # order asks the inventory to carry, plus the orchestrator and the two
         # pure modules whose debug lines the port records as NO-PORT (a pure
         # module has no tracing) and `help-search.ts` (the in-process cache the
         # port does not keep, so its load line has no event to log).
         + sorted(PIN.glob('app/api/v1/help-docs/**/route.ts'))
         + sorted(PIN.glob('app/api/v1/help-chats/**/route.ts'))
         + [PIN/'lib/services/help-chat/orchestrator.service.ts']
         + [PIN/'lib/help-chat/context-resolver.ts']
         + [PIN/'lib/help-chat/system-prompt-builder.ts']
         + [PIN/'lib/help-search.ts'])
         + [PIN/'app/api/v1/images/route.ts'])
CALL = re.compile(r"logger\.(info|warn|error|debug)\(\s*(?:'((?:[^'\\]|\\.)*)'|`((?:[^`\\]|\\.)*)`|\"((?:[^\"\\]|\\.)*)\")")

# every v5 source line, for sentence lookup
v5_files = list(V5.rglob('*.rs'))
v5_text = {p: p.read_text(encoding='utf-8', errors='replace') for p in v5_files}

def needle_of(sentence):
    needle = sentence
    # v4 template literals interpolate; use the longest literal run as the needle
    if '${' in needle:
        parts = re.split(r'\$\{[^}]*\}', needle)
        needle = max(parts, key=len)
    return needle.strip()

def find_v5(sentence):
    """Locate a v5 *tracing* line carrying this sentence.

    A match inside a `//` comment does NOT count: several v5 sites quote a v4
    sentence precisely to record that it was deliberately NOT ported."""
    needle = needle_of(sentence)
    if len(needle) < 12:
        return None, None
    for p, t in v5_text.items():
        lines = t.split('\n')
        for i, ln in enumerate(lines):
            if needle not in ln:
                continue
            if ln.lstrip().startswith(('//', '///', '//!', '*')):
                continue
            # a tracing macro opens within the preceding few lines
            window = '\n'.join(lines[max(0, i - 12):i + 1])
            if 'tracing::' not in window:
                continue
            rel = p.relative_to(pathlib.Path('crates/quilltap-core/src'))
            return f"{rel}:{i + 1}", None
    # not a tracing line — is it quoted in a comment as a recorded NO-PORT?
    for p, t in v5_text.items():
        for i, ln in enumerate(t.split('\n')):
            if needle in ln and ln.lstrip().startswith(('//', '///', '//!', '*')):
                rel = p.relative_to(pathlib.Path('crates/quilltap-core/src'))
                return None, f"{rel}:{i + 1}"
    return None, None

def pinned(sentence):
    """Is the sentence asserted by a test (a capture-layer pin or a harness family)?"""
    needle = needle_of(sentence)
    if len(needle) < 12: return False
    out = subprocess.run(['ggrep','-rlF','--include=*.rs','-e',needle,
                          'crates/quilltap-harness/tests'],
                         capture_output=True, text=True).stdout.strip()
    if out: return True
    # in-file capture pins live in `log_context_tests` modules
    for p, t in v5_text.items():
        if needle in t and 'log_context_tests' in t:
            # crude but adequate: the sentence appears AND the file has a pin module
            idx = [m.start() for m in re.finditer(re.escape(needle), t)]
            mod = t.find('mod log_context_tests')
            if any(i > mod for i in idx) and mod >= 0:
                return True
    return False

rows = []
for f in files:
    text = f.read_text()
    for m in CALL.finditer(text):
        line = text[:m.start()].count('\n') + 1
        level = m.group(1)
        sentence = m.group(2) or m.group(3) or m.group(4) or ''
        site, noted = find_v5(sentence)
        if site is None:
            disp = 'NO-PORT-RECORDED' if noted else 'NO-SITE'
        elif pinned(sentence):
            disp = 'PORTED-PINNED'
        else:
            disp = 'PORTED-UNPINNED'
        rows.append(dict(file=str(f.relative_to(PIN)), line=line, level=level,
                         sentence=sentence, v5=site or noted or '', disp=disp))
from collections import Counter, defaultdict
counts = Counter(r['disp'] for r in rows)

AREA = {
    'lib/background-jobs/handlers': 'The background-job handlers',
    'lib/services/help-chat': 'The help-chat orchestrator (P4.9I2A)',
    'lib/services': 'The system-events service',
    'lib/chat': 'The file-attachment fallback (the describe path)',
    'app/api/v1/help-docs': 'The help-docs routes (P4.9I2A)',
    'app/api/v1/help-chats': 'The help-chats routes (P4.9I2A)',
    'lib/help-chat': 'The help-chat pure modules (P4.9I2A)',
    'lib/help-search.ts': 'The HelpSearch cache (P4.9I2A — no v5 twin)',
}
def area_of(f):
    for k, v in AREA.items():
        if f.startswith(k):
            return v
    return 'Other'

by_file = defaultdict(list)
for r in rows:
    by_file[r['file']].append(r)

def esc(s):
    return s.replace('|', '\\|').replace('\n', ' ')

out = []
w = out.append
w('<!-- GENERATED by harness/tools/handler_log_inventory.py — do not hand-edit. -->')
w('')
w('# The handler-logging inventory')
w('')
w('Every v4 `logger.*` call in the surveyed areas, with its v5 counterpart and')
w('disposition. **Generated** — regenerate with:')
w('')
w('    python3 harness/tools/handler_log_inventory.py --v4 ~/source/quilltap-server')
w('')
w('| disposition | meaning | count |')
w('|---|---|---|')
w(f"| `PORTED-PINNED` | a v5 tracing site carries the sentence AND a test asserts it | {counts.get('PORTED-PINNED', 0)} |")
w(f"| `PORTED-UNPINNED` | a v5 tracing site carries it; nothing asserts it | {counts.get('PORTED-UNPINNED', 0)} |")
w(f"| `NO-PORT-RECORDED` | no v5 site, but a v5 comment records the decision | {counts.get('NO-PORT-RECORDED', 0)} |")
w(f"| `NO-SITE` | neither | {counts.get('NO-SITE', 0)} |")
w(f"| **total** | | **{len(rows)}** |")
w('')
w('A match inside a v5 comment never counts as a port: several sites quote a v4')
w('sentence precisely to record that it was deliberately not ported.')
w('')
last_area = None
for f in sorted(by_file):
    a = area_of(f)
    if a != last_area:
        w(f'## {a}')
        w('')
        last_area = a
    rs = by_file[f]
    c = Counter(r['disp'] for r in rs)
    summary = ', '.join(f'{k} {v}' for k, v in sorted(c.items()))
    w(f'### `{f}` — {len(rs)} line(s) ({summary})')
    w('')
    w('| v4 line | level | sentence | v5 site | disposition |')
    w('|---|---|---|---|---|')
    for r in sorted(rs, key=lambda r: r['line']):
        site = f"`{r['v5']}`" if r['v5'] else '—'
        w(f"| {r['line']} | {r['level']} | `{esc(r['sentence'])}` | {site} | {r['disp']} |")
    w('')
pathlib.Path(args.out).write_text('\n'.join(out) + '\n')
print(f"wrote {args.out}: {len(rows)} rows — {dict(counts)}")
