import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { ChatSettingsDto } from '../../../core/core-contract';

/**
 * The shared mount/settle harness for the eleven P4.6an Chat-tab card specs.
 * Every card reads through the same `chatSettings` query and writes through the
 * same `chatSettingsUpdate` dispatch, so the stub is written once here.
 *
 * @module screens/settings/chat/chat-settings.spec-harness
 */

/** A canned chat_settings row: only the keys a card reads need to be present. */
export function settingsRow(over: Record<string, unknown> = {}): ChatSettingsDto {
  return {
    avatarDisplayMode: 'ALWAYS',
    avatarDisplayStyle: 'CIRCULAR',
    ...over,
  } as ChatSettingsDto;
}

export interface CardStub {
  /** Every dispatch the card made, in order. */
  calls: { type: string; settings?: Record<string, unknown> }[];
  /** The `chatSettingsUpdate` payloads only — what the PUT assertions read. */
  updates: Record<string, unknown>[];
  client: Partial<CoreClient>;
}

/**
 * A CoreClient stub over the chat-settings dispatch pair. The GET answers with
 * `row`; an update echoes `{...row, ...settings}` the way the real edge does (a
 * partial merge at the server), so the cache seeds with the saved value.
 *
 * `failUpdate: true` makes every update reject — the dogfood-#6 error-surfacing
 * arm.
 */
export function cardStub(row: ChatSettingsDto, failUpdate = false): CardStub {
  const calls: { type: string; settings?: Record<string, unknown> }[] = [];
  const updates: Record<string, unknown>[] = [];
  let current = row;

  const dispatchExpect = vi.fn(
    async (req: { type: string; settings?: Record<string, unknown> }) => {
      calls.push(req);
      if (req.type === 'chatSettingsUpdate') {
        if (failUpdate) throw new Error('boom');
        updates.push(req.settings ?? {});
        current = { ...current, ...(req.settings ?? {}) } as ChatSettingsDto;
        return { type: 'chatSettings', data: current };
      }
      return { type: 'chatSettings', data: current };
    },
  );

  return {
    calls,
    updates,
    client: { dispatchExpect: dispatchExpect as unknown as CoreClient['dispatchExpect'] },
  };
}

/** Flush the query microtasks + the save round-trip (zoneless: no whenStable). */
export async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

/** Mount a card over the stub, with the settings query already resolved. */
export async function mountCard<T>(
  component: new (...args: never[]) => T,
  stub: CardStub,
  extraProviders: unknown[] = [],
): Promise<ComponentFixture<T>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [component as never],
    providers: [
      provideTanStackQuery(new QueryClient()),
      { provide: CoreClient, useValue: stub.client },
      ...(extraProviders as never[]),
    ],
  });
  const fixture = TestBed.createComponent(component as never) as ComponentFixture<T>;
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

/** The card's checkboxes, in DOM order. */
export function checkboxes(fixture: ComponentFixture<unknown>): HTMLInputElement[] {
  return Array.from(
    (fixture.nativeElement as HTMLElement).querySelectorAll('input[type="checkbox"]'),
  );
}

/** Tick/untick a checkbox the way a user would, then let the save settle. */
export async function toggle(
  fixture: ComponentFixture<unknown>,
  el: HTMLInputElement,
  checked: boolean,
): Promise<void> {
  el.checked = checked;
  el.dispatchEvent(new Event('change'));
  await settle(fixture);
}
