#!/usr/bin/env python3
"""Mechanically extract v4's memory-tasks prompt bodies into the v5
`memory_tasks/prompt_text.rs` generated module. No byte is transcribed by hand.

Usage: extract_prompts.py <v4-memory-tasks.ts> <out.rs>
"""
import re, sys

src = open(sys.argv[1], encoding='utf-8').read()

def template_after(marker):
    """Return the contents of the backtick template literal that starts right
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

SKIP = template_after('const ORIENTING_CONTEXT_SKIP_BULLET = ')
EVENT = template_after('const EVENT_INSTRUCTION_BLOCK = ')
TAGS = template_after('const TAGS_INSTRUCTION_BLOCK = ')

self_body = template_after('function selfBodyForCap(maxMemories: number): string {\n  return ')
other_body = template_after('function otherBodyForCap(perSubjectCap: number): string {\n  return ')

def substitute(body):
    body = body.replace('${ORIENTING_CONTEXT_SKIP_BULLET}', SKIP)
    body = body.replace('${EVENT_INSTRUCTION_BLOCK}', EVENT)
    body = body.replace('${TAGS_INSTRUCTION_BLOCK}', TAGS)
    return body

self_body = substitute(self_body)
other_body = substitute(other_body)

for name, body in (('self', self_body), ('other', other_body)):
    if '\\' in body:
        raise SystemExit(f'{name} body contains a backslash escape — handle it')
    leftover = [m for m in re.findall(r'\$\{[^}]*\}', body)
                if m not in ('${maxMemories}', '${perSubjectCap}')]
    if leftover:
        raise SystemExit(f'{name} body still has interpolations: {leftover}')

self_before, self_after = self_body.split('${maxMemories}')
other_before, other_after = other_body.split('${perSubjectCap}')

def raw(s):
    hashes = '#' * 4
    assert f'"{hashes}' not in s
    return f'r{hashes}"{s}"{hashes}'

out = '''//! GENERATED — the verbatim prompt-body text of v4
//! `lib/memory/cheap-llm-tasks/memory-tasks.ts` (`selfBodyForCap` /
//! `otherBodyForCap`, with the constant `ORIENTING_CONTEXT_SKIP_BULLET` /
//! `EVENT_INSTRUCTION_BLOCK` / `TAGS_INSTRUCTION_BLOCK` interpolations already
//! substituted, split at the one live interpolation — the candidate cap).
//! Extracted mechanically by the session script `extract_prompts.py` so no byte
//! was transcribed by hand; the tier-1 differential (`memory_tasks_equivalence`)
//! proves the bytes. Regenerate by re-running the extraction against the v4
//! checkout if the upstream prompts change.

'''
for const_name, value in (
    ('SELF_BODY_BEFORE_CAP', self_before),
    ('SELF_BODY_AFTER_CAP', self_after),
    ('OTHER_BODY_BEFORE_CAP', other_before),
    ('OTHER_BODY_AFTER_CAP', other_after),
):
    out += f'pub(crate) const {const_name}: &str = {raw(value)};\n'

open(sys.argv[2], 'w', encoding='utf-8').write(out)
print(f'wrote {sys.argv[2]}: self {len(self_before)}+{len(self_after)}, other {len(other_before)}+{len(other_after)}')
