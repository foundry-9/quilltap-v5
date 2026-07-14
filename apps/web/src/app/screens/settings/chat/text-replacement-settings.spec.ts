import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../core/core-client';
import type { TextReplacementRule } from '../../../core/core-contract';
import { TextReplacementSettings } from './text-replacement-settings';

function rule(over: Partial<TextReplacementRule>): TextReplacementRule {
  return {
    id: 'r1',
    fromText: 'teh',
    toText: 'the',
    caseSensitive: false,
    enabled: true,
    sortOrder: 0,
    createdAt: '2024-01-01T00:00:00.000Z',
    updatedAt: '2024-01-01T00:00:00.000Z',
    ...over,
  };
}

interface Stub {
  client: Partial<CoreClient>;
  dataCalls: { type: string; [k: string]: unknown }[];
  expectCalls: { type: string; [k: string]: unknown }[];
}

function stub(initialRules: TextReplacementRule[], enabled = true): Stub {
  const dataCalls: { type: string; [k: string]: unknown }[] = [];
  const expectCalls: { type: string; [k: string]: unknown }[] = [];
  let rules = initialRules;
  const client: Partial<CoreClient> = {
    dispatchData: (async (req: { type: string; [k: string]: unknown }) => {
      dataCalls.push(req);
      if (req.type === 'textReplacementsList') return { rules, count: rules.length };
      if (req.type === 'textReplacementCreate') {
        const created = rule({ id: 'new', fromText: req['fromText'] as string, toText: req['toText'] as string });
        rules = [...rules, created];
        return created as unknown as Record<string, unknown>;
      }
      if (req.type === 'textReplacementDelete') {
        rules = rules.filter((r) => r.id !== req['id']);
        return { success: true };
      }
      if (req.type === 'textReplacementUpdate') return rule({ id: req['id'] as string });
      return {};
    }) as CoreClient['dispatchData'],
    dispatchExpect: (async (req: { type: string; [k: string]: unknown }) => {
      expectCalls.push(req);
      return { data: { textReplacementsEnabled: enabled } };
    }) as unknown as CoreClient['dispatchExpect'],
  };
  return { client, dataCalls, expectCalls };
}

async function mount(s: Stub): Promise<ComponentFixture<TextReplacementSettings>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [TextReplacementSettings],
    providers: [{ provide: CoreClient, useValue: s.client }],
  });
  const fixture = TestBed.createComponent(TextReplacementSettings);
  fixture.detectChanges();
  for (let i = 0; i < 6; i++) {
    await Promise.resolve();
    fixture.detectChanges();
  }
  return fixture;
}

describe('TextReplacementSettings', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('loads the rule list and the master toggle', async () => {
    const s = stub([rule({}), rule({ id: 'r2', fromText: 'wat', toText: 'what' })], true);
    const fixture = await mount(s);
    expect(fixture.nativeElement.textContent).toContain('2 rules defined.');
    const master = fixture.nativeElement.querySelector('input[type="checkbox"]') as HTMLInputElement;
    expect(master.checked).toBe(true);
  });

  it('shows the empty state with no rules', async () => {
    const fixture = await mount(stub([], false));
    expect(fixture.nativeElement.textContent).toContain('No rules yet');
    expect(fixture.nativeElement.textContent).toContain('Master toggle is off');
  });

  it('creates a rule from the add form', async () => {
    const s = stub([]);
    const fixture = await mount(s);
    const [trigger, replacement] = Array.from(
      fixture.nativeElement.querySelectorAll('form input[type="text"]'),
    ) as HTMLInputElement[];
    trigger.value = 'teh';
    trigger.dispatchEvent(new Event('input'));
    replacement.value = 'the';
    replacement.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    (fixture.nativeElement.querySelector('form button[type="submit"]') as HTMLButtonElement).click();
    for (let i = 0; i < 6; i++) {
      await Promise.resolve();
      fixture.detectChanges();
    }
    const create = s.dataCalls.find((c) => c.type === 'textReplacementCreate');
    expect(create).toMatchObject({ fromText: 'teh', toText: 'the', caseSensitive: false });
    expect(fixture.nativeElement.textContent).toContain('1 rule defined.');
  });

  it('persists the master toggle through chatSettingsUpdate', async () => {
    const s = stub([], true);
    const fixture = await mount(s);
    const master = fixture.nativeElement.querySelector('input[type="checkbox"]') as HTMLInputElement;
    master.checked = false;
    master.dispatchEvent(new Event('change'));
    await Promise.resolve();
    const upd = s.expectCalls.find((c) => c.type === 'chatSettingsUpdate');
    expect(upd).toMatchObject({ settings: { textReplacementsEnabled: false } });
  });

  it('deletes a rule', async () => {
    const s = stub([rule({ id: 'r9', fromText: 'gone' })]);
    const fixture = await mount(s);
    (fixture.nativeElement.querySelector('li button.qt-button-secondary') as HTMLButtonElement).click();
    for (let i = 0; i < 6; i++) {
      await Promise.resolve();
      fixture.detectChanges();
    }
    const del = s.dataCalls.find((c) => c.type === 'textReplacementDelete');
    expect(del).toMatchObject({ id: 'r9' });
  });

  it('patches a rule when the On toggle is flipped', async () => {
    const s = stub([rule({ id: 'r5', enabled: true })]);
    const fixture = await mount(s);
    // Row checkboxes: [Case, On]; flip On (the last checkbox in the row).
    const rowChecks = Array.from(
      fixture.nativeElement.querySelectorAll('li input[type="checkbox"]'),
    ) as HTMLInputElement[];
    const onToggle = rowChecks[rowChecks.length - 1];
    onToggle.checked = false;
    onToggle.dispatchEvent(new Event('change'));
    for (let i = 0; i < 6; i++) {
      await Promise.resolve();
      fixture.detectChanges();
    }
    const patch = s.dataCalls.find((c) => c.type === 'textReplacementUpdate');
    expect(patch).toMatchObject({ id: 'r5', enabled: false });
  });
});
