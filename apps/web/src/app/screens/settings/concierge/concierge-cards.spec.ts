import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { DEFAULT_CONCIERGE_SETTINGS, type ConciergeSettings } from '../chat/chat-settings.types';
import {
  change,
  check,
  conciergeStub,
  el,
  mountConcierge,
  optionTexts,
  settingsRow,
  text,
  toggleByHeading,
  type ConciergeStub,
  type ProfileRow,
} from './concierge.spec-harness';
import { DisplayCard } from './display-card';
import { OnDutyCard } from './on-duty-card';
import { PreScreeningCard } from './pre-screening-card';
import { parseRefusalCount, RefusalsCard } from './refusals-card';
import { UncensoredDeskCard, profileLabel, withSelected } from './uncensored-desk-card';

/**
 * The five Concierge cards (v4 `components/settings/concierge-settings/*.tsx`
 * at `3b463d6b1`). Every id, label, help text, option and PUT key is pinned
 * here verbatim — the strings ARE the port (`spa-tooltip-copy-lives-in-ts-
 * tables`), so each is read off v4's source, not off this port.
 */

afterEach(() => TestBed.resetTestingModule());

function rowWith(over: Partial<ConciergeSettings> = {}) {
  return settingsRow({ conciergeSettings: { ...DEFAULT_CONCIERGE_SETTINGS, ...over } });
}

function lastConcierge(stub: ConciergeStub): ConciergeSettings {
  const last = stub.updates[stub.updates.length - 1];
  return last['conciergeSettings'] as ConciergeSettings;
}

function helpAfter(select: Element): string {
  return text(select.parentElement?.querySelector('p.qt-text-small'));
}

// ---------------------------------------------------------------------------

describe('Concierge On Duty card', () => {
  it('renders v4\'s one toggle row, verbatim', async () => {
    const fixture = await mountConcierge(OnDutyCard, conciergeStub(rowWith()));
    const box = toggleByHeading(fixture, 'The Concierge is on duty');
    expect(box.checked).toBe(true);
    expect(text(box.closest('label')?.querySelector('.qt-text-small'))).toBe(
      'When a provider declines a Moderated chat, the Concierge carries the request to the uncensored desk, says so in the chat, and may move the chat to Unmoderated after repeated refusals. Turn him off and nothing is rerouted, announced, switched or screened; every chat is answered by its own provider alone, and the per-chat Concierge select is disabled.',
    );
  });

  it('PUTs conciergeSettings.enabled inside the whole object', async () => {
    const stub = conciergeStub(rowWith());
    const fixture = await mountConcierge(OnDutyCard, stub);
    await check(fixture, toggleByHeading(fixture, 'The Concierge is on duty'), false);
    expect(stub.updates).toEqual([
      { conciergeSettings: { ...DEFAULT_CONCIERGE_SETTINGS, enabled: false } },
    ]);
  });
});

// ---------------------------------------------------------------------------

const PRIM: ProfileRow = { id: 'p1', name: 'Prim', provider: 'OPENAI', modelName: 'gpt', apiKey: { id: 'k' } };
const CANDID: ProfileRow = {
  id: 'p2',
  name: 'Candid',
  provider: 'GROK',
  modelName: 'grok',
  isDangerousCompatible: true,
  apiKey: { id: 'k' },
};
const SEER: ProfileRow = {
  id: 'p3',
  name: 'Seer',
  provider: 'OPENROUTER',
  modelName: 'vision',
  isDangerousCompatible: true,
  supportsImageUpload: true,
  apiKey: { id: 'k' },
};
const KEYLESS: ProfileRow = { id: 'p4', name: 'Keyless', provider: 'OLLAMA', apiKey: null };
const PAINTER: ProfileRow = {
  id: 'i1',
  name: 'Painter',
  provider: 'REPLICATE',
  modelName: 'flux',
  isDangerousCompatible: true,
};
const PRUDE_PAINTER: ProfileRow = { id: 'i2', name: 'Prude', provider: 'OPENAI', modelName: 'gpt-image' };

async function mountDesk(over: Partial<ConciergeSettings> = {}, stub?: ConciergeStub) {
  const s =
    stub ??
    conciergeStub(rowWith(over), {
      connectionProfiles: [PRIM, CANDID, SEER, KEYLESS],
      imageProfiles: [PAINTER, PRUDE_PAINTER],
    });
  return { stub: s, fixture: await mountConcierge(UncensoredDeskCard, s) };
}

describe('Concierge Uncensored Desk card', () => {
  it('profileLabel and withSelected are v4\'s', () => {
    expect(profileLabel({ id: 'a', name: 'A', provider: 'P', modelName: 'm' })).toBe('A (P • m)');
    expect(profileLabel({ id: 'a', name: 'A', provider: 'P' })).toBe('A (P)');
    const list = [CANDID];
    expect(withSelected(list, [PRIM, CANDID], null)).toBe(list);
    expect(withSelected(list, [PRIM, CANDID], 'p2')).toBe(list);
    expect(withSelected(list, [PRIM, CANDID], 'p1')).toEqual([CANDID, PRIM]);
    expect(withSelected(list, [PRIM, CANDID], 'gone')).toBe(list);
  });

  it('the three desk selects: ids, labels, empty options, help — verbatim', async () => {
    const { fixture } = await mountDesk();
    const table = [
      [
        'concierge-uncensored-text-profile',
        'Text profile',
        'Auto-detect (first uncensored-compatible profile)',
        'Answers a chat when its provider refuses, and every turn of an Unmoderated chat.',
      ],
      [
        'concierge-uncensored-image-profile',
        'Image profile',
        'Auto-detect (first uncensored-compatible profile)',
        'Paints what the usual image provider refuses to.',
      ],
      [
        'concierge-uncensored-vision-profile',
        'Vision profile',
        'Auto-detect (first uncensored-compatible vision profile)',
        'Describes an attached image when the image-description profile refuses. Must support image attachments.',
      ],
    ];
    for (const [id, label, empty, help] of table) {
      const select = el<HTMLSelectElement>(fixture, `#${id}`);
      expect(text(el(fixture, `label[for="${id}"]`))).toBe(label);
      expect(optionTexts(select)[0]).toBe(empty);
      expect(helpAfter(select)).toBe(help);
    }
  });

  it('filters each desk list: text = compatible, vision = compatible + images, image = compatible image profiles', async () => {
    const { fixture } = await mountDesk();
    expect(optionTexts(el(fixture, '#concierge-uncensored-text-profile')).slice(1)).toEqual([
      'Candid (GROK • grok)',
      'Seer (OPENROUTER • vision)',
    ]);
    expect(optionTexts(el(fixture, '#concierge-uncensored-vision-profile')).slice(1)).toEqual([
      'Seer (OPENROUTER • vision)',
    ]);
    expect(optionTexts(el(fixture, '#concierge-uncensored-image-profile')).slice(1)).toEqual([
      'Painter (REPLICATE • flux)',
    ]);
  });

  it('keeps a stale pick on the list, suffixed, and selected (v4 withSelected)', async () => {
    const { fixture } = await mountDesk({
      uncensoredTextProfileId: 'p1',
      uncensoredImageProfileId: 'i2',
      uncensoredVisionProfileId: 'p2',
    });
    const textSelect = el<HTMLSelectElement>(fixture, '#concierge-uncensored-text-profile');
    expect(optionTexts(textSelect).slice(1)).toEqual([
      'Candid (GROK • grok)',
      'Seer (OPENROUTER • vision)',
      'Prim (OPENAI • gpt) — not marked uncensored-compatible',
    ]);
    expect(textSelect.value).toBe('p1');
    const imageSelect = el<HTMLSelectElement>(fixture, '#concierge-uncensored-image-profile');
    expect(optionTexts(imageSelect).slice(1)).toEqual([
      'Painter (REPLICATE • flux)',
      'Prude (OPENAI • gpt-image) — not marked uncensored-compatible',
    ]);
    expect(imageSelect.value).toBe('i2');
    // Candid IS compatible — just not vision-capable — so it is appended but not suffixed.
    const vision = el<HTMLSelectElement>(fixture, '#concierge-uncensored-vision-profile');
    expect(optionTexts(vision).slice(1)).toEqual([
      'Seer (OPENROUTER • vision)',
      'Candid (GROK • grok)',
    ]);
    expect(vision.value).toBe('p2');
  });

  it('the image prompt crafter lists EVERY connection profile, keyless ones marked', async () => {
    const { fixture } = await mountDesk();
    const crafter = el<HTMLSelectElement>(fixture, '#concierge-image-prompt-profile');
    expect(text(el(fixture, 'label[for="concierge-image-prompt-profile"]'))).toBe(
      'Image prompt crafter',
    );
    expect(optionTexts(crafter)).toEqual([
      'Use the cheap LLM',
      'Prim (OPENAI • gpt)',
      'Candid (GROK • grok)',
      'Seer (OPENROUTER • vision)',
      'Keyless (OLLAMA) ⚠️ No API Key',
    ]);
    expect(helpAfter(crafter)).toBe(
      'Writes the image prompts for the uncensored desk. Any connection profile will do; leave it on the cheap LLM unless that one balks.',
    );
  });

  it('PUTs each pick under its own key; auto-detect / the cheap LLM send an explicit null', async () => {
    const { stub, fixture } = await mountDesk({ uncensoredVisionProfileId: 'p3' });
    await change(fixture, el(fixture, '#concierge-uncensored-text-profile'), 'p2');
    expect(lastConcierge(stub).uncensoredTextProfileId).toBe('p2');
    await change(fixture, el(fixture, '#concierge-uncensored-image-profile'), 'i1');
    expect(lastConcierge(stub).uncensoredImageProfileId).toBe('i1');
    await change(fixture, el(fixture, '#concierge-uncensored-vision-profile'), '');
    expect(lastConcierge(stub).uncensoredVisionProfileId).toBeNull();
    await change(fixture, el(fixture, '#concierge-image-prompt-profile'), 'p4');
    expect(lastConcierge(stub).imagePromptProfileId).toBe('p4');
    await change(fixture, el(fixture, '#concierge-image-prompt-profile'), '');
    const last = lastConcierge(stub);
    expect(last.imagePromptProfileId).toBeNull();
    // Each save carried the earlier ones (the whole object, merged at send time).
    expect(last.uncensoredTextProfileId).toBe('p2');
    expect(last.uncensoredImageProfileId).toBe('i1');
  });

  it('warns, with v4\'s two links, only when no profile of either kind is compatible', async () => {
    const none = conciergeStub(rowWith(), {
      connectionProfiles: [PRIM],
      imageProfiles: [PRUDE_PAINTER],
    });
    const { fixture } = await mountDesk({}, none);
    expect(text(el(fixture, '.qt-alert-warning'))).toBe(
      'No profile is marked uncensored-compatible yet, so the desk has no one to send for. Tick “Uncensored-compatible” on a connection profile in AI Providers or on an image profile in Images.',
    );
    const imageOnly = conciergeStub(rowWith(), {
      connectionProfiles: [PRIM],
      imageProfiles: [PAINTER],
    });
    const second = await mountDesk({}, imageOnly);
    expect((second.fixture.nativeElement as HTMLElement).querySelector('.qt-alert-warning')).toBeNull();
  });

  it('closes with v4\'s info paragraph', async () => {
    const { fixture } = await mountDesk();
    expect(text(el(fixture, '.qt-alert-info'))).toBe(
      'Want the warning badges but never an uncensored model? Set your chats to Locked, or leave every desk profile on auto-detect and untick “Uncensored-compatible” on every profile, so there is no one for the Concierge to send for.',
    );
  });
});

// ---------------------------------------------------------------------------

describe('Concierge Refusals card', () => {
  it('parseRefusalCount: parseInt, NaN ignored, clamped 0–10', () => {
    expect(parseRefusalCount('3')).toBe(3);
    expect(parseRefusalCount('12')).toBe(10);
    expect(parseRefusalCount('-4')).toBe(0);
    expect(parseRefusalCount('4.9')).toBe(4);
    expect(parseRefusalCount('')).toBeNull();
    expect(parseRefusalCount('abc')).toBeNull();
  });

  it('the refusal-count input: id, attributes, label and help verbatim', async () => {
    const fixture = await mountConcierge(RefusalsCard, conciergeStub(rowWith()));
    const input = el<HTMLInputElement>(fixture, '#concierge-auto-switch-after-refusals');
    expect(input.type).toBe('number');
    expect([input.min, input.max, input.step]).toEqual(['0', '10', '1']);
    expect(input.className).toBe('qt-input w-24');
    expect(input.value).toBe('2');
    expect(text(el(fixture, 'label[for="concierge-auto-switch-after-refusals"]'))).toBe(
      'Switch a chat to Unmoderated after this many refusals (0 = never)',
    );
    expect(helpAfter(input)).toBe(
      'Counts only refusals a provider actually states on a Moderated chat. When the tally is reached the Concierge moves the whole chat to Unmoderated and says so; returning the chat to Moderated clears it. Locked chats are never switched.',
    );
  });

  it('saves the count per keystroke, clamped; a NaN keystroke saves nothing', async () => {
    const stub = conciergeStub(rowWith());
    const fixture = await mountConcierge(RefusalsCard, stub);
    const input = el<HTMLInputElement>(fixture, '#concierge-auto-switch-after-refusals');
    await change(fixture, input, '15', 'input');
    expect(lastConcierge(stub).autoSwitchAfterRefusals).toBe(10);
    await change(fixture, input, '', 'input');
    expect(stub.updates).toHaveLength(1);
    await change(fixture, input, '0', 'input');
    expect(lastConcierge(stub).autoSwitchAfterRefusals).toBe(0);
  });

  it('New chats start as: Moderated and Unmoderated only — never Locked', async () => {
    const fixture = await mountConcierge(RefusalsCard, conciergeStub(rowWith()));
    const select = el<HTMLSelectElement>(fixture, '#concierge-new-chats-start-as');
    expect(text(el(fixture, 'label[for="concierge-new-chats-start-as"]'))).toBe('New chats start as');
    expect(Array.from(select.options).map((o) => [o.value, text(o)])).toEqual([
      ['moderated', 'Moderated'],
      ['unmoderated', 'Unmoderated'],
    ]);
    expect(helpAfter(select)).toBe(
      'New chats go to their own provider first; the Concierge steps in only when it refuses.',
    );
  });

  it('PUTs newChatsStartAs and shows the picked state\'s description', async () => {
    const stub = conciergeStub(rowWith());
    const fixture = await mountConcierge(RefusalsCard, stub);
    const select = el<HTMLSelectElement>(fixture, '#concierge-new-chats-start-as');
    await change(fixture, select, 'unmoderated');
    expect(lastConcierge(stub).newChatsStartAs).toBe('unmoderated');
    expect(select.value).toBe('unmoderated');
    expect(helpAfter(select)).toBe(
      'New chats go straight to the uncensored desk from the first message.',
    );
  });
});

// ---------------------------------------------------------------------------

describe('Concierge Display card', () => {
  it('Flagged content: three modes, #76\'s descriptions', async () => {
    const stub = conciergeStub(rowWith());
    const fixture = await mountConcierge(DisplayCard, stub);
    const select = el<HTMLSelectElement>(fixture, '#concierge-display-mode');
    expect(text(el(fixture, 'label[for="concierge-display-mode"]'))).toBe('Flagged content');
    expect(Array.from(select.options).map((o) => [o.value, text(o)])).toEqual([
      ['SHOW', 'Show'],
      ['BLUR', 'Blur'],
      ['COLLAPSE', 'Collapse'],
    ]);
    const expected: Record<string, string> = {
      SHOW: 'Flagged content is shown normally.',
      BLUR: 'Flagged content is blurred until you click to reveal it.',
      COLLAPSE: 'Flagged content is folded away behind a placeholder.',
    };
    expect(helpAfter(select)).toBe(expected['SHOW']);
    for (const mode of ['BLUR', 'COLLAPSE']) {
      await change(fixture, select, mode);
      expect(helpAfter(select)).toBe(expected[mode]);
      expect(lastConcierge(stub).display).toEqual({ mode, showWarningBadges: true });
    }
  });

  it('Show warning badges: PUTs display.showWarningBadges, keeping the mode', async () => {
    const stub = conciergeStub(rowWith({ display: { mode: 'BLUR', showWarningBadges: true } }));
    const fixture = await mountConcierge(DisplayCard, stub);
    const box = toggleByHeading(fixture, 'Show warning badges');
    expect(text(box.closest('label')?.querySelector('.qt-text-small'))).toBe(
      'Display category badges on flagged messages.',
    );
    await check(fixture, box, false);
    expect(lastConcierge(stub).display).toEqual({ mode: 'BLUR', showWarningBadges: false });
  });
});

// ---------------------------------------------------------------------------

const SCAN_HEADINGS = ['Text chat messages', 'Image prompts', 'Image generation'];
const SUMMARY_HEADING =
  "Read each chat's summary in the background and switch it when it looks dangerous";

describe('Concierge Pre-Screening card', () => {
  it('renders the six controls in v4\'s order, every string verbatim', async () => {
    const fixture = await mountConcierge(PreScreeningCard, conciergeStub(rowWith()));
    const root = fixture.nativeElement as HTMLElement;
    const rows = Array.from(root.querySelectorAll('label.qt-settings-toggle-row')).map((r) => [
      text(r.querySelector('.qt-settings-section-heading')),
      text(r.querySelector('.qt-text-small')),
    ]);
    expect(rows).toEqual([
      [
        'Pre-screen before sending',
        'Classify messages and image prompts before they are sent, and route anything flagged on a Moderated chat to the uncensored desk without waiting for a refusal. Costs a classification call per item.',
      ],
      ['Text chat messages', 'Classify your messages before they are sent to the LLM.'],
      ['Image prompts', 'Classify image generation prompts before expansion.'],
      ['Image generation', 'Classify the expanded prompt before it is sent to the image generator.'],
      [
        SUMMARY_HEADING,
        'Every ten minutes the Concierge reads the summaries of Moderated chats and moves any that read as dangerous to Unmoderated, with an announcement. Locked chats are never moved.',
      ],
    ]);
    expect(text(root.querySelector('div.qt-text-label'))).toBe('What to scan');
    expect(text(el(fixture, 'label[for="concierge-threshold"]'))).toBe('Detection threshold (0.7)');
    const range = el<HTMLInputElement>(fixture, '#concierge-threshold');
    expect([range.type, range.min, range.max, range.step]).toEqual(['range', '0.1', '1.0', '0.1']);
    expect(helpAfter(range)).toBe(
      'Lower values flag more content; higher values flag only strongly dangerous content.',
    );
    const area = el<HTMLTextAreaElement>(fixture, '#concierge-custom-classification-prompt');
    expect(area.rows).toBe(3);
    expect(area.placeholder).toBe('Additional instructions for the content classifier...');
    expect(text(el(fixture, 'label[for="concierge-custom-classification-prompt"]'))).toBe(
      'Custom classification prompt (optional)',
    );
    expect(helpAfter(area)).toBe(
      'Appended to the classification prompt. Use it to adjust sensitivity for your use case.',
    );
    expect(Array.from(root.querySelectorAll('.qt-alert-info li')).map((li) => text(li))).toEqual([
      'With an OpenAI connection profile, classification uses the free OpenAI moderation endpoint.',
      'Otherwise it falls back to your cheap LLM, at a small cost per item.',
      'Classification is fail-safe: an error never blocks a message.',
    ]);
  });

  it('only the three scan toggles wait on the pre-screen; the summary read does not', async () => {
    const off = await mountConcierge(PreScreeningCard, conciergeStub(rowWith()));
    for (const h of SCAN_HEADINGS) expect(toggleByHeading(off, h).disabled).toBe(true);
    expect(toggleByHeading(off, SUMMARY_HEADING).disabled).toBe(false);
    expect(toggleByHeading(off, 'Pre-screen before sending').disabled).toBe(false);
    expect(el<HTMLInputElement>(off, '#concierge-threshold').disabled).toBe(false);
    expect(el<HTMLTextAreaElement>(off, '#concierge-custom-classification-prompt').disabled).toBe(
      false,
    );

    const on = await mountConcierge(
      PreScreeningCard,
      conciergeStub(rowWith({ preScreen: { ...DEFAULT_CONCIERGE_SETTINGS.preScreen, enabled: true } })),
    );
    for (const h of SCAN_HEADINGS) expect(toggleByHeading(on, h).disabled).toBe(false);
  });

  it('PUTs each control under preScreen, merged over the rest', async () => {
    const stub = conciergeStub(rowWith({ preScreen: { ...DEFAULT_CONCIERGE_SETTINGS.preScreen, enabled: true } }));
    const fixture = await mountConcierge(PreScreeningCard, stub);
    await check(fixture, toggleByHeading(fixture, 'Image generation'), true);
    expect(lastConcierge(stub).preScreen.scanImageGeneration).toBe(true);
    await check(fixture, toggleByHeading(fixture, SUMMARY_HEADING), true);
    expect(lastConcierge(stub).preScreen.summaryClassification).toBe(true);
    await change(fixture, el(fixture, '#concierge-threshold'), '0.3');
    expect(lastConcierge(stub).preScreen.threshold).toBe(0.3);
    expect(text(el(fixture, 'label[for="concierge-threshold"]'))).toBe('Detection threshold (0.3)');
    const area = el<HTMLTextAreaElement>(fixture, '#concierge-custom-classification-prompt');
    await change(fixture, area, 'be gentle', 'input');
    expect(lastConcierge(stub).preScreen.customClassificationPrompt).toBe('be gentle');
    await change(fixture, area, '', 'input');
    const last = lastConcierge(stub).preScreen;
    expect(last.customClassificationPrompt).toBeNull();
    expect(last).toMatchObject({ enabled: true, scanImageGeneration: true, summaryClassification: true, threshold: 0.3 });
  });
});
