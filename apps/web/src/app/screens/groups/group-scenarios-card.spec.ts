import { TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { ScenariosManager } from '../scenarios/shared/scenarios-manager';
import { GroupScenariosCard } from './group-scenarios-card';

/** The group Scenarios card (P4.D231 — v4 `08c49319d` `GroupScenariosCard.tsx`). */
describe('GroupScenariosCard', () => {
  afterEach(() => TestBed.resetTestingModule());

  async function render(scenarios: Array<{ path: string; name: string }>) {
    const sent: Array<Record<string, unknown>> = [];
    const core = {
      dispatchData: vi.fn(async (req: Record<string, unknown>) => {
        sent.push(req);
        return { mountPointId: 'mp', scenarios, warnings: [] };
      }),
    };
    TestBed.configureTestingModule({
      imports: [GroupScenariosCard],
      providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: core }],
    });
    await TestBed.compileComponents();
    const fixture = TestBed.createComponent(GroupScenariosCard);
    fixture.componentRef.setInput('groupId', 'g1');
    fixture.detectChanges();
    for (let i = 0; i < 6; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    return { fixture, sent };
  }

  it('reads the group’s own shelf and counts it in the heading, with v4’s subtitle', async () => {
    const { fixture, sent } = await render([
      { path: 'Scenarios/a.md', name: 'A' },
      { path: 'Scenarios/b.md', name: 'B' },
    ]);
    expect(sent[0]).toEqual({ type: 'groupScenarioList', groupId: 'g1', includeArchived: false });
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Scenarios (2)');
    expect(text).toContain('Reusable starting scenes offered whenever a member takes a seat');
  });

  it('is collapsed by default; opened, it is a group shelf with v4’s empty message', async () => {
    const { fixture } = await render([]);
    expect(fixture.debugElement.query(By.directive(ScenariosManager))).toBeNull();
    const header = (fixture.nativeElement as HTMLElement).querySelector(
      'button',
    ) as HTMLButtonElement;
    header.click();
    fixture.detectChanges();
    const manager = fixture.debugElement.query(By.directive(ScenariosManager))
      .componentInstance as ScenariosManager;
    expect(manager.scopeLabel()).toBe('group');
    expect(manager.shelf()).toEqual({ kind: 'group', groupId: 'g1' });
    expect((fixture.nativeElement as HTMLElement).textContent).toContain(
      "No scenarios yet. Create one and it'll be offered whenever a member of this group joins a new chat.",
    );
  });
});
