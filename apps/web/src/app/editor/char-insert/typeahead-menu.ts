/**
 * The typeahead menu surface — v5's answer to v4's `MenuPortal` +
 * `useTypeaheadShell`, re-derived for ProseMirror.
 *
 * v4 gets its anchor for free: `LexicalTypeaheadMenuPlugin` mounts a positioned
 * element at the caret and `MenuPortal` renders into it. ProseMirror has no such
 * machinery, so this module owns BOTH halves — it creates the caret anchor from
 * `view.coordsAtPos` and renders the listbox into it — while keeping v4's DOM
 * shape, its `qt-typeahead-*` classes, its ARIA wiring and its flip/align rules
 * exactly. It renders plain DOM rather than an Angular component on purpose:
 * the row list changes on every keystroke, and the menu must be able to appear
 * over any host (composer, Document Mode, a form field) without that host
 * knowing about it.
 *
 * Dismissal and key handling belong to the plugin, not here.
 *
 * @module editor/char-insert/typeahead-menu
 */

/** One rendered row (v4 `typeahead/types.ts` `TypeaheadRow`). */
export interface TypeaheadRow {
  /** Stable identity — the inserted character. */
  key: string;
  /** Leading glyph: the character itself. */
  glyph?: string;
  /** Primary label. This is the row's accessible name. */
  label: string;
  /** Muted trailing detail, e.g. `:shortcode:` or `\to`. */
  detail?: string;
}

/** Row `id`s are derived, not stored, so the plugin can point at one by index. */
export function typeaheadOptionId(listboxId: string, index: number): string {
  return `${listboxId}-option-${index}`;
}

/** The caret box and viewport the placement is resolved against. */
export interface PlacementInput {
  anchorTop: number;
  anchorBottom: number;
  anchorLeft: number;
  menuHeight: number;
  menuWidth: number;
  viewportHeight: number;
  viewportWidth: number;
}

export interface Placement {
  placement: 'above' | 'below';
  align: 'left' | 'right';
}

/**
 * Where the menu goes — v4 `MenuPortal`'s layout effect, extracted as a pure
 * function so the geometry is spec-provable without a layout engine (jsdom
 * reports every box as zero, which would make an in-DOM assertion vacuous).
 *
 * Flipping is not a nicety: the Salon composer sits at the BOTTOM of the window,
 * so a menu that only ever opens downward is clipped by the viewport every
 * single time — which is exactly what v4's first live run showed. Same story
 * horizontally: a caret near the right edge would push a left-aligned menu
 * off-screen.
 */
export function resolvePlacement(input: PlacementInput): Placement {
  const roomBelow = input.viewportHeight - input.anchorBottom;
  const flipUp = roomBelow < input.menuHeight && input.anchorTop > roomBelow;
  const overflowsRight = input.anchorLeft + input.menuWidth > input.viewportWidth;
  return {
    placement: flipUp ? 'above' : 'below',
    align: overflowsRight ? 'right' : 'left',
  };
}

export interface TypeaheadMenuOptions {
  /** `id` of the listbox, referenced by the editor's `aria-activedescendant`. */
  listboxId: string;
  /** Shown when the query matches nothing. */
  emptyLabel: string;
  /**
   * The contenteditable that keeps focus while the menu is open. Focus never
   * moves to the menu, so this element must advertise the active option itself.
   */
  activeDescendantTarget: HTMLElement;
  onSelect: (index: number) => void;
  onHighlight: (index: number) => void;
}

/** The caret box the menu hangs from, in viewport coordinates. */
export interface CaretBox {
  left: number;
  top: number;
  bottom: number;
}

/**
 * The menu's DOM lifetime. Created lazily on the first open and removed on
 * destroy, so an editor whose writer never types a trigger costs nothing.
 */
export class TypeaheadMenu {
  private anchor: HTMLDivElement | null = null;
  private menu: HTMLDivElement | null = null;
  /**
   * The keys currently in the DOM, so a selection-only render can update the
   * existing rows instead of replacing them — see {@link render}.
   */
  private renderedKeys: string[] | null = null;

  constructor(private readonly options: TypeaheadMenuOptions) {}

  get isOpen(): boolean {
    return this.anchor !== null;
  }

  /** The menu element, for specs and for the plugin's own measurements. */
  get element(): HTMLElement | null {
    return this.menu;
  }

  render(rows: TypeaheadRow[], selectedIndex: number, caret: CaretBox): void {
    const { anchor, menu } = this.ensureElements();

    // The anchor IS the caret: a zero-width box at the caret's own line box, so
    // the menu's `top: 100%` clears the line rather than covering it (v4's
    // anchor comes from Lexical and has the same shape).
    anchor.style.left = `${caret.left}px`;
    anchor.style.top = `${caret.top}px`;
    anchor.style.height = `${Math.max(caret.bottom - caret.top, 1)}px`;

    // Rows are updated IN PLACE whenever the key list is unchanged — the whole
    // point being that the option elements survive (dogfood finding #85).
    //
    // v4 renders these rows from React with `key={row.key}`, so moving the
    // highlight rewrites `aria-selected` and `className` on nodes that stay put.
    // v5's first cut rebuilt the list on every render, and `replaceChildren`
    // under a stationary mouse pointer makes Chromium re-evaluate hover and fire
    // `mouseenter` on the freshly-created row beneath it — which calls
    // `onHighlight` and drags the selection straight back to the hovered row.
    // Arrow keys then did nothing at all for any writer whose pointer happened
    // to rest over the menu (it opens at the caret, right where they just
    // clicked); with the pointer elsewhere the same build moved the highlight
    // perfectly, which is what made it look like an arrow-key bug rather than a
    // rendering one.
    //
    // Equal keys mean equal rows: the key IS the inserted character, and glyph /
    // label / detail are a pure function of the dataset entry behind it. So only
    // the selection-bearing attributes can differ, and only those are written.
    const keys = rows.map((row) => row.key);
    const reusable =
      rows.length > 0 &&
      this.renderedKeys !== null &&
      this.renderedKeys.length === keys.length &&
      menu.children.length === keys.length &&
      this.renderedKeys.every((key, index) => key === keys[index]);

    if (reusable) {
      for (let index = 0; index < keys.length; index += 1) {
        const option = menu.children[index] as HTMLElement;
        option.setAttribute('aria-selected', String(index === selectedIndex));
        option.className =
          index === selectedIndex
            ? 'qt-typeahead-option qt-typeahead-option-active'
            : 'qt-typeahead-option';
      }
    } else {
      menu.replaceChildren(...this.renderRows(rows, selectedIndex));
      this.renderedKeys = keys;
    }

    const anchorRect = anchor.getBoundingClientRect();
    const { placement, align } = resolvePlacement({
      anchorTop: anchorRect.top,
      anchorBottom: anchorRect.bottom,
      anchorLeft: anchorRect.left,
      menuHeight: menu.offsetHeight,
      menuWidth: menu.offsetWidth,
      viewportHeight: window.innerHeight,
      viewportWidth: window.innerWidth,
    });
    menu.setAttribute('data-placement', placement);
    menu.setAttribute('data-align', align);

    this.announce(rows, selectedIndex);
  }

  /** Close the menu and hand the editor's ARIA state back. */
  close(): void {
    if (!this.anchor) return;
    this.anchor.remove();
    this.anchor = null;
    this.menu = null;
    this.renderedKeys = null;
    this.clearAnnouncement();
  }

  destroy(): void {
    this.close();
  }

  private ensureElements(): { anchor: HTMLDivElement; menu: HTMLDivElement } {
    if (this.anchor && this.menu) return { anchor: this.anchor, menu: this.menu };

    const anchor = document.createElement('div');
    // Fixed, so the caret coordinates `coordsAtPos` reports (viewport space)
    // are usable directly and no ancestor's overflow can clip the menu.
    anchor.className = 'qt-typeahead-anchor';
    anchor.style.position = 'fixed';
    anchor.style.width = '0';
    anchor.style.zIndex = '50';

    const menu = document.createElement('div');
    menu.className = 'qt-typeahead-menu';
    menu.setAttribute('role', 'listbox');
    menu.id = this.options.listboxId;

    anchor.appendChild(menu);
    document.body.appendChild(anchor);

    this.anchor = anchor;
    this.menu = menu;
    return { anchor, menu };
  }

  private renderRows(rows: TypeaheadRow[], selectedIndex: number): HTMLElement[] {
    if (rows.length === 0) {
      const empty = document.createElement('div');
      empty.className = 'qt-typeahead-empty';
      empty.textContent = this.options.emptyLabel;
      return [empty];
    }

    return rows.map((row, index) => {
      const option = document.createElement('div');
      option.id = typeaheadOptionId(this.options.listboxId, index);
      option.setAttribute('role', 'option');
      option.setAttribute('aria-selected', String(index === selectedIndex));
      option.setAttribute('aria-label', row.label);
      option.className =
        index === selectedIndex
          ? 'qt-typeahead-option qt-typeahead-option-active'
          : 'qt-typeahead-option';

      // The editor keeps focus throughout, so this is a pointer affordance on a
      // listbox option rather than a focusable control — the keyboard path is
      // the plugin's own key handling.
      option.addEventListener('mousedown', (event) => {
        // Stop the composer losing its selection before we can act on it.
        event.preventDefault();
        this.options.onSelect(index);
      });
      option.addEventListener('mouseenter', () => this.options.onHighlight(index));

      if (row.glyph !== undefined) {
        const glyph = document.createElement('span');
        glyph.className = 'qt-typeahead-option-glyph';
        glyph.setAttribute('aria-hidden', 'true');
        glyph.textContent = row.glyph;
        option.appendChild(glyph);
      }

      const label = document.createElement('span');
      label.className = 'qt-typeahead-option-label';
      label.textContent = row.label;
      option.appendChild(label);

      if (row.detail !== undefined) {
        const detail = document.createElement('span');
        detail.className = 'qt-typeahead-option-detail';
        detail.textContent = row.detail;
        option.appendChild(detail);
      }

      return option;
    });
  }

  private announce(rows: TypeaheadRow[], selectedIndex: number): void {
    const target = this.options.activeDescendantTarget;
    const activeId =
      selectedIndex >= 0 && rows[selectedIndex]
        ? typeaheadOptionId(this.options.listboxId, selectedIndex)
        : null;

    target.setAttribute('aria-controls', this.options.listboxId);
    target.setAttribute('aria-expanded', 'true');
    if (activeId) target.setAttribute('aria-activedescendant', activeId);
    else target.removeAttribute('aria-activedescendant');
  }

  /**
   * The menu closing must not leave the composer permanently claiming an
   * expanded listbox that no longer exists (v4's unmount cleanup).
   */
  private clearAnnouncement(): void {
    const target = this.options.activeDescendantTarget;
    target.removeAttribute('aria-activedescendant');
    target.removeAttribute('aria-controls');
    target.removeAttribute('aria-expanded');
  }
}
