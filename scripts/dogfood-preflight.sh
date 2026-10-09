#!/usr/bin/env bash
# Is everything /dogfood needs current? Read-only; builds and copies nothing.
#
# Usage:
#   scripts/dogfood-preflight.sh [--max-age-hours N]   (default 12)
#
# Exit 0 = all three checks PASS (carry on); 1 = something is STALE (the
# operator runs /cleanup, the SPA build and/or the data refresh first).
#
# 1. RELEASE BUILD. Cargo's dep-info file `target/release/quilltap-web.d`
#    lists every source file the binary was compiled from, `include_str!`
#    assets included — the authoritative input list, no guessing which
#    directories matter. STALE if the binary is missing, or if any listed
#    input (or a Cargo.toml / Cargo.lock) is newer than the binary or gone.
#    Mtimes, not contents: a `/unify` fast-forward rewrites the timestamps of
#    every touched file, so this can say STALE where cargo would rebuild only
#    a little — never the reverse.
#
# 2. SPA BUILD. `apps/web/dist/quilltap/browser/index.html` against every
#    build input under `apps/web/` (src/ and public/ minus `*.spec.ts`, plus
#    the package / angular / tsconfig files).
#
# 3. DOGFOOD DATA (~/qt-dogfood-friday, refreshed from live Friday by the
#    rsync recorded in .claude/commands/dogfood.md). `rsync -a` copies the
#    SOURCE's mtime, but a file's ctime is set when rsync writes it and
#    cannot be preserved — so on an untouched copy, ctime = refresh time and
#    mtime = live's mtime at refresh. A v5 server writing the copy moves both
#    to the write time. Per DB (main / mount-index / llm-logs):
#      - copy mtime == live mtime            → clean, identical to live
#      - copy mtime >  live mtime            → DIRTY (written since refresh)
#      - copy mtime <  live mtime, and ctime well after mtime
#                                            → clean, live has moved since
#      - copy mtime <  live mtime, ctime ≈ mtime → DIRTY (an earlier walk
#                                              wrote it, then v4 wrote live)
#    Plus: no `quilltap.lock` / `*.db-journal` left in the copy (a running or
#    crashed server), and the refresh (main DB's ctime) no older than
#    --max-age-hours.
set -uo pipefail

max_age_h=12
while [[ $# -gt 0 ]]; do
  case $1 in
    --max-age-hours) max_age_h=${2:?--max-age-hours needs a number}; shift ;;
    *) echo "usage: $0 [--max-age-hours N]" >&2; exit 2 ;;
  esac
  shift
done
[[ $max_age_h =~ ^[0-9]+$ ]] || { echo "--max-age-hours must be whole hours" >&2; exit 2; }

repo=$(cd "$(dirname "$0")/.." && pwd -P)
copy=$HOME/qt-dogfood-friday
live=$HOME/iCloud/Quilltap/Friday
stale=0

python3 - "$repo" "$copy" "$live" "$max_age_h" <<'PY' || stale=1
import glob, os, re, sys, time

repo, copy, live, max_age_h = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
now = time.time()
bad = False

def fmt(t):
    return time.strftime('%Y-%m-%d %H:%M:%S', time.localtime(t))

def report(name, problems, ok_line):
    global bad
    if problems:
        bad = True
        print(f'{name}: STALE')
        for p in problems[:8]:
            print(f'  - {p}')
        if len(problems) > 8:
            print(f'  - … and {len(problems) - 8} more')
    else:
        print(f'{name}: PASS — {ok_line}')

# 1. release build
binp = os.path.join(repo, 'target/release/quilltap-web')
dep = binp + '.d'
problems = []
if not os.path.exists(binp) or not os.path.exists(dep):
    problems.append('target/release/quilltap-web (or its .d) is missing')
    built = 0
else:
    built = os.stat(binp).st_mtime
    text = open(dep).read().replace('\\\n', ' ')
    rhs = text.split(': ', 1)[1] if ': ' in text else ''
    inputs = [p.replace('\\ ', ' ') for p in re.split(r'(?<!\\) +', rhs.strip()) if p]
    inputs += [os.path.join(repo, 'Cargo.toml'), os.path.join(repo, 'Cargo.lock')]
    inputs += glob.glob(os.path.join(repo, 'crates/*/Cargo.toml'))
    for p in inputs:
        try:
            m = os.stat(p).st_mtime
        except FileNotFoundError:
            problems.append(f'input gone since the build: {os.path.relpath(p, repo)}')
            continue
        if m > built:
            problems.append(f'newer than the binary: {os.path.relpath(p, repo)} ({fmt(m)})')
report('release build', problems,
       f'quilltap-web built {fmt(built)}, no input newer' if built else '')

# 2. SPA build
web = os.path.join(repo, 'apps/web')
index = os.path.join(web, 'dist/quilltap/browser/index.html')
problems = []
if not os.path.exists(index):
    problems.append('apps/web/dist/quilltap/browser/index.html is missing')
    spa = 0
else:
    spa = os.stat(index).st_mtime
    inputs = [os.path.join(web, f) for f in
              ('package.json', 'package-lock.json', 'angular.json')]
    inputs += glob.glob(os.path.join(web, 'tsconfig*.json'))
    for sub in ('src', 'public'):
        for root, _, files in os.walk(os.path.join(web, sub)):
            inputs += [os.path.join(root, f) for f in files
                       if not f.endswith('.spec.ts') and f != '.DS_Store']
    for p in inputs:
        if os.path.exists(p) and os.stat(p).st_mtime > spa:
            problems.append(f'newer than the build: {os.path.relpath(p, repo)} '
                            f'({fmt(os.stat(p).st_mtime)})')
report('SPA build', problems, f'dist built {fmt(spa)}, no input newer' if spa else '')

# 3. dogfood data
problems = []
data, ldata = os.path.join(copy, 'data'), os.path.join(live, 'data')
refreshed = None
for name in ('quilltap.lock',):
    if os.path.exists(os.path.join(data, name)):
        problems.append(f'{name} present in the copy (a server running, or one that '
                        'did not exit cleanly)')
for j in glob.glob(os.path.join(data, '*.db-journal')):
    problems.append(f'{os.path.basename(j)} present in the copy')
for db in ('quilltap.db', 'quilltap-mount-index.db', 'quilltap-llm-logs.db'):
    c, l = os.path.join(data, db), os.path.join(ldata, db)
    if not os.path.exists(c):
        problems.append(f'{db} missing from the copy')
        continue
    cs = os.stat(c)
    if db == 'quilltap.db':
        refreshed = cs.st_ctime
    lm = os.stat(l).st_mtime if os.path.exists(l) else None
    if lm is None:
        problems.append(f'{db}: cannot stat the live file (iCloud?) to compare')
    elif int(cs.st_mtime) == int(lm):
        pass
    elif cs.st_mtime > lm:
        problems.append(f'{db}: the copy was written {fmt(cs.st_mtime)}, after the '
                        f'refresh (live {fmt(lm)}) — dirtied by an earlier walk')
    elif cs.st_ctime - cs.st_mtime < 60:
        problems.append(f'{db}: written in place {fmt(cs.st_mtime)} — dirtied by an '
                        'earlier walk')
if refreshed is not None and now - refreshed > max_age_h * 3600:
    problems.append(f'last refresh {fmt(refreshed)} is older than {max_age_h} h')
report('dogfood data', problems,
       f'refreshed {fmt(refreshed)}, untouched since' if refreshed else '')

sys.exit(1 if bad else 0)
PY

if pgrep -x quilltap-web >/dev/null; then
  echo "note: a quilltap-web process is RUNNING (it holds the copy's lock and the old binary's inode)"
fi
exit $stale
