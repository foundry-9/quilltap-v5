import { afterEach, describe, expect, it, vi } from 'vitest';

import type { CoreClient } from '../core/core-client';
import { CoreDispatchError, type CoreRequest } from '../core/core-contract';
import type { InventoryContainer } from './inventory-container';
import {
  confirmCollapseAlert,
  createInventoryItemActions,
  describeInventoryRefusal,
  type InventoryItemActionsOptions,
} from './inventory-item-actions';
import { BOW, KIT, LAMP, PROJECT, obj } from './__fixtures__/inventory-ui.fixture';

/**
 * v4 `__tests__/unit/components/inventory/inventory-ui.test.tsx` "the
 * warnings" (cases 4–8) NAME FOR NAME, each refusal driven through the
 * `CoreDispatchError` the transports throw (`code` + `details`, R-L); plus the
 * overlay truth table the dialog's Escape gate reads, `addressOf`, the row
 * writes' toasts, and R-4's `confirm()` collapse binding.
 */
afterEach(() => vi.restoreAllMocks());

const QUEST: InventoryContainer = { scope: 'project', id: 'p-1' };

const refusal = (code: string, details: Record<string, unknown>, message = 'refused') =>
  new CoreDispatchError({ kind: 'conflict', message, code, details });

type Answer = (req: CoreRequest, n: number) => unknown;

function harness(answer: Answer = () => ({}), over: Partial<InventoryItemActionsOptions> = {}) {
  const seen: CoreRequest[] = [];
  const core = {
    dispatchData: vi.fn(async (req: CoreRequest) => {
      seen.push(req);
      const out = answer(req, seen.length);
      if (out instanceof Error) throw out;
      return out as Record<string, unknown>;
    }),
  } as unknown as CoreClient;
  const toasts = { showError: vi.fn(), showSuccess: vi.fn() };
  const refresh = vi.fn(async () => undefined);
  const requestConfirmation = vi.fn(async (_message: string) => true);
  const requestAlert = vi.fn(
    async (_message: string, _buttons?: string[]): Promise<string | null> => 'Detach all and delete',
  );
  const actions = createInventoryItemActions({
    core,
    toasts,
    container: () => QUEST,
    refresh,
    requestConfirmation,
    requestAlert,
    ...over,
  });
  return { actions, seen, toasts, refresh, requestConfirmation, requestAlert };
}

describe('the warnings (v4 inventory-ui.test.tsx)', () => {
  it('detach warns that later archetype edits will not reach it', async () => {
    const { actions, seen, requestConfirmation, toasts } = harness(() => ({ item: {} }));
    await actions.detach(obj('i-1', { parentId: 'bow', parentName: 'Magic bow' }));
    expect(requestConfirmation).toHaveBeenCalledWith(
      'This object will stop following "Magic bow". Its current values are kept; later edits to the archetype will not reach it.',
    );
    expect(seen).toEqual([{ type: 'inventoryDetach', container: { scope: 'character', id: 'c-viv' }, itemId: 'i-1' }]);
    expect(toasts.showSuccess).toHaveBeenCalledWith('"i-1" now stands on its own');
  });

  it('an archetype in use names the count and holders, then detaches all and deletes', async () => {
    const { actions, seen, requestAlert, toasts } = harness((req, n) =>
      n === 1
        ? refusal('ARCHETYPE_IN_USE', { dependents: 2, holders: [{ ownerName: 'Vivienne' }, { ownerName: 'Bertie' }] }, '2 objects derive from it')
        : { success: true },
    );
    await actions.remove(obj('bow', { isArchetype: true, origin: PROJECT }));
    expect(requestAlert).toHaveBeenCalledWith(
      '2 objects derive from "bow" (held by Vivienne, Bertie). Detach them all — each keeps its current values and stands on its own — and delete the archetype?',
      ['Detach all and delete', 'Cancel'],
    );
    expect(seen.at(-1)).toEqual({ type: 'inventoryDelete', container: QUEST, itemId: 'bow', force: true });
    expect(toasts.showSuccess).toHaveBeenCalledWith('"bow" is gone; 2 objects now stand on their own');
  });

  it('archived holders block the delete, by name', async () => {
    const { actions, seen, requestAlert } = harness(() => refusal('ARCHIVED_DEPENDENTS', { characters: [{ name: 'Ghost' }] }, 'sealed'));
    await actions.remove(obj('bow', { isArchetype: true, origin: PROJECT }));
    expect(requestAlert).toHaveBeenCalledWith(
      '"bow" cannot be deleted while archived characters hold instances of it: Ghost. Their vaults are sealed; restore them first.',
      ['Understood'],
    );
    expect(seen).toHaveLength(1);
  });

  it('a cap breach names the object and the cap', () => {
    expect(describeInventoryRefusal(refusal('CAP_EXCEEDED', { itemName: 'ring', cap: 2 }, 'x'), '')).toBe(
      '"ring" can be carried at most 2 at a time — set one down before taking up another.',
    );
  });

  it('a copy refusal says why in words, never in ids', () => {
    const one = refusal('NOT_COPYABLE', { itemIds: ['a1'] }, 'Cannot be copied: a1');
    expect(describeInventoryRefusal(one, '')).toContain('one of a kind');
    expect(describeInventoryRefusal(one, '')).not.toContain('a1');
    const two = refusal('NOT_COPYABLE', { itemIds: ['a1', 'b2'] }, 'Cannot be copied: a1, b2');
    expect(describeInventoryRefusal(two, '')).toMatch(/^2 of those objects/);
  });
});

describe('the remove paths (v4 :95-150)', () => {
  it('a declined confirmation deletes nothing; an accepted one toasts "is gone from the table"', async () => {
    const declined = harness();
    declined.requestConfirmation.mockResolvedValueOnce(false);
    await declined.actions.remove(LAMP);
    expect(declined.seen).toHaveLength(0);
    const accepted = harness();
    await accepted.actions.remove(LAMP);
    expect(accepted.requestConfirmation).toHaveBeenCalledWith('Delete "lantern"? Its pictures go with it.');
    expect(accepted.seen).toEqual([{ type: 'inventoryDelete', container: { scope: 'character', id: 'c-viv' }, itemId: 'lamp' }]);
    expect(accepted.toasts.showSuccess).toHaveBeenCalledWith('"lantern" is gone from the table');
  });

  it('Cancel on the in-use alert stops; a forced delete refused by archived holders alerts without the restore hint', async () => {
    const cancelled = harness(() => refusal('ARCHETYPE_IN_USE', { dependents: 1, holders: [] }));
    cancelled.requestAlert.mockResolvedValueOnce('Cancel');
    await cancelled.actions.remove(BOW);
    expect(cancelled.seen).toHaveLength(1);
    expect(cancelled.requestAlert.mock.calls[0][0]).toBe(
      '1 object derives from "magic bow" (held by ). Detach them all — each keeps its current values and stands on its own — and delete the archetype?',
    );

    const forced = harness((_, n) =>
      n === 1 ? refusal('ARCHETYPE_IN_USE', { dependents: 1, holders: [{ ownerName: null }] }) : refusal('ARCHIVED_DEPENDENTS', { characters: [{}] }),
    );
    await forced.actions.remove(BOW);
    expect(forced.requestAlert.mock.calls[0][0]).toContain('(held by somewhere)');
    expect(forced.requestAlert).toHaveBeenLastCalledWith(
      '"magic bow" cannot be deleted while archived characters hold instances of it: an archived character.',
      ['Understood'],
    );
  });

  it('any other refusal toasts in words; a non-dispatch failure toasts its message', async () => {
    const other = harness(() => refusal('KIT_STRANDED', {}, 'A kit would be left empty'));
    await other.actions.remove(KIT);
    expect(other.toasts.showError).toHaveBeenCalledWith('A kit would be left empty');
    const plain = harness(() => new Error('socket closed'));
    await plain.actions.remove(KIT);
    expect(plain.toasts.showError).toHaveBeenCalledWith('socket closed');
  });
});

describe('the other row writes', () => {
  it('toggleArchived puts away, and restores, through the item’s own tier', async () => {
    const { actions, seen, toasts, refresh } = harness(() => ({ item: {} }));
    await actions.toggleArchived(BOW);
    await actions.toggleArchived(obj('lamp', { name: 'lantern', archivedAt: '2026-10-01T00:00:00.000Z' }));
    expect(seen).toEqual([
      { type: 'inventoryUpdate', container: QUEST, itemId: 'bow', body: { archived: true } },
      { type: 'inventoryUpdate', container: { scope: 'character', id: 'c-viv' }, itemId: 'lamp', body: { archived: false } },
    ]);
    expect(toasts.showSuccess.mock.calls.map((c) => c[0])).toEqual(['"magic bow" is put away', '"lantern" is back on the shelf']);
    expect(refresh).toHaveBeenCalledTimes(2);
  });

  it('duplicate toasts the copy’s label; a refusal still refreshes (v4 onSettled)', async () => {
    const ok = harness(() => ({ item: obj('copy', { name: 'lantern', title: 'Lantern II' }) }));
    await ok.actions.duplicate(LAMP);
    expect(ok.toasts.showSuccess).toHaveBeenCalledWith('"Lantern II" is laid beside the original');
    const refused = harness(() => refusal('NOT_COPYABLE', { itemIds: ['lamp'] }));
    await refused.actions.duplicate(LAMP);
    expect(refused.toasts.showError).toHaveBeenCalledWith(
      'That object is one of a kind and cannot be copied — tick "Can be copied" in its editor first, or move it instead.',
    );
    expect(refused.refresh).toHaveBeenCalledTimes(1);
  });
});

describe('the overlays and addressOf', () => {
  it('each overlay opens the gate; closing it shuts the gate', () => {
    const { actions } = harness();
    expect(actions.overlayOpen()).toBe(false);
    const steps: Array<[() => void, () => void]> = [
      [() => actions.openCreate('kit'), () => actions.closeEditor()],
      [() => actions.openEditor(LAMP), () => actions.closeEditor()],
      [() => actions.openFolderPicker([LAMP]), () => actions.closeFolderPicker()],
      [() => actions.openTransfer(LAMP, 'copy'), () => actions.closeTransfer()],
      [() => actions.openInstantiateArchetype(BOW), () => actions.closeInstantiate()],
      [() => actions.openInstantiateInto(QUEST, 'Arms'), () => actions.closeInstantiate()],
    ];
    for (const [open, close] of steps) {
      open();
      expect(actions.overlayOpen()).toBe(true);
      close();
      expect(actions.overlayOpen()).toBe(false);
    }
    actions.openCreate('archetype');
    expect(actions.editor()).toEqual({ kind: 'create', mode: 'archetype', folder: '' });
  });

  it('addressOf follows the origin, falling back to the browsed tier (or General with none)', () => {
    const { actions } = harness();
    expect(actions.addressOf(BOW)).toEqual(QUEST);
    expect(actions.addressOf(obj('loose', { origin: undefined }))).toEqual(QUEST);
    const none = harness(() => ({}), { container: () => null });
    expect(none.actions.addressOf(obj('loose', { origin: undefined }))).toEqual({ scope: 'general', id: null });
  });
});

describe('confirmCollapseAlert (R-4, ALERT_CONFIRM_COLLAPSE)', () => {
  it('two buttons: confirm true → the first label, false → null', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValueOnce(true).mockReturnValueOnce(false);
    expect(await confirmCollapseAlert('m', ['Detach all and delete', 'Cancel'])).toBe('Detach all and delete');
    expect(await confirmCollapseAlert('m', ['Detach all and delete', 'Cancel'])).toBeNull();
    expect(confirm).toHaveBeenCalledWith('m');
  });

  it('one button (or none): alert, then that button (or null)', async () => {
    const alert = vi.spyOn(window, 'alert').mockImplementation(() => undefined);
    expect(await confirmCollapseAlert('sealed', ['Understood'])).toBe('Understood');
    expect(await confirmCollapseAlert('plain')).toBeNull();
    expect(alert.mock.calls.map((c) => c[0])).toEqual(['sealed', 'plain']);
  });
});
