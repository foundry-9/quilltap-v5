#!/usr/bin/env python3
"""Mechanically extract v4's consolidation system prompt into the v5
`consolidation_tasks/prompt_text.rs` generated module. No byte is transcribed
by hand (P4.D271).

Usage: extract-consolidation-prompt.py <v4-consolidation-tasks.ts> <out.rs>

Emits `CONSOLIDATION_SYSTEM_PROMPT` (v4
`lib/memory/cheap-llm-tasks/consolidation-tasks.ts`), a template literal with
NO interpolation. Template escapes are EVALUATED the way JS evaluates the
literal (`\\``, `\\\\`, `\\$` -> their runtime character); any other escape,
and any `${` interpolation, aborts the run rather than guessing.
"""
import sys

src = open(sys.argv[1], encoding='utf-8').read()


def template_after(marker):
    i = src.index(marker) + len(marker)
    assert src[i] == '`', repr(src[i - 20:i + 5])
    i += 1
    out = []
    while True:
        c = src[i]
        if c == '\\':
            out.append(src[i:i + 2]); i += 2; continue
        if c == '`':
            break
        out.append(c); i += 1
    return ''.join(out)


def js_evaluate_escapes(raw, what):
    out = []
    i = 0
    while i < len(raw):
        c = raw[i]
        if c == '\\':
            nxt = raw[i + 1]
            if nxt not in {'`', '\\', '$'}:
                raise SystemExit(f'{what}: unhandled template escape \\{nxt} — teach the script')
            out.append(nxt); i += 2; continue
        out.append(c); i += 1
    return ''.join(out)


raw = template_after('export const CONSOLIDATION_SYSTEM_PROMPT = ')
if '${' in raw:
    raise SystemExit('CONSOLIDATION_SYSTEM_PROMPT interpolates — teach the script')
text = js_evaluate_escapes(raw, 'CONSOLIDATION_SYSTEM_PROMPT')
assert '"####' not in text

header = '''//! GENERATED — the verbatim system prompt of v4
//! `lib/memory/cheap-llm-tasks/consolidation-tasks.ts`
//! (`CONSOLIDATION_SYSTEM_PROMPT`, no interpolation; template escapes
//! evaluated). Extracted mechanically by
//! `harness/oracle/scripts/extract-consolidation-prompt.py` so no byte was
//! transcribed by hand; the tier-1 differential
//! (`consolidation_tasks_equivalence`) proves the bytes. Regenerate by
//! re-running that script against the pinned v4 checkout if the upstream
//! prompt changes:
//!   python3 harness/oracle/scripts/extract-consolidation-prompt.py \\
//!     <v4>/lib/memory/cheap-llm-tasks/consolidation-tasks.ts \\
//!     crates/quilltap-core/src/consolidation_tasks/prompt_text.rs

'''
with open(sys.argv[2], 'w', encoding='utf-8') as f:
    f.write(header)
    f.write('pub(crate) const CONSOLIDATION_SYSTEM_PROMPT: &str = r####"')
    f.write(text)
    f.write('"####;\n')
