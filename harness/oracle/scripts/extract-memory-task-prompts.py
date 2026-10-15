#!/usr/bin/env python3
"""Mechanically extract v4's memory-extraction prompt text into the v5
`memory_extraction/prompts/prompt_text.rs` generated module. No byte is
transcribed by hand.

Usage: extract-memory-task-prompts.py <v4>/lib/memory/extraction/prompts <out.rs>

Since v4 `ac6348c5f` (memory refactor Phase D) the prompts live in four files
under `lib/memory/extraction/prompts/` — `self.ts`, `other.ts`, `shared.ts`,
`episode.ts` — with their bodies moved UNCHANGED from the retired
`cheap-llm-tasks/memory-tasks.ts`. The script reads the four in that fixed
order and concatenates them, so every marker below (each a unique
declaration) still finds its literal (work order P4.D296 Tier 1 #1). A body
that moved files, or a marker that appears twice, aborts the run.

Emits the SELF / OTHER extraction bodies (`selfBodyForCap` /
`otherBodyForCap`, the latter's leading `${grainBlock}` stripped), the constant
`FOLD_GRAIN_OTHER_BLOCK`, and `FOLD_EPISODE_PROMPT`, each body split at its
one live interpolation (the candidate cap / `FOLD_EPISODE_CAP`). The constant blocks
the bodies interpolate (`ORIENTING_CONTEXT_SKIP_BULLET`,
`AGREEMENTS_INSTRUCTION_BLOCK`, `EVENT_INSTRUCTION_BLOCK`,
`TAGS_INSTRUCTION_BLOCK`) are substituted here. Template escapes are
EVALUATED (`\\``, `\\\\`, `\\$` -> their runtime character) the way JS evaluates
the template literal — v4 `ca363178d`'s TAGS `future` gloss carries the first
escaped backticks (`\\`moment\\``), and a literal backslash in the output would
be a byte v4 never sends. Any other escape, and any interpolation left over
after substitution, aborts the run rather than guessing.
"""
import re, sys

import os
PROMPT_FILES = ('self.ts', 'other.ts', 'shared.ts', 'episode.ts')
src = '\n'.join(
    open(os.path.join(sys.argv[1], name), encoding='utf-8').read() for name in PROMPT_FILES
)

def template_after(marker):
    """Return the RAW source of the backtick template literal that starts right
    after `marker` (marker must end at the opening backtick's position - 1)."""
    if src.count(marker) != 1:
        raise SystemExit(f'marker {marker!r} found {src.count(marker)} times — teach the script')
    i = src.index(marker) + len(marker)
    assert src[i] == '`', repr(src[i-20:i+5])
    i += 1
    out = []
    while True:
        c = src[i]
        if c == '\\':
            out.append(src[i:i+2]); i += 2; continue
        if c == '`':
            break
        out.append(c); i += 1
    return ''.join(out)

# The only escapes these templates use (or may plausibly use); each evaluates
# to its second character. Anything else is a shape this script has not been
# taught — stop and teach it, never emit the backslash.
JS_SIMPLE_ESCAPES = {'`', '\\', '$'}

def js_evaluate_escapes(raw, what):
    out = []
    i = 0
    while i < len(raw):
        c = raw[i]
        if c == '\\':
            nxt = raw[i+1]
            if nxt not in JS_SIMPLE_ESCAPES:
                raise SystemExit(f'{what}: unhandled template escape \\{nxt} — teach the script')
            out.append(nxt); i += 2; continue
        out.append(c); i += 1
    return ''.join(out)

def constant(name):
    raw = template_after(f'const {name} = ')
    if '${' in raw:
        raise SystemExit(f'{name} interpolates — teach the script its substitutions')
    return js_evaluate_escapes(raw, name)

# A block absent from the source (AGREEMENTS before v4 `ca363178d`) is simply
# not substituted; a body that still references it trips the leftover check.
BLOCKS = {
    name: constant(name)
    for name in (
        'ORIENTING_CONTEXT_SKIP_BULLET',
        'AGREEMENTS_INSTRUCTION_BLOCK',
        'EVENT_INSTRUCTION_BLOCK',
        'TAGS_INSTRUCTION_BLOCK',
    )
    if f'const {name} = ' in src
}

def body(marker, cap, what):
    raw = template_after(marker)
    # A body's own escapes (none today) would have to be evaluated BEFORE the
    # constants go in — the constants are already runtime text. Today the
    # script ABORTS instead of guessing (the mandate: never emit a backslash
    # by guesswork); teach it the escape when one appears.
    if '\\' in raw:
        raise SystemExit(f'{what} body has its own template escape — handle it')
    for name, value in BLOCKS.items():
        raw = raw.replace('${' + name + '}', value)
    leftover = [m for m in re.findall(r'\$\{[^}]*\}', raw) if m != '${' + cap + '}']
    if leftover:
        raise SystemExit(f'{what} body still has interpolations: {leftover}')
    parts = raw.split('${' + cap + '}')
    if len(parts) != 2:
        raise SystemExit(f'{what} body: expected exactly one ${{{cap}}}, found {len(parts) - 1}')
    return parts

self_before, self_after = body(
    'function selfBodyForCap(maxMemories: number): string {\n  return ', 'maxMemories', 'self')
# v4 `d58548051` gave `otherBodyForCap` a `grain` parameter: the fold grain
# prepends `${FOLD_GRAIN_OTHER_BLOCK}\n\n` through a `grainBlock` local, and
# the turn body is byte-identical to before. The script asserts that exact
# shape, strips the leading `${grainBlock}` (and ONLY it), and emits the block
# as its own constant — v5's `other_body_for_cap(cap, grain)` re-adds it.
OTHER_GRAIN_PREAMBLE = (
    "function otherBodyForCap(perSubjectCap: number, grain: 'turn' | 'fold' = 'turn'): string {\n"
    "  const grainBlock = grain === 'fold' ? `${FOLD_GRAIN_OTHER_BLOCK}\\n\\n` : ''\n"
    "  return "
)
if OTHER_GRAIN_PREAMBLE not in src:
    raise SystemExit('otherBodyForCap: the grain preamble moved — teach the script')
src_other_raw = template_after(OTHER_GRAIN_PREAMBLE)
if not src_other_raw.startswith('${grainBlock}'):
    raise SystemExit('otherBodyForCap: the body no longer opens with ${grainBlock} — teach the script')
src = src.replace(OTHER_GRAIN_PREAMBLE + '`${grainBlock}', OTHER_GRAIN_PREAMBLE + '`', 1)
other_before, other_after = body(OTHER_GRAIN_PREAMBLE, 'perSubjectCap', 'other')
FOLD_GRAIN_OTHER_BLOCK = constant('FOLD_GRAIN_OTHER_BLOCK')
fold_before, fold_after = body('const FOLD_EPISODE_PROMPT = ', 'FOLD_EPISODE_CAP', 'fold-episode')

def raw_rs(s):
    hashes = '#' * 4
    assert f'"{hashes}' not in s
    return f'r{hashes}"{s}"{hashes}'

out = '''//! GENERATED — the verbatim prompt text of v4
//! `lib/memory/extraction/prompts/{self,other,shared,episode}.ts` (moved
//! unchanged from `cheap-llm-tasks/memory-tasks.ts` by `ac6348c5f`; the
//! episode prompt's `participants` line removed by `e0ac7493a`): the SELF / OTHER extraction
//! bodies (`selfBodyForCap` / `otherBodyForCap`, with the constant
//! `ORIENTING_CONTEXT_SKIP_BULLET` / `AGREEMENTS_INSTRUCTION_BLOCK` /
//! `EVENT_INSTRUCTION_BLOCK` / `TAGS_INSTRUCTION_BLOCK` interpolations already
//! substituted and the template escapes evaluated; `otherBodyForCap`'s
//! leading `${grainBlock}` stripped), the fold-grain `FOLD_GRAIN_OTHER_BLOCK`,
//! and `FOLD_EPISODE_PROMPT`, each body split at its one live interpolation
//! (the candidate cap / `FOLD_EPISODE_CAP`). Extracted mechanically by
//! `harness/oracle/scripts/extract-memory-task-prompts.py` so no byte was
//! transcribed by hand; the tier-1 differential (`memory_tasks_equivalence`)
//! proves the bytes. Regenerate by re-running that script against the pinned
//! v4 checkout if the upstream prompts change:
//!   python3 harness/oracle/scripts/extract-memory-task-prompts.py \\
//!     <v4>/lib/memory/extraction/prompts \\
//!     crates/quilltap-core/src/memory_extraction/prompts/prompt_text.rs

'''
for const_name, value in (
    ('SELF_BODY_BEFORE_CAP', self_before),
    ('SELF_BODY_AFTER_CAP', self_after),
    ('OTHER_BODY_BEFORE_CAP', other_before),
    ('OTHER_BODY_AFTER_CAP', other_after),
    ('FOLD_EPISODE_PROMPT_BEFORE_CAP', fold_before),
    ('FOLD_EPISODE_PROMPT_AFTER_CAP', fold_after),
    ('FOLD_GRAIN_OTHER_BLOCK', FOLD_GRAIN_OTHER_BLOCK),
):
    out += f'pub(crate) const {const_name}: &str = {raw_rs(value)};\n'

open(sys.argv[2], 'w', encoding='utf-8').write(out)
print(f'wrote {sys.argv[2]}: self {len(self_before)}+{len(self_after)}, '
      f'other {len(other_before)}+{len(other_after)}, fold {len(fold_before)}+{len(fold_after)}')
