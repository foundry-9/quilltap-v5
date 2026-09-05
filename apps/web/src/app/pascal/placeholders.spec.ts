/**
 * The placeholder classifier's parity spec — the client twin of
 * `quilltap-core::pascal::placeholders`, both of them ports of v4's
 * `lib/pascal/placeholders.ts` (NEW at `0506517d3`, correction (e)).
 *
 * v4 ships no test file for the module: it was extracted as a refactor and is
 * covered indirectly by the seven readers' own suites. So this spec is grown
 * from the module ITSELF — every branch of `classifyPlaceholder`, and the
 * `lastIndex` hazard `scanPlaceholders` exists to remove — and it is the only
 * thing standing between the two ports on the client side, since the three
 * Workbench draft audits have no Rust counterpart to diff against.
 *
 * The rows below are deliberately the same questions
 * `pascal::placeholders::tests` asks in Rust. Read them together.
 */

import { describe, expect, it } from 'vitest';

import { classifyPlaceholder, scanPlaceholders, PLACEHOLDER_PATTERN } from './placeholders';

describe('classifyPlaceholder', () => {
  it('names the four bare families exactly, not by prefix', () => {
    expect(classifyPlaceholder('value')).toEqual({ kind: 'value' });
    expect(classifyPlaceholder('roll')).toEqual({ kind: 'roll' });
    expect(classifyPlaceholder('dice')).toEqual({ kind: 'dice' });
    expect(classifyPlaceholder('llm')).toEqual({ kind: 'llm' });
    // A near miss is unknown, not a truncated match.
    expect(classifyPlaceholder('values')).toEqual({ kind: 'unknown', key: 'values' });
    expect(classifyPlaceholder('llm.output')).toEqual({ kind: 'unknown', key: 'llm.output' });
    expect(classifyPlaceholder('params')).toEqual({ kind: 'unknown', key: 'params' });
  });

  it('splits the three prefixed families at their prefix', () => {
    expect(classifyPlaceholder('params.bonus')).toEqual({ kind: 'params', name: 'bonus' });
    // A metadata key is taken WHOLE — dots inside it are the user's vocabulary.
    expect(classifyPlaceholder('metadata.dots.are.fine')).toEqual({
      kind: 'metadata',
      key: 'dots.are.fine',
    });
    // A state path keeps its dot/bracket syntax for the path parser.
    expect(classifyPlaceholder('state.party[0].hp')).toEqual({
      kind: 'state',
      path: 'party[0].hp',
    });
  });

  it('reads a bare family prefix as unknown, not as an empty name', () => {
    // THE correction. Pre-`0506517d3` the three draft audits reported
    // `{{params.}}` as "names no declared parameter" (the empty string is not a
    // declared parameter) and passed `{{metadata.}}` in silence.
    expect(classifyPlaceholder('params.')).toEqual({ kind: 'unknown', key: 'params.' });
    expect(classifyPlaceholder('metadata.')).toEqual({ kind: 'unknown', key: 'metadata.' });
    expect(classifyPlaceholder('state.')).toEqual({ kind: 'unknown', key: 'state.' });
  });

  it('does not reach the prototype for a prototype-named key', () => {
    // The other half of correction (e): v4's pre-fix renderer tested `name in
    // vars.params`, which finds `Object.prototype.toString`, and then rendered
    // `String(v)` — the function source, spliced into a character's message.
    // Classification itself never indexes an object, which is what makes the
    // rule safe wherever it is read; the values these classify to are the
    // reader's business, and the server corpus pins the render.
    expect(classifyPlaceholder('params.toString')).toEqual({
      kind: 'params',
      name: 'toString',
    });
    expect(classifyPlaceholder('params.constructor')).toEqual({
      kind: 'params',
      name: 'constructor',
    });
    expect(classifyPlaceholder('params.__proto__')).toEqual({
      kind: 'params',
      name: '__proto__',
    });
    expect(classifyPlaceholder('metadata.hasOwnProperty')).toEqual({
      kind: 'metadata',
      key: 'hasOwnProperty',
    });
  });
});

describe('scanPlaceholders', () => {
  it('finds every occurrence in order, trimmed and classified', () => {
    expect(scanPlaceholders('a {{ value }} b {{params.x}} c')).toEqual([
      { whole: '{{ value }}', key: 'value', ref: { kind: 'value' } },
      { whole: '{{params.x}}', key: 'params.x', ref: { kind: 'params', name: 'x' } },
    ]);
  });

  it('never matches empty braces, and stops at the first closing brace', () => {
    expect(scanPlaceholders('empty {{}} here')).toEqual([]);
    // `[^}]+` cannot cross a `}`, so brace soup yields the inner run only.
    expect(scanPlaceholders('{{{value}}}').map((p) => p.key)).toEqual(['{value']);
    expect(scanPlaceholders('a {{value b')).toEqual([]);
  });

  it('never shares `lastIndex` between calls', () => {
    // The hazard the module exists to remove: the three draft audits used to
    // share one `g`-flagged regex and reset `lastIndex` by hand at each
    // entrance. A missed reset makes the SECOND string start mid-way.
    expect(scanPlaceholders('{{value}}{{roll}}')).toHaveLength(2);
    expect(scanPlaceholders('{{dice}}').map((p) => p.ref.kind)).toEqual(['dice']);
    // The exported pattern is stateful by declaration; scanning must not leave
    // it advanced, because `OutcomesSection`-style readers `matchAll` it.
    scanPlaceholders('{{value}} {{roll}} {{dice}}');
    expect(PLACEHOLDER_PATTERN.lastIndex).toBe(0);
  });
});
