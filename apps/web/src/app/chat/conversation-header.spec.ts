import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { ChatDetail, ChatSettingsDto } from '../core/core-contract';
import { ConversationHeader } from './conversation-header';

function chatDetail(overrides: Partial<ChatDetail> = {}): ChatDetail {
  return {
    id: 'chat-1',
    title: 'Tea',
    contextSummary: null,
    roleplayTemplateId: null,
    chatType: 'salon',
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    isPaused: false,
    isManuallyRenamed: false,
    participants: [],
    user: { id: 'user1', name: 'Bertie', image: null },
    messages: [],
    projectId: null,
    projectName: null,
    turnSkippingEnabled: null,
    agentModeEnabled: false,
    resolvedAgentModeEnabled: false,
    agentModeSource: 'global',
    isDangerousChat: false,
    dangerCategories: [],
    conciergeOverride: null,
    offSceneCharacters: [],
    lastTurnParticipantId: null,
    ...overrides,
  };
}

function render(chat: ChatDetail): ComponentFixture<ConversationHeader> {
  TestBed.configureTestingModule({
    imports: [ConversationHeader],
    providers: [provideRouter([])],
  });
  const fixture = TestBed.createComponent(ConversationHeader);
  fixture.componentRef.setInput('chat', chat);
  fixture.detectChanges();
  return fixture;
}

describe('ConversationHeader — the chat-totals summary gate (v4 SalonView.tsx:990-1027)', () => {
  afterEach(() => TestBed.resetTestingModule());

  function settingsRow(showChatTotals: boolean): ChatSettingsDto {
    return {
      avatarDisplayMode: 'ALWAYS',
      avatarDisplayStyle: 'CIRCULAR',
      tokenDisplaySettings: {
        showPerMessageTokens: false,
        showPerMessageCost: false,
        showChatTotals,
        showSystemEvents: false,
      },
    } as ChatSettingsDto;
  }

  function renderWith(settings: ChatSettingsDto | null): ComponentFixture<ConversationHeader> {
    TestBed.configureTestingModule({
      imports: [ConversationHeader],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        // The summary only mounts when the gate opens; when it does, it needs a
        // CoreClient. A never-resolving fetch is enough — this asserts the GATE,
        // not the summary's own rendering (chat-cost-summary.spec covers that).
        { provide: CoreClient, useValue: { dispatchData: vi.fn(() => new Promise(() => {})) } },
      ],
    });
    const fixture = TestBed.createComponent(ConversationHeader);
    fixture.componentRef.setInput('chat', chatDetail());
    fixture.componentRef.setInput('settings', settings);
    fixture.componentRef.setInput('messageCount', 7);
    fixture.detectChanges();
    return fixture;
  }

  it('mounts the summary when showChatTotals is on', () => {
    const fixture = renderWith(settingsRow(true));
    expect(fixture.nativeElement.querySelector('qt-chat-cost-summary')).not.toBeNull();
  });

  it('omits the summary when showChatTotals is off', () => {
    const fixture = renderWith(settingsRow(false));
    expect(fixture.nativeElement.querySelector('qt-chat-cost-summary')).toBeNull();
  });

  it('omits the summary when settings have not loaded (v4 default false)', () => {
    const fixture = renderWith(null);
    expect(fixture.nativeElement.querySelector('qt-chat-cost-summary')).toBeNull();
  });

  it('keeps the copy-id entry alongside the summary', () => {
    // The summary joins the right cluster; it must not displace what was there.
    const fixture = renderWith(settingsRow(true));
    expect(fixture.nativeElement.querySelector('qt-copy-chat-id-button')).not.toBeNull();
  });
});

describe('ConversationHeader — the LLM-Inspector button (v4 SalonView.tsx:995-1024)', () => {
  afterEach(() => TestBed.resetTestingModule());

  function renderWith(
    settings: ChatSettingsDto | null,
    inspectorOpen = false,
  ): ComponentFixture<ConversationHeader> {
    // Reset first so a test may render more than once.
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [ConversationHeader],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: { dispatchData: vi.fn(() => new Promise(() => {})) } },
      ],
    });
    const fixture = TestBed.createComponent(ConversationHeader);
    fixture.componentRef.setInput('chat', chatDetail());
    fixture.componentRef.setInput('settings', settings);
    fixture.componentRef.setInput('inspectorOpen', inspectorOpen);
    fixture.detectChanges();
    return fixture;
  }

  function button(fixture: ComponentFixture<ConversationHeader>): HTMLButtonElement | null {
    return fixture.nativeElement.querySelector('button[aria-label="Toggle LLM Inspector"]');
  }

  function loggingRow(enabled: boolean | null | undefined): ChatSettingsDto {
    return { llmLoggingSettings: { enabled } } as unknown as ChatSettingsDto;
  }

  it('renders with v4’s title copy and the code glyph', () => {
    const fixture = renderWith(loggingRow(true));
    expect(button(fixture)).not.toBeNull();
    expect(button(fixture)!.getAttribute('title')).toBe('LLM Inspector (Cmd+Shift+L)');
    expect(button(fixture)!.querySelector('qt-icon')!.getAttribute('name')).toBe('code');
  });

  it('DEFAULTS VISIBLE — v4’s gate is `enabled !== false`, not a truthiness check', () => {
    // An absent settings row, an absent bag, an absent key, and an explicit null
    // must all leave the button up. Only `false` hides it.
    expect(button(renderWith(null))).not.toBeNull();
    expect(button(renderWith({} as ChatSettingsDto))).not.toBeNull();
    expect(button(renderWith(loggingRow(undefined)))).not.toBeNull();
    expect(button(renderWith(loggingRow(null)))).not.toBeNull();
  });

  it('hides only on an explicit false', () => {
    expect(button(renderWith(loggingRow(false)))).toBeNull();
  });

  it('swaps to the active classes while the panel is open (v4 :1002-1006)', () => {
    expect(button(renderWith(loggingRow(true), false))!.className).toContain('qt-text-secondary');
    const open = button(renderWith(loggingRow(true), true))!;
    expect(open.className).toContain('qt-bg-primary/15');
    expect(open.className).toContain('text-primary');
  });

  it('emits toggleInspector on click', () => {
    const fixture = renderWith(loggingRow(true));
    let fired = false;
    fixture.componentInstance.toggleInspector.subscribe(() => (fired = true));
    button(fixture)!.click();
    expect(fired).toBe(true);
  });

  it('precedes the cost summary in the right cluster (v4 :995-1024)', () => {
    // The two come out of one v4 toolbar effect and their ORDER is v4's.
    const settings = {
      llmLoggingSettings: { enabled: true },
      tokenDisplaySettings: {
        showPerMessageTokens: false,
        showPerMessageCost: false,
        showChatTotals: true,
        showSystemEvents: false,
      },
    } as unknown as ChatSettingsDto;
    const fixture = renderWith(settings);
    const header = fixture.nativeElement.querySelector('header');
    const inspector = button(fixture)!;
    const summary = header.querySelector('qt-chat-cost-summary');
    expect(summary).not.toBeNull();
    // Node.DOCUMENT_POSITION_FOLLOWING === 4
    expect(inspector.compareDocumentPosition(summary) & 4).toBeTruthy();
  });

  it('renders independently of the cost summary — the gates are separate', () => {
    const fixture = renderWith(loggingRow(true));
    expect(button(fixture)).not.toBeNull();
    expect(fixture.nativeElement.querySelector('qt-chat-cost-summary')).toBeNull();
  });
});

describe('ConversationHeader — the sidebar reclaimed its entries (P4.9H1)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('no longer carries the four entries that only ever lived in v4’s sidebar', () => {
    const fixture = render(chatDetail({ chatType: 'autonomous' }));
    const header = fixture.nativeElement.querySelector('header');
    for (const label of [
      'Toggle all whispers', // → Visibility
      'Edit Enclave', // → Organize
      'Regenerate Background', // → Chat
      'View chat photos', // → Organize
    ]) {
      expect(header.querySelector(`[aria-label="${label}"]`)).toBeNull();
    }
    // What v4's toolbar does carry stays: title, badges, copy-id.
    expect(header.querySelector('qt-copy-chat-id-button')).not.toBeNull();
  });
});

describe('ConversationHeader — the Concierge badge (v4 SalonView.tsx:1088-1112 @ c43d3b1b4)', () => {
  afterEach(() => TestBed.resetTestingModule());

  function badges(fixture: ComponentFixture<ConversationHeader>): HTMLElement[] {
    return Array.from(
      fixture.nativeElement.querySelectorAll('.qt-danger-badge'),
    ) as HTMLElement[];
  }

  /** Advance fake time, then let the render + afterRenderEffect passes run. */
  async function tick(fixture: ComponentFixture<unknown>, ms: number): Promise<void> {
    await vi.advanceTimersByTimeAsync(ms);
    fixture.detectChanges();
    await vi.advanceTimersByTimeAsync(0);
    fixture.detectChanges();
  }

  async function hover(fixture: ComponentFixture<unknown>): Promise<void> {
    (fixture.nativeElement as HTMLElement)
      .querySelector('qt-tooltip')!
      .dispatchEvent(new Event('pointerenter'));
    await tick(fixture, 250);
  }

  const bubble = (): HTMLElement | null => document.body.querySelector('.qt-tooltip');

  it('renders NO badge for Monitored — the pill means "something other than the default"', () => {
    expect(badges(render(chatDetail()))).toHaveLength(0);
    // `render` configures a fresh TestBed, so the second case needs its own.
    TestBed.resetTestingModule();
    expect(badges(render(chatDetail({ isDangerousChat: null })))).toHaveLength(0);
  });

  it('retires the four native titles in favour of the drawn bubble (v4 c43d3b1b4)', () => {
    // The Flagged title used to be the only place the categories were named;
    // the bubble's Categories section is where they live now.
    const [pill] = badges(
      render(chatDetail({ isDangerousChat: true, dangerCategories: ['nsfw', 'violence'] })),
    );
    expect(pill.hasAttribute('title')).toBe(false);
  });

  it('renders the red Flagged pill, labelled and iconed off the presentation table', () => {
    const fixture = render(
      chatDetail({ isDangerousChat: true, dangerCategories: ['nsfw', 'violence'] }),
    );
    const [pill, ...rest] = badges(fixture);
    expect(rest).toHaveLength(0);
    expect(pill.textContent?.trim()).toBe('Flagged');
    expect(pill.getAttribute('role')).toBe('img');
    expect(pill.getAttribute('aria-label')).toBe('Concierge: Flagged');
    expect(pill.className).not.toContain('qt-danger-badge-muted');
    expect(pill.className).not.toContain('qt-danger-badge-info');
    expect(pill.querySelector('qt-icon span[data-icon]')?.getAttribute('data-icon')).toBe(
      'alert-triangle',
    );
  });

  it('renders ONE muted Vouched Safe pill even when the label underneath is true', () => {
    // The pre-existing v5 divergence P4.D141 fixed: two INDEPENDENT `@if` pills
    // rendered BOTH an off-duty and a flagged badge for this exact chat, where
    // v4 renders one.
    const fixture = render(chatDetail({ isDangerousChat: true, conciergeOverride: 'OFF' }));
    const pills = badges(fixture);
    expect(pills).toHaveLength(1);
    expect(pills[0].textContent?.trim()).toBe('Vouched Safe');
    expect(pills[0].className).toContain('qt-danger-badge-muted');
    expect(pills[0].getAttribute('aria-label')).toBe('Concierge: Vouched Safe');
    expect(pills[0].querySelector('qt-icon span[data-icon]')?.getAttribute('data-icon')).toBe(
      'check-circle',
    );
  });

  it('renders ONE info Uncensored pill, never the danger one', () => {
    const fixture = render(
      chatDetail({ isDangerousChat: true, conciergeOverride: 'UNCENSORED' }),
    );
    const pills = badges(fixture);
    expect(pills).toHaveLength(1);
    expect(pills[0].textContent?.trim()).toBe('Uncensored');
    expect(pills[0].className).toContain('qt-danger-badge-info');
    expect(pills[0].getAttribute('aria-label')).toBe('Concierge: Uncensored');
    expect(pills[0].querySelector('qt-icon span[data-icon]')?.getAttribute('data-icon')).toBe(
      'eye-off',
    );
  });

  describe('the bubble (placement="bottom")', () => {
    beforeEach(() => {
      vi.useFakeTimers({
        toFake: [
          'setTimeout',
          'clearTimeout',
          'setInterval',
          'clearInterval',
          'requestAnimationFrame',
          'cancelAnimationFrame',
        ],
      });
    });
    afterEach(() => {
      vi.useRealTimers();
      TestBed.resetTestingModule();
    });

    it("speaks the presentation table's words, and names the categories on Flagged", async () => {
      const fixture = render(
        chatDetail({ isDangerousChat: true, dangerCategories: ['nsfw', 'violence'] }),
      );

      await hover(fixture);

      const text = bubble()!.textContent ?? '';
      expect(text).toContain('Flagged');
      expect(text).toContain(
        'The Concierge has this chat down as dangerous, and routes it through the uncensored providers.',
      );
      expect(text).toContain('Categories');
      expect(text).toContain('nsfw, violence');
      expect(text).toContain("Change it from the Salon sidebar's Chat section.");
      // v4 asks for the bubble BELOW the toolbar; the primitive may still flip
      // it away from a viewport edge, which is why the attribute is read
      // rather than the input.
      expect(bubble()!.getAttribute('data-placement')).toBe('bottom');
    });

    it('never surfaces a preserved category list on an operator state', async () => {
      const fixture = render(
        chatDetail({
          isDangerousChat: true,
          conciergeOverride: 'OFF',
          dangerCategories: ['nsfw'],
        }),
      );

      await hover(fixture);

      const text = bubble()!.textContent ?? '';
      expect(text).toContain(
        'You have vouched for this chat. The Concierge stops watching; the ordinary providers still apply, and may still refuse.',
      );
      expect(text).not.toContain('Categories');
    });
  });
});
