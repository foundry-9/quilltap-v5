import { NgTemplateOutlet } from '@angular/common';
import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
  untracked,
} from '@angular/core';

import { CoreClient } from '../core/core-client';
import { CoreDispatchError, type ResolvedInventoryItem } from '../core/core-contract';
import { RealtimeService } from '../core/realtime.service';
import { AlertModalService } from '../ui/alert-modal.service';
import { Modal } from '../ui/modal';
import { ToastService } from '../ui/toast.service';
import { onTabActivated } from '../workspace/workspace-contract';
import { CarryingNowPane } from './carrying-now-pane';
import { createChatInventoryData } from './chat-inventory-data';
import { InventoryBrowser, type InventoryBrowserEquip } from './inventory-browser';
import {
  GENERAL_INVENTORY_CONTAINER,
  GENERAL_INVENTORY_NAME,
  containerOfOrigin,
  decodeInventoryContainer,
  encodeInventoryContainer,
  type InventoryContainer,
} from './inventory-container';
import { InventoryDialogService } from './inventory-dialog.service';
import { InventoryFolderPickerDialog } from './inventory-folder-picker-dialog';
import { createInventoryFoldersStore } from './inventory-folders-data';
import { InventoryInstantiateDialog } from './inventory-instantiate-dialog';
import { createInventoryItemActions, type InventoryInstantiateRequest } from './inventory-item-actions';
import { InventoryItemEditor } from './inventory-item-editor';
import { createInventoryListData } from './inventory-list-data';
import { describeInventoryRefusal } from './inventory-refusal';
import { InventoryTransferDialog, catalogueGroups, type InventoryCatalogue } from './inventory-transfer-dialog';

/** Two containers name the same tier (by their encoded value — the select's own key). */
function sameContainer(a: InventoryContainer | null, b: InventoryContainer | null): boolean {
  return a === b || (!!a && !!b && encodeInventoryContainer(a) === encodeInventoryContainer(b));
}

/** v4 `:49` — the catalogue's sort (default `localeCompare`, no options). */
const byName = (a: { name: string }, b: { name: string }): number => a.name.localeCompare(b.name);

/**
 * The Inventory dialog's inner body — a port of v4
 * `components/inventory/inventory-control-dialog.tsx` (`322a5488e`)'s
 * `InventoryControlDialogInner`: Pascal the Croupier's table of things to
 * carry.
 *
 * A container select covers every tier an object can live in (a character's
 * vault, Quilltap General, a project, a group); below it the Inventory
 * browser. Opened from a chat with a character chosen, rows gain Equip /
 * Unequip and a *Carrying now* pane shows what that cast member carries there.
 * Out of a chat (the workspace tab, the Prospero card) it is browse-and-edit
 * only. With nothing chosen it falls back — once the character list has
 * settled — to the first character, else Quilltap General.
 *
 * ONE reload set (R-1 / R-2): every write refreshes the list, the folder tree
 * and the chat's carried read (v4's five-family `onSettled`); the `inventory`
 * realtime topic reloads the list and the tree, the `chats` topic (this chat)
 * the carried read; returning to the tab reloads all three.
 *
 * The chrome: as a tab (`asTab`) the body renders bare in `qt-inventory-tab`
 * (an unstyled hook, like `qt-wardrobe-tab`), or — `fixedContainer`, the
 * Prospero card — in a plain column with no select; otherwise the floating
 * dialog (title `Inventory`, `4xl`, footer `Done`). Backdrop and Escape stand
 * down while an overlay, the Take-up picker or a confirmation is up.
 *
 * **Recorded divergences.** `requestConfirmation` binds `window.confirm` (the
 * P4.D318 R-2 precedent); `requestAlert` is the labelled-button alert
 * (`AlertModalService`, R-W — the Tier-1 `confirm()` collapse is retired).
 * v4's three TanStack LIST reads are one-shot dispatches.
 */
@Component({
  selector: 'qt-inventory-control-dialog-inner',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    NgTemplateOutlet,
    Modal,
    InventoryBrowser,
    CarryingNowPane,
    InventoryItemEditor,
    InventoryFolderPickerDialog,
    InventoryTransferDialog,
    InventoryInstantiateDialog,
  ],
  host: { '(document:keydown.escape)': 'onEscape()' },
  template: `
    @if (asTab()) {
      <div [attr.class]="fixedContainer() ? 'flex flex-col min-h-0' : 'qt-inventory-tab flex flex-col h-full min-h-0 overflow-y-auto p-4'">
        <ng-container [ngTemplateOutlet]="body" />
        <ng-container [ngTemplateOutlet]="overlays" />
      </div>
    } @else {
      <qt-modal title="Inventory" maxWidth="4xl" [closeOnBackdrop]="!overlayOpen() && !confirming()" (close)="closed.emit()">
        <ng-container [ngTemplateOutlet]="body" />
        <div qt-modal-footer class="flex items-center justify-end gap-2 w-full">
          <button type="button" (click)="closed.emit()" class="qt-button-secondary qt-button-sm">Done</button>
        </div>
      </qt-modal>
      <ng-container [ngTemplateOutlet]="overlays" />
    }

    <ng-template #body>
      @if (!fixedContainer()) {
        <div class="flex flex-col gap-2 mb-3">
          <div class="flex items-center gap-2">
            <label for="inventory-container-select" class="text-sm qt-text-secondary">Inventory:</label>
            <!-- [selected] per option, not [value] on the select: the options
                 arrive after the value (the LIST reads), and a select bound by
                 value keeps whatever the browser picked meanwhile. -->
            <select
              id="inventory-container-select"
              class="qt-select flex-1 max-w-md"
              (change)="choose($any($event.target).value)"
            >
              @if (!container()) {
                <option value="" disabled [selected]="true">Select an inventory</option>
              }
              @for (group of selectGroups(); track group.label) {
                <optgroup [label]="group.label">
                  @for (option of group.options; track option.value) {
                    <option [value]="option.value" [selected]="option.value === containerValue()">{{ option.name }}</option>
                  }
                </optgroup>
              }
            </select>
          </div>
          @if (container(); as c) {
            @if (c.scope !== 'character') {
              <p class="qt-text-xs qt-text-secondary px-1">
                A shared inventory — every character who can reach this
                {{ c.scope === 'general' ? 'library' : c.scope }} can carry what is here, and its archetypes can be
                instantiated by any of them.
              </p>
            }
          }
        </div>
      }
      <div [attr.class]="'grid gap-4 ' + (inChat() ? 'md:grid-cols-2' : '')">
        <qt-inventory-browser
          [container]="container()"
          [data]="data"
          [actions]="actions"
          [folders]="folders"
          [showArchived]="showArchived()"
          (showArchivedChange)="showArchived.set($event)"
          [scrollList]="!fixedContainer()"
          [equip]="browserEquip()"
        />
        @if (inChat()) {
          <qt-carrying-now-pane
            [characterName]="characterName()"
            [carried]="carried()"
            [pool]="data.listItems()"
            [busy]="busy()"
            (equip)="gesture('equip', $event)"
            (unequip)="gesture('unequip', $event)"
            (reset)="reset()"
            [(picking)]="picking"
          />
        }
      </div>
    </ng-template>

    <ng-template #overlays>
      @if (actions.editor(); as editor) {
        @if (container()) {
          <qt-inventory-item-editor
            [container]="editorContainer()!"
            [containerLabel]="containerLabel()"
            [item]="editor.kind === 'edit' ? editor.item : null"
            [createMode]="editor.kind === 'create' ? editor.mode : 'item'"
            [initialFolder]="editor.kind === 'create' ? editor.folder : ''"
            [poolItems]="data.listItems()"
            [projectId]="effectiveProjectId()"
            [actions]="actions"
            (closed)="actions.closeEditor()"
            (saved)="actions.closeEditor()"
          />
        }
      }
      @if (actions.folderPicker(); as picker) {
        @if (container()) {
          <qt-inventory-folder-picker-dialog
            [container]="folderPickerContainer()!"
            [items]="picker.items"
            [actions]="actions"
            (closed)="actions.closeFolderPicker()"
          />
        }
      }
      @if (actions.transfer(); as transfer) {
        @if (container()) {
          <qt-inventory-transfer-dialog
            [item]="transfer.item"
            [mode]="transfer.mode"
            [from]="transferFrom()!"
            [catalogue]="catalogue()"
            [actions]="actions"
            (closed)="actions.closeTransfer()"
          />
        }
      }
      @if (actions.instantiateFor(); as request) {
        <qt-inventory-instantiate-dialog
          [archetype]="archetypeOf(request)"
          [destination]="instantiateDestination()"
          [projectId]="effectiveProjectId()"
          [catalogue]="catalogue()"
          [actions]="actions"
          (closed)="actions.closeInstantiate()"
        />
      }
    </ng-template>
  `,
})
export class InventoryControlDialogInner {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly alerts = inject(AlertModalService);

  readonly initialContainer = input<InventoryContainer | null>(null);
  readonly chatId = input<string | null>(null);
  /** The project tier for a character's pool (a chat's project, or the card's). */
  readonly projectId = input<string | null>(null);
  readonly asTab = input(false);
  /** Hide the container select (a card shows one tier). */
  readonly fixedContainer = input(false);
  readonly closed = output<void>();

  // ---- the catalogue (v4 `useInventoryCatalogue`, `:52-77`) ----
  protected readonly characters = signal<Array<{ id: string; name: string }>>([]);
  protected readonly charactersSettled = signal(false);
  private readonly projects = signal<Array<{ id: string; name: string }>>([]);
  private readonly groups = signal<Array<{ id: string; name: string }>>([]);
  protected readonly catalogue = computed<InventoryCatalogue>(() => ({
    characters: this.characters(),
    projects: this.projects(),
    groups: this.groups(),
  }));
  protected readonly selectGroups = computed(() => catalogueGroups(this.catalogue()));

  // ---- which tier (v4 `:146-152`) ----
  private readonly chosen = signal<InventoryContainer | null>(null);
  protected readonly container = computed<InventoryContainer | null>(() => {
    const chosen = this.chosen();
    if (chosen) return chosen;
    if (!this.charactersSettled()) return null;
    const first = this.characters()[0]?.id ?? null;
    return first ? { scope: 'character', id: first } : GENERAL_INVENTORY_CONTAINER;
  });
  protected readonly containerValue = computed(() => {
    const c = this.container();
    return c ? encodeInventoryContainer(c) : '';
  });
  private readonly characterId = computed(() => {
    const c = this.container();
    return c?.scope === 'character' ? c.id : null;
  });
  protected readonly showArchived = signal(false);

  // ---- the data (R-1 / R-2) ----
  protected readonly chat = createChatInventoryData(this.core, {
    chatId: this.chatId,
    reloadList: () => this.data.reload(),
  });
  /** A chat names its own project tier; otherwise the opener's (v4 `:156`). */
  protected readonly effectiveProjectId = computed(() => this.chat.projectId() ?? this.projectId());
  protected readonly data = createInventoryListData(this.core, {
    container: this.container,
    includeArchived: this.showArchived,
    projectId: this.effectiveProjectId,
  });
  protected readonly folders = createInventoryFoldersStore(this.core);

  protected readonly confirming = signal(false);
  protected readonly picking = signal(false);
  protected readonly busy = signal(false);

  protected readonly actions = createInventoryItemActions({
    core: this.core,
    toasts: this.toasts,
    container: this.container,
    refresh: () => this.refresh(),
    requestConfirmation: (message) => this.requestConfirmation(message),
    requestAlert: (message, buttons) => this.requestAlert(message, buttons),
  });

  protected readonly characterName = computed(
    () => this.characters().find((c) => c.id === this.characterId())?.name ?? 'this character',
  );
  protected readonly carried = computed(() => this.chat.carriedBy(this.characterId()));
  protected readonly inChat = computed(() => this.chatId() !== null && this.characterId() !== null);
  protected readonly overlayOpen = computed(() => this.actions.overlayOpen() || this.picking());

  /** v4 `:218-230`. */
  protected readonly containerLabel = computed(() => {
    const container = this.container();
    if (!container) return '';
    switch (container.scope) {
      case 'character':
        return `${this.characters().find((c) => c.id === container.id)?.name ?? 'this character'}'s inventory`;
      case 'general':
        return GENERAL_INVENTORY_NAME;
      case 'project':
        return this.projects().find((p) => p.id === container.id)?.name ?? 'this project';
      case 'group':
        return this.groups().find((g) => g.id === container.id)?.name ?? 'this group';
    }
  });

  /** The browser's Equip / Unequip, in a chat only — one object per carried read. */
  protected readonly browserEquip = computed<InventoryBrowserEquip | undefined>(() =>
    this.inChat()
      ? {
          carried: this.carried(),
          onEquip: (item) => void this.gesture('equip', [item.id]),
          onUnequip: (item) => void this.gesture('unequip', [item.id]),
        }
      : undefined,
  );

  constructor() {
    // v4 `useState(initialContainer)` — seeded once from the mount's input.
    effect(() => {
      const initial = this.initialContainer();
      untracked(() => this.chosen.set(initial));
    });
    effect(() => {
      const container = this.container();
      untracked(() => void this.folders.load(container));
    });
    void this.loadCatalogue();
    // v4 `useOnTabActivated(() => invalidate(inventory.all))` (trap 9).
    onTabActivated(() => {
      void this.refresh();
    });
    const realtime = inject(RealtimeService);
    // R-2: the lists and trees are signal-held, so the dialog subscribes itself.
    realtime.onTopic('inventory', () => {
      void this.data.reload();
      void this.folders.reload();
    });
    realtime.onTopic('chats', () => void this.chat.reload(), () => this.chatId());
  }

  /** Every read a write may have changed (v4's mutation `onSettled`). */
  private async refresh(): Promise<void> {
    await Promise.all([this.data.reload(), this.folders.reload(), this.chat.reload()]);
  }

  /** v4 `:52-77` — three LIST reads, each failing soft; characters settle the fallback. */
  private async loadCatalogue(): Promise<void> {
    const [characters, projects, groups] = await Promise.all([
      this.core.dispatchData({ type: 'characterList' }).catch(() => null),
      this.core.dispatchData({ type: 'projectList' }).catch(() => null),
      this.core.dispatchData({ type: 'groupList' }).catch(() => null),
    ]);
    const list = (data: Record<string, unknown> | null, key: string) =>
      ((data?.[key] as Array<{ id: string; name: string }> | undefined) ?? []);
    this.characters.set([...list(characters, 'characters')].map((c) => ({ id: c.id, name: c.name })).sort(byName));
    this.projects.set(list(projects, 'projects').map((p) => ({ id: p.id, name: p.name || 'Untitled project' })).sort(byName));
    this.groups.set(list(groups, 'groups').map((g) => ({ id: g.id, name: g.name || 'Untitled group' })).sort(byName));
    this.charactersSettled.set(true);
  }

  protected choose(value: string): void {
    this.chosen.set(decodeInventoryContainer(value));
  }

  /** v4 `:158-165`. */
  private async requestConfirmation(message: string): Promise<boolean> {
    this.confirming.set(true);
    try {
      return await Promise.resolve(window.confirm(message));
    } finally {
      this.confirming.set(false);
    }
  }

  /** v4 `:166-173`. */
  private async requestAlert(message: string, buttons?: string[]): Promise<string | null> {
    this.confirming.set(true);
    try {
      return await this.alerts.show(message, buttons);
    } finally {
      this.confirming.set(false);
    }
  }

  /** v4 `:356-358` — the instantiate dialog's two ways in. */
  protected archetypeOf(request: InventoryInstantiateRequest): ResolvedInventoryItem | null {
    return 'archetype' in request ? request.archetype : null;
  }

  /*
   * The overlays' container inputs are COMPUTED, not template method calls: a
   * call builds a NEW object every change detection, and each child keys an
   * effect on its input — the instantiate dialog re-seeded `chosen = null` and
   * refetched its archetypes on every click (so no archetype could ever be
   * chosen; v4 seeds once with `useState`), and the editor refetched its
   * General list, its pictures and the folder tree on every keystroke. A
   * computed recomputes only when its sources move, and `sameContainer` keeps
   * the old object when an unrelated source (a list reload) re-runs it.
   */

  /** v4 `:356-358` — the destination the instantiate dialog is opened INTO. */
  protected readonly instantiateDestination = computed(
    (): { container: InventoryContainer; folder: string } | null => {
      const request = this.actions.instantiateFor();
      return request && 'destination' in request ? { container: request.destination, folder: request.folder } : null;
    },
    {
      equal: (a, b) =>
        a === b || (!!a && !!b && a.folder === b.folder && sameContainer(a.container, b.container)),
    },
  );

  /** v4 `:180-181` — an object's own tier, for the editor. */
  protected readonly editorContainer = computed(
    (): InventoryContainer | null => {
      const editor = this.actions.editor();
      const item = editor?.kind === 'edit' ? editor.item : null;
      const container = this.container();
      return item && container ? containerOfOrigin(item.origin, container) : container;
    },
    { equal: sameContainer },
  );

  /** The folder picker's tier — its first object's own (v4 `addressOf`). */
  protected readonly folderPickerContainer = computed(
    (): InventoryContainer | null => {
      const picker = this.actions.folderPicker();
      return picker && picker.items.length > 0 ? this.actions.addressOf(picker.items[0]) : null;
    },
    { equal: sameContainer },
  );

  /** The transfer's source tier (v4 `addressOf`). */
  protected readonly transferFrom = computed(
    (): InventoryContainer | null => {
      const transfer = this.actions.transfer();
      return transfer ? this.actions.addressOf(transfer.item) : null;
    },
    { equal: sameContainer },
  );

  /** v4 `gesture` (`:187-205`). */
  protected async gesture(kind: 'equip' | 'unequip', ids: string[]): Promise<void> {
    const characterId = this.characterId();
    if (!characterId) return;
    this.busy.set(true);
    try {
      const result = (kind === 'equip'
        ? await this.chat.equip(characterId, ids)
        : await this.chat.unequip(characterId, ids)) as { skipped?: Array<{ reason: string }> };
      if (result.skipped && result.skipped.length > 0) {
        this.toasts.showSuccess(
          `Done — ${result.skipped.length} kit member${result.skipped.length === 1 ? '' : 's'} could not be carried and stayed behind`,
        );
      }
    } catch (error) {
      const message = describeInventoryRefusal(error, `The object would not ${kind}`);
      if (error instanceof CoreDispatchError && error.code === 'CAP_EXCEEDED') await this.requestAlert(message, ['Understood']);
      else this.toasts.showError(message);
    } finally {
      this.busy.set(false);
    }
  }

  /** v4 `reset` (`:207-216`). */
  protected async reset(): Promise<void> {
    const characterId = this.characterId();
    if (!characterId) return;
    const name = this.characterName();
    if (
      !(await this.requestConfirmation(
        `Put back everything ${name} carries here and start again from their equipped-by-default objects?`,
      ))
    ) {
      return;
    }
    this.busy.set(true);
    try {
      await this.chat.resetToDefaults(characterId);
      this.toasts.showSuccess(`${name} is carrying their defaults again`);
    } catch (error) {
      this.toasts.showError(describeInventoryRefusal(error, 'The inventory would not reset'));
    } finally {
      this.busy.set(false);
    }
  }

  /** v4 `BaseModal` `closeOnEscape={!overlayOpen && !confirming}`; a tab never closes. */
  protected onEscape(): void {
    if (this.asTab() || this.overlayOpen() || this.confirming()) return;
    this.closed.emit();
  }
}

/**
 * The shell-mounted host (v4 `InventoryControlDialog`, `:80-101`): renders
 * nothing while closed; the keyed `@for` reproduces v4's inner `key` (the
 * four-part `remountKey`), so a new context REMOUNTS the inner. A chat-opened
 * dialog never starts on the project tier (`{ projectId, chatId }` → null —
 * trap 13); the project only names the pool's project tier.
 */
@Component({
  selector: 'qt-inventory-control-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [InventoryControlDialogInner],
  template: `
    @if (dialog.isOpen()) {
      @for (key of [dialog.remountKey()]; track key) {
        <qt-inventory-control-dialog-inner
          [initialContainer]="initialContainer()"
          [chatId]="dialog.context()?.chatId ?? null"
          [projectId]="dialog.context()?.projectId ?? null"
          (closed)="dialog.close()"
        />
      }
    }
  `,
})
export class InventoryControlDialog {
  protected readonly dialog = inject(InventoryDialogService);

  /** v4 `:88-95`. */
  protected readonly initialContainer = computed<InventoryContainer | null>(() => {
    const ctx = this.dialog.context();
    if (ctx?.characterId) return { scope: 'character', id: ctx.characterId };
    if (ctx?.groupId) return { scope: 'group', id: ctx.groupId };
    if (ctx?.projectId && !ctx.chatId) return { scope: 'project', id: ctx.projectId };
    return null;
  });
}

/**
 * The Inventory as a workspace tab (v4 `InventoryView`, `:104-116`): browse
 * and edit, no chat. Starts on the payload's character, else its project,
 * else the fallback. `projectId` is DEAD at the pin — no caller sets it
 * (R-9); it ports as type + binding only.
 */
@Component({
  selector: 'qt-inventory-tab-view',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [InventoryControlDialogInner],
  template: `
    <qt-inventory-control-dialog-inner
      [asTab]="true"
      [initialContainer]="initialContainer()"
      [chatId]="null"
      [projectId]="projectId() ?? null"
    />
  `,
})
export class InventoryTabView {
  readonly characterId = input<string | undefined>(undefined);
  readonly projectId = input<string | undefined>(undefined);

  protected readonly initialContainer = computed<InventoryContainer | null>(() => {
    const characterId = this.characterId();
    const projectId = this.projectId();
    if (characterId) return { scope: 'character', id: characterId };
    return projectId ? { scope: 'project', id: projectId } : null;
  });
}

/** The Inventory pinned to one tier, for a card — Prospero's project page (v4 `InventoryPanel`, `:119-130`). */
@Component({
  selector: 'qt-inventory-panel',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [InventoryControlDialogInner],
  template: `
    <qt-inventory-control-dialog-inner
      [asTab]="true"
      [fixedContainer]="true"
      [initialContainer]="container()"
      [chatId]="null"
      [projectId]="projectId() ?? null"
    />
  `,
})
export class InventoryPanel {
  readonly container = input.required<InventoryContainer>();
  readonly projectId = input<string | null | undefined>(undefined);
}
