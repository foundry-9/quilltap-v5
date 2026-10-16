/**
 * Composer slash commands — pure tier. See `types.ts` for the portability
 * contract.
 *
 * Transcribed from v4 `lib/slash-commands/index.ts` at `25b8fab6a`.
 *
 * @module chat/slash-commands
 */

export * from './types';
export { SLASH_TRIGGER, findSlashTrigger } from './trigger';
export { rankSlashCommands } from './ranking';
export { COMPOSER_ACTION_SPECS, buildSlashCommands } from './registry';
