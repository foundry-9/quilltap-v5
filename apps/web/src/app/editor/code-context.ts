// code-context.ts — the shared "am I in code?" bail for every typing aid.
// v4's $isInCodeContext (bug 63): one helper so the bail lists cannot drift.
import type { EditorState } from 'prosemirror-state';

export function isInCodeContext(state: EditorState): boolean {
  const { $from } = state.selection;
  if ($from.parent.type === state.schema.nodes['code_block']) return true;
  const codeMark = state.schema.marks['code'];
  if (!codeMark) return false;
  if (state.storedMarks?.some((m) => m.type === codeMark)) return true;
  return $from.marks().some((m) => m.type === codeMark);
}
