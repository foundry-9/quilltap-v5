import { describe, expect, it } from 'vitest';

import { cardStub, checkboxes, mountCard, settingsRow, settle, toggle } from '../chat/chat-settings.spec-harness';
import { LlmLoggingSettingsCard } from './llm-logging-settings-card';

describe('LlmLoggingSettingsCard', () => {
  it('defaults to enabled/non-verbose/30 days when the bag is absent', async () => {
    const stub = cardStub(settingsRow());
    const fixture = await mountCard(LlmLoggingSettingsCard, stub);
    const [enabled, verbose] = checkboxes(fixture);
    const retention = fixture.nativeElement.querySelector(
      'input[type="number"]',
    ) as HTMLInputElement;
    expect(enabled.checked).toBe(true);
    expect(verbose.checked).toBe(false);
    expect(retention.value).toBe('30');
  });

  it('loads the persisted bag', async () => {
    const stub = cardStub(
      settingsRow({ llmLoggingSettings: { enabled: false, verboseMode: true, retentionDays: 7 } }),
    );
    const fixture = await mountCard(LlmLoggingSettingsCard, stub);
    const [enabled, verbose] = checkboxes(fixture);
    expect(enabled.checked).toBe(false);
    expect(verbose.checked).toBe(true);
    // Verbose + retention rows are disabled while logging is off.
    expect(verbose.disabled).toBe(true);
  });

  it('PUTs the whole bag with the one changed key merged', async () => {
    const stub = cardStub(
      settingsRow({ llmLoggingSettings: { enabled: true, verboseMode: false, retentionDays: 30 } }),
    );
    const fixture = await mountCard(LlmLoggingSettingsCard, stub);
    const [, verbose] = checkboxes(fixture);
    await toggle(fixture, verbose, true);
    expect(stub.updates).toContainEqual({
      llmLoggingSettings: { enabled: true, verboseMode: true, retentionDays: 30 },
    });
  });

  it('coerces an empty retention field to 0', async () => {
    const stub = cardStub(
      settingsRow({ llmLoggingSettings: { enabled: true, verboseMode: false, retentionDays: 30 } }),
    );
    const fixture = await mountCard(LlmLoggingSettingsCard, stub);
    const retention = fixture.nativeElement.querySelector(
      'input[type="number"]',
    ) as HTMLInputElement;
    retention.value = '';
    retention.dispatchEvent(new Event('change'));
    await settle(fixture);
    expect(stub.updates.at(-1)).toEqual({
      llmLoggingSettings: { enabled: true, verboseMode: false, retentionDays: 0 },
    });
  });
});
