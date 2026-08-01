import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { StateEditorModal } from './state-editor-modal';
import type { StateEntityType } from './state.api';
import { ToastService } from '../../ui/toast.service';

/**
 * The shared four-entity State editor — asserted against v4
 * `components/state/StateEditorModal.tsx`: the per-tier verb wiring (§A), the
 * chat-only inherited-layers note (with omit-when-empty and the ambiguous-group
 * arm), the `State must be a JSON object` save guard, and the reset flow.
 */

interface Req {
  type: string;
  [k: string]: unknown;
}

function stubClient(
  hooks: {
    onDispatch?: (req: Req) => void;
    get?: Record<string, unknown>;
    set?: Record<string, unknown>;
    reset?: Record<string, unknown>;
  } = {},
): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Req) => {
      hooks.onDispatch?.(req);
      if (req.type.endsWith('StateReset')) return hooks.reset ?? { success: true, previousState: {} };
      if (req.type.endsWith('StateSet')) return hooks.set ?? { success: true, state: req['state'] };
      // *StateGet
      return hooks.get ?? { success: true, state: {} };
    }) as CoreClient['dispatchData'],
  };
}

async function render(
  entityType: StateEntityType,
  client: Partial<CoreClient>,
  opts: { entityId?: string; entityName?: string } = {},
): Promise<ComponentFixture<StateEditorModal>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [StateEditorModal],
    providers: [{ provide: CoreClient, useValue: client }],
  });
  const fixture = TestBed.createComponent(StateEditorModal);
  fixture.componentRef.setInput('entityType', entityType);
  if (opts.entityId !== undefined) fixture.componentRef.setInput('entityId', opts.entityId);
  if (opts.entityName !== undefined) fixture.componentRef.setInput('entityName', opts.entityName);
  fixture.detectChanges(); // ngOnInit → load() dispatches
  // The fire-and-forget load resolves on a microtask that zoneless whenStable
  // does not track — flush a macrotask, then re-render (the P4.6x gotcha).
  await new Promise((resolve) => setTimeout(resolve));
  fixture.detectChanges();
  return fixture;
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('StateEditorModal (v4 StateEditorModal.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('fetches the project tier through projectStateGet with the id', async () => {
    const seen: Req[] = [];
    await render('project', stubClient({ onDispatch: (r) => seen.push(r), get: { state: { hp: 3 } } }), {
      entityId: 'proj-1',
      entityName: 'Adventure',
    });
    expect(seen[0]).toEqual({ type: 'projectStateGet', projectId: 'proj-1' });
  });

  it('fetches the group tier through groupStateGet with the id', async () => {
    const seen: Req[] = [];
    await render('group', stubClient({ onDispatch: (r) => seen.push(r) }), { entityId: 'grp-1' });
    expect(seen[0]).toEqual({ type: 'groupStateGet', groupId: 'grp-1' });
  });

  it('fetches the general tier through generalStateGet with NO id', async () => {
    const seen: Req[] = [];
    await render('general', stubClient({ onDispatch: (r) => seen.push(r) }));
    expect(seen[0]).toEqual({ type: 'generalStateGet' });
  });

  it('titles the modal per tier — general is bare, the rest carry the name', async () => {
    const general = await render('general', stubClient());
    expect(text(general)).toContain('General State');

    const chat = await render('chat', stubClient(), { entityId: 'c1', entityName: 'Aria' });
    expect(text(chat)).toContain('Chat State - Aria');

    const project = await render('project', stubClient(), { entityId: 'p1' });
    // No name → no trailing " - ".
    expect(text(project)).toContain('Project State');
    expect(text(project)).not.toContain('Project State -');
  });

  it('renders the chat cascade note from the enriched §A body, omitting empty tiers', async () => {
    const fixture = await render(
      'chat',
      stubClient({
        get: {
          state: { hp: 5, mood: 'wry' },
          chatState: { hp: 5 },
          projectState: { mood: 'wry' },
          // groupState omitted by the server (zero keys)
          generalState: { theme: 'brass' },
          groupTier: { status: 'single', candidates: [], appliedGroupId: 'g1' },
        },
      }),
      { entityId: 'c1' },
    );
    const body = text(fixture);
    expect(body).toContain('narrower tiers win');
    expect(body).toContain('Inherited from project: mood');
    expect(body).toContain('Inherited from general: theme');
    // groupState was omitted, so no group line.
    expect(body).not.toContain('Inherited from group:');
  });

  it('shows the ambiguous-groups note when more than one group applies', async () => {
    const fixture = await render(
      'chat',
      stubClient({
        get: {
          state: {},
          chatState: {},
          groupTier: {
            status: 'ambiguous',
            candidates: [
              { id: 'g1', name: 'One' },
              { id: 'g2', name: 'Two' },
            ],
          },
        },
      }),
      { entityId: 'c1' },
    );
    const body = text(fixture);
    expect(body).toContain('2 groups apply');
    expect(body).toContain('Edit each group');
  });

  it('never renders the cascade note for a non-chat tier', async () => {
    const fixture = await render(
      'project',
      stubClient({ get: { state: { a: 1 }, projectState: { b: 2 } } }),
      { entityId: 'p1' },
    );
    expect(text(fixture)).not.toContain('narrower tiers win');
  });

  it('sends the set verb for the tier and refuses a non-object payload', async () => {
    const seen: Req[] = [];
    const fixture = await render('group', stubClient({ onDispatch: (r) => seen.push(r) }), {
      entityId: 'grp-1',
    });
    const modal = fixture.componentInstance;

    modal.stateText.set('[1, 2]');
    await modal.save();
    expect(toasts().at(-1)).toEqual({ type: 'error', message: 'State must be a JSON object' });
    expect(seen.some((r) => r.type === 'groupStateSet')).toBe(false);

    modal.stateText.set('{"gold": 12}');
    await modal.save();
    const setReq = seen.find((r) => r.type === 'groupStateSet');
    expect(setReq).toEqual({ type: 'groupStateSet', groupId: 'grp-1', state: { gold: 12 } });
  });

  it('sends generalStateSet with no id for the general tier', async () => {
    const seen: Req[] = [];
    const fixture = await render('general', stubClient({ onDispatch: (r) => seen.push(r) }));
    fixture.componentInstance.stateText.set('{"season": "autumn"}');
    await fixture.componentInstance.save();
    expect(seen.find((r) => r.type === 'generalStateSet')).toEqual({
      type: 'generalStateSet',
      state: { season: 'autumn' },
    });
  });

  it('resets through the tier verb and returns the editor to an empty object', async () => {
    const seen: Req[] = [];
    const fixture = await render(
      'project',
      stubClient({ onDispatch: (r) => seen.push(r), get: { state: { hp: 9 } } }),
      { entityId: 'p1' },
    );
    const modal = fixture.componentInstance;
    expect(modal.hasState()).toBe(true);

    await modal.reset();
    expect(seen.find((r) => r.type === 'projectStateReset')).toEqual({
      type: 'projectStateReset',
      projectId: 'p1',
    });
    expect(modal.stateText()).toBe('{}');
    expect(modal.hasState()).toBe(false);
  });
});
