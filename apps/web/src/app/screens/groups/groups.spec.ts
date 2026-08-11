import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ActivatedRoute, provideRouter } from '@angular/router';
import { convertToParamMap } from '@angular/router';
import { of } from 'rxjs';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { GroupEditor } from './group-editor';
import { GroupsSection } from './groups-section';
import { ToastService } from '../../ui/toast.service';

interface DispatchReq {
  type: string;
  [k: string]: unknown;
}

function stubClient(handler: (req: DispatchReq) => unknown): Partial<CoreClient> {
  return {
    dispatchData: (async (req: DispatchReq) => {
      const out = handler(req);
      if (out instanceof Error) {
        throw out;
      }
      return (out ?? {}) as Record<string, unknown>;
    }) as CoreClient['dispatchData'],
  };
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 8): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('GroupsSection', () => {
  async function render(client: Partial<CoreClient>): Promise<ComponentFixture<GroupsSection>> {
    TestBed.configureTestingModule({
      imports: [GroupsSection],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: client },
      ],
    });
    const fixture = TestBed.createComponent(GroupsSection);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  it('shows the empty state when there are no groups', async () => {
    const fixture = await render(stubClient((r) => (r.type === 'groupList' ? { groups: [] } : {})));
    expect(fixture.nativeElement.textContent).toContain('No groups yet');
  });

  it('renders a card per group with its member count', async () => {
    const fixture = await render(
      stubClient((r) =>
        r.type === 'groupList'
          ? {
              groups: [
                {
                  id: 'g1',
                  name: 'Adventuring Party',
                  description: 'The regulars',
                  color: '#123456',
                  icon: '🎭',
                  officialMountPointId: null,
                  createdAt: '2024-01-01T00:00:00Z',
                  updatedAt: '2024-01-01T00:00:00Z',
                  _count: { members: 3 },
                },
              ],
            }
          : {},
      ),
    );
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Adventuring Party');
    expect(text).toContain('3 members');
    const edit = fixture.nativeElement.querySelector('a[href="/characters/groups/g1"]');
    expect(edit).toBeTruthy();
  });

  it('deletes immediately (no confirm) and toasts when the delete fails', async () => {
    const groups = [
      {
        id: 'g1',
        name: 'Doomed Group',
        description: null,
        color: null,
        icon: null,
        officialMountPointId: null,
        createdAt: '2024-01-01T00:00:00Z',
        updatedAt: '2024-01-01T00:00:00Z',
        _count: { members: 0 },
      },
    ];
    const fixture = await render(
      stubClient((r) => {
        if (r.type === 'groupList') return { groups };
        if (r.type === 'groupDelete') return new Error('Failed to delete group');
        return {};
      }),
    );
    const trash = fixture.nativeElement.querySelector(
      'button[aria-label="Delete group"]',
    ) as HTMLButtonElement;
    expect(trash).toBeTruthy();
    trash.click();
    await settle(fixture);
    // The failed delete rolls back and surfaces the fallback microcopy.
    expect(toasts().at(-1)).toEqual({ type: 'error', message: 'Failed to delete group' });
  });
});

describe('GroupEditor', () => {
  const group = {
    id: 'g1',
    name: 'Adventuring Party',
    description: 'The regulars',
    color: '#123456',
    icon: '🎭',
    officialMountPointId: null,
    createdAt: '2024-01-01T00:00:00Z',
    updatedAt: '2024-01-01T00:00:00Z',
  };

  async function render(client: Partial<CoreClient>): Promise<ComponentFixture<GroupEditor>> {
    TestBed.configureTestingModule({
      imports: [GroupEditor],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: client },
        {
          provide: ActivatedRoute,
          useValue: { paramMap: of(convertToParamMap({ id: 'g1' })) },
        },
      ],
    });
    const fixture = TestBed.createComponent(GroupEditor);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  function baseHandler(over?: (r: DispatchReq) => unknown) {
    return (r: DispatchReq) => {
      const custom = over?.(r);
      if (custom !== undefined) return custom;
      switch (r.type) {
        case 'groupGet':
          return { group };
        case 'groupMembers':
          return { members: [{ id: 'c1', name: 'Jeeves' }] };
        case 'groupMountPointList':
          return { mountPoints: [] };
        case 'characterList':
          return {
            characters: [
              { id: 'c1', name: 'Jeeves' },
              { id: 'c2', name: 'Wooster' },
            ],
          };
        default:
          return {};
      }
    };
  }

  it('seeds the form from the loaded group', async () => {
    const fixture = await render(stubClient(baseHandler()));
    const name = fixture.nativeElement.querySelector('#qt-group-name') as HTMLInputElement;
    expect(name.value).toBe('Adventuring Party');
    const icon = fixture.nativeElement.querySelector('#qt-group-icon') as HTMLInputElement;
    expect(icon.value).toBe('🎭');
  });

  it('saves the edited fields via groupUpdate (name/description/color/icon)', async () => {
    const seen: DispatchReq[] = [];
    const fixture = await render(
      stubClient((r) => {
        seen.push(r);
        return baseHandler()(r);
      }),
    );
    // Set inputs AFTER first render (the async-options lesson).
    const name = fixture.nativeElement.querySelector('#qt-group-name') as HTMLInputElement;
    name.value = 'Renamed Party';
    name.dispatchEvent(new Event('input'));
    await settle(fixture);
    const form = fixture.nativeElement.querySelector('form') as HTMLFormElement;
    form.dispatchEvent(new Event('submit'));
    await settle(fixture);
    const put = seen.find((r) => r.type === 'groupUpdate');
    expect(put).toBeTruthy();
    expect(put).toMatchObject({
      groupId: 'g1',
      group: {
        name: 'Renamed Party',
        description: 'The regulars',
        color: '#123456',
        icon: '🎭',
      },
    });
    // v4 `GroupDetailView.tsx:100` — toast only, no inline surface (P4.29).
    expect(toasts()).toEqual([{ type: 'success', message: 'Group updated successfully!' }]);
  });

  it('toasts a failed save with NO inline alert (v4 has none on this path, P4.29)', async () => {
    const fixture = await render(
      stubClient(baseHandler((r) => (r.type === 'groupUpdate' ? new Error('boom') : undefined))),
    );
    const form = fixture.nativeElement.querySelector('form') as HTMLFormElement;
    form.dispatchEvent(new Event('submit'));
    await settle(fixture);
    expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeNull();
    expect(fixture.nativeElement.querySelector('qt-error-alert')).toBeNull();
    expect(toasts()).toEqual([{ type: 'error', message: 'boom' }]);
  });

  it('toasts "Member added to group!" on a successful add (v4 useGroupMembers.ts:64, P4.29)', async () => {
    const fixture = await render(stubClient(baseHandler()));
    const memberHeader = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.includes('Members'),
    ) as HTMLButtonElement;
    memberHeader.click();
    await settle(fixture);
    const addBtn = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.trim() === 'Add Member',
    ) as HTMLButtonElement;
    addBtn.click();
    await settle(fixture);
    const select = fixture.nativeElement.querySelector('select') as HTMLSelectElement;
    select.value = 'c2';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);
    const confirmAdd = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.trim() === 'Add',
    ) as HTMLButtonElement;
    confirmAdd.click();
    await settle(fixture);

    expect(toasts()).toEqual([{ type: 'success', message: 'Member added to group!' }]);
  });

  it('toasts the server message on a failed add (P4.29)', async () => {
    const fixture = await render(
      stubClient(
        baseHandler((r) =>
          r.type === 'groupMemberAdd' ? new Error('no room at the club') : undefined,
        ),
      ),
    );
    const memberHeader = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.includes('Members'),
    ) as HTMLButtonElement;
    memberHeader.click();
    await settle(fixture);
    const addBtn = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.trim() === 'Add Member',
    ) as HTMLButtonElement;
    addBtn.click();
    await settle(fixture);
    const select = fixture.nativeElement.querySelector('select') as HTMLSelectElement;
    select.value = 'c2';
    select.dispatchEvent(new Event('change'));
    await settle(fixture);
    const confirmAdd = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.trim() === 'Add',
    ) as HTMLButtonElement;
    confirmAdd.click();
    await settle(fixture);

    expect(toasts()).toEqual([{ type: 'error', message: 'no room at the club' }]);
  });

  it('toasts "Member removed from group!" on a successful remove (v4 useGroupMembers.ts:85, P4.29)', async () => {
    const fixture = await render(stubClient(baseHandler()));
    const memberHeader = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.includes('Members'),
    ) as HTMLButtonElement;
    memberHeader.click();
    await settle(fixture);
    const removeBtn = fixture.nativeElement.querySelector(
      'button[title="Remove member"]',
    ) as HTMLButtonElement;
    expect(removeBtn).toBeTruthy();
    removeBtn.click();
    await settle(fixture);

    expect(toasts()).toEqual([{ type: 'success', message: 'Member removed from group!' }]);
  });

  it('toasts the server message on a failed remove (P4.29)', async () => {
    const fixture = await render(
      stubClient(
        baseHandler((r) =>
          r.type === 'groupMemberRemove' ? new Error('the club rules forbid it') : undefined,
        ),
      ),
    );
    const memberHeader = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.includes('Members'),
    ) as HTMLButtonElement;
    memberHeader.click();
    await settle(fixture);
    const removeBtn = fixture.nativeElement.querySelector(
      'button[title="Remove member"]',
    ) as HTMLButtonElement;
    removeBtn.click();
    await settle(fixture);

    expect(toasts()).toEqual([{ type: 'error', message: 'the club rules forbid it' }]);
  });

  it('the Add-Member picker select uses per-option [selected] (finding-#6 discipline)', async () => {
    const fixture = await render(stubClient(baseHandler()));
    // Expand the Members card, then reveal the picker.
    const memberHeader = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.includes('Members'),
    ) as HTMLButtonElement;
    memberHeader.click();
    await settle(fixture);
    const addBtn = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.trim() === 'Add Member',
    ) as HTMLButtonElement;
    expect(addBtn).toBeTruthy();
    addBtn.click();
    await settle(fixture);
    const select = fixture.nativeElement.querySelector('select') as HTMLSelectElement;
    expect(select).toBeTruthy();
    // The non-member 'Wooster' is offered (Jeeves is already a member).
    const options = [...select.querySelectorAll('option')].map((o) => o.textContent?.trim());
    expect(options).toContain('Wooster');
    expect(options).not.toContain('Jeeves');
    // No `[value]` binding on the <select> itself — options carry [selected].
    expect(select.getAttribute('ng-reflect-value')).toBeNull();
  });

  it('opens the Group State editor and round-trips the group state verbs', async () => {
    const seen: DispatchReq[] = [];
    const fixture = await render(
      stubClient((r) => {
        seen.push(r);
        if (r.type === 'groupStateGet') return { success: true, state: { gold: 7 } };
        if (r.type === 'groupStateSet') return { success: true, state: r['state'] };
        if (r.type === 'groupStateReset') return { success: true, previousState: { gold: 7 } };
        return baseHandler()(r);
      }),
    );

    const stateBtn = [...fixture.nativeElement.querySelectorAll('button')].find(
      (b: HTMLButtonElement) => b.textContent?.trim() === 'Group State',
    ) as HTMLButtonElement;
    expect(stateBtn).toBeTruthy();
    stateBtn.click();
    await settle(fixture);

    // The modal loaded THIS group's own state tier (no cascade), titled by name.
    expect(seen.some((r) => r.type === 'groupStateGet' && r['groupId'] === 'g1')).toBe(true);
    expect(fixture.nativeElement.textContent).toContain('Group State - Adventuring Party');
    // The group tier never renders the chat cascade note.
    expect(fixture.nativeElement.textContent).not.toContain('narrower tiers win');
  });
});

// ===========================================================================
// P4.D64 — archived members (v4 `GroupMembersCard.tsx` at `d553f72a`)
// ===========================================================================

describe('GroupMembersCard — the archived member line (P4.D64)', () => {
  const group = {
    id: 'g1',
    name: 'Adventuring Party',
    description: 'The regulars',
    color: '#123456',
    icon: '🎭',
    officialMountPointId: null,
    createdAt: '2024-01-01T00:00:00Z',
    updatedAt: '2024-01-01T00:00:00Z',
  };

  async function renderWithMembers(
    members: Array<{ id: string; name: string; archivedAt?: string | null }>,
  ): Promise<ComponentFixture<GroupEditor>> {
    TestBed.configureTestingModule({
      imports: [GroupEditor],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        {
          provide: CoreClient,
          useValue: stubClient((r) => {
            switch (r.type) {
              case 'groupGet':
                return { group };
              case 'groupMembers':
                return { members };
              case 'groupMountPointList':
                return { mountPoints: [] };
              case 'characterList':
                return { characters: [] };
              default:
                return {};
            }
          }),
        },
        {
          provide: ActivatedRoute,
          useValue: { paramMap: of(convertToParamMap({ id: 'g1' })) },
        },
      ],
    });
    const fixture = TestBed.createComponent(GroupEditor);
    fixture.detectChanges();
    await settle(fixture);
    return fixture;
  }

  it('leaves an all-live roster reading plainly (3 members)', async () => {
    const fixture = await renderWithMembers([
      { id: 'c1', name: 'Jeeves' },
      { id: 'c2', name: 'Wooster' },
      { id: 'c3', name: 'Aunt Agatha' },
    ]);
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('3 members');
    expect(text).not.toContain('can speak');
    expect(text).not.toContain('Archived');
  });

  it('appends the can-speak clause when a member is archived', async () => {
    const fixture = await renderWithMembers([
      { id: 'c1', name: 'Jeeves' },
      { id: 'c2', name: 'Wooster' },
      { id: 'c3', name: 'Aunt Agatha', archivedAt: '2026-08-01T00:00:00.000Z' },
    ]);
    // v4's exact shape — the base count is unchanged; membership survives
    // archiving, the seat simply takes no turns.
    expect(fixture.nativeElement.textContent).toContain('3 members / 2 can speak (1 archived)');
  });

  it('badges the archived member row, and only that row', async () => {
    const fixture = await renderWithMembers([
      { id: 'c1', name: 'Jeeves' },
      { id: 'c3', name: 'Aunt Agatha', archivedAt: '2026-08-01T00:00:00.000Z' },
    ]);
    // The Members card is collapsed by default (v4's too), so the SUBTITLE is
    // visible but the rows are not rendered at all — expand it first.
    const header = fixture.nativeElement.querySelector(
      'qt-group-members-card .qt-collapsible-card-header',
    ) as HTMLButtonElement;
    expect(header).toBeTruthy();
    header.click();
    await settle(fixture);
    const badges = (
      Array.from(fixture.nativeElement.querySelectorAll('span')) as HTMLElement[]
    ).filter((el) => el.textContent?.trim() === 'Archived');
    expect(badges).toHaveLength(1);
    expect(badges[0].getAttribute('title')).toBe(
      'Resting in the archive — still a member, but takes no turns until rehydrated',
    );
  });
});
