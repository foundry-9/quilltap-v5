/**
 * Tier-1 differential oracle for the W4.6b Commonplace-Book PURE whisper builders
 * — driving v4's REAL exports so the byte-exact persona / LLM-context bodies are
 * proven against the Rust port (`services::commonplace_notifications`).
 *
 * Covers `buildCommonplacePersonaWhisper` + `buildCommonplaceLLMContext` over the
 * `CommonplaceParts` branches: each optional section present in isolation, all six
 * present, whitespace-only (dropped), and the all-empty → empty-string behavior.
 * The `postCommonplaceWhisper` row + the refresh end-to-end are covered by the
 * central tier-3 differentials the parent owns.
 *
 * Pure — no DB, runs under `npx tsx`. Emits NDJSON to stdout.
 *
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5/harness/oracle/cases/post-office-commonplace.ts \
 *       > /tmp/oracle-po-commonplace.ndjson
 */

import {
  buildCommonplacePersonaWhisper,
  buildCommonplaceLLMContext,
  type CommonplaceParts,
} from '@/lib/services/commonplace-notifications/writer';

const cases: Array<{ id: string; parts: CommonplaceParts }> = [
  { id: 'all-empty', parts: {} },
  { id: 'only-current-state', parts: { currentState: 'It is dusk in the parlour.' } },
  { id: 'only-recap', parts: { recap: '## Recap\nThey argued, then reconciled.' } },
  { id: 'only-relevant', parts: { relevant: '## Relevant Memories\n- A kept promise.' } },
  { id: 'only-inter-char', parts: { interChar: 'You recall Ada distrusts locksmiths.' } },
  { id: 'only-knowledge', parts: { knowledge: '### Ledger\nThe accounts balance.' } },
  {
    id: 'only-relevant-conversations',
    parts: {
      relevantConversations:
        '### Relevant Past Conversations\n\n#### The Heist (`conv-1`)\n\n_note_',
    },
  },
  {
    id: 'all-six',
    parts: {
      currentState: 'The clock reads noon.',
      recap: 'A long recap.',
      relevant: 'Relevant entries.',
      interChar: 'About the others.',
      knowledge: 'From the shelves.',
      relevantConversations: 'Past dialogues.',
    },
  },
  { id: 'whitespace-only-current-state', parts: { currentState: '   \n\t ' } },
  {
    id: 'whitespace-dropped-mixed',
    parts: { currentState: '  ', recap: 'Kept recap.', relevant: '\n\n' },
  },
  {
    id: 'recap-and-relevant',
    parts: { recap: 'The gist.', relevant: 'The entries.' },
  },
  {
    id: 'padded-values-trimmed',
    parts: { knowledge: '   padded knowledge body   ' },
  },
  {
    id: 'current-state-and-relevant-conversations',
    parts: { currentState: 'Now.', relevantConversations: 'Then.' },
  },
];

function main(): void {
  const out: string[] = [];
  const emit = (kind: string, id: string, value: unknown) =>
    out.push(JSON.stringify({ kind, id, value }));

  for (const c of cases) {
    emit('persona', c.id, buildCommonplacePersonaWhisper(c.parts));
    emit('llm', c.id, buildCommonplaceLLMContext(c.parts));
  }

  for (const line of out) process.stdout.write(line + '\n');
}

main();
