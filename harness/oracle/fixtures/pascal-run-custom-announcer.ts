/**
 * P4.D319 (v4 `a05c6f755`) — the `announced_dresser` definition of the committed
 * `pascal-run-custom-{main,mount}.db` pair, shared by the full builder
 * (`build-pascal-run-custom-fixture.ts`) and the in-place widen
 * (`extend-pascal-run-custom-announcer.ts`) that put it into the committed pair.
 *
 * It lives on the trigger room's Trigger Shelf (the project tier of
 * `CHAT_TRIGGERS`, `pascal-run-custom-triggers.ts`), so all three entrances —
 * the model's `run_custom`, the operator's Run Tool route and a spoken trigger
 * phrase — reach it in ONE room, and no other room's roster moves.
 *
 * `announceAsCharacter: true` with a `{{character}}` message and chip, NO
 * effects (keeps P4.D316's outfit-history provenance out of these rows), and
 * one trigger phrase. Written through v4's REAL `storeMountFile` (the caller's
 * `writeVaultFile`); nothing here is a hand-written row.
 */

import { SHELF_MP } from './pascal-run-custom-triggers';

/** The definition, by id (`name`) — the union merges the two lanes' modules side by side. */
export const ANNOUNCED_DRESSER = {
  name: 'announced_dresser',
  title: 'The Announced Dresser',
  description: 'The rolling character says how it went, in their own voice.',
  triggers: ['dress the part'],
  announceAsCharacter: true,
  chipLabel: 'For {{character}}',
  roll: { min: 0.4, max: 0.4 },
  outcomes: [{ when: true, message: '{{character}} rolls {{value}}', state: 'info' }],
};

/** The file the definition is written to on the Trigger Shelf. */
export const ANNOUNCED_DRESSER_PATH = `Tools/${ANNOUNCED_DRESSER.name}.tool.json`;

export async function addAnnouncedDresser(
  writeVaultFile: (mountPointId: string, relativePath: string, body: unknown) => Promise<void>,
): Promise<void> {
  await writeVaultFile(SHELF_MP, ANNOUNCED_DRESSER_PATH, ANNOUNCED_DRESSER);
}
