import { describe, expect, it } from 'vitest';

import fieldCatalogue from './fixtures/wizard-field-catalogue.json';
import { FIELD_DESCRIPTIONS, FIELD_LABELS } from './wizard-types';

/**
 * Parity spec: `FIELD_LABELS`/`FIELD_DESCRIPTIONS` vs v4's `types.ts` EXECUTED
 * at the pin (the P4.D144 "table diffed against the executed v4 module"
 * precedent).
 *
 * `fixtures/wizard-field-catalogue.json` is a recorded snapshot — the ground
 * truth was NOT re-derived from this port's own understanding of v4; it was
 * produced by running v4's real module through `tsx` at the `f699da6f6` pin
 * and dumping its exports to JSON. Regen recipe:
 *
 * ```bash
 * PIN=/tmp/qt-v4-pin-p49k3-f699da6f6
 * cat > "$PIN/dump-wizard-types.ts" <<'TS'
 * import { FIELD_LABELS, FIELD_DESCRIPTIONS } from './components/characters/ai-wizard/types'
 * console.log(JSON.stringify({ FIELD_LABELS, FIELD_DESCRIPTIONS }, null, 2))
 * TS
 * cd "$PIN" && npx tsx ./dump-wizard-types.ts > /tmp/wizard-field-catalogue.json
 * rm "$PIN/dump-wizard-types.ts"
 * cp /tmp/wizard-field-catalogue.json apps/web/src/app/screens/characters/generators/wizard/fixtures/
 * ```
 */
describe('the wizard field catalogue vs v4 `types.ts` executed at the pin', () => {
  it('FIELD_LABELS matches byte-for-byte', () => {
    expect(FIELD_LABELS).toEqual(fieldCatalogue.FIELD_LABELS);
  });

  it('FIELD_DESCRIPTIONS matches byte-for-byte (typographic apostrophes and all)', () => {
    expect(FIELD_DESCRIPTIONS).toEqual(fieldCatalogue.FIELD_DESCRIPTIONS);
  });

  it('carries every GeneratableField the recorded catalogue does', () => {
    expect(Object.keys(FIELD_LABELS).sort()).toEqual(
      Object.keys(fieldCatalogue.FIELD_LABELS).sort(),
    );
  });
});
