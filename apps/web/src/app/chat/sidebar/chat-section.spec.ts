import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { beforeEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { ConciergeState } from '../concierge-state';
import type { ConciergeProvenanceNote } from '../concierge-state-presentation';
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
/** The shared `chatSettingsKeys.all` read — the Concierge's on-duty source (§S.2). */
let chatSettingsAnswer: Record<string, unknown> = {};
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
    dispatchExpect: (async (req: Req) =>
      req['type'] === 'chatSettings'
        ? { type: 'chatSettings', data: chatSettingsAnswer }
        : {
            type: 'apiKeys',
            data: { apiKeys: [], count: 0 },
          }) as unknown as CoreClient['dispatchExpect'],
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
      [conciergeState]="conciergeState()"
      [conciergeProvenance]="conciergeProvenance()"
      (chatUpdated)="refetched = refetched + 1"
      (openProject)="projectOpened = projectOpened + 1"
      (openToolSettings)="toolSettingsOpened = toolSettingsOpened + 1"
    />
  `,
})
class Host {
  readonly chatState = signal<ChatSectionState>(state());
  readonly conciergeState = signal<ConciergeState | undefined>(undefined);
  readonly conciergeProvenance = signal<ConciergeProvenanceNote>({});
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
    // Scoped by LABEL, not by index. This used to destructure the section's
    // `<select>` list positionally, which broke the moment P4.D141 added the
    // Concierge control at the head of the panel (v4's own slot) — and worse,
    // silently drove the wrong control before it broke.
    const byLabel = (text: string): HTMLSelectElement => {
      const labels = Array.from(fixture.nativeElement.querySelectorAll('label')) as HTMLElement[];
      const label = labels.find((l) => l.textContent?.includes(text));
      if (!label) throw new Error(`no <label> containing ${JSON.stringify(text)}`);
      return label.querySelector('select') as HTMLSelectElement;
    };
    const template = byLabel('Roleplay Template');
    const imageProvider = byLabel('Image Provider');
    const announce = byLabel('Announce Generated Images');

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

/** The Concierge control's `<select>` (P4.D141). */
function conciergeSelect(fixture: ComponentFixture<Host>): HTMLSelectElement {
  const labels = Array.from(fixture.nativeElement.querySelectorAll('label')) as HTMLElement[];
  const label = labels.find((l) => l.textContent?.includes('The Concierge'))!;
  return label.querySelector('select') as HTMLSelectElement;
}

function conciergeHelp(fixture: ComponentFixture<Host>): string {
  const labels = Array.from(fixture.nativeElement.querySelectorAll('label')) as HTMLElement[];
  const label = labels.find((l) => l.textContent?.includes('The Concierge'))!;
  return (label.querySelector('span.qt-text-secondary') as HTMLElement)
    .textContent!.replace(/\s+/g, ' ')
    .trim();
}

describe('ChatSection — the Concierge three-state (v4 ChatSidebar.tsx @ ce2f1dabf)', () => {
  beforeEach(() => {
    sent.length = 0;
    failNext = false;
    chatSettingsAnswer = {};
  });

  async function show(
    fixture: ComponentFixture<Host>,
    state: ConciergeState | undefined,
    provenance: ConciergeProvenanceNote = {},
  ): Promise<void> {
    fixture.componentInstance.conciergeState.set(state);
    fixture.componentInstance.conciergeProvenance.set(provenance);
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
  }

  it('offers the three states as a FLAT list of bare labels, in control order', async () => {
    const fixture = await render();
    const select = conciergeSelect(fixture);
    expect(select.querySelectorAll('optgroup')).toHaveLength(0);
    expect(Array.from(select.options).map((o) => [o.value, o.textContent?.trim()])).toEqual([
      ['moderated', 'Moderated'],
      ['unmoderated', 'Unmoderated'],
      ['locked', 'Locked'],
    ]);
  });

  it('shows the server-derived state, reading an absent one as Moderated', async () => {
    const fixture = await render();
    await show(fixture, undefined);
    expect(conciergeSelect(fixture).value).toBe('moderated');
    await show(fixture, 'unmoderated', { setBy: 'operator' });
    expect(conciergeSelect(fixture).value).toBe('unmoderated');
    await show(fixture, 'locked', { setBy: 'operator' });
    expect(conciergeSelect(fixture).value).toBe('locked');
  });

  it('carries the table’s helper sentence, picked by provenance, with the refusal count', async () => {
    const fixture = await render();
    await show(fixture, 'moderated');
    expect(conciergeHelp(fixture)).toBe(
      'The Concierge sends everything to the usual providers first, and to the uncensored desk only when one of them refuses. After enough refusals he moves the whole chat himself.',
    );
    await show(fixture, 'unmoderated', { setBy: 'operator', reason: 'manual' });
    expect(conciergeHelp(fixture)).toBe(
      'You have opened the uncensored door yourself. Nothing here goes near a moderated provider.',
    );
    await show(fixture, 'unmoderated', { setBy: 'concierge', reason: 'refusals', refusalCount: 2 });
    expect(conciergeHelp(fixture)).toBe(
      'The Concierge moved this chat to the uncensored desk after two refusals. Set it back to Moderated if you disagree.',
    );
    await show(fixture, 'unmoderated', { setBy: 'concierge', reason: 'classifier' });
    expect(conciergeHelp(fixture)).toBe(
      'The Concierge moved this chat to the uncensored desk on reading the conversation. Set it back to Moderated if you disagree.',
    );
    await show(fixture, 'locked', { setBy: 'operator' });
    expect(conciergeHelp(fixture)).toBe(
      'Only the usual providers, ever. If one refuses, the refusal stands. For the chat that must never reach an uncensored model.',
    );
  });

  it('gives each state its own icon — the second, colourblind-safe channel', async () => {
    const fixture = await render();
    const icon = async (state: ConciergeState) => {
      await show(fixture, state);
      const labels = Array.from(fixture.nativeElement.querySelectorAll('label')) as HTMLElement[];
      const label = labels.find((l) => l.textContent?.includes('The Concierge'))!;
      // `qt-icon` aliases `class` to an INPUT that lands on the inner span (the
      // host itself is `display: contents`), so the tint is read there.
      const span = label.querySelector('qt-icon span[data-icon]') as HTMLElement;
      return { name: span.getAttribute('data-icon'), tint: span.className };
    };
    expect(await icon('moderated')).toMatchObject({ name: 'eye' });
    expect((await icon('moderated')).tint).toContain('qt-text-success');
    expect(await icon('unmoderated')).toMatchObject({ name: 'eye-off' });
    expect((await icon('unmoderated')).tint).toContain('qt-text-danger');
    expect(await icon('locked')).toMatchObject({ name: 'shield' });
    expect((await icon('locked')).tint).toContain('qt-text-muted');
  });

  it('PUTs conciergeState as a SIBLING of the chat bag, not a bag key', async () => {
    const fixture = await render();
    await choose(fixture, conciergeSelect(fixture), 'unmoderated');
    const put = sent.find((r) => r['type'] === 'chatUpdate')!;
    expect(put['conciergeState']).toBe('unmoderated');
    // v4's `chatUpdateRequestSchema` declares it at the TOP level; a bag key
    // would be stripped by `updateChatSchema` and silently do nothing.
    expect(put['chat']).toEqual({});
    expect(fixture.componentInstance.refetched).toBe(1);
  });

  it('raises v4’s own success sentence for each state', async () => {
    for (const [value, message] of [
      ['unmoderated', 'The uncensored door stands open'],
      ['locked', 'Locked to the usual desks'],
      ['moderated', 'The Concierge is on watch'],
    ] as const) {
      const fixture = await render();
      // Start somewhere else so every pick is a real change.
      await show(fixture, value === 'locked' ? 'unmoderated' : 'locked', { setBy: 'operator' });
      await choose(fixture, conciergeSelect(fixture), value);
      expect(toasts().at(-1)).toEqual({ type: 'success', message });
    }
  });

  it('reverts the select AND the model when the write fails', async () => {
    const fixture = await render();
    await show(fixture, 'locked', { setBy: 'operator' });
    expect(conciergeSelect(fixture).value).toBe('locked');

    failNext = true;
    await choose(fixture, conciergeSelect(fixture), 'unmoderated');
    expect(toasts().at(-1)).toEqual({ type: 'error', message: 'the clock is stuck' });
    // The rejected choice must not be left on screen (the standing
    // controlled-select idiom: reverting the signal alone is not enough).
    expect(conciergeSelect(fixture).value).toBe('locked');
    expect(fixture.componentInstance.refetched).toBe(0);
  });

  it('fires the PUT even when the pick matches the stored state (v4 `:1097-1123` has no short-circuit)', async () => {
    const fixture = await render();
    await choose(fixture, conciergeSelect(fixture), 'moderated');
    expect(sent.filter((r) => r['type'] === 'chatUpdate')).toHaveLength(1);
  });

  it('holds no latch: after a successful pick the control shows the STORED state, then follows the refetch (v4 derives from props)', async () => {
    const fixture = await render();
    await choose(fixture, conciergeSelect(fixture), 'unmoderated');
    expect(sent.filter((r) => r['type'] === 'chatUpdate')).toHaveLength(1);
    expect(fixture.componentInstance.refetched).toBe(1);
    // The stored state has not moved yet, so the element reads the stored state.
    expect(conciergeSelect(fixture).value).toBe('moderated');
    // The parent's refetch lands the new state → the control follows it.
    await show(fixture, 'unmoderated', { setBy: 'operator' });
    expect(conciergeSelect(fixture).value).toBe('unmoderated');
    // … and a later change from elsewhere (an auto-switch, another tab) wins too.
    await show(fixture, 'locked', { setBy: 'operator' });
    expect(conciergeSelect(fixture).value).toBe('locked');
  });

  // --- The on-duty gate (v4 `3b463d6b1` #76) -----------------------------------

  /** Let the shared chat-settings query resolve and the view catch up. */
  async function settle(fixture: ComponentFixture<Host>): Promise<void> {
    for (let i = 0; i < 6; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
  }

  it('is enabled on duty — the default while the settings row has no conciergeSettings', async () => {
    const fixture = await render();
    expect(conciergeSelect(fixture).disabled).toBe(false);
    expect(fixture.nativeElement.querySelector('qt-concierge-off-duty-hint')).toBeNull();
  });

  it('is DISABLED off duty, the helper replaced by the hint that points at the switch', async () => {
    chatSettingsAnswer = { conciergeSettings: { enabled: false } };
    const fixture = await render();
    await show(fixture, 'unmoderated', { setBy: 'operator' });
    await settle(fixture);
    expect(conciergeSelect(fixture).disabled).toBe(true);
    const labels = Array.from(fixture.nativeElement.querySelectorAll('label')) as HTMLElement[];
    const label = labels.find((l) => l.textContent?.includes('The Concierge'))!;
    expect(label.textContent!.replace(/\s+/g, ' ')).toContain(
      'The Concierge is off duty — turn him on in Settings → The Concierge.',
    );
    expect(label.textContent).not.toContain('opened the uncensored door yourself');
    const link = label.querySelector('qt-concierge-off-duty-hint a') as HTMLAnchorElement;
    expect(link.getAttribute('href')).toBe('/settings?tab=concierge&section=on-duty');
    // The select still SHOWS the chat's state — disabled, not hidden.
    expect(conciergeSelect(fixture).value).toBe('unmoderated');
  });

  it('flips live when a Concierge-tab save lands on the shared settings key', async () => {
    chatSettingsAnswer = { conciergeSettings: { enabled: false } };
    const fixture = await render();
    await settle(fixture);
    expect(conciergeSelect(fixture).disabled).toBe(true);
    // The Settings tab `setQueryData`s the SAME key (chatSettingsKeys.all).
    TestBed.inject(QueryClient).setQueryData(['chatSettings'], {
      conciergeSettings: { enabled: true },
    });
    await settle(fixture);
    expect(conciergeSelect(fixture).disabled).toBe(false);
    expect(fixture.nativeElement.querySelector('qt-concierge-off-duty-hint')).toBeNull();
  });
});
