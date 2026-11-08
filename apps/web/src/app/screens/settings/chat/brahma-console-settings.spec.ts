import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import { ToastService } from '../../../ui/toast.service';
import { BrahmaConsoleSettings } from './brahma-console-settings';

/**
 * The Brahma Console budget card (P4.D59, v4 `6452e2c3`). Mirrors the Data
 * Retention card's number-input behaviour over the `brahmaConsoleSettings` /
 * `brahmaConsoleSettingsUpdate` dispatch verbs: load-on-init, commit-on-blur,
 * revert-out-of-range-without-nagging, no-op-when-unchanged.
 *
 * The card reads/writes through `CoreClient.dispatchData`, so the stub answers at
 * that seam (the settings precedent). The PUT echoes the STORED value, which is
 * what the card renders next.
 */
const INLINE_PROFILES = [
  { id: 'p1', name: 'Aria GPT', provider: 'OPENAI', modelName: 'gpt-5', isDefault: true, allowToolUse: true },
  { id: 'p2', name: 'Bare', provider: 'OLLAMA', modelName: '', isDefault: false, allowToolUse: true },
];

function stubClient(initial: number, onUpdate?: (turns: number) => void): Partial<CoreClient> {
  return {
    dispatchData: (async (req: { type: string; maxAgentTurns?: number }) => {
      if (req.type === 'brahmaConsoleSettings') {
        return { maxAgentTurns: initial };
      }
      if (req.type === 'brahmaConsoleSettingsUpdate') {
        onUpdate?.(req.maxAgentTurns as number);
        return { maxAgentTurns: req.maxAgentTurns };
      }
      return {};
    }) as unknown as CoreClient['dispatchData'],
  };
}

async function mount(
  client: Partial<CoreClient>,
): Promise<ComponentFixture<BrahmaConsoleSettings>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [BrahmaConsoleSettings],
    providers: [
      // P4.D388: the inline-engine select reads the shared profiles query.
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      {
        provide: CoreClient,
        useValue: {
          dispatchExpect: (async () => ({
            type: 'connectionProfiles',
            data: { profiles: INLINE_PROFILES },
          })) as unknown as CoreClient['dispatchExpect'],
          ...client,
        },
      },
    ],
  });
  const fixture = TestBed.createComponent(BrahmaConsoleSettings);
  fixture.detectChanges();
  // Flush the constructor's async load (and the profiles query), then render.
  for (let i = 0; i < 3; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
  return fixture;
}

function input(fixture: ComponentFixture<BrahmaConsoleSettings>): HTMLInputElement | null {
  return fixture.nativeElement.querySelector('#brahma-max-turns');
}

describe('BrahmaConsoleSettings', () => {
  it('loads the persisted turn budget into the input', async () => {
    const fixture = await mount(stubClient(80));
    expect(input(fixture)?.value).toBe('80');
  });

  it('falls back to the default (50) when the setting is unset', async () => {
    const fixture = await mount({
      dispatchData: (async () => ({})) as unknown as CoreClient['dispatchData'],
    });
    expect(input(fixture)?.value).toBe('50');
  });

  it('persists a valid new value on blur', async () => {
    const onUpdate = vi.fn();
    const fixture = await mount(stubClient(50, onUpdate));
    const el = input(fixture)!;
    el.value = '120';
    el.dispatchEvent(new Event('input'));
    el.dispatchEvent(new Event('blur'));
    await Promise.resolve();
    expect(onUpdate).toHaveBeenCalledWith(120);
  });

  it('reverts an out-of-range entry without persisting', async () => {
    const onUpdate = vi.fn();
    const fixture = await mount(stubClient(50, onUpdate));
    const el = input(fixture)!;
    el.value = '500';
    el.dispatchEvent(new Event('input'));
    fixture.detectChanges(); // render the intermediate draft so the revert is a visible change
    el.dispatchEvent(new Event('blur'));
    await Promise.resolve();
    fixture.detectChanges();
    expect(onUpdate).not.toHaveBeenCalled();
    expect(input(fixture)?.value).toBe('50');
  });

  it('reverts a below-minimum entry without persisting', async () => {
    const onUpdate = vi.fn();
    const fixture = await mount(stubClient(50, onUpdate));
    const el = input(fixture)!;
    el.value = '4';
    el.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    el.dispatchEvent(new Event('blur'));
    await Promise.resolve();
    fixture.detectChanges();
    expect(onUpdate).not.toHaveBeenCalled();
    expect(input(fixture)?.value).toBe('50');
  });

  it('skips the round-trip when the value is unchanged', async () => {
    const onUpdate = vi.fn();
    const fixture = await mount(stubClient(50, onUpdate));
    const el = input(fixture)!;
    el.value = '50';
    el.dispatchEvent(new Event('input'));
    el.dispatchEvent(new Event('blur'));
    await Promise.resolve();
    expect(onUpdate).not.toHaveBeenCalled();
  });
});

/**
 * P4.D388 (C10): v4 `c539c82e0`'s `InlineBrahmaProfileSetting`
 * (`BrahmaConsoleSettings.tsx:24-70` at the pin `624d0d25c`).
 */
describe('BrahmaConsoleSettings — the inline Brahma engine (P4.D388, v4 c539c82e0)', () => {
  type Req = { type: string; maxAgentTurns?: number; inlineConnectionProfileId?: string | null };

  function inlineClient(
    stored: Record<string, unknown>,
    onUpdate: (req: Req) => Record<string, unknown> = (req) => ({ ...stored, ...req, type: undefined }),
  ): { client: Partial<CoreClient>; seen: Req[] } {
    const seen: Req[] = [];
    return {
      seen,
      client: {
        dispatchData: (async (req: Req) => {
          if (req.type === 'brahmaConsoleSettings') return stored;
          if (req.type === 'brahmaConsoleSettingsUpdate') {
            seen.push(req);
            return onUpdate(req);
          }
          return {};
        }) as unknown as CoreClient['dispatchData'],
      },
    };
  }

  const engine = (f: ComponentFixture<BrahmaConsoleSettings>) =>
    f.nativeElement.querySelector('#brahma-inline-profile') as HTMLSelectElement | null;
  const toasts = () =>
    TestBed.inject(ToastService)
      .toasts()
      .map((t) => ({ type: t.type, message: t.message }));

  it('renders nothing of the section while the settings load', async () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [BrahmaConsoleSettings],
      providers: [
        provideTanStackQuery(new QueryClient()),
        { provide: CoreClient, useValue: { dispatchData: () => new Promise(() => undefined) } },
      ],
    });
    const fixture = TestBed.createComponent(BrahmaConsoleSettings);
    fixture.detectChanges();
    expect(engine(fixture)).toBeNull();
    expect(fixture.nativeElement.textContent).not.toContain('Engine for');
  });

  it("carries v4's label, paragraph and select, wrapped with the budget in space-y-6", async () => {
    const { client } = inlineClient({ maxAgentTurns: 50 });
    const fixture = await mount(client);
    const el = fixture.nativeElement as HTMLElement;
    const label = el.querySelector('label[for="brahma-inline-profile"]')!;
    expect(label.className).toBe('qt-text-label block');
    expect(label.textContent?.replace(/\s+/g, ' ').trim()).toBe('Engine for @Brahma in the Salon');
    expect(label.querySelector('code')?.textContent).toBe('@Brahma');
    const section = label.parentElement!;
    expect(section.className).toBe('space-y-2');
    const para = section.querySelector('p')!;
    expect(para.className).toBe('qt-text-small qt-text-muted');
    expect(para.textContent?.replace(/\s+/g, ' ').trim()).toBe(
      'Summon Brahma mid-conversation with @Brahma: or @Brahma? and he consults the ledgers on whichever engine is named here. A project may name an engine of its own on its Prospero page, which takes precedence within that project’s chats; should the chosen profile be dismissed from service, Brahma quietly falls back on your default.',
    );
    expect(engine(fixture)!.options[0].textContent?.trim()).toBe('Use my default profile');
    expect(engine(fixture)!.getAttribute('class')).toBe('qt-select w-full max-w-md');
    expect(section.parentElement!.className).toBe('space-y-6');
    // The budget block comes first inside the same wrapper.
    expect(section.parentElement!.querySelector('#brahma-max-turns')).not.toBeNull();
  });

  it('shows the stored engine, and inherit when none is stored', async () => {
    const a = await mount(inlineClient({ maxAgentTurns: 50, inlineConnectionProfileId: 'p2' }).client);
    expect(engine(a)!.value).toBe('p2');
    const b = await mount(inlineClient({ maxAgentTurns: 50 }).client);
    expect(engine(b)!.value).toBe('');
  });

  it('a stored id naming no profile shows "(a profile that has since left the building)"', async () => {
    const fixture = await mount(inlineClient({ maxAgentTurns: 50, inlineConnectionProfileId: 'gone' }).client);
    const select = engine(fixture)!;
    expect(select.value).toBe('gone');
    expect(select.options[select.selectedIndex].textContent?.trim()).toBe(
      '(a profile that has since left the building)',
    );
  });

  it("saving sends ONLY inlineConnectionProfileId, adopts the response, toasts v4's sentence", async () => {
    const { client, seen } = inlineClient({ maxAgentTurns: 80 });
    const fixture = await mount(client);
    const select = engine(fixture)!;
    select.value = 'p1';
    select.dispatchEvent(new Event('change'));
    for (let i = 0; i < 3; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(seen).toEqual([{ type: 'brahmaConsoleSettingsUpdate', inlineConnectionProfileId: 'p1' }]);
    expect(engine(fixture)!.value).toBe('p1');
    expect(toasts()).toEqual([{ type: 'success', message: 'Inline Brahma engine saved' }]);
    // The budget is untouched.
    expect((fixture.nativeElement.querySelector('#brahma-max-turns') as HTMLInputElement).value).toBe('80');
  });

  it('choosing inherit sends null', async () => {
    const { client, seen } = inlineClient({ maxAgentTurns: 50, inlineConnectionProfileId: 'p1' });
    const fixture = await mount(client);
    const select = engine(fixture)!;
    select.value = '';
    select.dispatchEvent(new Event('change'));
    await new Promise((r) => setTimeout(r, 0));
    expect(seen).toEqual([{ type: 'brahmaConsoleSettingsUpdate', inlineConnectionProfileId: null }]);
  });

  it('a budget save never carries inlineConnectionProfileId', async () => {
    const { client, seen } = inlineClient({ maxAgentTurns: 50, inlineConnectionProfileId: 'p1' });
    const fixture = await mount(client);
    const el = fixture.nativeElement.querySelector('#brahma-max-turns') as HTMLInputElement;
    el.value = '120';
    el.dispatchEvent(new Event('input'));
    el.dispatchEvent(new Event('blur'));
    await new Promise((r) => setTimeout(r, 0));
    expect(seen).toEqual([{ type: 'brahmaConsoleSettingsUpdate', maxAgentTurns: 120 }]);
  });

  it("today's v5 server (it echoes {maxAgentTurns} alone) leaves the select on inherit — the gated reason", async () => {
    const { client } = inlineClient({ maxAgentTurns: 50 }, () => ({ maxAgentTurns: 50 }));
    const fixture = await mount(client);
    const select = engine(fixture)!;
    select.value = 'p1';
    select.dispatchEvent(new Event('change'));
    for (let i = 0; i < 3; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(select.value).toBe('');
  });

  it('a failed save logs at debug and toasts the server sentence (or v4\'s fallback)', async () => {
    const debug = vi.spyOn(console, 'debug').mockImplementation(() => undefined);
    const { client } = inlineClient({ maxAgentTurns: 50 }, () => {
      throw new Error('Validation error: inlineConnectionProfileId');
    });
    const fixture = await mount(client);
    const select = engine(fixture)!;
    select.value = 'p1';
    select.dispatchEvent(new Event('change'));
    for (let i = 0; i < 3; i++) {
      await new Promise((r) => setTimeout(r, 0));
      fixture.detectChanges();
    }
    expect(toasts()).toEqual([{ type: 'error', message: 'Validation error: inlineConnectionProfileId' }]);
    expect(debug).toHaveBeenCalledWith('[BrahmaConsoleSettings] Inline profile save failed', {
      error: 'Validation error: inlineConnectionProfileId',
    });
    expect(select.value).toBe('');
    debug.mockRestore();
  });
});
