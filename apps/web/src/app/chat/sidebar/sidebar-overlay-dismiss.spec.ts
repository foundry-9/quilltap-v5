/**
 * Bug 169: a click inside a body-portaled dialog must not collapse the
 * narrow-pane sidebar overlay that opened it — v4's own seven vectors
 * (`__tests__/unit/components/chat/sidebar-overlay-dismiss.test.ts` at
 * `b0b6656b5`), transcribed case for case.
 */
import { shouldDismissSidebarOverlay } from './sidebar-overlay-dismiss';

describe('shouldDismissSidebarOverlay (v4 b0b6656b5)', () => {
  let panel: HTMLDivElement;
  let inside: HTMLButtonElement;
  let dialogOverlay: HTMLDivElement;
  let dialogInput: HTMLInputElement;
  let elsewhere: HTMLDivElement;

  beforeEach(() => {
    document.body.innerHTML = '';
    panel = document.createElement('div');
    inside = document.createElement('button');
    panel.appendChild(inside);
    document.body.appendChild(panel);

    // The dialog portals to <body>, a sibling of the sidebar rather than a child.
    dialogOverlay = document.createElement('div');
    dialogOverlay.className = 'qt-dialog-overlay';
    const dialog = document.createElement('div');
    dialog.className = 'qt-dialog';
    dialogInput = document.createElement('input');
    dialog.appendChild(dialogInput);
    dialogOverlay.appendChild(dialog);
    document.body.appendChild(dialogOverlay);

    elsewhere = document.createElement('div');
    document.body.appendChild(elsewhere);
  });

  afterEach(() => {
    document.body.innerHTML = '';
  });

  it('keeps the overlay open for a click inside the panel', () => {
    expect(shouldDismissSidebarOverlay(panel, inside)).toBe(false);
  });

  it('keeps the overlay open for a click inside a body-portaled dialog', () => {
    expect(shouldDismissSidebarOverlay(panel, dialogInput)).toBe(false);
  });

  it('keeps the overlay open for a click on the dialog backdrop', () => {
    expect(shouldDismissSidebarOverlay(panel, dialogOverlay)).toBe(false);
  });

  it('keeps the overlay open for a text node inside a dialog', () => {
    const text = document.createTextNode('Where');
    dialogInput.parentElement!.appendChild(text);
    expect(shouldDismissSidebarOverlay(panel, text)).toBe(false);
  });

  it('collapses the overlay for a click elsewhere on the page', () => {
    expect(shouldDismissSidebarOverlay(panel, elsewhere)).toBe(true);
  });

  it('does nothing without a mounted panel or a node target', () => {
    expect(shouldDismissSidebarOverlay(null, elsewhere)).toBe(false);
    expect(shouldDismissSidebarOverlay(panel, null)).toBe(false);
  });
});
