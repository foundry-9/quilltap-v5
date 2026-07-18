import { afterEach, describe, expect, it } from 'vitest';
import { TestBed } from '@angular/core/testing';

import { CustomToolsSettings } from './custom-tools-settings';
import { cardStub, checkboxes, mountCard, settingsRow, toggle } from './chat-settings.spec-harness';

describe('CustomToolsSettings (v4 CustomToolsSettings.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('defaults the toggle ON when the setting is unset (v4 DEFAULT_CUSTOM_TOOLS = true)', async () => {
    const fixture = await mountCard(CustomToolsSettings, cardStub(settingsRow()));
    expect(checkboxes(fixture)[0].checked).toBe(true);
  });

  it('reflects an explicit false', async () => {
    const fixture = await mountCard(CustomToolsSettings, cardStub(settingsRow({ customTools: false })));
    expect(checkboxes(fixture)[0].checked).toBe(false);
  });

  it('writes the customTools scalar on toggle', async () => {
    const stub = cardStub(settingsRow({ customTools: true }));
    const fixture = await mountCard(CustomToolsSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], false);
    expect(stub.updates).toEqual([{ customTools: false }]);
  });

  it('surfaces a save failure', async () => {
    const stub = cardStub(settingsRow({ customTools: true }), true);
    const fixture = await mountCard(CustomToolsSettings, stub);
    await toggle(fixture, checkboxes(fixture)[0], false);
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';
    expect(text).toContain('Failed to update custom tools setting');
  });

  it('carries the Workbench button, live even while the toggle is OFF', async () => {
    // Authoring while the table isn't dealing custom tools is legitimate — v4
    // keeps the link visible either way. (This assertion replaces the P4.6ba
    // placeholder that pinned the button's ABSENCE while the Workbench was
    // unported.)
    const off = await mountCard(CustomToolsSettings, cardStub(settingsRow({ customTools: false })));
    const button = [...(off.nativeElement as HTMLElement).querySelectorAll('button')].find((b) =>
      b.textContent?.includes('Workbench'),
    );
    expect(button).toBeDefined();
    expect(button?.disabled).toBe(false);
    expect(button?.textContent).toContain("Open Pascal’s Workbench");
  });
});
