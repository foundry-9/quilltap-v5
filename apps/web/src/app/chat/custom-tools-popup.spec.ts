import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { CoreDispatchError, type CustomToolsRosterData } from '../core/core-contract';
import { CustomToolsPopup } from './custom-tools-popup';

/** A roster with one no-param tool that defaults to a whisper, plus one broken file. */
function roster(over: Partial<CustomToolsRosterData> = {}): CustomToolsRosterData {
  return {
    tools: [
      {
        name: 'pick_lock',
        title: 'Pick Lock',
        description: 'Try the tumblers.',
        parameters: { scale: { type: 'integer', default: 3, min: 1, max: 6 } },
        defaultVisibility: 'whisper',
        sourceTier: 'global',
        asCharacterId: 'char-1',
        definitionPath: 'Tools/pick-lock.tool.json',
        mountName: 'Toolbox',
        mountPointId: 'mount-1',
      },
    ],
    errors: [],
    ...over,
  };
}

interface Stub {
  calls: Record<string, unknown>[];
  client: Partial<CoreClient>;
}

/** dispatchData answers the list verb with `list`, the run verb with `run` (or throws). */
function stub(list: CustomToolsRosterData, run: Record<string, unknown> | Error = { messages: [], result: {} }): Stub {
  const calls: Record<string, unknown>[] = [];
  const dispatchData = vi.fn(async (req: Record<string, unknown>) => {
    calls.push(req);
    if (req['type'] === 'chatCustomToolsList') return list as unknown as Record<string, unknown>;
    if (req['type'] === 'chatCustomToolRun') {
      if (run instanceof Error) throw run;
      return run;
    }
    return {};
  });
  return { calls, client: { dispatchData: dispatchData as unknown as CoreClient['dispatchData'] } };
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function mount(s: Stub): Promise<ComponentFixture<CustomToolsPopup>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [CustomToolsPopup],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: s.client }],
  });
  const fixture = TestBed.createComponent(CustomToolsPopup);
  fixture.componentRef.setInput('chatId', 'chat-1');
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

function button(fixture: ComponentFixture<CustomToolsPopup>): HTMLButtonElement | null {
  return fixture.nativeElement.querySelector('button[aria-label="Custom tools"]');
}

async function open(fixture: ComponentFixture<CustomToolsPopup>): Promise<void> {
  button(fixture)!.click();
  fixture.detectChanges();
  await settle(fixture);
}

describe('CustomToolsPopup (v4 CustomToolsDropdown)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('hides the button entirely when the roster is empty and error-free', async () => {
    const fixture = await mount(stub({ tools: [], errors: [] }));
    expect(button(fixture)).toBeNull();
  });

  it('shows the button when a definition failed to load, even with zero runnable tools', async () => {
    const fixture = await mount(
      stub({
        tools: [],
        errors: [{ definitionPath: 'Tools/broken.tool.json', mountName: 'Toolbox', tier: 'global', reason: 'bad outcome table' }],
      }),
    );
    expect(button(fixture)).not.toBeNull();
    await open(fixture);
    // The badge renders the loader's verbatim reason; the Workbench repair link is P4.6bb.
    const text = (fixture.nativeElement.textContent ?? '').replace(/\s+/g, ' ');
    expect(text).toContain('Tools/broken.tool.json');
    expect(text).toContain('bad outcome table');
    expect(text).not.toContain('New contrivance');
  });

  it('re-resolves the roster fresh on every open (v4 enabled: isOpen)', async () => {
    const s = stub(roster());
    const fixture = await mount(s);
    const listCalls = () => s.calls.filter((c) => c['type'] === 'chatCustomToolsList').length;
    const afterMount = listCalls();
    await open(fixture); // open → refetch
    expect(listCalls()).toBe(afterMount + 1);
  });

  it('shows the tool but NEVER its odds/outcome table', async () => {
    const fixture = await mount(stub(roster()));
    await open(fixture);
    const text = (fixture.nativeElement.textContent ?? '').replace(/\s+/g, ' ');
    expect(text).toContain('Pick Lock');
    expect(text).toContain('Try the tumblers.');
    // The payload carries no roll spec / outcome table, and the popup shows none.
    expect(text).not.toMatch(/outcome|odds|notation|dice/i);
  });

  it('runs a tool with coerced params, the private default, and the asCharacterId, then emits ran', async () => {
    const s = stub(roster());
    const fixture = await mount(s);
    let ranCount = 0;
    fixture.componentInstance.ran.subscribe(() => (ranCount += 1));

    await open(fixture);
    // Expand the tool.
    fixture.nativeElement.querySelector('button[role="menuitem"]').click();
    fixture.detectChanges();
    await settle(fixture);

    // Run it.
    const runBtn = [...fixture.nativeElement.querySelectorAll('button')].find((b: HTMLButtonElement) =>
      b.textContent?.includes('Run Pick Lock'),
    ) as HTMLButtonElement;
    runBtn.click();
    await settle(fixture);

    const runCall = s.calls.find((c) => c['type'] === 'chatCustomToolRun');
    expect(runCall).toEqual({
      type: 'chatCustomToolRun',
      chatId: 'chat-1',
      tool: 'pick_lock',
      parameters: { scale: 3 }, // coerced integer default
      private: true, // defaultVisibility 'whisper'
      asCharacterId: 'char-1',
    });
    expect(ranCount).toBe(1);
    // The popup closed on success.
    expect(fixture.nativeElement.querySelector('[role="menu"]')).toBeNull();
  });

  it('surfaces the 400 server message inline and does NOT emit ran on a run error', async () => {
    const s = stub(roster(), new CoreDispatchError({ kind: 'bad-request', message: 'Unknown custom tool "pick_lock".' }));
    const fixture = await mount(s);
    let ranCount = 0;
    fixture.componentInstance.ran.subscribe(() => (ranCount += 1));

    await open(fixture);
    fixture.nativeElement.querySelector('button[role="menuitem"]').click();
    fixture.detectChanges();
    await settle(fixture);
    const runBtn = [...fixture.nativeElement.querySelectorAll('button')].find((b: HTMLButtonElement) =>
      b.textContent?.includes('Run Pick Lock'),
    ) as HTMLButtonElement;
    runBtn.click();
    await settle(fixture);

    const alert = fixture.nativeElement.querySelector('[role="alert"]');
    expect(alert.textContent).toContain('Unknown custom tool "pick_lock".');
    expect(ranCount).toBe(0);
    // The popup stayed open so the operator can see the failure.
    expect(fixture.nativeElement.querySelector('[role="menu"]')).not.toBeNull();
  });

  it('surfaces the roster cap notice (droppedForCap)', async () => {
    const fixture = await mount(stub(roster({ droppedForCap: ['flip_coin', 'draw_card'] })));
    await open(fixture);
    const text = (fixture.nativeElement.textContent ?? '').replace(/\s+/g, ' ');
    expect(text).toContain('these were left off: flip_coin, draw_card');
  });
});
