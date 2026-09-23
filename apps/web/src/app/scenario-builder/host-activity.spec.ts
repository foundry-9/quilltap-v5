import { describeHostActivity } from './host-activity';
import { SCENARIO_BUILDER_ORACLE } from './oracle-corpus.testing';

/**
 * v4's `describeHostActivity` — its own `ScenarioBuilderDialog.test.tsx` cases
 * by name, then every recorded line from v4's REAL function (exact bytes: the
 * curly quotes are the Host's own).
 */
describe('describeHostActivity — v4 ScenarioBuilderDialog.test.tsx', () => {
  it('describes a web search by its query', () => {
    expect(describeHostActivity({ name: 'search_web', arguments: { query: 'Gare du Nord' } })).toBe(
      'Consulting the wider world about “Gare du Nord”',
    );
  });

  it('describes curl by its url, falling back when absent', () => {
    expect(describeHostActivity({ name: 'curl', arguments: { url: 'https://example.com' } })).toBe(
      'Reading https://example.com',
    );
    expect(describeHostActivity({ name: 'curl', arguments: {} })).toBe(
      'Reading a page from the wider world',
    );
  });

  it('describes the document-store search tool', () => {
    expect(describeHostActivity({ name: 'search', arguments: { query: 'inn' } })).toBe(
      'Leafing through the stores for “inn”',
    );
  });

  it('describes doc_read_file by path or uri', () => {
    expect(
      describeHostActivity({ name: 'doc_read_file', arguments: { path: 'Knowledge/lore.md' } }),
    ).toBe('Opening Knowledge/lore.md');
    expect(describeHostActivity({ name: 'doc_read_file', arguments: {} })).toBe(
      'Opening a document',
    );
  });

  it('falls back to a generic line for an unrecognised tool', () => {
    expect(describeHostActivity({ name: 'mystery_tool', arguments: {} })).toBe(
      'Busying himself with mystery_tool',
    );
  });
});

describe('describeHostActivity — v4 oracle corpus (d1c06cd9d)', () => {
  const { activityCases } = SCENARIO_BUILDER_ORACLE;

  it('carries the whole corpus (the empty-file guard)', () => {
    expect(activityCases).toHaveLength(40);
  });

  activityCases.forEach((c, i) => {
    it(`#${i} ${c.name || '(empty name)'} ${JSON.stringify(c.arguments ?? null)}`, () => {
      expect(describeHostActivity({ name: c.name, arguments: c.arguments })).toBe(c.line);
    });
  });
});
