import type { ComponentFixture } from '@angular/core/testing';
import { TestBed } from '@angular/core/testing';
import { ActivatedRoute } from '@angular/router';
import { of } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { DEFAULT_CONCIERGE_SETTINGS } from '../chat/chat-settings.types';
import {
  conciergeStub,
  mountConcierge,
  optionTexts,
  settingsRow,
  text,
  type ConciergeStub,
  type ProfileRow,
} from './concierge.spec-harness';
import { ConciergeTab } from './concierge-tab';

/**
 * The Concierge's own Settings tab (v4 `__tests__/unit/components/settings/
 * ConciergeTabContent.test.tsx` at `3b463d6b1`, its six cases by name).
 *
 * Pins what the deep links and the help promise: the five sections exist under
 * their stable ids, `?section=uncensored-desk` opens and scrolls to the desk,
 * the collapsed-by-default pre-screen opens for its own deep link, and a house
 * with no uncensored-compatible profile is told where to tick one.
 */

const SECTION_IDS = ['on-duty', 'uncensored-desk', 'refusals', 'display', 'pre-screening'];

const PRIM: ProfileRow = {
  id: 'p1',
  name: 'Prim',
  provider: 'OPENAI',
  modelName: 'gpt',
  isDefault: true,
  isDangerousCompatible: false,
};
const CANDID: ProfileRow = {
  id: 'p2',
  name: 'Candid',
  provider: 'GROK',
  modelName: 'grok',
  isDefault: false,
  isDangerousCompatible: true,
};

function routeWith(section: string | null) {
  return {
    provide: ActivatedRoute,
    useValue: { queryParamMap: of({ get: (k: string) => (k === 'section' ? section : null) }) },
  };
}

async function mountTab(
  opts: { section?: string | null; stub?: ConciergeStub; hosted?: boolean } = {},
): Promise<ComponentFixture<ConciergeTab>> {
  const stub =
    opts.stub ?? conciergeStub(settingsRow({ conciergeSettings: DEFAULT_CONCIERGE_SETTINGS }));
  const fixture = await mountConcierge(
    ConciergeTab,
    stub,
    opts.hosted ? [] : [routeWith(opts.section ?? null)],
  );
  // The force-open scroll runs from a requestAnimationFrame — let one pass.
  await new Promise((r) => setTimeout(r, 50));
  return fixture;
}

function root(fixture: ComponentFixture<unknown>): HTMLElement {
  return fixture.nativeElement as HTMLElement;
}

function sectionHeader(fixture: ComponentFixture<unknown>, sectionId: string): HTMLElement {
  const section = root(fixture).querySelector(`#${sectionId}`);
  if (!section) throw new Error(`section ${sectionId} not rendered`);
  return section.querySelector('button') as HTMLElement;
}

function selectByLabel(fixture: ComponentFixture<unknown>, label: string): HTMLSelectElement {
  const lab = Array.from(root(fixture).querySelectorAll('label')).find((l) => text(l) === label);
  const id = lab?.getAttribute('for');
  if (!id) throw new Error(`no label "${label}"`);
  return root(fixture).querySelector(`#${id}`) as HTMLSelectElement;
}

function link(fixture: ComponentFixture<unknown>, within: Element, name: string): HTMLAnchorElement | undefined {
  return Array.from(within.querySelectorAll('a')).find((a) => text(a) === name) as
    | HTMLAnchorElement
    | undefined;
}

describe('ConciergeTabContent', () => {
  let scrollIntoView: ReturnType<typeof vi.fn>;
  let savedScroll: unknown;

  beforeEach(() => {
    const proto = HTMLElement.prototype as unknown as { scrollIntoView?: unknown };
    savedScroll = proto.scrollIntoView;
    scrollIntoView = vi.fn();
    proto.scrollIntoView = scrollIntoView;
  });

  afterEach(() => {
    (HTMLElement.prototype as unknown as { scrollIntoView?: unknown }).scrollIntoView = savedScroll;
    vi.restoreAllMocks();
    TestBed.resetTestingModule();
  });

  it('renders the five sections under their stable ids', async () => {
    const fixture = await mountTab();
    for (const id of SECTION_IDS) {
      expect(root(fixture).querySelector(`#${id}`)).not.toBeNull();
    }
    const titles = Array.from(root(fixture).querySelectorAll('.qt-card-title')).map((t) => text(t));
    expect(titles).toEqual([
      'On Duty',
      'The Uncensored Desk',
      'When a Provider Refuses',
      'Display',
      'Pre-Screening (Advanced)',
    ]);
  });

  it('keeps pre-screening collapsed by default', async () => {
    const fixture = await mountTab();
    expect(sectionHeader(fixture, 'pre-screening').getAttribute('aria-expanded')).toBe('false');
    expect(sectionHeader(fixture, 'uncensored-desk').getAttribute('aria-expanded')).toBe('true');
    for (const id of ['on-duty', 'refusals', 'display']) {
      expect(sectionHeader(fixture, id).getAttribute('aria-expanded')).toBe('true');
    }
    expect(scrollIntoView).not.toHaveBeenCalled();
  });

  it('force-opens and scrolls to the desk for ?section=uncensored-desk', async () => {
    const fixture = await mountTab({ section: 'uncensored-desk' });
    expect(sectionHeader(fixture, 'uncensored-desk').getAttribute('aria-expanded')).toBe('true');
    expect(selectByLabel(fixture, 'Text profile')).not.toBeNull();
    expect(scrollIntoView).toHaveBeenCalledTimes(1);
    // v5's `qt-collapsible-card` scrolls its HOST element, which wraps the
    // `#uncensored-desk` div (v4 scrolls the div itself — the same box on screen).
    const desk = root(fixture).querySelector('#uncensored-desk') as HTMLElement;
    expect(scrollIntoView.mock.instances[0]).toBe(desk.parentElement);
  });

  it('opens the collapsed pre-screen for ?section=pre-screening', async () => {
    const fixture = await mountTab({ section: 'pre-screening' });
    expect(sectionHeader(fixture, 'pre-screening').getAttribute('aria-expanded')).toBe('true');
    expect(text(root(fixture))).toContain('Pre-screen before sending');
  });

  it('points at AI Providers and Images when no profile is uncensored-compatible', async () => {
    const stub = conciergeStub(settingsRow({ conciergeSettings: DEFAULT_CONCIERGE_SETTINGS }), {
      connectionProfiles: [PRIM],
    });
    const fixture = await mountTab({ stub });
    const desk = root(fixture).querySelector('#uncensored-desk') as HTMLElement;
    expect(link(fixture, desk, 'AI Providers')?.getAttribute('href')).toBe('/settings?tab=providers');
    expect(link(fixture, desk, 'Images')?.getAttribute('href')).toBe('/settings?tab=images');
  });

  it('lists only uncensored-compatible profiles on the desk, and says nothing about ticking one', async () => {
    const stub = conciergeStub(settingsRow({ conciergeSettings: DEFAULT_CONCIERGE_SETTINGS }), {
      connectionProfiles: [PRIM, CANDID],
    });
    const fixture = await mountTab({ stub });
    const desk = root(fixture).querySelector('#uncensored-desk') as HTMLElement;
    expect(link(fixture, desk, 'AI Providers')).toBeUndefined();
    const optionLabels = optionTexts(selectByLabel(fixture, 'Text profile'));
    expect(optionLabels.some((l) => l.includes('Candid'))).toBe(true);
    expect(optionLabels.some((l) => l.includes('Prim'))).toBe(false);

    // The crafter may be any connection profile.
    const crafterLabels = optionTexts(selectByLabel(fixture, 'Image prompt crafter'));
    expect(crafterLabels[0]).toBe('Use the cheap LLM');
    expect(crafterLabels.some((l) => l.includes('Prim'))).toBe(true);
  });
});

describe('ConciergeTab — v5 edges', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('carries the hard-coded intro (ruling E.6 — v4 reads the Foundry registry default)', async () => {
    const fixture = await mountTab();
    expect(text(root(fixture).querySelector('p.italic'))).toBe(
      'Who gets asked when the usual providers refuse, and how flagged content is shown',
    );
  });

  it('shows the off-duty banner only while the Concierge is off duty', async () => {
    const on = await mountTab();
    expect(root(on).querySelector('.qt-alert-warning.mb-4')).toBeNull();

    const off = await mountTab({
      stub: conciergeStub(
        settingsRow({ conciergeSettings: { ...DEFAULT_CONCIERGE_SETTINGS, enabled: false } }),
      ),
    });
    expect(text(root(off).querySelector('.qt-alert-warning.mb-4'))).toBe(
      'The Concierge is off duty. Nothing is rerouted, announced, switched or screened until he is back at his post.',
    );
  });

  it('reads an absent conciergeSettings as the defaults (on duty, no banner)', async () => {
    const fixture = await mountTab({ stub: conciergeStub(settingsRow()) });
    expect(root(fixture).querySelector('.qt-alert-warning.mb-4')).toBeNull();
    expect(root(fixture).querySelector('#on-duty')).not.toBeNull();
  });

  it('shows v4\'s failure line when the settings read fails', async () => {
    const fixture = await mountTab({ stub: conciergeStub(settingsRow(), { failRead: true }) });
    expect(text(root(fixture).querySelector('.qt-alert-error'))).toBe(
      "Failed to load the Concierge's settings",
    );
    expect(root(fixture).querySelector('#on-duty')).toBeNull();
  });

  it('hosted (no ActivatedRoute): renders, and no section is forced open (ruling E.9)', async () => {
    const fixture = await mountTab({ hosted: true });
    expect(sectionHeader(fixture, 'pre-screening').getAttribute('aria-expanded')).toBe('false');
  });
});
