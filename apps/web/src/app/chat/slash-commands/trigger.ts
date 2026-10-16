/**
 * Composer slash commands — trigger detection (pure logic).
 *
 * `/` opens the command menu only where `findTrigger`'s opener rule allows —
 * the start of a run, after whitespace, or after an opening bracket or quote —
 * so `and/or`, `10/15`, `http://` and every path stay prose. The query must
 * start with a letter (`/1`, `/-` are never commands), and a space ends it:
 * commands take no arguments.
 *
 * Transcribed from v4 `lib/slash-commands/trigger.ts` at `25b8fab6a`.
 *
 * @module chat/slash-commands/trigger
 */

import { findTrigger } from '../../editor/char-insert/trigger';
import type { TriggerConfig, TriggerMatch } from '../../editor/char-insert/types';

export const SLASH_TRIGGER: TriggerConfig = {
  opener: '/',
  queryPattern: /[A-Za-z0-9_-]/,
  // `/1`, `/-` are never commands; keeps `1/2` and `--/--` quiet.
  queryStartPattern: /[A-Za-z]/,
  // A bare `/` opens the full menu.
  minQueryLength: 0,
  // Longer than any tool id; pasted text must not hold a menu open.
  maxQueryLength: 64,
  closer: null,
  // Command words are lowercase by construction.
  lowercaseQuery: true,
};

/** Find the active `/query` in the text before the cursor, or null. */
export function findSlashTrigger(textBefore: string): TriggerMatch | null {
  return findTrigger(textBefore, SLASH_TRIGGER);
}
