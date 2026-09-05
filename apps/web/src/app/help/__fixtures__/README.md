# P4.9I2B — the v4 help-guide capture

Every file here is **recorded from v4's real modules**, never hand-written.
They pin the Angular transcriptions in `help-categories.ts`, `help-stream.ts`
and `help-entity-picker.ts` byte-for-byte (memory note
`byte-exact-static-data-transcription`: ship the generator).

| File | Recorded from |
| --- | --- |
| `help-guide-tables.json` | `lib/help-guide/categories.ts` — `HELP_CATEGORIES`, `URL_CATEGORY_MAP`, `EXCLUDED_DOCUMENTS` |
| `help-guide-vectors.json` | 32 `getCategoryForUrl(...)` results from v4's real function |
| `label-from-url-vectors.json` | 35 `labelFromUrl(...)` results from `hooks/useHelpChatStreaming.ts` |
| `param-routes-vectors.json` | `HelpEntityPicker.tsx`'s private `PARAM_ROUTES`, probed through the exported `hasParamSegments` / `findParamRoute` |
| `welcome-card.json` | `HelpWelcomeCard` rendered to static markup — the four `WELCOME_LINKS` and the Wodehouse copy |

**The recorder is `apps/web/oracle/help-guide-capture.test.tsx`**; its header
carries the regen recipe (a pinned v4 worktree — jest ignores paths outside the
checkout, so the file is copied in). Recorded at v4 `d883a5ee1`, this lane's
oracle baseline. Fix the PORT, never these files.
