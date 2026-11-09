/**
 * Tier-1 oracle case — v4's `sendMessageSchema` (P4.D372, v4 `db6190b44`,
 * pin `624d0d25c`, `lib/services/chat-message/orchestrator.service.ts:118-158`).
 *
 * `safeParse` of the REAL schema over a grid of `{content, targetParticipantIds,
 * isEstablishedFact, fileIds, pendingToolResults}` → `{success, issues:[{path,
 * message}]}`. Zod collects EVERY issue in the `superRefine`'s order (whispered
 * → needs words → content/files/tools); v5's `ChatSend` arm answers the FIRST
 * issue's message (`SEND_REFINE_ISSUE_LIST`, both ways in the Rust family).
 *
 * Run from the pinned v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   $N/npx tsx $V5W/harness/oracle/cases/send-message-schema.ts \
 *     > /tmp/oracle-send-message-schema.ndjson
 */

async function main(): Promise<void> {
  const { sendMessageSchema } = await import('@/lib/services/chat-message/orchestrator.service');
  const ABSENT = Symbol('absent');
  const contents: unknown[] = [ABSENT, '', '   ', '﻿', '　 ', 'The bridge fell.'];
  const targets: unknown[] = [ABSENT, null, [], ['p-1']];
  const flags: unknown[] = [ABSENT, true, false];
  const files: unknown[] = [ABSENT, ['f-1']];
  const tools: unknown[] = [
    ABSENT,
    [{ tool: 't', success: true, result: 'r', prompt: 'p', arguments: {}, createdAt: '2026-01-01T00:00:00.000Z' }],
  ];
  for (const content of contents) {
    for (const t of targets) {
      for (const flag of flags) {
        for (const f of files) {
          for (const tl of tools) {
            const input: Record<string, unknown> = {};
            if (content !== ABSENT) input.content = content;
            if (t !== ABSENT) input.targetParticipantIds = t;
            if (flag !== ABSENT) input.isEstablishedFact = flag;
            if (f !== ABSENT) input.fileIds = f;
            if (tl !== ABSENT) input.pendingToolResults = tl;
            const r = sendMessageSchema.safeParse(input);
            process.stdout.write(
              JSON.stringify({
                input,
                success: r.success,
                issues: r.success ? [] : r.error.issues.map((i) => ({ path: i.path, message: i.message })),
              }) + '\n',
            );
          }
        }
      }
    }
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
