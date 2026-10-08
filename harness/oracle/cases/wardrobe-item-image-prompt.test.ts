/**
 * @jest-environment node
 *
 * Tier-1 ORACLE for the wardrobe item picture prompt (P4.D263; v4
 * `lib/wardrobe/item-image-prompt.ts` + `lib/wardrobe/avatar-prompt.ts`'s
 * `buildFigureIdentityBlock`, `7c8572869`), ported to
 * `quilltap-core::services::{wardrobe_item_image_prompt, avatar_prompt}`.
 *
 * Imports v4's REAL `buildWardrobeItemImagePrompt`, `buildWardrobeItemCue` and
 * `buildFigureIdentityBlock`. The ONE seam is `resolveAesthetic` (the Aurora
 * aesthetic read — a document-store lookup the Rust builder takes as an
 * argument instead): it is mocked to answer each case's `aesthetic` (null =
 * none), so the builder's own trim / 600-unit cap / preamble all run for real.
 *
 * The prompt is emitted as the bytes a provider RECEIVES — `Buffer.from(prompt,
 * 'utf8').toString('utf8')` — because v4's `slice(0, 600)` can split a
 * surrogate pair into a lone surrogate, which JSON cannot carry and UTF-8
 * encoding replaces with U+FFFD on the wire (the Rust helper's lossy decode
 * lands on the same bytes). `utf16Length` records the in-memory length.
 *
 * Run (Node 24, from the v4 checkout — a /tmp mirror, jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-wiip-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/wardrobe-item-image-prompt.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/wardrobe-item-image-prompt.json" "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-wardrobe-item-image-prompt.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "cases/wardrobe-item-image-prompt\.test\.ts$"
 */

import * as fs from 'fs';
import { join } from 'node:path';

let mockAesthetic: string | null = null;
jest.mock('@/lib/image-gen/aesthetic', () => ({
  __esModule: true,
  resolveAesthetic: jest.fn(async () => mockAesthetic),
  getProjectOfficialMountPointId: jest.fn(async () => null),
}));

import {
  buildWardrobeItemCue,
  buildWardrobeItemImagePrompt,
} from '@/lib/wardrobe/item-image-prompt';
import { buildFigureIdentityBlock } from '@/lib/wardrobe/avatar-prompt';

interface Case {
  name: string;
  item: Record<string, unknown>;
  components: Array<Record<string, unknown>>;
  owner: Record<string, unknown> | null;
  aesthetic: string | null;
}

const wire = (s: string): string => Buffer.from(s, 'utf8').toString('utf8');

test('wardrobe-item-image-prompt tier-1 oracle', async () => {
  const spec = JSON.parse(
    fs.readFileSync(join(__dirname, '..', 'fixtures', 'wardrobe-item-image-prompt.json'), 'utf8'),
  ) as { cases: Case[] };
  const out: string[] = [];
  for (const c of spec.cases) {
    mockAesthetic = c.aesthetic;
    const built = await buildWardrobeItemImagePrompt({
      item: c.item as never,
      components: c.components as never,
      owner: c.owner as never,
      projectOfficialMountPointId: null,
    });
    out.push(
      JSON.stringify({
        name: c.name,
        prompt: wire(built.prompt),
        utf16Length: built.prompt.length,
        orientation: built.orientation,
        subject: built.subject,
        cue: buildWardrobeItemCue(c.item as never, c.components as never),
        figure: c.owner
          ? {
              headAndShoulders: buildFigureIdentityBlock(c.owner as never, 'head-and-shoulders'),
              fullLength: buildFigureIdentityBlock(c.owner as never, 'full-length'),
            }
          : null,
      }),
    );
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  fs.writeFileSync(outPath, out.join('\n') + '\n');
});
