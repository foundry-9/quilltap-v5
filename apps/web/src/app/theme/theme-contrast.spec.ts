import { beforeAll, describe, expect, it } from 'vitest';

import {
  CHECKLIST_PAIRS,
  contrastRatio,
  cssBlockColors,
  evaluatePairs,
  parseCssColor,
  type PairResult,
  type ThemeColors,
} from './contrast-ratio';
import residuesJson from './theme-contrast.residues.json';

/**
 * The token-contrast spec (P4.D357): v4's release-checklist-13 step 1 at
 * `236c04e35` (`.claude/commands/release-checklist-13-ui.md:19-23`), kept as a
 * test instead of a throwaway script:
 *
 * > every `X` / `XForeground` pair (`primary`, `secondary`, `muted`, `accent`,
 * > `destructive`, `card`, `popover`, `success`, `warning`, `info`, `chatUser`,
 * > …) — **≥ 4.5:1** (SC 1.4.3);
 * > `foreground` and `mutedForeground` on `background`, `card`, `muted` and
 * > `popover` — `mutedForeground` is the usual offender;
 * > `border` / `input` against `background` and `card`, and `ring` against
 * > everything a focused control can sit on — **≥ 3:1** (SC 1.4.11, 2.4.7);
 * > status colours used as text (`destructive`, `success`, `warning`, `info`
 * > on `background`).
 *
 * Over every bundled pack's `public/themes/<id>/tokens.json` (every colour
 * block present — Madman's Box carries a `dark` block though it does not
 * support dark mode) and the default palette's `:root` / `.dark` blocks in
 * `src/styles.css`.
 *
 * `theme-contrast.residues.json` is v4's OWN residue at 4.11.0 — the 3:1
 * border / input / ring pairs v4 did not move, and Earl Grey's `transparent`
 * user bubble — and it is BOTH-WAYS: a pair outside it that fails reddens
 * (a regression), and a row in it that now passes or measures differently
 * reddens too (a v4 fix or a pack edit: delete or re-record the row). No
 * text / fill pair may sit in it except as NOT_A_FILL.
 *
 * Files are read through `node:fs` at RUN time — the
 * `ui/inventory-icon.spec.ts` mechanism.
 */

const PACK_IDS = ['art-deco', 'earl-grey', 'great-estate', 'madmans-box', 'old-school', 'rains'] as const;

/** `theme.json` at the pin — re-vendor from v4 and re-record when one moves. */
const PACK_MANIFESTS: Record<(typeof PACK_IDS)[number], { version: string; supportsDarkMode: boolean }> = {
  'art-deco': { version: '1.0.14', supportsDarkMode: true },
  'earl-grey': { version: '1.3.11', supportsDarkMode: true },
  'great-estate': { version: '1.0.11', supportsDarkMode: true },
  'madmans-box': { version: '1.1.10', supportsDarkMode: false },
  'old-school': { version: '1.0.15', supportsDarkMode: true },
  rains: { version: '1.3.15', supportsDarkMode: true },
};

/** v5's one adaptation of the vendored packs: asset URLs served from `/themes/<id>/`. */
const ADAPTED_URLS: Record<string, readonly string[]> = {
  'art-deco': [
    "url('/themes/art-deco/textures/background-light.webp')",
    "url('/themes/art-deco/textures/background-dark.webp')",
  ],
  'great-estate': ['/themes/great-estate/textures/background.webp', '/themes/great-estate/textures/carbon-fibre.png'],
  'madmans-box': ['/themes/madmans-box/'],
};

interface Residue {
  theme: string;
  mode: string;
  pair: string;
  min: number;
  measured: number | null;
  class: 'NON_TEXT_3_1' | 'NOT_A_FILL';
}

const RESIDUES = residuesJson as Residue[];

/** The slice of `node:fs` this spec reads. */
interface NodeFs {
  readFileSync(path: string, encoding: 'utf8'): string;
}

let fs: NodeFs;

beforeAll(async () => {
  const specifier = 'node:fs';
  fs = (await import(/* @vite-ignore */ specifier)) as NodeFs;
});

/** A path under `apps/web`, from the runner's working directory. */
function webPath(rel: string): string {
  const cwd = (globalThis as unknown as { process: { cwd(): string } }).process.cwd();
  return `${cwd.replace(/\\/g, '/')}/${rel}`;
}

function read(rel: string): string {
  return fs.readFileSync(webPath(rel), 'utf8');
}

/** Every theme × mode the spec measures. */
function themeModes(): { theme: string; mode: string; colors: ThemeColors }[] {
  const css = read('src/styles.css');
  const out = [
    { theme: 'default', mode: 'light', colors: cssBlockColors(css, ':root') },
    { theme: 'default', mode: 'dark', colors: cssBlockColors(css, '.dark') },
  ];
  for (const id of PACK_IDS) {
    const tokens = JSON.parse(read(`public/themes/${id}/tokens.json`)) as {
      colors: Record<string, ThemeColors>;
    };
    for (const [mode, colors] of Object.entries(tokens.colors)) out.push({ theme: id, mode, colors });
  }
  return out;
}

const round2 = (n: number) => Math.round(n * 100) / 100;

function describeResult(theme: string, mode: string, r: PairResult): string {
  const measured = r.measured === null ? r.reason : round2(r.measured).toFixed(2);
  return `${theme} ${mode}: ${r.pair} = ${measured} (min ${r.min})`;
}

describe('contrastRatio', () => {
  const rgb = (v: string) => parseCssColor(v)!;

  it('is 21 for white on black, in either order and either notation', () => {
    expect(contrastRatio(rgb('#ffffff'), rgb('#000'))).toBeCloseTo(21, 10);
    expect(contrastRatio(rgb('hsl(0 0% 0%)'), rgb('hsl(0 0% 100%)'))).toBeCloseTo(21, 10);
  });

  it('puts #777777 on white just under the 4.5 boundary', () => {
    expect(round2(contrastRatio(rgb('#777777'), rgb('#ffffff')))).toBe(4.48);
  });

  it('parses the legacy comma form and an alpha', () => {
    expect(parseCssColor('hsl(210, 75%, 44.5%)')).toEqual(parseCssColor('hsl(210 75% 44.5%)'));
    expect(parseCssColor('hsl(0 0% 100% / 0.1)')?.a).toBe(0.1);
    expect(parseCssColor('hsla(0, 0%, 100%, 50%)')?.a).toBe(0.5);
  });

  it('reads transparent as alpha 0 and refuses what it does not know', () => {
    expect(parseCssColor('transparent')).toEqual({ r: 0, g: 0, b: 0, a: 0 });
    expect(parseCssColor('var(--x)')).toBeNull();
  });
});

describe('theme token contrast (release checklist 13)', () => {
  const residueKey = (r: { theme: string; mode: string; pair: string }) => `${r.theme}|${r.mode}|${r.pair}`;

  it('measures every bundled pack and both default modes', () => {
    expect(themeModes().map((t) => `${t.theme} ${t.mode}`)).toHaveLength(14);
  });

  it('measures each checklist pair once', () => {
    const names = CHECKLIST_PAIRS.map(([fg, bg]) => `${fg} on ${bg}`);
    expect(new Set(names).size).toBe(names.length);
    expect(names).toHaveLength(29);
  });

  it('allowlists only non-text 3:1 pairs and unfilled backgrounds', () => {
    for (const r of RESIDUES) {
      expect(['NON_TEXT_3_1', 'NOT_A_FILL']).toContain(r.class);
      if (r.class === 'NON_TEXT_3_1') expect(r.min, residueKey(r)).toBe(3);
      if (r.class === 'NOT_A_FILL') expect(r.measured, residueKey(r)).toBeNull();
    }
    expect(new Set(RESIDUES.map(residueKey)).size).toBe(RESIDUES.length);
  });

  for (const theme of ['default', ...PACK_IDS]) {
    for (const mode of ['light', 'dark']) {
      describe(`${theme} ${mode}`, () => {
        it('meets every checklist minimum outside the residue allowlist', () => {
          const tm = themeModes().find((t) => t.theme === theme && t.mode === mode);
          if (!tm) return;
          const allowed = new Set(RESIDUES.filter((r) => r.theme === theme && r.mode === mode).map(residueKey));
          const failures = evaluatePairs(tm.colors)
            .filter((r) => !allowed.has(residueKey({ theme, mode, pair: r.pair })))
            .filter((r) => r.measured === null || r.measured < r.min)
            .map((r) => describeResult(theme, mode, r));
          expect(failures).toEqual([]);
        });

        it('still fails every allowlisted residue, as measured', () => {
          const tm = themeModes().find((t) => t.theme === theme && t.mode === mode);
          const rows = RESIDUES.filter((r) => r.theme === theme && r.mode === mode);
          if (!tm) {
            expect(rows, 'residues for a mode the theme no longer carries').toEqual([]);
            return;
          }
          const results = new Map(evaluatePairs(tm.colors).map((r) => [r.pair, r]));
          const stale = rows.flatMap((row) => {
            const r = results.get(row.pair);
            if (!r) return [`${residueKey(row)}: no such pair`];
            const measured = r.measured === null ? null : round2(r.measured);
            const still = r.measured === null ? r.reason === row.class : r.measured < row.min;
            return still && measured === row.measured
              ? []
              : [`${describeResult(theme, mode, r)} — allowlisted as ${row.measured}: delete or re-record the row`];
          });
          expect(stale).toEqual([]);
        });
      });
    }
  }
});

describe('bundled theme packs', () => {
  for (const id of PACK_IDS) {
    it(`${id}: theme.json is at v4's 624d0d25c (4.13.0-dev.8) version`, () => {
      const manifest = JSON.parse(read(`public/themes/${id}/theme.json`)) as {
        version: string;
        supportsDarkMode: boolean;
      };
      expect(
        { version: manifest.version, supportsDarkMode: manifest.supportsDarkMode },
        `${id} moved — re-vendor from v4 and re-record`,
      ).toEqual(PACK_MANIFESTS[id]);
    });

    it(`${id}: styles.css carries v5's asset URLs, never v4's bundle route`, () => {
      const css = read(`public/themes/${id}/styles.css`);
      expect(css).not.toContain('/api/themes/assets/bundle:');
      for (const url of ADAPTED_URLS[id] ?? []) expect(css).toContain(url);
    });
  }

  it("madman's box points the thinking glyph at its brand drawing", () => {
    const css = read('public/themes/madmans-box/styles.css');
    expect(css).toContain(
      '[data-theme="madmans-box"] [data-icon="thinking"] {\n  --_qt-icon-mask: url("/themes/madmans-box/icons/brand.svg");\n  --_qt-icon-bg: none;\n}',
    );
  });
});

describe('toast colours (v4 58120a7fe: the solid status triple)', () => {
  const TYPES = { success: 'success', error: 'destructive', warning: 'warning', info: 'info' } as const;

  for (const [type, token] of Object.entries(TYPES)) {
    it(`.qt-toast-${type} paints --color-${token} with its foreground`, () => {
      const css = read('src/styles/qt-components/_toast.css');
      const start = css.indexOf(`.qt-toast-${type} {`);
      expect(start, `.qt-toast-${type} rule`).toBeGreaterThan(-1);
      const rule = css.slice(start, css.indexOf('}', start));
      expect(rule).toContain(`background-color: var(--qt-toast-${type}-bg, var(--color-${token}));`);
      expect(rule).toContain(`border-color: var(--qt-toast-${type}-border, var(--color-${token}));`);
      expect(rule).toContain(`color: var(--qt-toast-${type}-fg, var(--color-${token}-foreground));`);
      expect(rule).not.toContain('--qt-status-');
    });
  }
});
