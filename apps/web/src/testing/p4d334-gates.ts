/**
 * The four gate constants of P4.D334 (round R-V). Each is `false` on this
 * branch because the code it waits for lives on a sibling lane; the unifier
 * flips all four here at §S.9 — one file, four lines — and deletes the two
 * stand-in modules (`pascal/p4d333-stand-ins.ts`,
 * `screens/custom-tools/p4d330-stand-ins.ts`).
 *
 * LANE-REMOVE at §S.9 (P4.D334 R-1): this file stays (the flags are the
 * deletion list's last entry), but every constant becomes `true` and the
 * `skipIf` guards that read them fall away with the stand-ins.
 *
 * The unifier's deletion list is `rg "p4d33[03]-stand-ins|p4d334-gates" apps/web/src`.
 */

/** ACTIVATE-AT-UNIFY: P4.D333 lands the grammar exports (`custom-tool-types.ts`, `tool-gate.ts`, `metadata-match.ts`, `placeholders.ts`). */
export const P4D333_SPA_LANDED = false;

/** ACTIVATE-AT-UNIFY: P4.D330 lands the reference picker, the vantage loader and the Inventory display / glyph modules. */
export const P4D330_SPA_LANDED = false;

/** ACTIVATE-AT-UNIFY: P4.D329 lands the preview / audit `inventory` body and the object roll form on the server. */
export const P4D329_SERVER_LANDED = false;

/** ACTIVATE-AT-UNIFY: P4.D331 lands the Log effect (`{ index, log }` on the bench's `effects`). */
export const P4D331_SERVER_LANDED = false;
