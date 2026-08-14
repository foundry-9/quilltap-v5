import { ChangeDetectionStrategy, Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import type { ChatSettingsDto } from '../core/core-contract';
import { MessageContent } from '../chat/message-content';
import { clearRenderCache } from '../chat/render/render-cache';
import { chatSettingsKeys } from '../screens/settings/chat/chat-settings.api';
import { SmartTypographySettings, readDisplayQuotes, readTypingOptions } from './settings';

/**
 * The one seam: `qt-message-content` reads `smartTypographySettings.displayQuotes`
 * itself, for every surface that renders a message (v4 `MessageContent.tsx:345`
 * does the same, and says why).
 */

function settingsRow(over: Record<string, unknown> = {}): ChatSettingsDto {
  return {
    avatarDisplayMode: 'ALWAYS',
    avatarDisplayStyle: 'CIRCULAR',
    ...over,
  } as ChatSettingsDto;
}

/** A CoreClient stub whose chat-settings GET answers with `row`. */
function coreStub(row: ChatSettingsDto) {
  const dispatchExpect = vi.fn(async () => ({ type: 'chatSettings', data: row }));
  return { dispatchExpect: dispatchExpect as unknown as CoreClient['dispatchExpect'] };
}

async function settle(): Promise<void> {
  for (let i = 0; i < 6; i++) await new Promise((r) => setTimeout(r, 0));
}

describe('readDisplayQuotes', () => {
  it('defaults to false through every missing level', () => {
    expect(readDisplayQuotes(undefined)).toBe(false);
    expect(readDisplayQuotes(null)).toBe(false);
    expect(readDisplayQuotes(settingsRow())).toBe(false);
    expect(readDisplayQuotes(settingsRow({ smartTypographySettings: {} }))).toBe(false);
    expect(readDisplayQuotes(settingsRow({ smartTypographySettings: null }))).toBe(false);
  });

  it('reads the stored value when present', () => {
    expect(
      readDisplayQuotes(settingsRow({ smartTypographySettings: { displayQuotes: true } })),
    ).toBe(true);
    expect(
      readDisplayQuotes(settingsRow({ smartTypographySettings: { displayQuotes: false } })),
    ).toBe(false);
  });
});

describe('SmartTypographySettings', () => {
  it('is inert without a QueryClient — the signal stays false', () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ providers: [] });
    // No provideTanStackQuery, no CoreClient: a bare component spec. The
    // service must not reach for either, which is what keeps
    // `qt-message-content` mountable with no query layer at all.
    expect(TestBed.inject(SmartTypographySettings).displayQuotes()).toBe(false);
  });

  it('fetches the row itself when nobody else has (the Brahma/help-chat case)', async () => {
    TestBed.resetTestingModule();
    const client = new QueryClient();
    const core = coreStub(settingsRow({ smartTypographySettings: { displayQuotes: true } }));
    TestBed.configureTestingModule({
      providers: [provideTanStackQuery(client), { provide: CoreClient, useValue: core }],
    });
    const service = TestBed.inject(SmartTypographySettings);
    expect(service.displayQuotes()).toBe(false); // nothing cached yet
    await settle();
    expect(service.displayQuotes()).toBe(true);
    expect(core.dispatchExpect).toHaveBeenCalled();
  });

  it('picks up a row already in the cache without fetching again', async () => {
    TestBed.resetTestingModule();
    const client = new QueryClient();
    client.setQueryData(
      chatSettingsKeys.all,
      settingsRow({ smartTypographySettings: { displayQuotes: true } }),
    );
    const core = coreStub(settingsRow());
    TestBed.configureTestingModule({
      providers: [provideTanStackQuery(client), { provide: CoreClient, useValue: core }],
    });
    expect(TestBed.inject(SmartTypographySettings).displayQuotes()).toBe(true);
  });

  it('follows the setting when the card saves a new value', async () => {
    TestBed.resetTestingModule();
    const client = new QueryClient();
    client.setQueryData(chatSettingsKeys.all, settingsRow());
    TestBed.configureTestingModule({
      providers: [provideTanStackQuery(client), { provide: CoreClient, useValue: coreStub(settingsRow()) }],
    });
    const service = TestBed.inject(SmartTypographySettings);
    await settle();
    expect(service.displayQuotes()).toBe(false);

    // What the settings card does after a successful PUT: seed the cache.
    client.setQueryData(
      chatSettingsKeys.all,
      settingsRow({ smartTypographySettings: { displayQuotes: true } }),
    );
    expect(service.displayQuotes()).toBe(true);

    client.setQueryData(chatSettingsKeys.all, settingsRow({ smartTypographySettings: { displayQuotes: false } }));
    expect(service.displayQuotes()).toBe(false);
  });

  it('leaves the default in place when the settings fetch fails', async () => {
    TestBed.resetTestingModule();
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const failing = {
      dispatchExpect: vi.fn(async () => {
        throw new Error('boom');
      }) as unknown as CoreClient['dispatchExpect'],
    };
    TestBed.configureTestingModule({
      providers: [provideTanStackQuery(client), { provide: CoreClient, useValue: failing }],
    });
    const service = TestBed.inject(SmartTypographySettings);
    await settle();
    // A renderer must never surface a settings error; it just renders plainly.
    expect(service.displayQuotes()).toBe(false);
  });
});

describe('qt-message-content — the rendered bytes follow the setting', () => {
  const LINE = '"Hello there," she said warmly.';

  async function mount(row: ChatSettingsDto | null) {
    clearRenderCache();
    TestBed.resetTestingModule();
    const client = new QueryClient();
    if (row) client.setQueryData(chatSettingsKeys.all, row);
    TestBed.configureTestingModule({
      imports: [MessageContent],
      providers: [
        provideTanStackQuery(client),
        { provide: CoreClient, useValue: coreStub(row ?? settingsRow()) },
      ],
    });
    const fixture = TestBed.createComponent(MessageContent);
    fixture.componentRef.setInput('content', LINE);
    fixture.detectChanges();
    await settle();
    fixture.detectChanges();
    return fixture;
  }

  it('renders straight quotes by default', async () => {
    const fixture = await mount(settingsRow());
    expect((fixture.nativeElement as HTMLElement).innerHTML).toContain('"Hello there,"');
  });

  it('renders curled quotes when the setting is on', async () => {
    const fixture = await mount(settingsRow({ smartTypographySettings: { displayQuotes: true } }));
    expect((fixture.nativeElement as HTMLElement).innerHTML).toContain('“Hello there,”');
  });

  it('repaints an already-rendered message when the setting flips', async () => {
    const fixture = await mount(settingsRow());
    expect((fixture.nativeElement as HTMLElement).innerHTML).toContain('"Hello there,"');

    TestBed.inject(QueryClient).setQueryData(
      chatSettingsKeys.all,
      settingsRow({ smartTypographySettings: { displayQuotes: true } }),
    );
    fixture.detectChanges();
    // The memo must not go on serving the straight-quoted render — this is the
    // cache-key half of the feature, seen from the component.
    expect((fixture.nativeElement as HTMLElement).innerHTML).toContain('“Hello there,”');
  });
});

describe('readTypingOptions', () => {
  it('defaults BOTH rules to ON through every missing level (v4 `?? true`)', () => {
    expect(readTypingOptions(undefined)).toEqual({ dashes: true, ellipsis: true });
    expect(readTypingOptions(settingsRow())).toEqual({ dashes: true, ellipsis: true });
    expect(readTypingOptions(settingsRow({ smartTypographySettings: {} }))).toEqual({
      dashes: true,
      ellipsis: true,
    });
  });

  it('reads each rule independently — one off is a legal state', () => {
    expect(
      readTypingOptions(settingsRow({ smartTypographySettings: { dashes: false } })),
    ).toEqual({ dashes: false, ellipsis: true });
    expect(
      readTypingOptions(settingsRow({ smartTypographySettings: { ellipsis: false } })),
    ).toEqual({ dashes: true, ellipsis: false });
    expect(
      readTypingOptions(
        settingsRow({ smartTypographySettings: { dashes: false, ellipsis: false } }),
      ),
    ).toEqual({ dashes: false, ellipsis: false });
  });

  it('is independent of displayQuotes — Part A and Part B share a bag, not a switch', () => {
    const row = settingsRow({ smartTypographySettings: { displayQuotes: true, dashes: false } });
    expect(readDisplayQuotes(row)).toBe(true);
    expect(readTypingOptions(row)).toEqual({ dashes: false, ellipsis: true });
  });
});

describe('SmartTypographySettings — the typing half', () => {
  it("holds v4's defaults with no query layer at all", () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ providers: [] });
    expect(TestBed.inject(SmartTypographySettings).typing()).toEqual({
      dashes: true,
      ellipsis: true,
    });
  });

  it('follows the saved bag, and keeps the reference stable when nothing moved', async () => {
    TestBed.resetTestingModule();
    const client = new QueryClient();
    client.setQueryData(chatSettingsKeys.all, settingsRow());
    TestBed.configureTestingModule({
      providers: [
        provideTanStackQuery(client),
        { provide: CoreClient, useValue: coreStub(settingsRow()) },
      ],
    });
    const service = TestBed.inject(SmartTypographySettings);
    const before = service.typing();

    // An unrelated settings save must not hand the plugin a new closure value.
    client.setQueryData(chatSettingsKeys.all, settingsRow({ composerSpellcheck: false }));
    expect(service.typing()).toBe(before);

    client.setQueryData(
      chatSettingsKeys.all,
      settingsRow({ smartTypographySettings: { dashes: false } }),
    );
    expect(service.typing()).toEqual({ dashes: false, ellipsis: true });
  });
});

/**
 * Mounted through a PARENT template rather than as a TestBed root — the shape
 * every real surface uses, and the one under which the service is first
 * `inject()`ed mid-render.
 *
 * ⚠ Read the honest limit of this case: it did NOT catch the NG0600 regression
 * that shipped in unit 5 and was found by the live walk (a signal written while
 * the service was constructed, unwinding the render and leaving the chat list
 * empty). Re-introducing that write leaves this case GREEN — TestBed's creation
 * pass evidently does not reproduce the browser's reactive-consumer window. The
 * guard for that bug is the Playwright walk, not this spec; what this one adds
 * is that the renderer still paints when its settings dependency is constructed
 * from inside a host template at all.
 */
@Component({
  selector: 'qt-host-for-message-content',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [MessageContent],
  template: `<qt-message-content [content]="text" />`,
})
class MessageContentHost {
  readonly text = '"Hello there," she said warmly.';
}

describe('qt-message-content — constructed from a parent template', () => {
  it('renders when the settings service is first injected mid-render', async () => {
    clearRenderCache();
    TestBed.resetTestingModule();
    const client = new QueryClient();
    client.setQueryData(
      chatSettingsKeys.all,
      settingsRow({ smartTypographySettings: { displayQuotes: true } }),
    );
    TestBed.configureTestingModule({
      imports: [MessageContentHost],
      providers: [
        provideTanStackQuery(client),
        { provide: CoreClient, useValue: coreStub(settingsRow()) },
      ],
    });
    const fixture = TestBed.createComponent(MessageContentHost);
    fixture.detectChanges();
    await settle();
    fixture.detectChanges();

    expect((fixture.nativeElement as HTMLElement).innerHTML).toContain('\u201cHello there,\u201d');
  });
});
