/**
 * P4.D358 — the Shell's landmark plumbing (v4 `app-layout.tsx:113`,
 * `10e09b7c3`): a "Skip to content" link as the layout's FIRST element, ahead of
 * the navigation rail, targeting the ONE `<main id="main-content" tabindex="-1">`.
 *
 * It reads SOURCE: the Shell injects too much (router, workspace, realtime,
 * settings) to render cheaply, and the property is purely structural.
 */

import { beforeAll, describe, expect, it } from 'vitest';

import { skipToMain } from './shell';

/** The slice of `node:fs` this spec reads (loaded at run time — see `ui/dialog-roles.spec.ts`). */
interface NodeFs {
  readFileSync(path: string, encoding: 'utf8'): string;
}

let template: string;

beforeAll(async () => {
  const specifier = 'node:fs';
  const fs = (await import(/* @vite-ignore */ specifier)) as NodeFs;
  const cwd = (globalThis as unknown as { process: { cwd(): string } }).process.cwd();
  const source = fs.readFileSync(`${cwd.replace(/\\/g, '/')}/src/app/shell/shell.ts`, 'utf8');
  const start = source.indexOf('template: `');
  expect(start).toBeGreaterThan(-1);
  const end = source.indexOf('`', start + 'template: `'.length);
  // Comments may mention `<main` or `<aside` in prose; measure markup only.
  template = source.slice(start + 'template: `'.length, end).replace(/<!--[\s\S]*?-->/g, '');
});

describe('Shell — skip link + main landmark', () => {
  it('the first element inside .qt-app-layout is the skip link, ahead of the <aside>', () => {
    const open = '<div class="qt-app-layout">';
    const at = template.indexOf(open);
    expect(at).toBeGreaterThan(-1);
    const after = template.slice(at + open.length);
    const firstTag = after.match(/<[a-zA-Z][^>]*>/)!;
    expect(firstTag[0]).toBe(
      '<a href="#main-content" class="qt-skip-link" (click)="skipToMain($event)">',
    );
    expect(after.slice(firstTag.index!).startsWith(
      '<a href="#main-content" class="qt-skip-link" (click)="skipToMain($event)">Skip to content</a>',
    )).toBe(true);
    expect(firstTag.index!).toBeLessThan(after.indexOf('<aside'));
  });

  it('there is exactly one <main, carrying id="main-content" and tabindex="-1"', () => {
    const mains = template.match(/<main[\s>][^>]*>/g) ?? [];
    expect(mains).toHaveLength(1);
    expect(mains[0]).toContain('id="main-content"');
    expect(mains[0]).toContain('tabindex="-1"');
  });
});

describe('skipToMain (SKIP_LINK_BASE_HREF)', () => {
  it('focuses <main id="main-content"> and cancels the href navigation', () => {
    const main = document.createElement('main');
    main.id = 'main-content';
    main.tabIndex = -1;
    document.body.appendChild(main);
    const event = new MouseEvent('click', { cancelable: true });
    skipToMain(event);
    expect(event.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(main);
    main.remove();
  });

  it('leaves the event alone when there is no landmark', () => {
    const event = new MouseEvent('click', { cancelable: true });
    skipToMain(event);
    expect(event.defaultPrevented).toBe(false);
  });
});
