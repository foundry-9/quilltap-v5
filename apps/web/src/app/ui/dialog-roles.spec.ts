/**
 * The dialog-roles census (P4.D358 item 7): every row of
 * `./dialog-roles.table.ts` names a v5 file whose template must carry ONE
 * element with the row's `role`, `aria-modal`, and a name binding — v4
 * `dff76ebef`'s dialog shape. It reads SOURCE, so any lane's later edit that
 * drops a dialog's semantics fails `npm test`.
 *
 * Measured red-first on unported `main`: every role-less row failed (and every
 * row whose role sat without a linked name).
 */

import { beforeAll, describe, expect, it } from 'vitest';

import { DIALOG_ROLES, type DialogRoleRow } from './dialog-roles.table';

/** The slice of `node:fs` this census reads (loaded at run time — see `pascal/raw-gate-reads.census.spec.ts`). */
interface NodeFs {
  readFileSync(path: string, encoding: 'utf8'): string;
}

let fs: NodeFs;

beforeAll(async () => {
  const specifier = 'node:fs';
  fs = (await import(/* @vite-ignore */ specifier)) as NodeFs;
});

function appRoot(): string {
  const cwd = (globalThis as unknown as { process: { cwd(): string } }).process.cwd();
  return `${cwd.replace(/\\/g, '/')}/src/app`;
}

/** Opening tags, attribute values quoted (Angular bindings may hold `>`). */
const OPEN_TAG = /<[a-zA-Z][\w-]*(?:\s+[^\s=>"'/]+(?:\s*=\s*(?:"[^"]*"|'[^']*'))?)*\s*\/?>/g;

/** Does one opening tag satisfy the row? */
function tagSatisfies(tag: string, row: Pick<DialogRoleRow, 'role' | 'name' | 'nonModal'>): boolean {
  if (!new RegExp(`\\srole="${row.role}"`).test(tag)) return false;
  const modal = row.nonModal ? 'false' : 'true';
  if (!new RegExp(`\\saria-modal="${modal}"`).test(tag)) return false;
  const attr = row.name === 'labelledby' ? 'aria-labelledby' : 'aria-label';
  return new RegExp(`\\s(?:${attr}|\\[attr\\.${attr}\\])=`).test(tag);
}

function satisfied(row: DialogRoleRow): boolean {
  const src = fs.readFileSync(`${appRoot()}/${row.v5}`, 'utf8');
  return (src.match(OPEN_TAG) ?? []).some((tag) => tagSatisfies(tag, row));
}

describe('dialog-roles census (v4 dff76ebef dialog shape)', () => {
  it('the matcher demands role, aria-modal and the name binding together', () => {
    const row = { role: 'dialog', name: 'labelledby' } as const;
    expect(tagSatisfies('<div role="dialog" aria-modal="true" [attr.aria-labelledby]="titleId">', row)).toBe(true);
    expect(tagSatisfies('<div role="dialog" aria-modal="true">', row)).toBe(false);
    expect(tagSatisfies('<div role="dialog" aria-labelledby="t">', row)).toBe(false);
    expect(tagSatisfies('<div role="alertdialog" aria-modal="true" aria-labelledby="t">', row)).toBe(false);
    expect(tagSatisfies('<div role="dialog" aria-modal="true" aria-label="x">', row)).toBe(false);
    expect(
      tagSatisfies('<div role="dialog" aria-modal="false" aria-labelledby="t">', { ...row, nonModal: true }),
    ).toBe(true);
  });

  for (const row of DIALOG_ROLES) {
    it(`${row.v5} — role="${row.role}" named by ${row.name} (v4 ${row.v4})`, () => {
      expect(satisfied(row)).toBe(true);
    });
  }
});
