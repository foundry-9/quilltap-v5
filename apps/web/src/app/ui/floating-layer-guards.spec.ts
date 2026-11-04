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
 * Then behavioural cases transcribed from v4
 * `__tests__/unit/hooks/useClickOutside.test.ts:298-345` ("topmost floating
 * layer"), one per representative kind: `chat/speaker-selector.ts` (a document
 * click), `ui/modal.ts` (a document Escape) and `ui/tooltip.ts`'s capture-phase
 * `pointerdown` (a pinned card must survive a click inside the console) — and
 * the exemption's own case: `brahma/model-picker.ts`, which lives INSIDE the
 * layer, still closes on a press elsewhere in it.
 */
import { ChangeDetectionStrategy, Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';

import { ModelPicker } from '../brahma/model-picker';
import { SpeakerSelector } from '../chat/speaker-selector';
import { FLOATING_LAYER_ATTR } from './floating-layer';
import { Modal } from './modal';
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
}

/**
 * The census of 2026-10-29 (36 sites, 32 files) + the nine keydown sites it
 * missed (marked `// +`), as reconciled at the P4.D358 ∥ P4.D359 union:
 *
 *  - nine rows dropped — P4.D358 moved those hosts' Escape onto `qt-modal`'s
 *    own document listener and deleted the host handler (`library-file-picker-
 *    modal`, `save-image-dialog`, `inventory-transfer-dialog`, `inventory-item-
 *    editor`, `inventory-instantiate-dialog`, `carrying-now-pane`,
 *    `inventory-reference-picker`, `item-folder-picker-dialog`,
 *    `wardrobe-reference-picker`), so `ui/modal.ts`'s ONE guarded listener
 *    covers them;
 *  - the three former HANDOFF rows (`ui/modal.ts`, `ui/slide-over-panel.ts`,
 *    `screens/salon/salon-conversation.ts`) are ordinary rows;
 *  - `brahma/model-picker.ts` moved to {@link EXEMPT_SITES} (it lives INSIDE
 *    the floating layer; v4's `ModelPicker.tsx:29-42` is unguarded);
 *  - fourteen P4.D358 `(document:keydown.escape)` hosts added (R-4: guard
 *    every document-level dismiss site);
 *  - five `addEventListener` / `@HostListener` sites the old site regex could
 *    not see, each a v4 `useClickOutside` caller (guarded there).
 */
export const GUARDED: readonly GuardRow[] = [
  { file: 'ui/modal.ts', handler: 'onDocumentKeydown', kind: 'escape' },
  { file: 'ui/slide-over-panel.ts', handler: 'onDocumentKeydown', kind: 'keydown' }, // +
  { file: 'screens/salon/salon-conversation.ts', handler: 'onInspectorKeydown', kind: 'keydown' }, // +
  { file: 'ui/alert-modal.service.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'ui/tooltip.ts', handler: 'onPointerDown', kind: 'pointerdown' },
  { file: 'ui/tooltip.ts', handler: 'onKeyDown', kind: 'keydown' }, // +
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
  { file: 'chat/sidebar/chat-sidebar.ts', handler: 'onPointerDown', kind: 'pointerdown' },
  { file: 'chat/sidebar/chat-sidebar.ts', handler: 'onKeyDown', kind: 'keydown' }, // +
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
  { file: 'search/search-bar.ts', handler: 'onGlobalKeydown', kind: 'keydown' },
  { file: 'search/search-bar.ts', handler: 'onDocumentMouseDown', kind: 'mousedown' },
  { file: 'search/search-dialog.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'screens/photos/photos-page.ts', handler: 'onEscape', kind: 'escape' },
  {
    file: 'editor/char-insert/char-picker-panel.ts',
    handler: 'onDocumentMouseDown',
    kind: 'mousedown',
  },
  { file: 'editor/char-insert/char-picker-panel.ts', handler: 'onDocumentEscape', kind: 'escape' },
  // P4.D359's own name-chip card (a pinned card closes on a click elsewhere / Escape).
  { file: 'chat/absent-mentions/mention-chip-host.ts', handler: 'onDocumentPointerDown', kind: 'pointerdown' },
  { file: 'chat/absent-mentions/mention-chip-host.ts', handler: 'onDocumentKeyDown', kind: 'keydown' },
  // The fourteen P4.D358 `(document:keydown.escape)` hosts (union, R-4).
  { file: 'chat/bulk-character-replace-modal.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'chat/memory-cascade-dialog.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'chat/reattribute-message-dialog.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'chat/cast/create-npc-dialog.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'chat/cast/add-character-dialog.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'chat/merge-conversation-modal.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'chat/post-office/whisper-dialog.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'help/help-dialog.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'memory/maintenance-dialog.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'memory/memory-digest-viewer.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'progressions/progression-editor-modal.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'screens/custom-tools/destination-picker.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'screens/profile/avatar-picker.ts', handler: 'onEscape', kind: 'escape' },
  { file: 'wardrobe/item-editor/wardrobe-mode-change-prompt.ts', handler: 'onEscape', kind: 'escape' },
  // `addEventListener` / `@HostListener` forms the old site regex missed (union).
  // v4 `wardrobe-item-row.tsx:162-171` — `useClickOutside`, `escapeCapture`.
  { file: 'wardrobe/wardrobe-item-row.ts', handler: 'onDocumentMousedown', kind: 'mousedown' },
  { file: 'wardrobe/wardrobe-item-row.ts', handler: 'keydown', kind: 'escape' },
  // v4 `inventory-item-row.tsx:87` — `useClickOutside`, `escapeCapture`.
  { file: 'inventory/inventory-item-row.ts', handler: 'mousedown', kind: 'mousedown' },
  { file: 'inventory/inventory-item-row.ts', handler: 'keydown', kind: 'escape' },
  // v4 `ScenarioBrowserDialog.tsx:243` — a `BaseModal` (its Escape is `useClickOutside`'s).
  { file: 'scenario/scenario-browser-dialog.ts', handler: 'onEscape', kind: 'escape' },
  // v4 `SpeakerSelector.tsx:51` / `settings/model-selector.tsx:106` — `useClickOutside`.
  { file: 'chat/speaker-selector.ts', handler: 'onDocumentClick', kind: 'click' },
  { file: 'ui/model-selector.ts', handler: 'onDocumentClick', kind: 'click' },
  // Window / `@HostListener` keydown sites v4 does NOT route through
  // `useClickOutside` (`DiagramLightbox.tsx:144-183`, `FilePreviewActions.tsx:
  // 26-48`), guarded by R-4 as the census's other `// +` keydown rows are.
  { file: 'chat/diagrams/diagram-lightbox.ts', handler: 'onKey', kind: 'keydown' }, // +
  { file: 'screens/files/file-preview-modal.ts', handler: 'onKeydown', kind: 'keydown' }, // +
  // The wardrobe item editor's Escape, NEW at union (H-out-5; v4 `useEscapeKey`, `:196`).
  { file: 'wardrobe/wardrobe-item-editor.ts', handler: 'onEscape', kind: 'escape' },
];

/**
 * Document-level listeners that are NOT dismissals of a surface the console
 * could sit over — each by name, with the reason.
 */
const EXEMPT_SITES: readonly string[] = [
  // The floating layer's own listeners (it IS the layer).
  'ui/floating-dialog.ts',
  // Lives INSIDE the floating layer (the Brahma Console's header). v4
  // `brahma-console/ModelPicker.tsx:29-42` is unguarded — a guard would keep
  // its dropdown open on any click or Escape elsewhere in the console.
  'brahma/model-picker.ts',
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

  it('carries 61 rows: the 48 of the lane (36 census + 7 keydown misses + 2 chip-card + 3 HANDOFF), less the 9 P4.D358 deleted and model-picker’s 2 (exempt), + 14 P4.D358 hosts + the 9 sites the widened regex found + the wardrobe item editor’s new Escape', () => {
    expect(GUARDED).toHaveLength(48 - 9 - 2 + 14 + 9 + 1);
  });

  for (const row of GUARDED) {
    it(`${row.file} — ${row.handler} (${row.kind})`, () => {
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
    // Every spelling of a document- or window-level dismiss listener: a host
    // binding, `@HostListener`, or `addEventListener` on `document` / an
    // injected `doc` / `window` / a `win?.` alias.
    const site = new RegExp(
      [
        String.raw`\b(?:document|doc|window|win)\??\.addEventListener\(\s*'(?:mousedown|pointerdown|click|keydown)'`,
        String.raw`'\((?:document|window):(?:mousedown|click|pointerdown|keydown(?:\.escape)?)\)'`,
        String.raw`@HostListener\(\s*'(?:document|window):(?:mousedown|click|pointerdown|keydown(?:\.escape)?)'`,
      ].join('|'),
    );
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

  const CHARACTERS = [
    { participantId: 'p-a', name: 'Ada' },
    { participantId: 'p-b', name: 'Bea' },
  ];

  function openSelector() {
    TestBed.configureTestingModule({ imports: [SpeakerSelector] });
    const fixture = TestBed.createComponent(SpeakerSelector);
    fixture.componentRef.setInput('characters', CHARACTERS);
    fixture.componentRef.setInput('activeParticipantId', 'p-a');
    document.body.appendChild(fixture.nativeElement);
    fixture.detectChanges();
    (fixture.nativeElement.querySelector('button') as HTMLButtonElement).click();
    fixture.detectChanges();
    const isOpen = () => fixture.nativeElement.querySelector('[role="listbox"]') !== null;
    expect(isOpen()).toBe(true);
    return { fixture, isOpen };
  }

  it('does not treat a click inside the floating layer as outside (speaker selector)', () => {
    const { fixture, isOpen } = openSelector();
    const { el, inner } = layer();
    cleanup = () => {
      el.remove();
      (fixture.nativeElement as HTMLElement).remove();
    };
    inner.click();
    fixture.detectChanges();
    expect(isOpen()).toBe(true);
    // …while a click anywhere else still dismisses.
    document.body.click();
    fixture.detectChanges();
    expect(isOpen()).toBe(false);
  });

  it('ignores Escape pressed inside the floating layer; still handles Escape elsewhere (qt-modal)', () => {
    @Component({
      changeDetection: ChangeDetectionStrategy.OnPush,
      imports: [Modal],
      template: `<qt-modal title="Rename Chat" (close)="closes = closes + 1">body</qt-modal>`,
    })
    class Host {
      closes = 0;
    }
    TestBed.configureTestingModule({ imports: [Host] });
    const fixture = TestBed.createComponent(Host);
    fixture.detectChanges();
    const { el, inner } = layer();
    cleanup = () => el.remove();
    inner.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(fixture.componentInstance.closes).toBe(0);
    document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(fixture.componentInstance.closes).toBe(1);
  });

  it('the exemption: the console’s own model picker closes on a press or Escape elsewhere IN the console (v4 ModelPicker.tsx:29-42, unguarded)', () => {
    TestBed.configureTestingModule({ imports: [ModelPicker] });
    const fixture = TestBed.createComponent(ModelPicker);
    fixture.componentRef.setInput('profiles', PROFILES);
    fixture.componentRef.setInput('activeId', null);
    const { el, inner } = layer();
    el.appendChild(fixture.nativeElement);
    cleanup = () => el.remove();
    fixture.detectChanges();
    const isOpen = () => fixture.nativeElement.querySelector('[role="listbox"]') !== null;
    const open = () => {
      (fixture.nativeElement.querySelector('button') as HTMLButtonElement).click();
      fixture.detectChanges();
      expect(isOpen()).toBe(true);
    };
    open();
    inner.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    fixture.detectChanges();
    expect(isOpen()).toBe(false);
    open();
    inner.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
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
