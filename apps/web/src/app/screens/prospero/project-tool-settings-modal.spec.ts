import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { AvailableTool } from '../../chat/tools/tool-settings';
import { ProjectToolSettingsModal } from './project-tool-settings-modal';
import { ToastService } from '../../ui/toast.service';

function tool(over: Partial<AvailableTool> & { name: string }): AvailableTool {
  return {
    id: over.name,
    description: `The ${over.name} tool`,
    source: 'built-in',
    category: 'memory',
    available: true,
    ...over,
  };
}

const TOOLS: AvailableTool[] = [
  tool({ name: 'search_memories' }),
  tool({ name: 'read_document' }),
];

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(
  dispatchData: ReturnType<typeof vi.fn>,
  disabledTools: string[] = [],
  disabledToolGroups: string[] = [],
): Promise<ComponentFixture<ProjectToolSettingsModal>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [ProjectToolSettingsModal],
    providers: [
      provideTanStackQuery(new QueryClient()),
      {
        provide: CoreClient,
        useValue: { dispatchData: dispatchData as unknown as CoreClient['dispatchData'] },
      },
    ],
  });
  const fixture = TestBed.createComponent(ProjectToolSettingsModal);
  fixture.componentRef.setInput('projectId', 'proj-1');
  fixture.componentRef.setInput('disabledTools', disabledTools);
  fixture.componentRef.setInput('disabledToolGroups', disabledToolGroups);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

function toolsClient(): ReturnType<typeof vi.fn> {
  return vi.fn(async (req: { type: string }) => {
    if (req.type === 'toolsList') return { tools: TOOLS };
    return {};
  });
}

function saveButton(fixture: ComponentFixture<unknown>): HTMLButtonElement {
  return Array.from(
    fixture.nativeElement.querySelectorAll('[qt-modal-footer] button') as NodeListOf<HTMLButtonElement>,
  ).find((b) => b.textContent?.includes('Save'))!;
}

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('qt-project-tool-settings-modal (v4 ProjectToolSettingsModal)', () => {
  it('asks for the inventory with NO chatId — a project has no chat to check against', async () => {
    const dispatchData = toolsClient();
    await render(dispatchData);
    const call = dispatchData.mock.calls.find(
      (c) => (c[0] as { type: string }).type === 'toolsList',
    );
    expect(call).toBeDefined();
    expect(call![0]).toEqual({ type: 'toolsList' });
  });

  it('carries the project footer note and the title, verbatim', async () => {
    const fixture = await render(toolsClient());
    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('Default Tool Settings');
    expect(text).toContain(
      'Configure which tools are enabled by default for new chats in this project. Existing chats are not affected.',
    );
  });

  it('gates Save on set-vs-set change, not on reference (v4 hasChanges)', async () => {
    const fixture = await render(toolsClient(), ['search_memories']);
    expect(saveButton(fixture).disabled).toBe(true);

    // Re-supplying an EQUAL-but-new array must not enable Save.
    fixture.componentRef.setInput('disabledTools', ['search_memories']);
    fixture.detectChanges();
    expect(saveButton(fixture).disabled).toBe(true);

    // A genuine change does.
    const checkbox = Array.from(
      fixture.nativeElement.querySelectorAll('input[type="checkbox"]') as NodeListOf<HTMLInputElement>,
    )[0];
    checkbox.click();
    fixture.detectChanges();
    expect(saveButton(fixture).disabled).toBe(false);
  });

  it('writes both arrays whole through projectToolSettingsUpdate, then reports and closes', async () => {
    const dispatchData = toolsClient();
    const fixture = await render(dispatchData);
    const saved: { disabledTools: string[]; disabledToolGroups: string[] }[] = [];
    let closed = 0;
    fixture.componentInstance.saved.subscribe((s) => saved.push(s));
    fixture.componentInstance.close.subscribe(() => closed++);

    const checkbox = Array.from(
      fixture.nativeElement.querySelectorAll('input[type="checkbox"]') as NodeListOf<HTMLInputElement>,
    )[0];
    checkbox.click();
    fixture.detectChanges();
    saveButton(fixture).click();
    await settle(fixture);

    const write = dispatchData.mock.calls
      .map((c) => c[0] as Record<string, unknown>)
      .find((r) => r['type'] === 'projectToolSettingsUpdate');
    expect(write).toBeDefined();
    expect(write!['projectId']).toBe('proj-1');
    expect(Array.isArray(write!['defaultDisabledTools'])).toBe(true);
    expect(Array.isArray(write!['defaultDisabledToolGroups'])).toBe(true);
    expect(saved).toHaveLength(1);
    expect(closed).toBe(1);
  });

  it('surfaces a failed save and stays open', async () => {
    const dispatchData = vi.fn(async (req: { type: string }) => {
      if (req.type === 'toolsList') return { tools: TOOLS };
      throw new Error('Failed to save tool settings');
    });
    const fixture = await render(dispatchData);
    let closed = 0;
    fixture.componentInstance.close.subscribe(() => closed++);

    const checkbox = Array.from(
      fixture.nativeElement.querySelectorAll('input[type="checkbox"]') as NodeListOf<HTMLInputElement>,
    )[0];
    checkbox.click();
    fixture.detectChanges();
    saveButton(fixture).click();
    await settle(fixture);

    expect(toasts()).toEqual([{ type: 'error', message: 'Failed to save tool settings' }]);
    expect(closed).toBe(0);
  });
});
