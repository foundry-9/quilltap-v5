/**
 * The floating-layer guard census (P4.D359 item 3; ruling R-4).
 *
 * v4 `99463fc56` put ONE `isInFloatingLayer(event.target)` early return in the
 * one hook every dismiss site uses (`hooks/useClickOutside.ts:76-80, :98-111`),
 * so a click or an Escape inside the always-on Brahma Console never dismisses a
 * popover or modal beneath. v5 has no such hook — each surface binds its own
 * document listener — so the guard sits in every handler, and this census is
 * the tripwire for the next listener someone adds:
 *
 *  1. every row of {@link GUARDED} names a handler whose body OPENS with the
 *     guard (read from source, the `dialog-roles` idiom);
 *  2. every document-level `mousedown` / `pointerdown` / `click` / `keydown`
 *     site in `src/app` is a row here, a sibling HANDOFF row, or a named
 *     exemption — a new unguarded listener fails `npm test`;
 *  3. a self-test: the matcher refuses a handler without the guard.
 *
 * Then three behavioural cases transcribed from v4
 * `__tests__/unit/hooks/useClickOutside.test.ts:298-345` ("topmost floating
 * layer"), one per representative kind: `brahma/model-picker.ts` (a document
 * `mousedown` and a document Escape) and `ui/tooltip.ts`'s capture-phase
 * `pointerdown` (a pinned card must survive a click inside the console).
 */
import { ChangeDetectionStrategy, Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';

import { ModelPicker } from '../brahma/model-picker';
import { FLOATING_LAYER_ATTR } from './floating-layer';
import { Tooltip } from './tooltip';

interface NodeFs {
  readdirSync(path: string): string[];
  readFileSync(path: string, encoding: 'utf8'): string;
  statSync(path: string): { isDirectory(): boolean };
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

interface GuardRow {
  /** Path under `src/app`. */
  readonly file: string;
  /** The handler's name — a method `name(` or an arrow `name = (`. */
  readonly handler: string;
  readonly kind: 'mousedown' | 'pointerdown' | 'click' | 'escape' | 'keydown';
  /** A sibling lane applies this one (a HANDOFF); skipped until its gate flips. */
  readonly owner?: 'P4.D358' | 'P4.D344';
}

/**
 * The census of 2026-10-29 (36 sites, 32 files) + the nine keydown sites it
 * missed, found by this lane (marked `// +` — seven here, two sibling-owned),
 * + `ui/modal.ts`'s new Escape listener (P4.D358's R-N).
 */
export const GUARDED: readonly GuardRow[] = [
  { file: 'ui/alert-modal.service.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'ui/tooltip.ts', handler: 'onPointerDown', kind: 'pointerdown' },
  { file: 'ui/tooltip.ts', handler: 'onKeyDown', kind: 'keydown' }, // +
  { file: 'wardrobe/wardrobe-reference-picker.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'wardrobe/equipped-slot-row.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'wardrobe/wardrobe-control-dialog.ts', handler: 'onEscape', kind: 'escape' },
  {
    file: 'wardrobe/wardrobe-control-dialog.ts',
    handler: 'onDocumentMouseDown',
    kind: 'mousedown',
  },
  { file: 'wardrobe/outfit-quick-pick.ts', handler: 'onDocMouseDown', kind: 'mousedown' },
  { file: 'wardrobe/outfit-quick-pick.ts', handler: 'onKeyDown', kind: 'keydown' }, // +
  { file: 'chat/rng-dropdown.ts', handler: 'onDocumentMouseDown', kind: 'mousedown' },
  { file: 'chat/custom-tools-popup.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'chat/library-picker/library-file-picker-modal.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'chat/sidebar/chat-sidebar.ts', handler: 'onPointerDown', kind: 'pointerdown' },
  { file: 'chat/sidebar/chat-sidebar.ts', handler: 'onKeyDown', kind: 'keydown' }, // +
  { file: 'images/save-image-dialog.ts', handler: 'onDocumentEscape', kind: 'escape' },
  { file: 'images/standalone-generate-image-dialog.ts', handler: 'onDocumentClick', kind: 'click' },
  { file: 'images/image-modal.ts', handler: 'onKeyDown', kind: 'keydown' }, // +
  { file: 'images/image-navigation.ts', handler: 'handleKeyDown', kind: 'keydown' }, // +
  { file: 'almanack/almanack-report-dialog.ts', handler: 'onKeyDown', kind: 'keydown' }, // +
  { file: 'workspace/chrome/workspace-host.ts', handler: 'onClick', kind: 'click' },
  {
    file: 'screens/scenarios/shared/scenario-row.ts',
    handler: 'onDocMouseDown',
    kind: 'mousedown',
  },
  { file: 'screens/scenarios/shared/scenario-row.ts', handler: 'onKeyDown', kind: 'keydown' }, // +
  {
    file: 'screens/generate-image/generate-image-page.ts',
    handler: 'onDocumentClick',
    kind: 'click',
  },
  { file: 'inventory/inventory-transfer-dialog.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'inventory/inventory-item-editor.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'inventory/inventory-instantiate-dialog.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'inventory/carrying-now-pane.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'inventory/inventory-reference-picker.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'inventory/inventory-control-dialog.ts', handler: 'onEscape', kind: 'escape' },
  {
    file: 'screens/characters/generators/optimizer/character-optimizer-modal.ts',
    handler: 'onEscape',
    kind: 'escape',
  },
  {
    file: 'screens/characters/generators/wizard/ai-wizard-modal.ts',
    handler: 'onEscape',
    kind: 'escape',
  },
  {
    file: 'screens/characters/generators/external-prompt/external-prompt-dialog.ts',
    handler: 'onEscape',
    kind: 'escape',
  },
  {
    file: 'screens/characters/generators/external-prompt/external-prompt-result-dialog.ts',
    handler: 'onDocumentEscape',
    kind: 'escape',
  },
  {
    file: 'screens/characters/generators/ai-import/ai-import-wizard.ts',
    handler: 'onEscape',
    kind: 'escape',
  },
  { file: 'item-browser/item-folder-picker-dialog.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'search/search-bar.ts', handler: 'onGlobalKeydown', kind: 'keydown' },
  { file: 'search/search-bar.ts', handler: 'onDocumentMouseDown', kind: 'mousedown' },
  { file: 'search/search-dialog.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'screens/photos/photos-page.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'brahma/model-picker.ts', handler: 'onDocumentMousedown', kind: 'mousedown' },
  { file: 'brahma/model-picker.ts', handler: 'onDocumentEscape', kind: 'escape' },
  {
    file: 'editor/char-insert/char-picker-panel.ts',
    handler: 'onDocumentMouseDown',
    kind: 'mousedown',
  },
  { file: 'editor/char-insert/char-picker-panel.ts', handler: 'onDocumentEscape', kind: 'escape' },
  // HANDOFFs — the sibling applies the one-line guard; the unifier deletes `owner`.
  { file: 'ui/modal.ts', handler: 'onDocumentKeydown', kind: 'escape', owner: 'P4.D358' }, // R-N's new listener
  {
    file: 'ui/slide-over-panel.ts',
    handler: 'onDocumentKeydown',
    kind: 'keydown',
    owner: 'P4.D358',
  }, // +
  {
    file: 'screens/salon/salon-conversation.ts',
    handler: 'onInspectorKeydown',
    kind: 'keydown',
    owner: 'P4.D344',
  }, // +
];

/** The gates the unifier flips as each sibling's guard lands (§S.9). */
const P4D358_MODAL_ESCAPE_LANDED = false;
const P4D344_INSPECTOR_GUARD_LANDED = false;
const landed = (row: GuardRow): boolean =>
  row.owner === undefined ||
  (row.owner === 'P4.D358' ? P4D358_MODAL_ESCAPE_LANDED : P4D344_INSPECTOR_GUARD_LANDED);

/**
 * Document-level listeners that are NOT dismissals of a surface the console
 * could sit over: the floating dialog's own (it IS the layer), and the
 * comment that names the binding idiom.
 */
const EXEMPT_SITES: readonly string[] = [
  'ui/floating-dialog.ts', // the floating layer's own listeners
];

/** Does the handler's body open with the guard? */
export function handlerIsGuarded(source: string, handler: string): boolean {
  const def = new RegExp(
    `(?:^|\\n)\\s*(?:(?:protected|private|public)\\s+)?(?:readonly\\s+)?(?:const\\s+)?${handler}\\s*(?:=\\s*\\([^)]*\\)\\s*(?::[^=]*)?=>|\\([^)]*\\)(?::\\s*void)?)\\s*\\{\\n`,
  );
  const m = def.exec(source);
  if (!m) return false;
  const body = source
    .slice(m.index + m[0].length)
    .split('\n')
    .slice(0, 2)
    .join('\n');
  return /isInFloatingLayer\(/.test(body.split('\n')[0]);
}

describe('the floating-layer guard census (ruling R-4)', () => {
  it('the matcher demands the guard as the handler’s first statement (self-test)', () => {
    const guarded = `class X {\n  protected onEscape(event?: Event): void {\n    if (isInFloatingLayer(event?.target ?? null)) return;\n    this.close();\n  }\n}`;
    const late = `class X {\n  protected onEscape(event?: Event): void {\n    this.close();\n    if (isInFloatingLayer(event?.target ?? null)) return;\n  }\n}`;
    const bare = `class X {\n  protected onEscape(): void {\n    this.close();\n  }\n}`;
    const arrow = `const onKeyDown = (e: KeyboardEvent) => {\n  if (isInFloatingLayer(e.target)) return;\n};`;
    expect(handlerIsGuarded(guarded, 'onEscape')).toBe(true);
    expect(handlerIsGuarded(late, 'onEscape')).toBe(false);
    expect(handlerIsGuarded(bare, 'onEscape')).toBe(false);
    expect(handlerIsGuarded(arrow, 'onKeyDown')).toBe(true);
    expect(handlerIsGuarded(arrow, 'onPointerDown')).toBe(false);
  });

  it('carries the 36 census rows + the 7 this lane found, and 3 HANDOFF rows (2 found here + ui/modal)', () => {
    expect(GUARDED.filter((r) => !r.owner)).toHaveLength(43);
    expect(GUARDED.filter((r) => r.owner)).toHaveLength(3);
  });

  for (const row of GUARDED) {
    const title = `${row.file} — ${row.handler} (${row.kind})`;
    if (!landed(row)) {
      it.skip(`${title} — HANDOFF to ${row.owner}`, () => undefined);
      continue;
    }
    it(title, () => {
      const source = fs.readFileSync(`${appRoot()}/${row.file}`, 'utf8');
      expect(handlerIsGuarded(source, row.handler)).toBe(true);
    });
  }

  it('every document-level dismiss listener in src/app is in the table (the tripwire)', () => {
    const root = appRoot();
    const walk = (dir: string): string[] =>
      fs.readdirSync(dir ? `${root}/${dir}` : root).flatMap((entry) => {
        const rel = dir ? `${dir}/${entry}` : entry;
        if (fs.statSync(`${root}/${rel}`).isDirectory()) return walk(rel);
        return entry.endsWith('.ts') && !entry.endsWith('.spec.ts') ? [rel] : [];
      });
    const site =
      /document\.addEventListener\(\s*'(?:mousedown|pointerdown|click|keydown)'|'\(document:(?:mousedown|click|pointerdown|keydown\.escape|keydown)\)'/;
    const files = new Set(GUARDED.map((r) => r.file));
    const missing = walk('').filter(
      (rel) =>
        !EXEMPT_SITES.includes(rel) &&
        !files.has(rel) &&
        site.test(fs.readFileSync(`${root}/${rel}`, 'utf8')),
    );
    expect(missing).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// Behaviour (transcribed — v4 useClickOutside.test.ts:298-345)
// ---------------------------------------------------------------------------

const PROFILES = [
  { id: 'p1', name: 'Everyday', provider: 'OPENAI', modelName: 'gpt-4o', isDefault: false },
];

function layer(): { el: HTMLDivElement; inner: HTMLButtonElement } {
  const el = document.createElement('div');
  el.setAttribute(FLOATING_LAYER_ATTR, '');
  const inner = document.createElement('button');
  el.appendChild(inner);
  document.body.appendChild(el);
  return { el, inner };
}

describe('the guard in place (v4 "topmost floating layer")', () => {
  let cleanup: (() => void) | null = null;
  afterEach(() => {
    cleanup?.();
    cleanup = null;
    TestBed.resetTestingModule();
  });

  function openPicker() {
    TestBed.configureTestingModule({ imports: [ModelPicker] });
    const fixture = TestBed.createComponent(ModelPicker);
    fixture.componentRef.setInput('profiles', PROFILES);
    fixture.componentRef.setInput('activeId', null);
    document.body.appendChild(fixture.nativeElement);
    fixture.detectChanges();
    (fixture.nativeElement.querySelector('button') as HTMLButtonElement).click();
    fixture.detectChanges();
    const isOpen = () => fixture.nativeElement.querySelector('[role="listbox"]') !== null;
    expect(isOpen()).toBe(true);
    return { fixture, isOpen };
  }

  it('does not treat a mousedown inside the floating layer as outside', () => {
    const { fixture, isOpen } = openPicker();
    const { el, inner } = layer();
    cleanup = () => el.remove();
    inner.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    fixture.detectChanges();
    expect(isOpen()).toBe(true);
    // …while a mousedown anywhere else still dismisses.
    document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    fixture.detectChanges();
    expect(isOpen()).toBe(false);
  });

  it('ignores Escape pressed inside the floating layer; still handles Escape elsewhere', () => {
    const { fixture, isOpen } = openPicker();
    const { el, inner } = layer();
    cleanup = () => el.remove();
    inner.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    fixture.detectChanges();
    expect(isOpen()).toBe(true);
    document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    fixture.detectChanges();
    expect(isOpen()).toBe(false);
  });

  it('a pinned tooltip survives a click inside the console (capture-phase pointerdown)', () => {
    @Component({
      changeDetection: ChangeDetectionStrategy.OnPush,
      imports: [Tooltip],
      template: `<qt-tooltip content="A substantial note" pinnable
        ><button id="anchor">a</button></qt-tooltip
      >`,
    })
    class Host {}
    TestBed.configureTestingModule({ imports: [Host] });
    const fixture = TestBed.createComponent(Host);
    document.body.appendChild(fixture.nativeElement);
    fixture.detectChanges();
    (fixture.nativeElement.querySelector('qt-tooltip') as HTMLElement).click();
    fixture.detectChanges();
    const pinned = () => document.querySelector('.qt-tooltip[data-pinned="true"]') !== null;
    expect(pinned()).toBe(true);
    const { el, inner } = layer();
    cleanup = () => {
      el.remove();
      fixture.destroy();
      (fixture.nativeElement as HTMLElement).remove();
    };
    inner.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
    fixture.detectChanges();
    expect(pinned()).toBe(true);
    document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
    fixture.detectChanges();
    expect(pinned()).toBe(false);
  });
});
