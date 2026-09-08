/**
 * The prompt-templates listing client (P4.83) — what fills the
 * Import-from-Template catalogue on both character hosts.
 *
 * v4 reaches `GET /api/v1/prompt-templates` with a bare `fetch` from two hooks
 * (`useSystemPrompts.ts:92-107` and `NewCharacterView.tsx:93-108`, byte-alike
 * bodies with DIFFERENT open semantics — see each host). v5 goes through the
 * dispatch verb §B declares, which the same server handler answers.
 *
 * ⚠ **The wire OMITS a null column, it does not send `null`.** v4's SQLite
 * backend hydrates every SQL NULL to `undefined` before Zod sees it, and the
 * four nullable columns are `.nullable().optional()`, so a built-in carries no
 * `userId` key and a bare user template carries no `description` / `category` /
 * `modelHint`. v4's own client never normalizes — it reads `template.description
 * && …`, for which absent and null are the same — so neither does this, and the
 * DTO below says `?: T | null` rather than the work order's §B `T | null`.
 * (Measured 2026-09-07 by `prompt_templates_routes_equivalence`; the correction
 * is recorded in the lane record.)
 *
 * @module screens/characters/generators/prompts-editor/prompt-templates.api
 */

import type { CoreClient } from '../../../../core/core-client';
import type { CoreRequest } from '../../../../core/core-contract';

/** §B — v4 `GET /api/v1/prompt-templates` → `{ templates, count }`. */
export interface PromptTemplateRecord {
  id: string;
  /** Absent on a built-in (the column is NULL). */
  userId?: string | null;
  name: string;
  content: string;
  description?: string | null;
  isBuiltIn: boolean;
  category?: string | null;
  modelHint?: string | null;
  tags: string[];
  createdAt: string;
  updatedAt: string;
}

/** §B — the request. */
export interface PromptTemplateListRequest {
  type: 'promptTemplateList';
}

/**
 * v4 `fetchTemplates` — `data.templates || []`.
 *
 * v4 distinguishes two failure arms: a non-2xx leaves `templates` untouched and
 * logs NOTHING, while a thrown request logs `Error fetching templates`. The
 * dispatch transport folds them (an error envelope throws a
 * `CoreDispatchError`), so v5 takes the logging arm for both — a recorded
 * mechanism divergence, and one the list route makes unreachable in practice:
 * v4's `findAllForUser` swallows a DB failure to `[]` and still answers 200.
 * The callers keep the rest of v4's contract: `templates` unchanged on failure.
 */
export async function fetchPromptTemplates(core: CoreClient): Promise<PromptTemplateRecord[]> {
  // `core-contract.ts` is frozen for this lane; §B's request type is declared
  // above and the unifier folds it in (retiring this cast) at unification.
  const request = { type: 'promptTemplateList' } as unknown as CoreRequest;
  const data = await core.dispatchData(request);
  return (data['templates'] as PromptTemplateRecord[]) ?? [];
}
