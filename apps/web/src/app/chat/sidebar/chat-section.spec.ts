import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { beforeEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import { ChatSection, type ChatSectionState } from './chat-section';
import { ToastService } from '../../ui/toast.service';

/**
 * The sidebar's Chat section — above all **the Story's Clock**: the copy
 * (byte-for-byte from v4 `ChatSidebar.tsx:1147-1165`), the `{chat:
 * {timelineMode}}` write, and the value the select keeps afterwards.
 */

interface Req {
  type: string;
  [k: string]: unknown;
}

const sent: Req[] = [];
/** `dispatchData` calls, kept apart so the chat-bag assertions stay exact. */
const sentData: Req[] = [];
let failNext = false;
let toggleAnswer: Record<string, unknown> | Error = { avatarGenerationEnabled: true };
let agentAnswer: Record<string, unknown> | Error = {
  agentModeEnabled: true,
  resolvedAgentModeEnabled: true,
  agentModeSource: 'chat',
  message: 'Agent mode enabled',
};

function stubClient(): Partial<CoreClient> {
  return {
    dispatch: (async (req: Req) => {
      sent.push(req);
      if (failNext) {
        failNext = false;
        return { type: 'error', data: { message: 'the clock is stuck' } };
      }
      return { type: 'chat', data: { chat: {} } };
    }) as CoreClient['dispatch'],
    dispatchData: (async (req: Req) => {
      sentData.push(req);
      if (req['type'] === 'chatToggleAvatarGeneration') {
        if (toggleAnswer instanceof Error) throw toggleAnswer;
        return toggleAnswer;
      }
      if (req['type'] === 'chatToggleAgentMode') {
        if (agentAnswer instanceof Error) throw agentAnswer;
        return agentAnswer;
      }
      return {};
    }) as unknown as CoreClient['dispatchData'],
    dispatchExpect: (async () => ({
      type: 'apiKeys',
      data: { apiKeys: [], count: 0 },
    })) as unknown as CoreClient['dispatchExpect'],
  };
}

function state(over: Partial<ChatSectionState> = {}): ChatSectionState {
  return {
    roleplayTemplateId: null,
    avatarGenerationEnabled: null,
    timelineMode: null,
    imageProfileId: null,
    alertCharactersOfLanternImages: null,
    projectId: null,
    projectName: null,
    scenarioText: null,
    agentModeEnabled: null,
    ...over,
  };
}

@Component({
  imports: [ChatSection],
  template: `
    <qt-chat-section
      [chatId]="'chat-1'"
      [state]="chatState()"
      [sectionOpen]="true"
      (chatUpdated)="refetched = refetched + 1"
      (openProject)="projectOpened = projectOpened + 1"
      (openToolSettings)="toolSettingsOpened = toolSettingsOpened + 1"
    />
  `,
})
class Host {
  readonly chatState = signal<ChatSectionState>(state());
  refetched = 0;
  projectOpened = 0;
  toolSettingsOpened = 0;
}

async function render(): Promise<ComponentFixture<Host>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [Host],
    providers: [
      { provide: CoreClient, useValue: stubClient() },
      provideTanStackQuery(new QueryClient()),
      provideRouter([]),
    ],
  });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

function clockSelect(fixture: ComponentFixture<Host>): HTMLSelectElement {
  const labels = Array.from(fixture.nativeElement.querySelectorAll('label')) as HTMLElement[];
  const label = labels.find((l) => l.textContent?.includes('The Story’s Clock'))!;
  return label.querySelector('select') as HTMLSelectElement;
}

function clockHelp(fixture: ComponentFixture<Host>): string {
  const labels = Array.from(fixture.nativeElement.querySelectorAll('label')) as HTMLElement[];
  const label = labels.find((l) => l.textContent?.includes('The Story’s Clock'))!;
  return (label.querySelector('span.qt-text-secondary') as HTMLElement).textContent!
    .replace(/\s+/g, ' ')
    .trim();
}

async function choose(
  fixture: ComponentFixture<Host>,
  select: HTMLSelectElement,
  value: string,
): Promise<void> {
  select.value = value;
  select.dispatchEvent(new Event('change'));
  await fixture.whenStable();
  fixture.detectChanges();
}

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('ChatSection — the Story’s Clock', () => {
  beforeEach(() => {
    sent.length = 0;
    failNext = false;
  });

  it('offers v4’s two options and reads a null column as Real time', async () => {
    const fixture = await render();
    const select = clockSelect(fixture);
    expect(Array.from(select.options).map((o) => [o.value, o.textContent?.trim()])).toEqual([
      ['realtime', 'Real time'],
      ['narrative', 'Story time'],
    ]);
    expect(select.value).toBe('realtime');
  });

  it('seeds the select from the projected column so it survives a reload (bug 22)', async () => {
    // A reload delivers the saved column through the projection (v4 `bd419ae9`);
    // the seed effect adopts it and the select shows the stored clock.
    const fixture = await render();
    fixture.componentInstance.chatState.set(state({ timelineMode: 'narrative' }));
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(clockSelect(fixture).value).toBe('narrative');
  });

  it('carries v4’s helper copy for both clocks, byte for byte', async () => {
    const fixture = await render();
    expect(clockHelp(fixture)).toBe(
      'Memories are filed by the calendar on the wall — “last Tuesday” means the actual Tuesday. ' +
        'Switch to Story time if this chat runs on a fictional clock.',
    );

    fixture.componentInstance.chatState.set(state({ timelineMode: 'narrative' }));
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(clockHelp(fixture)).toBe(
      'The tale keeps its own hours: the Commonplace Book files memories by the story’s reckoning ' +
        '— the third night at sea, the eve of the coronation — rather than the calendar on the wall.',
    );
  });

  it('writes {chat:{timelineMode}} , keeps the choice, and asks the parent to refetch', async () => {
    const fixture = await render();
    await choose(fixture, clockSelect(fixture), 'narrative');

    expect(sent).toEqual([
      { type: 'chatUpdate', chatId: 'chat-1', chat: { timelineMode: 'narrative' } },
    ]);
    expect(fixture.componentInstance.refetched).toBe(1);
    // The local adoption keeps the choice immediately; since v4 `bd419ae9`
    // (bug 22) the chat GET also projects the column, so the refetch that
    // follows shows the same value rather than snapping back.
    expect(clockSelect(fixture).value).toBe('narrative');
    expect(toasts().at(-1)).toEqual({
      type: 'success',
      message: 'The story now keeps its own hours',
    });

    await choose(fixture, clockSelect(fixture), 'realtime');
    expect(sent[1]).toEqual({
      type: 'chatUpdate',
      chatId: 'chat-1',
      chat: { timelineMode: 'realtime' },
    });
    expect(clockSelect(fixture).value).toBe('realtime');
    expect(toasts().at(-1)).toEqual({
      type: 'success',
      message: 'The story is back on the clock on the wall',
    });
  });

  it('keeps the previous clock and shows the error when the write fails', async () => {
    const fixture = await render();
    failNext = true;
    await choose(fixture, clockSelect(fixture), 'narrative');

    expect(fixture.componentInstance.refetched).toBe(0);
    expect(clockSelect(fixture).value).toBe('realtime');
    expect(toasts()).toEqual([{ type: 'error', message: 'the clock is stuck' }]);
  });
});

describe('ChatSection — the other per-chat controls', () => {
  beforeEach(() => {
    sent.length = 0;
    sentData.length = 0;
    failNext = false;
    toggleAnswer = { avatarGenerationEnabled: true };
  });

  it('writes the roleplay template, the image profile and the announce tri-state through the chat bag', async () => {
    const fixture = await render();
    const selects = Array.from(
      fixture.nativeElement.querySelectorAll('select'),
    ) as HTMLSelectElement[];
    // Order follows v4's panel: template, clock, image provider, announce.
    const [template, , imageProvider, announce] = selects;

    await choose(fixture, template, '');
    await choose(fixture, imageProvider, '');
    await choose(fixture, announce, 'disabled');

    expect(sent).toEqual([
      { type: 'chatUpdate', chatId: 'chat-1', chat: { roleplayTemplateId: null } },
      { type: 'chatUpdate', chatId: 'chat-1', chat: { imageProfileId: null } },
      { type: 'chatUpdate', chatId: 'chat-1', chat: { alertCharactersOfLanternImages: false } },
    ]);
    expect(toasts().at(-1)).toEqual({
      type: 'success',
      message: 'Lantern images will stay silent for this chat',
    });
  });

  /**
   * The Project entry is UNCONDITIONAL (v4 :1166-1177) — it was the REDUCED
   * Prospero link, shown only when the chat already had a project, until P4.9E3C
   * brought `ChatProjectModal` across. An unfiled chat could not be filed at all.
   */
  it('always offers the project entry, naming the project when there is one', async () => {
    const fixture = await render();
    const entry = (): HTMLButtonElement =>
      Array.from(fixture.nativeElement.querySelectorAll('button.qt-tool-palette-button')).find(
        (b) => ((b as HTMLElement).textContent ?? '').includes('Project'),
      ) as HTMLButtonElement;
    expect(entry().textContent!.trim()).toBe('Project');
    expect(entry().title).toBe('Assign to project');

    fixture.componentInstance.chatState.set(
      state({ projectId: 'p1', projectName: 'The Long Voyage' }),
    );
    fixture.detectChanges();
    expect(entry().textContent).toContain('Project: The Long Voyage');
    expect(entry().title).toBe('In project: The Long Voyage');

    entry().click();
    fixture.detectChanges();
    expect(fixture.componentInstance.projectOpened).toBe(1);
  });
  /** The avatar-generation checkbox (v4 ChatSidebar :1218-1228). */
  function avatarBox(fixture: ComponentFixture<Host>): HTMLInputElement {
    return fixture.nativeElement.querySelector(
      '[aria-label="Auto-generate character avatars"]',
    ) as HTMLInputElement;
  }

  it('reads the avatar-generation switch off the chat’s own projected value', async () => {
    const fixture = await render();
    expect(avatarBox(fixture).checked).toBe(false);
    fixture.componentInstance.chatState.set(state({ avatarGenerationEnabled: true }));
    fixture.detectChanges();
    expect(avatarBox(fixture).checked).toBe(true);
  });

  it('toggles avatar generation, reports v4’s wording, and asks for a refetch', async () => {
    const fixture = await render();
    avatarBox(fixture).dispatchEvent(new Event('change'));
    await fixture.whenStable();
    fixture.detectChanges();
    expect(sentData.filter((r) => r['type'] === 'chatToggleAvatarGeneration')).toEqual([
      { type: 'chatToggleAvatarGeneration', chatId: 'chat-1' },
    ]);
    expect(toasts().at(-1)).toEqual({ type: 'success', message: 'Avatar generation enabled' });
    expect(fixture.componentInstance.refetched).toBe(1);

    toggleAnswer = { avatarGenerationEnabled: false };
    avatarBox(fixture).dispatchEvent(new Event('change'));
    await fixture.whenStable();
    fixture.detectChanges();
    expect(toasts().at(-1)).toEqual({ type: 'success', message: 'Avatar generation disabled' });
  });

  it('reports a failed toggle and does not ask for a refetch', async () => {
    toggleAnswer = new Error('unknown variant');
    const fixture = await render();
    avatarBox(fixture).dispatchEvent(new Event('change'));
    await fixture.whenStable();
    fixture.detectChanges();
    expect(toasts()).toEqual([{ type: 'error', message: 'unknown variant' }]);
    expect(fixture.componentInstance.refetched).toBe(0);
  });

});

/**
 * The agent-mode badge (v4 `ChatSidebar.tsx:1116-1127` +
 * `useChatControls.ts:367-395`). The load-bearing claim is that a TWO-state
 * badge drives a THREE-state verb without ever reaching for the third arm.
 */
describe('ChatSection — the agent-mode badge', () => {
  beforeEach(() => {
    sent.length = 0;
    sentData.length = 0;
    agentAnswer = {
      agentModeEnabled: true,
      resolvedAgentModeEnabled: true,
      agentModeSource: 'chat',
      message: 'Agent mode enabled',
    };
  });

  function badge(fixture: ComponentFixture<Host>): HTMLButtonElement {
    return fixture.nativeElement.querySelector('button.qt-tool-palette-badge');
  }

  it('carries v4’s classes, copy and title for both states', async () => {
    const fixture = await render();
    expect(badge(fixture).className).toContain('qt-tool-palette-badge-off');
    expect(badge(fixture).textContent).toContain('Agent Off');
    expect(badge(fixture).title).toBe('Enable agent mode');

    fixture.componentInstance.chatState.set(state({ agentModeEnabled: true }));
    fixture.detectChanges();
    expect(badge(fixture).className).toContain('qt-tool-palette-badge-on');
    expect(badge(fixture).textContent).toContain('Agent On');
    expect(badge(fixture).title).toBe('Disable agent mode');
  });

  it('sends `true` from a CLEARED override — never the null arm', async () => {
    const fixture = await render();
    badge(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();

    const req = sentData.find((r) => r.type === 'chatToggleAgentMode')!;
    expect(req).toEqual({ type: 'chatToggleAgentMode', chatId: 'chat-1', enabled: true });
    // The tri-state's third arm is reachable on the wire and NOT reachable here.
    expect(req['enabled']).not.toBeNull();
  });

  it('sends `false` from an enabled chat', async () => {
    const fixture = await render();
    fixture.componentInstance.chatState.set(state({ agentModeEnabled: true }));
    fixture.detectChanges();
    agentAnswer = {
      agentModeEnabled: false,
      resolvedAgentModeEnabled: false,
      agentModeSource: 'chat',
      message: 'Agent mode disabled',
    };
    badge(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();

    expect(sentData.find((r) => r.type === 'chatToggleAgentMode')!['enabled']).toBe(false);
    expect(badge(fixture).textContent).toContain('Agent Off');
    expect(toasts()).toEqual([{ type: 'success', message: 'Agent mode disabled' }]);
  });

  it('adopts `agentModeEnabled` when the server drops the resolved key', async () => {
    // The server omits `agentModeEnabled` for a NULL column and always sends
    // `resolvedAgentModeEnabled`; v4's `??` fallback is what covers the reverse.
    const fixture = await render();
    agentAnswer = { agentModeEnabled: true, message: 'Agent mode enabled' };
    badge(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(badge(fixture).textContent).toContain('Agent On');
    // v4 computes the wording from `resolvedAgentModeEnabled` ALONE, so an
    // absent resolved key reads as "set to inherit" even though the badge lit.
    expect(toasts()).toEqual([{ type: 'success', message: 'Agent mode set to inherit' }]);
  });

  it('leaves the badge where it was when the toggle fails', async () => {
    const fixture = await render();
    agentAnswer = new Error('the switchboard is out');
    badge(fixture).click();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(badge(fixture).textContent).toContain('Agent Off');
    expect(toasts()).toEqual([{ type: 'error', message: 'the switchboard is out' }]);
  });
});

describe('ChatSection — the Tools… entry', () => {
  /**
   * This used to pin a loud REFUSAL: the entry was present and answered with a
   * sentence naming the unported tool inventory. P4.9E3C landed
   * `ChatToolSettingsModal` over P4.9E3B's inventory, so the same entry now
   * opens it — and the assertion is that it reports up rather than refusing.
   */
  it('opens the tool-settings modal instead of refusing', async () => {
    const fixture = await render();
    const entry = Array.from(
      fixture.nativeElement.querySelectorAll('button.qt-tool-palette-button'),
    ).find((b) => ((b as HTMLElement).textContent ?? '').includes('Tools…')) as HTMLButtonElement;
    expect(entry).toBeTruthy();
    entry.click();
    fixture.detectChanges();
    expect(fixture.componentInstance.toolSettingsOpened).toBe(1);
    expect(fixture.nativeElement.textContent).not.toContain('has not been ported');
  });
});
