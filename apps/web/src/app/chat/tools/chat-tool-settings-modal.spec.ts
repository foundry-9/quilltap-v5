import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { beforeEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { ChatToolSettingsModal } from './chat-tool-settings-modal';
import type { AvailableTool } from './tool-settings';
import { ToastService } from '../../ui/toast.service';

/**
 * LLM Tool Settings (v4 `ChatToolSettingsModal.tsx` + `ToolSettingsContent.tsx`).
 * The load-bearing claim is that built-in groups and plugin groups disable by
 * DIFFERENT mechanisms — ids versus glob patterns.
 */

interface Req {
  type: string;
  [k: string]: unknown;
}

const sent: Req[] = [];
let saveFails: Error | null = null;
let tools: AvailableTool[] = [];

function stubClient(): Partial<CoreClient> {
  return {
    dispatchData: (async (req: Req) => {
      sent.push(req);
      if (req['type'] === 'toolsList') return { tools };
      if (saveFails) throw saveFails;
      return {};
    }) as unknown as CoreClient['dispatchData'],
  };
}

@Component({
  imports: [ChatToolSettingsModal],
  template: `
    <qt-chat-tool-settings-modal
      [chatId]="'chat-1'"
      [disabledTools]="disabledTools()"
      [disabledToolGroups]="disabledGroups()"
      (saved)="saves.push($event)"
      (close)="closed = closed + 1"
    />
  `,
})
class Host {
  readonly disabledTools = signal<string[]>([]);
  readonly disabledGroups = signal<string[]>([]);
  readonly saves: { disabledTools: string[]; disabledToolGroups: string[] }[] = [];
  closed = 0;
}

async function render(): Promise<ComponentFixture<Host>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [
      { provide: CoreClient, useValue: stubClient() },
      provideTanStackQuery(new QueryClient()),
    ],
  });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

async function settle(fixture: ComponentFixture<Host>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

function groupToggle(fixture: ComponentFixture<Host>, label: string): HTMLButtonElement {
  return fixture.nativeElement.querySelector(`[aria-label="Toggle all ${label}"]`);
}

function toolBox(fixture: ComponentFixture<Host>, name: string): HTMLInputElement {
  return fixture.nativeElement.querySelector(`input[aria-label="${name}"]`);
}

function save(fixture: ComponentFixture<Host>): HTMLButtonElement {
  return (Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]).find(
    (b) => b.textContent!.trim() === 'Save' || b.textContent!.trim() === 'Saving...',
  )!;
}

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('ChatToolSettingsModal', () => {
  beforeEach(() => {
    sent.length = 0;
    saveFails = null;
    tools = [
      { id: 'search', name: 'search', description: '', source: 'built-in' },
      { id: 'recall', name: 'recall', description: '', source: 'built-in' },
      {
        id: 'mcp_read',
        name: 'mcp_read',
        description: '',
        source: 'plugin',
        pluginName: 'mcp',
        subgroupId: 'files',
        subgroupDisplayName: 'Files',
      },
    ];
  });

  it('asks for the inventory WITH the chat id, so availability comes back', async () => {
    await render();
    expect(sent[0]).toEqual({ type: 'toolsList', chatId: 'chat-1' });
  });

  it('keeps Save dead until something moves', async () => {
    const fixture = await render();
    expect(save(fixture).disabled).toBe(true);

    toolBox(fixture, 'search').dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(save(fixture).disabled).toBe(false);

    // Toggling back is not "a change" — v4 compares set against set.
    toolBox(fixture, 'search').dispatchEvent(new Event('change'));
    fixture.detectChanges();
    expect(save(fixture).disabled).toBe(true);
  });

  it('disables a BUILT-IN group by writing every member id', async () => {
    const fixture = await render();
    groupToggle(fixture, 'Built-in Tools').click();
    fixture.detectChanges();
    save(fixture).click();
    await settle(fixture);

    const write = sent.find((r) => r.type === 'chatUpdateToolSettings')!;
    expect((write['disabledTools'] as string[]).sort()).toEqual(['recall', 'search']);
    expect(write['disabledToolGroups']).toEqual([]);
  });

  it('disables a PLUGIN group by writing one glob pattern', async () => {
    const fixture = await render();
    groupToggle(fixture, 'MCP Server Connector').click();
    fixture.detectChanges();
    save(fixture).click();
    await settle(fixture);

    const write = sent.find((r) => r.type === 'chatUpdateToolSettings')!;
    expect(write['disabledTools']).toEqual([]);
    expect(write['disabledToolGroups']).toEqual(['plugin:mcp']);
  });

  it('clears a member’s own disable when its plugin group is re-enabled', async () => {
    // Otherwise a tool switched off individually would stay off after its whole
    // group came back on.
    const fixture = await render();
    fixture.componentInstance.disabledTools.set(['mcp_read']);
    fixture.componentInstance.disabledGroups.set(['plugin:mcp']);
    fixture.detectChanges();
    await settle(fixture);

    groupToggle(fixture, 'MCP Server Connector').click();
    fixture.detectChanges();
    save(fixture).click();
    await settle(fixture);

    const write = sent.find((r) => r.type === 'chatUpdateToolSettings')!;
    expect(write['disabledToolGroups']).toEqual([]);
    expect(write['disabledTools']).toEqual([]);
  });

  it('refuses to add a subgroup pattern under an already-disabled plugin', async () => {
    const fixture = await render();
    fixture.componentInstance.disabledGroups.set(['plugin:mcp']);
    fixture.detectChanges();
    await settle(fixture);

    groupToggle(fixture, 'Files').click();
    fixture.detectChanges();
    // Nothing moved, so Save stays dead.
    expect(save(fixture).disabled).toBe(true);
  });

  it('Disable All uses patterns for plugins and ids for built-ins', async () => {
    const fixture = await render();
    (
      Array.from(fixture.nativeElement.querySelectorAll('button')) as HTMLButtonElement[]
    ).find((b) => b.textContent!.trim() === 'Disable All')!.click();
    fixture.detectChanges();
    save(fixture).click();
    await settle(fixture);

    const write = sent.find((r) => r.type === 'chatUpdateToolSettings')!;
    expect((write['disabledTools'] as string[]).sort()).toEqual(['recall', 'search']);
    expect(write['disabledToolGroups']).toEqual(['plugin:mcp']);
  });

  it('discounts an unreachable tool from the counts and greys it out', async () => {
    tools = [
      { id: 'search', name: 'search', description: '', source: 'built-in' },
      {
        id: 'shell',
        name: 'shell',
        description: '',
        source: 'built-in',
        available: false,
        unavailableReason: 'No workspace is open',
      },
    ];
    const fixture = await render();
    expect(fixture.nativeElement.textContent).toContain('1 of 1 tools enabled');
    expect(fixture.nativeElement.textContent).toContain('(unavailable)');
    expect(fixture.nativeElement.textContent).toContain('No workspace is open');
    expect(toolBox(fixture, 'shell')).toBeNull();
  });

  it('reports a failed save and stays open', async () => {
    saveFails = new Error('the cabinet is jammed');
    const fixture = await render();
    toolBox(fixture, 'search').dispatchEvent(new Event('change'));
    fixture.detectChanges();
    save(fixture).click();
    await settle(fixture);
    expect(toasts()).toEqual([{ type: 'error', message: 'the cabinet is jammed' }]);
    expect(fixture.componentInstance.closed).toBe(0);
  });
});
