import {
  ChangeDetectionStrategy,
  Component,
  computed,
  input,
  OnInit,
  output,
  signal,
} from '@angular/core';

import type { SubpromptRecord } from '../core/core-contract';
import { Icon } from '../ui/icon';
import { SubpromptEditorModal, type SubpromptSaved } from './subprompt-editor-modal';
import { injectCharacterSubprompts } from './subprompts.api';

/** One `aria-controls` target per instance — v4's `useId()`. */
let nextPickerId = 0;

/**
 * `qt-subprompt-picker` — the "Subprompts" dropdown that sits under a
 * character's system-prompt selector (the New Chat dialog, the Salon
 * Participants drawer). v4 `components/subprompts/SubpromptPicker.tsx` at
 * `2f4254b42`.
 *
 * A disclosure button summarising how many are in play, expanding to one
 * checkbox per subprompt in the character's vault, plus a "New subprompt…"
 * action that opens the editor right there. A freshly created subprompt is
 * ticked on automatically.
 *
 * The list expands INLINE rather than floating, so it works the same inside a
 * scrolling sidebar as it does in a modal — nothing to clip or z-fight (v4's
 * own reasoning, and the reason no portal is involved here either).
 *
 * Controlled: the caller owns `selectedIds` and hears every change through
 * `selectionChange` with the FULL next set (v4's `onChange(nextIds)`).
 */
@Component({
  selector: 'qt-subprompt-picker',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, SubpromptEditorModal],
  // v4's outer element is a plain `<div class="subprompt-picker">`; an Angular
  // custom-element host defaults to `display: inline`, which would collapse the
  // button's width inside a flex/grid parent (dogfood #97 / #107's class).
  host: { class: 'subprompt-picker block' },
  template: `
    <button
      type="button"
      [class]="buttonClass()"
      [disabled]="disabled()"
      [attr.aria-expanded]="open()"
      [attr.aria-controls]="listId"
      title="Subprompts in play for this chat"
      (click)="open.set(!open())"
    >
      <span class="truncate">
        <span [class]="small() ? '' : 'font-medium'">Subprompts</span>
        <span class="qt-text-secondary"> · {{ summary() }}</span>
      </span>
      <qt-icon
        [name]="open() ? 'chevron-down' : 'chevron-right'"
        class="w-3.5 h-3.5 flex-shrink-0"
      />
    </button>

    @if (open()) {
      <div [id]="listId" [class]="panelClass()">
        @if (subprompts().length === 0 && !isLoading()) {
          <p class="text-xs qt-text-secondary px-1 py-0.5">
            {{ characterName() ?? 'This character' }} has no subprompts yet.
          </p>
        }
        @for (s of subprompts(); track s.id) {
          <label [class]="rowClass()" [title]="s.content.slice(0, 200)">
            <input
              type="checkbox"
              class="qt-checkbox mt-0.5"
              [checked]="selected().has(s.id)"
              [disabled]="disabled()"
              [attr.aria-label]="'Subprompt ' + s.title"
              (change)="toggle(s.id, $any($event.target).checked)"
            />
            <span class="text-sm text-foreground min-w-0 truncate">{{ s.title }}</span>
          </label>
        }
        <!-- Ids ticked on that no longer match a file — still shown so they can
             be unticked (v4 :121-137). -->
        @for (id of missingIds(); track id) {
          <label
            class="flex items-start gap-2 px-1 py-0.5 rounded cursor-pointer"
            title="This subprompt no longer exists in the vault"
          >
            <input
              type="checkbox"
              class="qt-checkbox mt-0.5"
              checked
              [disabled]="disabled()"
              [attr.aria-label]="'Missing subprompt ' + id"
              (change)="toggle(id, false)"
            />
            <span class="text-sm qt-text-secondary line-through min-w-0 truncate">{{ id }}</span>
          </label>
        }
        <button
          type="button"
          class="qt-button-ghost qt-button-sm w-full flex items-center justify-start gap-1.5 mt-1"
          [disabled]="disabled()"
          (click)="editorOpen.set(true)"
        >
          <qt-icon name="plus" class="w-3.5 h-3.5" />
          New subprompt…
        </button>
      </div>
    }

    <qt-subprompt-editor-modal
      [open]="editorOpen()"
      [characterId]="characterId()"
      [characterName]="characterName()"
      (close)="editorOpen.set(false)"
      (saved)="onSaved($event)"
    />
  `,
})
export class SubpromptPicker implements OnInit {
  readonly characterId = input.required<string>();
  readonly characterName = input<string | undefined>(undefined);
  readonly selectedIds = input<readonly string[]>([]);
  readonly disabled = input(false);
  /** Compact styling for the sidebar card (v4 `size`). */
  readonly size = input<'sm' | 'md'>('md');
  /** Start expanded (v4 `defaultOpen` — the New Chat card, where there is room). */
  readonly defaultOpen = input(false);
  /** The FULL next set, v4's `onChange(nextIds)`. */
  readonly selectionChange = output<string[]>();

  protected readonly open = signal(false);
  protected readonly editorOpen = signal(false);
  /** v4's `useId()` — one per instance, so several pickers never collide. */
  protected readonly listId = `subprompt-list-${nextPickerId++}`;

  /**
   * v4 `enabled: open || selectedIds.length > 0` — a collapsed picker with
   * nothing ticked reads nothing, but one carrying a selection must read so the
   * summary can count and the missing-id rows can be told apart from unloaded
   * ones.
   */
  private readonly api = injectCharacterSubprompts(() => this.characterId(), {
    enabled: () => this.open() || this.selectedIds().length > 0,
  });

  protected readonly subprompts = computed(() => this.api.subprompts());
  protected readonly isLoading = computed(() => this.api.isLoading());
  protected readonly selected = computed(() => new Set(this.selectedIds()));
  protected readonly small = computed(() => this.size() === 'sm');

  private readonly known = computed(
    () => new Set(this.subprompts().map((s: SubpromptRecord) => s.id)),
  );
  private readonly activeCount = computed(
    () => this.subprompts().filter((s: SubpromptRecord) => this.selected().has(s.id)).length,
  );

  /**
   * The missing rows appear ONLY once the list has answered — while it is
   * loading, every ticked id is "unknown" and would flash as a dead one.
   */
  protected readonly missingIds = computed(() =>
    this.isLoading() ? [] : this.selectedIds().filter((id) => !this.known().has(id)),
  );

  /** v4's three-armed summary, in v4's order (`:65-70`). */
  protected readonly summary = computed(() => {
    if (this.subprompts().length === 0 && !this.isLoading()) return 'None on file';
    if (this.isLoading() && this.subprompts().length === 0) return 'Loading…';
    return `${this.activeCount()} of ${this.subprompts().length} in play`;
  });

  protected readonly buttonClass = computed(() =>
    this.small()
      ? 'qt-select qt-select-sm w-full flex items-center justify-between gap-2 text-left'
      : 'w-full flex items-center justify-between gap-2 rounded-lg border qt-border-default bg-background px-3 py-1.5 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring text-left',
  );

  protected readonly panelClass = computed(
    () =>
      `mt-1 rounded-lg border qt-border-default qt-bg-card ${this.small() ? 'p-1.5' : 'p-2'} space-y-1`,
  );

  protected readonly rowClass = computed(
    () =>
      `flex items-start gap-2 px-1 py-0.5 rounded cursor-pointer ${
        this.disabled() ? 'opacity-60 cursor-not-allowed' : ''
      }`,
  );

  /** v4 seeds `useState(defaultOpen)` at mount; inputs are bound by `ngOnInit`. */
  ngOnInit(): void {
    this.open.set(this.defaultOpen());
  }

  /** v4 `toggle` (`:53-57`) — a no-op when the box is already where it is asked to be. */
  protected toggle(id: string, on: boolean): void {
    if (on === this.selected().has(id)) return;
    const next = on ? [...this.selectedIds(), id] : this.selectedIds().filter((x) => x !== id);
    this.selectionChange.emit(next);
  }

  /** v4 `handleCreated` (`:59-63`) — a NEW subprompt is ticked on for this chat. */
  protected onSaved(event: SubpromptSaved): void {
    if (event.mode === 'created' && !this.selected().has(event.subprompt.id)) {
      this.selectionChange.emit([...this.selectedIds(), event.subprompt.id]);
    }
  }
}
