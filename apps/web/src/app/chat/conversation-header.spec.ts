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
    conciergeState: 'moderated',
    conciergeSetBy: null,
    conciergeReason: null,
    conciergeRefusalCount: 0,
    offSceneCharacters: [],
    lastTurnParticipantId: null,
    ...overrides,
  };
}

/** Matches outside the breadcrumb's measuring copies (`aria-hidden` + `inert`). */
function visible(fixture: ComponentFixture<unknown>, selector: string): HTMLElement[] {
  return (
    Array.from((fixture.nativeElement as HTMLElement).querySelectorAll(selector)) as HTMLElement[]
  ).filter((el) => el.closest('[inert]') === null);
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
    // The summary sits in the right cluster; the copy-id button rides the
    // breadcrumb on the left (v4 3c41cf3da) — neither displaces the other.
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

describe('ConversationHeader — the Concierge badge (v4 SalonView.tsx:1190-1222 @ ce2f1dabf)', () => {
  afterEach(() => TestBed.resetTestingModule());

  /** The VISIBLE row's pills — the breadcrumb's inert measuring copies carry their own. */
  function badges(fixture: ComponentFixture<ConversationHeader>): HTMLElement[] {
    return visible(fixture, '.qt-danger-badge');
  }

  /** Advance fake time, then let the render + afterRenderEffect passes run. */
  async function tick(fixture: ComponentFixture<unknown>, ms: number): Promise<void> {
    await vi.advanceTimersByTimeAsync(ms);
    fixture.detectChanges();
    await vi.advanceTimersByTimeAsync(0);
    fixture.detectChanges();
  }

  async function hover(fixture: ComponentFixture<ConversationHeader>): Promise<void> {
    badges(fixture)[0].closest('qt-tooltip')!.dispatchEvent(new Event('pointerenter'));
    await tick(fixture, 250);
  }

  const bubble = (): HTMLElement | null => document.body.querySelector('.qt-tooltip');

  it('renders NO badge for Moderated — the pill means "something other than the default"', () => {
    expect(badges(render(chatDetail()))).toHaveLength(0);
    // `render` configures a fresh TestBed, so the second case needs its own.
    TestBed.resetTestingModule();
    // The legacy telemetry is read by no display: a dangerous label on a
    // Moderated chat still draws nothing.
    expect(badges(render(chatDetail({ isDangerousChat: true })))).toHaveLength(0);
    TestBed.resetTestingModule();
    expect(badges(render(chatDetail({ conciergeState: undefined })))).toHaveLength(0);
  });

  it('carries no native title — the drawn bubble is the only one', () => {
    const [pill] = badges(
      render(chatDetail({ conciergeState: 'unmoderated', conciergeSetBy: 'operator' })),
    );
    expect(pill.hasAttribute('title')).toBe(false);
  });

  it('renders the red Unmoderated pill, labelled and iconed off the presentation table', () => {
    for (const setBy of ['operator', 'concierge'] as const) {
      TestBed.resetTestingModule();
      const fixture = render(chatDetail({ conciergeState: 'unmoderated', conciergeSetBy: setBy }));
      const [pill, ...rest] = badges(fixture);
      expect(rest).toHaveLength(0);
      expect(pill.textContent?.trim()).toBe('Unmoderated');
      expect(pill.getAttribute('role')).toBe('img');
      expect(pill.getAttribute('aria-label')).toBe('Concierge: Unmoderated');
      // ONE tone whoever set it — provenance is never a colour.
      expect(pill.className).not.toContain('qt-danger-badge-muted');
      expect(pill.querySelector('qt-icon span[data-icon]')?.getAttribute('data-icon')).toBe(
        'eye-off',
      );
    }
  });

  it('renders ONE muted Locked pill', () => {
    const fixture = render(chatDetail({ conciergeState: 'locked', conciergeSetBy: 'operator' }));
    const pills = badges(fixture);
    expect(pills).toHaveLength(1);
    expect(pills[0].textContent?.trim()).toBe('Locked');
    expect(pills[0].className).toContain('qt-danger-badge-muted');
    expect(pills[0].getAttribute('aria-label')).toBe('Concierge: Locked');
    expect(pills[0].querySelector('qt-icon span[data-icon]')?.getAttribute('data-icon')).toBe(
      'shield',
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

    it("names the classifier's categories when the classifier moved the chat", async () => {
      const fixture = render(
        chatDetail({
          conciergeState: 'unmoderated',
          conciergeSetBy: 'concierge',
          conciergeReason: 'classifier',
          dangerCategories: ['nsfw', 'violence'],
        }),
      );

      await hover(fixture);

      const text = bubble()!.textContent ?? '';
      expect(text).toContain('Unmoderated');
      expect(text).toContain(
        'The Concierge moved this chat to the uncensored desk on reading the conversation. Set it back to Moderated if you disagree.',
      );
      expect(text).toContain('Categories');
      expect(text).toContain('nsfw, violence');
      expect(text).toContain("Change it from the Salon sidebar's Chat section.");
      // v4 asks for the bubble BELOW the toolbar; the primitive may still flip
      // it away from a viewport edge, which is why the attribute is read
      // rather than the input.
      expect(bubble()!.getAttribute('data-placement')).toBe('bottom');
    });

    it('is the ONE place that names the refusal count (v4 SalonView :1197-1201)', async () => {
      const fixture = render(
        chatDetail({
          conciergeState: 'unmoderated',
          conciergeSetBy: 'concierge',
          conciergeReason: 'refusals',
          conciergeRefusalCount: 2,
          dangerCategories: ['nsfw'],
        }),
      );

      await hover(fixture);

      const text = bubble()!.textContent ?? '';
      expect(text).toContain(
        'The Concierge moved this chat to the uncensored desk after two refusals. Set it back to Moderated if you disagree.',
      );
      // A refusals move never lists the categories.
      expect(text).not.toContain('Categories');
    });

    it("never surfaces a preserved category list on the operator's state", async () => {
      const fixture = render(
        chatDetail({
          conciergeState: 'unmoderated',
          conciergeSetBy: 'operator',
          conciergeReason: 'manual',
          dangerCategories: ['nsfw'],
        }),
      );

      await hover(fixture);

      const text = bubble()!.textContent ?? '';
      expect(text).toContain(
        'You have opened the uncensored door yourself. Nothing here goes near a moderated provider.',
      );
      expect(text).not.toContain('Categories');
    });
  });
});

describe("ConversationHeader — v4's left / right split (3c41cf3da, ruling R3)", () => {
  afterEach(() => TestBed.resetTestingModule());

  const FOLLOWING = 4; // Node.DOCUMENT_POSITION_FOLLOWING

  function renderFull(): ComponentFixture<ConversationHeader> {
    TestBed.configureTestingModule({
      imports: [ConversationHeader],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: { dispatchData: vi.fn(() => new Promise(() => {})) } },
      ],
    });
    const fixture = TestBed.createComponent(ConversationHeader);
    fixture.componentRef.setInput(
      'chat',
      chatDetail({
        projectId: 'proj-1',
        projectName: 'The Brass Owl',
        conciergeState: 'unmoderated',
        conciergeSetBy: 'operator',
      }),
    );
    fixture.componentRef.setInput('settings', {
      llmLoggingSettings: { enabled: true },
      tokenDisplaySettings: {
        showPerMessageTokens: false,
        showPerMessageCost: false,
        showChatTotals: true,
        showSystemEvents: false,
      },
    } as unknown as ChatSettingsDto);
    fixture.componentRef.setInput('characters', [
      { id: 'char-1', name: 'Abigail', avatarUrl: null },
    ]);
    fixture.componentRef.setInput('storyBackground', {
      src: '/api/v1/files/bg-1',
      filename: 'bg.png',
      fileId: 'bg-1',
    });
    fixture.detectChanges();
    return fixture;
  }

  it('orders the left row project / character / background / Concierge / title / copy-id', () => {
    const fixture = renderFull();
    const crumb = fixture.nativeElement.querySelector('qt-conversation-breadcrumb') as HTMLElement;
    const pick = (sel: string): HTMLElement => visible(fixture, sel)[0];
    const sequence = [
      pick('a[href="/prospero/proj-1"]'),
      pick('a[href="/characters/char-1?tab=conversations"]'),
      pick('button[aria-label="Display story background"]'),
      pick('.qt-danger-badge'),
      pick('a[href="/salon/chat-1"]'),
      pick('qt-copy-chat-id-button'),
    ];
    for (const el of sequence) {
      expect(el).toBeTruthy();
      expect(crumb.contains(el)).toBe(true);
    }
    for (let i = 1; i < sequence.length; i++) {
      expect(sequence[i - 1].compareDocumentPosition(sequence[i]) & FOLLOWING).toBeTruthy();
    }
  });

  it('keeps the Inspector and the summary right of the spacer, outside the breadcrumb', () => {
    const fixture = renderFull();
    const header = fixture.nativeElement.querySelector('header') as HTMLElement;
    const children = Array.from(header.children) as HTMLElement[];
    expect(children[0].tagName.toLowerCase()).toBe('qt-conversation-breadcrumb');
    expect(children[1].className).toBe('flex-1');
    expect(children[2].getAttribute('aria-label')).toBe('Toggle LLM Inspector');
    expect(children[3].tagName.toLowerCase()).toBe('qt-chat-cost-summary');
    expect(children).toHaveLength(4);
  });

  it('re-emits the thumbnail click as openStoryBackground', () => {
    const fixture = renderFull();
    const seen: unknown[] = [];
    fixture.componentInstance.openStoryBackground.subscribe((bg) => seen.push(bg));
    visible(fixture, 'button[aria-label="Display story background"]')[0].click();
    expect(seen).toEqual([{ src: '/api/v1/files/bg-1', filename: 'bg.png', fileId: 'bg-1' }]);
  });

  it("falls back to v5's Untitled chat for an empty title", () => {
    TestBed.configureTestingModule({ imports: [ConversationHeader], providers: [provideRouter([])] });
    const fixture = TestBed.createComponent(ConversationHeader);
    fixture.componentRef.setInput('chat', chatDetail({ title: '' }));
    fixture.detectChanges();
    expect(visible(fixture, 'a[href="/salon/chat-1"]')[0].textContent?.trim()).toBe(
      'Untitled chat',
    );
  });
});
