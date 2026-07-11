import { ComponentFixture, TestBed } from '@angular/core/testing';
import { ActivatedRoute, provideRouter } from '@angular/router';
import { convertToParamMap } from '@angular/router';
import { of } from 'rxjs';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { GroupEditor } from './group-editor';
import { GroupsSection } from './groups-section';

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

  it('deletes immediately (no confirm) and surfaces an alert when the delete fails', async () => {
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
    expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeTruthy();
    expect(fixture.nativeElement.textContent).toContain('Failed to delete group');
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
      name: 'Renamed Party',
      description: 'The regulars',
      color: '#123456',
      icon: '🎭',
    });
  });

  it('surfaces an alert when the save fails', async () => {
    const fixture = await render(
      stubClient(baseHandler((r) => (r.type === 'groupUpdate' ? new Error('boom') : undefined))),
    );
    const form = fixture.nativeElement.querySelector('form') as HTMLFormElement;
    form.dispatchEvent(new Event('submit'));
    await settle(fixture);
    expect(fixture.nativeElement.querySelector('.qt-alert-error')).toBeTruthy();
    expect(fixture.nativeElement.textContent).toContain('boom');
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
});
