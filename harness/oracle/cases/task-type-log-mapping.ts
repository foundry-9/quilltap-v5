/**
 * Oracle case (P4.D179, v4 `686954937`): `mapTaskTypeToLogType` — the
 * cheap-LLM task type → `llm_logs.type` allowlist.
 *
 * Drives the REAL export:
 *   mapTaskTypeToLogType (lib/memory/cheap-llm-tasks/core-execution.ts)
 *   LLMLogTypeEnum       (lib/schemas/llm-log.types.ts)
 *
 * v4 made the function `export`ed in `686954937` precisely so it could be
 * tested (its own `__tests__/task-type-log-mapping.test.ts` is this corpus's
 * shape). The failure mode it guards is SILENT: the map is a CLOSED allowlist
 * whose default is `SUMMARIZATION`, so an unmapped task type does not throw —
 * it files itself among the chat summaries in the Wire Records and the LLM
 * inspector. Both voice rehearsals did exactly that (`announcement-rewrite`
 * since v4 4.4; the port reproduced it) until `VOICE_REWRITE` was added.
 *
 * The corpus is EVERY task-type string the v5 match names — the Rust side
 * asserts that by censusing its own source, so an arm added later without a
 * corpus row is a red, not a silence — plus v4's four probe strings
 * (`something-nobody-mapped`, an ABSENT task type, `''`, `not-a-real-task`).
 * `admitted` carries v4's own enum verdict so the Rust side can pin that every
 * mapped value is one `LLMLogTypeEnum` accepts.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/task-type-log-mapping.ts \
 *     > /tmp/oracle-task-type-log-mapping.ndjson
 */

import { mapTaskTypeToLogType } from '@/lib/memory/cheap-llm-tasks/core-execution';
import { LLMLogTypeEnum } from '@/lib/schemas/llm-log.types';

/**
 * Every key of v4's mapping, in v4's own declaration order, followed by the
 * probe strings. `null` is the ABSENT argument (`mapTaskTypeToLogType()` — v4's
 * `taskType?: string`), which is a different call from `''` even though the
 * `taskType || ''` collapse makes them answer alike; both are pinned.
 */
const TASK_TYPES: Array<string | null> = [
  'memory-extraction-self',
  'memory-extraction-other',
  'batch-memory-extraction',
  'memory-keyword-extraction',
  'title-chat',
  'title-from-summary',
  'consider-title-update',
  'compress-conversation-history',
  'compress-system-prompt',
  'compress-memories',
  'summarize-chat',
  'update-context-summary',
  'fold-chat-summary',
  'memory-recap-summarization',
  'derive-scene-context',
  'outfit-selection',
  'craft-image-prompt',
  'craft-story-background-prompt',
  'describe-attachment',
  'resolve-character-appearances',
  'sanitize-appearance',
  'scene-state-tracking',
  'answer-confirmation',
  'answer-reaffirmation',
  'custom-tool-consult',
  // The two rows `686954937` adds.
  'announcement-rewrite',
  'impersonation-voice-rewrite',
  // v4's own probe strings — the closed allowlist's default path.
  'something-nobody-mapped',
  null,
  '',
  'not-a-real-task',
];

const admitted = new Set<string>(LLMLogTypeEnum.options);

for (const taskType of TASK_TYPES) {
  const logType = taskType === null ? mapTaskTypeToLogType() : mapTaskTypeToLogType(taskType);
  process.stdout.write(
    JSON.stringify({ taskType, logType, admitted: admitted.has(logType) }) + '\n',
  );
}
