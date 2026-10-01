#!/usr/bin/env python3
"""Mechanically extract v4's memory-tasks prompt text into the v5
`memory_tasks/prompt_text.rs` generated module. No byte is transcribed by hand.

Usage: extract-memory-task-prompts.py <v4-memory-tasks.ts> <out.rs>

Emits the SELF / OTHER extraction bodies (`selfBodyForCap` /
`otherBodyForCap`) and `FOLD_EPISODE_PROMPT`, each split at its one live
interpolation (the candidate cap / `FOLD_EPISODE_CAP`). The constant blocks
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

src = open(sys.argv[1], encoding='utf-8').read()

def template_after(marker):
    """Return the RAW source of the backtick template literal that starts right
    after `marker` (marker must end at the opening backtick's position - 1)."""
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
other_before, other_after = body(
    'function otherBodyForCap(perSubjectCap: number): string {\n  return ', 'perSubjectCap', 'other')
fold_before, fold_after = body('const FOLD_EPISODE_PROMPT = ', 'FOLD_EPISODE_CAP', 'fold-episode')

def raw_rs(s):
    hashes = '#' * 4
    assert f'"{hashes}' not in s
    return f'r{hashes}"{s}"{hashes}'

out = '''//! GENERATED — the verbatim prompt text of v4
//! `lib/memory/cheap-llm-tasks/memory-tasks.ts`: the SELF / OTHER extraction
//! bodies (`selfBodyForCap` / `otherBodyForCap`, with the constant
//! `ORIENTING_CONTEXT_SKIP_BULLET` / `AGREEMENTS_INSTRUCTION_BLOCK` /
//! `EVENT_INSTRUCTION_BLOCK` / `TAGS_INSTRUCTION_BLOCK` interpolations already
//! substituted and the template escapes evaluated) and `FOLD_EPISODE_PROMPT`,
//! each split at its one live interpolation (the candidate cap /
//! `FOLD_EPISODE_CAP`). Extracted mechanically by
//! `harness/oracle/scripts/extract-memory-task-prompts.py` so no byte was
//! transcribed by hand; the tier-1 differential (`memory_tasks_equivalence`)
//! and the fold-episode families prove the bytes. Regenerate by re-running
//! that script against the pinned v4 checkout if the upstream prompts change:
//!   python3 harness/oracle/scripts/extract-memory-task-prompts.py \\
//!     <v4>/lib/memory/cheap-llm-tasks/memory-tasks.ts \\
//!     crates/quilltap-core/src/memory_tasks/prompt_text.rs

'''
for const_name, value in (
    ('SELF_BODY_BEFORE_CAP', self_before),
    ('SELF_BODY_AFTER_CAP', self_after),
    ('OTHER_BODY_BEFORE_CAP', other_before),
    ('OTHER_BODY_AFTER_CAP', other_after),
    ('FOLD_EPISODE_PROMPT_BEFORE_CAP', fold_before),
    ('FOLD_EPISODE_PROMPT_AFTER_CAP', fold_after),
):
    out += f'pub(crate) const {const_name}: &str = {raw_rs(value)};\n'

open(sys.argv[2], 'w', encoding='utf-8').write(out)
print(f'wrote {sys.argv[2]}: self {len(self_before)}+{len(self_after)}, '
      f'other {len(other_before)}+{len(other_after)}, fold {len(fold_before)}+{len(fold_after)}')
