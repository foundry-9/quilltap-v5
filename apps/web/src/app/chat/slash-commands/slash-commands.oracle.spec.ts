import { describe, expect, it } from 'vitest';

import corpusText from '../../../testing/fixtures/slash-commands.ndjson';
import type { TriggerMatch } from '../../editor/char-insert/types';
import {
  COMPOSER_ACTION_SPECS,
  SLASH_TRIGGER,
  buildSlashCommands,
  findSlashTrigger,
  rankSlashCommands,
  type ComposerAction,
  type SlashBuiltInToolSource,
  type SlashCommand,
  type SlashCustomToolSource,
} from './index';

/**
 * The corpus differential for v4 `lib/slash-commands/` (`4877fc2da`, the
 * composer's `/` slash commands). Recorded by
 * `apps/web/oracle/slash-commands.recorder.ts` against v4's REAL module at the
 * round pin `25b8fab6a` (see that file's header for the pinned-worktree
 * invocation): v4's own vectors plus a wider corpus, every exported function,
 * compared exactly — trigger matches whole, rankings as the ordered key list,
 * built command lists whole WITH key order (through `JSON.stringify`, since
 * `toEqual` is blind to it).
 *
 * This spec is also the portability census v4 keeps with ESLint: `apps/web`
 * has no ESLint, so a transcription that drifted onto a different helper would
 * have to drift onto one that answers every row the same.
 *
 * `localeCompare` / `toLocaleLowerCase` are the runtime's ICU on both sides (the
 * recorder runs under Node 24.13.1; this spec under Node's vitest), exactly as
 * v4's client uses the browser's.
 */

interface Base {
  id: string;
}
interface ConstantsRow extends Base {
  kind: 'constants';
  trigger: Record<string, unknown>;
  specs: unknown;
}
interface TriggerRow extends Base {
  kind: 'trigger';
  text: string;
  out: TriggerMatch | null;
}
interface RankRow extends Base {
  kind: 'rank';
  commands: SlashCommand[];
  query: string;
  limit: number | null;
  out: string[];
}
interface BuildRow extends Base {
  kind: 'build';
  sources: {
    actions: ComposerAction[];
    builtInTools: SlashBuiltInToolSource[];
    customTools: SlashCustomToolSource[];
  };
  out: SlashCommand[];
}
type Row = ConstantsRow | TriggerRow | RankRow | BuildRow;

const ROWS: Row[] = (corpusText as unknown as string)
  .split('\n')
  .filter((line) => line.trim() !== '')
  .map((line) => JSON.parse(line) as Row);

function ofKind<K extends Row['kind']>(kind: K): Array<Extract<Row, { kind: K }>> {
  return ROWS.filter((row): row is Extract<Row, { kind: K }> => row.kind === kind);
}

describe("slash-commands agrees with v4's lib/slash-commands row for row", () => {
  it('carries the whole recorded corpus', () => {
    // A truncated fixture would make every it.each below vacuously green.
    expect(ROWS).toHaveLength(186);
    expect(ofKind('constants')).toHaveLength(1);
    expect(ofKind('trigger')).toHaveLength(54);
    expect(ofKind('rank')).toHaveLength(107);
    expect(ofKind('build')).toHaveLength(24);
  });

  it('the corpus discriminates: open and shut triggers, empty and capped rankings', () => {
    const triggers = ofKind('trigger');
    expect(triggers.filter((r) => r.out === null).length).toBeGreaterThanOrEqual(15);
    expect(triggers.filter((r) => r.out !== null).length).toBeGreaterThanOrEqual(15);
    const ranks = ofKind('rank');
    expect(ranks.some((r) => r.out.length === 0)).toBe(true);
    expect(ranks.some((r) => r.out.length > 12)).toBe(true);
    expect(ranks.some((r) => r.limit === 0)).toBe(true);
  });

  it("the trigger config and the action table are v4's", () => {
    const [row] = ofKind('constants');
    expect({
      opener: SLASH_TRIGGER.opener,
      queryPattern: { source: SLASH_TRIGGER.queryPattern.source, flags: SLASH_TRIGGER.queryPattern.flags },
      queryStartPattern: SLASH_TRIGGER.queryStartPattern
        ? { source: SLASH_TRIGGER.queryStartPattern.source, flags: SLASH_TRIGGER.queryStartPattern.flags }
        : null,
      minQueryLength: SLASH_TRIGGER.minQueryLength,
      maxQueryLength: SLASH_TRIGGER.maxQueryLength,
      closer: SLASH_TRIGGER.closer,
      lowercaseQuery: SLASH_TRIGGER.lowercaseQuery,
    }).toEqual(row.trigger);
    // Key order is the action order `buildSlashCommands` lists in.
    expect(JSON.stringify(COMPOSER_ACTION_SPECS)).toBe(JSON.stringify(row.specs));
  });

  it.each(ofKind('trigger'))('$id', (row) => {
    expect(findSlashTrigger(row.text)).toEqual(row.out);
  });

  it.each(ofKind('rank'))('$id', (row) => {
    const ranked =
      row.limit === null
        ? rankSlashCommands(row.commands, row.query)
        : rankSlashCommands(row.commands, row.query, row.limit);
    expect(ranked.map((c) => c.key)).toEqual(row.out);
  });

  it.each(ofKind('build'))('$id', (row) => {
    const out = buildSlashCommands({
      actions: new Set(row.sources.actions),
      builtInTools: row.sources.builtInTools,
      customTools: row.sources.customTools,
    });
    expect(JSON.stringify(out)).toBe(JSON.stringify(row.out));
  });
});
