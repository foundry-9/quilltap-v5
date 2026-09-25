import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { ChatSettingsDto } from '../../../core/core-contract';
import { settingsRow, settle } from '../chat/chat-settings.spec-harness';

/**
 * The Concierge specs' shared stub (P4.D230). One CoreClient over the three
 * reads the tab makes (`chatSettings`, `connectionProfileList`,
 * `imageProfileList`) and the one write (`chatSettingsUpdate`).
 *
 * The update echoes `{ ...row, ...settings }` — a TOP-LEVEL merge, so a sent
 * `conciergeSettings` REPLACES the stored one whole, exactly as the server's
 * `ConciergeSettingsSchema.safeParse` does. A partial object sent here would
 * therefore read back partial, which is what the deep-merge specs rely on.
 *
 * @module screens/settings/concierge/concierge.spec-harness
 */

export interface ProfileRow {
  id: string;
  name: string;
  provider: string;
  modelName?: string;
  isDefault?: boolean;
  isDangerousCompatible?: boolean;
  supportsImageUpload?: boolean;
  apiKey?: unknown;
}

export interface ConciergeStub {
  /** Every `chatSettingsUpdate` payload, in dispatch order. */
  updates: Record<string, unknown>[];
  client: Partial<CoreClient>;
  /** The row the stub now serves. */
  current: () => ChatSettingsDto;
}

export function conciergeStub(
  row: ChatSettingsDto = settingsRow(),
  opts: {
    connectionProfiles?: ProfileRow[];
    imageProfiles?: ProfileRow[];
    failUpdate?: boolean;
    failRead?: boolean;
  } = {},
): ConciergeStub {
  const updates: Record<string, unknown>[] = [];
  let current = row;
  const connectionProfiles = opts.connectionProfiles ?? [];

  const dispatchExpect = vi.fn(
    async (req: { type: string; settings?: Record<string, unknown> }) => {
      if (req.type === 'chatSettingsUpdate') {
        updates.push(req.settings ?? {});
        if (opts.failUpdate) throw new Error('boom');
        current = { ...current, ...(req.settings ?? {}) } as ChatSettingsDto;
        return { type: 'chatSettings', data: current };
      }
      if (req.type === 'connectionProfileList') {
        return {
          type: 'connectionProfiles',
          data: { profiles: connectionProfiles, count: connectionProfiles.length },
        };
      }
      if (opts.failRead) throw new Error('read failed');
      return { type: 'chatSettings', data: current };
    },
  );
  const dispatchData = vi.fn(async (req: { type: string }) => {
    if (req.type === 'imageProfileList') return { profiles: opts.imageProfiles ?? [] };
    return {};
  });

  return {
    updates,
    current: () => current,
    client: {
      dispatchExpect: dispatchExpect as unknown as CoreClient['dispatchExpect'],
      dispatchData: dispatchData as unknown as CoreClient['dispatchData'],
    },
  };
}

/** Configure the module (a fresh query client; retries off so a failed read settles). */
export function configureConcierge(stub: ConciergeStub, extraProviders: unknown[] = []): void {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    providers: [
      provideRouter([]),
      provideTanStackQuery(new QueryClient({ defaultOptions: { queries: { retry: false } } })),
      { provide: CoreClient, useValue: stub.client },
      ...(extraProviders as never[]),
    ],
  });
}

/** Mount one component over the stub and let its queries resolve. */
export async function mountConcierge<T>(
  component: new (...args: never[]) => T,
  stub: ConciergeStub,
  extraProviders: unknown[] = [],
): Promise<ComponentFixture<T>> {
  configureConcierge(stub, extraProviders);
  const fixture = TestBed.createComponent(component as never) as ComponentFixture<T>;
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

export function el<T extends Element = HTMLElement>(fixture: ComponentFixture<unknown>, sel: string): T {
  const found = (fixture.nativeElement as HTMLElement).querySelector(sel);
  if (!found) throw new Error(`no element ${sel}`);
  return found as T;
}

/** Collapse runs of whitespace — Angular's template text keeps one space per line break. */
export function text(node: Element | null | undefined): string {
  return (node?.textContent ?? '').replace(/\s+/g, ' ').trim();
}

export function optionTexts(select: HTMLSelectElement): string[] {
  return Array.from(select.options).map((o) => text(o));
}

/** Change a select/input the way a user does, then let the save settle. */
export async function change(
  fixture: ComponentFixture<unknown>,
  target: HTMLSelectElement | HTMLInputElement | HTMLTextAreaElement,
  value: string,
  event: 'change' | 'input' = 'change',
): Promise<void> {
  target.value = value;
  target.dispatchEvent(new Event(event));
  await settle(fixture);
}

export async function check(
  fixture: ComponentFixture<unknown>,
  box: HTMLInputElement,
  checked: boolean,
): Promise<void> {
  box.checked = checked;
  box.dispatchEvent(new Event('change'));
  await settle(fixture);
}

/** The toggle row whose heading reads `heading` → its checkbox. */
export function toggleByHeading(fixture: ComponentFixture<unknown>, heading: string): HTMLInputElement {
  const rows = Array.from(
    (fixture.nativeElement as HTMLElement).querySelectorAll('label.qt-settings-toggle-row'),
  );
  const row = rows.find((r) => text(r.querySelector('.qt-settings-section-heading')) === heading);
  if (!row) throw new Error(`no toggle row "${heading}"`);
  return row.querySelector('input[type="checkbox"]') as HTMLInputElement;
}

export { settingsRow, settle };
