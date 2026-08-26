/**
 * Differential oracle — the realtime topic computation (P4.D124, v4
 * `f3892158d`). Tier 1, PURE.
 *
 * Drives v4's REAL `lib/realtime/job-topics.ts` (imported, never reimplemented)
 * over a fixed corpus and emits, per case, BOTH the input and v4's hints. The
 * Rust diff reads the INPUT back out of the NDJSON and runs its own
 * `topics_for_completed_job` / `topics_for_write_batch` on it, so the corpus
 * exists exactly once and there is no chance of the two sides drifting apart
 * (`blinded-comparand-hides-the-new-arm.md`).
 *
 * ⚠ The work order expected the write-batch leg to need a PAIRED corpus, on the
 * premise that v5's buffered writes are typed and would have to be constructed
 * separately. That premise is FALSE: v5's `write_partition::ChildWritePayload`
 * is v4's `{method, args}` verbatim — the Phase-2 partition port kept that
 * representation because it is the correctness property, not a Node workaround.
 * So one corpus serves both sides directly.
 *
 * Run (Node 24, from the v4 checkout — or a pinned worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/npx tsx <this> > /tmp/oracle-realtime-topics.ndjson
 */

import {
  topicsForCompletedJob,
  topicsForWriteBatch,
} from '@/lib/realtime/job-topics';
import { BackgroundJobTypeEnum } from '@/lib/schemas/job.types';

type Payload = Record<string, unknown> | undefined;

interface CompletedCase {
  name: string;
  jobType?: string;
  payload?: Payload;
}

interface BatchCase {
  name: string;
  writes: { method: string; args?: unknown[] }[];
}

const CHAT = 'chat-1111';
const PROJECT = 'proj-2222';
const CHARACTER = 'char-3333';
const MOUNT = 'mount-4444';

/** Every enum member with a full payload, so no arm goes unexercised. */
const FULL_PAYLOAD: Payload = {
  chatId: CHAT,
  projectId: PROJECT,
  characterId: CHARACTER,
  mountPointId: MOUNT,
};

function completedCases(): CompletedCase[] {
  const cases: CompletedCase[] = [];

  // 1. Every job type the enum knows, with everything readable.
  for (const type of BackgroundJobTypeEnum.options) {
    cases.push({ name: `completed_full_${type}`, jobType: type, payload: { ...FULL_PAYLOAD } });
  }
  // 2. …and every job type with NO payload at all (the `str()` undefined arm).
  for (const type of BackgroundJobTypeEnum.options) {
    cases.push({ name: `completed_nopayload_${type}`, jobType: type });
  }

  // 3. The story-background probe order: chat BEATS project when both are
  //    present; project answers when chat is absent, empty, or not a string;
  //    nothing at all when neither reads.
  cases.push(
    {
      name: 'story_background_chat_beats_project',
      jobType: 'STORY_BACKGROUND_GENERATION',
      payload: { chatId: CHAT, projectId: PROJECT },
    },
    {
      name: 'story_background_project_only',
      jobType: 'STORY_BACKGROUND_GENERATION',
      payload: { projectId: PROJECT },
    },
    {
      name: 'story_background_empty_chat_falls_through_to_project',
      jobType: 'STORY_BACKGROUND_GENERATION',
      payload: { chatId: '', projectId: PROJECT },
    },
    {
      name: 'story_background_nonstring_chat_falls_through_to_project',
      jobType: 'STORY_BACKGROUND_GENERATION',
      payload: { chatId: 42, projectId: PROJECT },
    },
    {
      name: 'story_background_null_chat_falls_through_to_project',
      jobType: 'STORY_BACKGROUND_GENERATION',
      payload: { chatId: null, projectId: PROJECT },
    },
    {
      name: 'story_background_neither_readable',
      jobType: 'STORY_BACKGROUND_GENERATION',
      payload: { chatId: '', projectId: '' },
    },
    { name: 'story_background_empty_payload', jobType: 'STORY_BACKGROUND_GENERATION', payload: {} },
  );

  // 4. An unreadable id still emits its hint — collection-wide.
  cases.push(
    {
      name: 'avatar_empty_chat_and_character',
      jobType: 'CHARACTER_AVATAR_GENERATION',
      payload: { chatId: '', characterId: '' },
    },
    {
      name: 'avatar_character_only',
      jobType: 'CHARACTER_AVATAR_GENERATION',
      payload: { characterId: CHARACTER },
    },
    {
      name: 'title_update_nonstring_chat',
      jobType: 'TITLE_UPDATE',
      payload: { chatId: { nested: true } },
    },
    {
      name: 'conversation_render_no_chat',
      jobType: 'CONVERSATION_RENDER',
      payload: { mountPointId: MOUNT },
    },
    {
      name: 'autonomous_turn_no_chat',
      jobType: 'AUTONOMOUS_ROOM_TURN',
      payload: {},
    },
  );

  // 5. The dispatcher-never-saw-the-row arm, and a type from a newer build.
  cases.push(
    { name: 'completed_undefined_job_type', payload: { ...FULL_PAYLOAD } },
    { name: 'completed_unknown_job_type', jobType: 'SOME_FUTURE_JOB', payload: { ...FULL_PAYLOAD } },
    { name: 'completed_undefined_type_and_payload' },
  );

  return cases;
}

function batchCases(): BatchCase[] {
  return [
    // Every mapped namespace, first-arg-string id.
    {
      name: 'batch_every_namespace_string_id',
      writes: [
        { method: 'characters.update', args: [CHARACTER, { name: 'x' }] },
        { method: 'chats.update', args: [CHAT, {}] },
        { method: 'projects.update', args: [PROJECT, {}] },
        { method: 'docMountPoints.update', args: [MOUNT, {}] },
        { method: 'docMountFiles.create', args: ['file-1'] },
        { method: 'docMountFileLinks.create', args: ['link-1'] },
        { method: 'docMountFolders.create', args: ['folder-1'] },
        { method: 'docMountDocuments.create', args: ['doc-1'] },
      ],
    },
    // Object-arg ids through each TOPIC_ID_FIELDS row, preferred field first.
    {
      name: 'batch_object_ids_preferred_field',
      writes: [
        { method: 'characters.create', args: [{ characterId: CHARACTER, id: 'other' }] },
        { method: 'chats.create', args: [{ chatId: CHAT, id: 'other' }] },
        { method: 'projects.create', args: [{ projectId: PROJECT, id: 'other' }] },
        { method: 'docMountPoints.create', args: [{ mountPointId: MOUNT, id: 'other' }] },
      ],
    },
    // …and the `id` fallback when the preferred field is absent.
    {
      name: 'batch_object_ids_id_fallback',
      writes: [
        { method: 'characters.create', args: [{ id: CHARACTER }] },
        { method: 'chats.create', args: [{ id: CHAT }] },
        { method: 'projects.create', args: [{ id: PROJECT }] },
        { method: 'docMountFiles.create', args: [{ id: 'file-9' }] },
      ],
    },
    // Unreadable ids → collection-wide, never dropped.
    {
      name: 'batch_unreadable_ids_go_collection_wide',
      writes: [
        { method: 'chats.update', args: [] },
        { method: 'characters.update' },
        { method: 'projects.update', args: [null] },
        { method: 'docMountFiles.update', args: [{ nothingUseful: true }] },
      ],
    },
    // An EMPTY-string first arg is not an id.
    {
      name: 'batch_empty_string_first_arg',
      writes: [{ method: 'chats.update', args: ['', { patch: 1 }] }],
    },
    // …nor is an empty string inside an object.
    {
      name: 'batch_empty_string_object_field',
      writes: [{ method: 'chats.update', args: [{ chatId: '', id: CHAT }] }],
    },
    // A non-string first arg that is neither object nor string.
    {
      name: 'batch_number_first_arg',
      writes: [{ method: 'chats.update', args: [7] }],
    },
    // Unmapped namespaces are skipped entirely.
    {
      name: 'batch_unmapped_namespaces_skipped',
      writes: [
        { method: 'memories.create', args: ['m-1'] },
        { method: 'llmLogs.create', args: ['l-1'] },
        { method: 'docMountChunks.create', args: ['ch-1'] },
        { method: 'docMountBlobs.create', args: ['b-1'] },
        { method: 'projectDocMountLinks.create', args: ['pl-1'] },
        { method: '__finalizeFile', args: ['/tmp/a', '/tmp/b'] },
        { method: 'backgroundJobs.markCompleted', args: ['j-1'] },
      ],
    },
    // Dedup by topic:id, order-preserving; the collection-wide key is its own.
    {
      name: 'batch_dedup_and_order',
      writes: [
        { method: 'chats.update', args: [CHAT] },
        { method: 'characters.update', args: [CHARACTER] },
        { method: 'chats.update', args: [CHAT, { again: true }] },
        { method: 'chats.update', args: ['other-chat'] },
        { method: 'chats.update', args: [] },
        { method: 'chats.update', args: [] },
        { method: 'characters.update', args: [CHARACTER] },
      ],
    },
    // Two mount-index namespaces sharing ONE topic still dedupe by id.
    {
      name: 'batch_shared_topic_dedupes_across_namespaces',
      writes: [
        { method: 'docMountFiles.update', args: [MOUNT] },
        { method: 'docMountFolders.update', args: [MOUNT] },
        { method: 'docMountPoints.update', args: [MOUNT] },
      ],
    },
    // A method with no dot at all still reads as a namespace.
    {
      name: 'batch_dotless_method',
      writes: [{ method: 'chats', args: [CHAT] }],
    },
    { name: 'batch_empty', writes: [] },
  ];
}

function emit(row: Record<string, unknown>): void {
  process.stdout.write(JSON.stringify(row) + '\n');
}

for (const c of completedCases()) {
  emit({
    name: c.name,
    kind: 'completed',
    input: {
      ...(c.jobType !== undefined ? { jobType: c.jobType } : {}),
      ...(c.payload !== undefined ? { payload: c.payload } : {}),
    },
    output: topicsForCompletedJob(c.jobType, c.payload),
  });
}

for (const c of batchCases()) {
  emit({
    name: c.name,
    kind: 'batch',
    input: { writes: c.writes },
    output: topicsForWriteBatch(c.writes as { method: string; args?: unknown[] }[]),
  });
}
