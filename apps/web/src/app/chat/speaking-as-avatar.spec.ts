import { signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { tooltipContentOf } from '../../testing/tooltip-content';
import { IMAGES_HIDDEN } from './hidden-image/images-hidden';
import {
  ESTABLISHED_FACT_TOGGLE_TIP,
  SpeakingAsAvatar,
  voiceRehearsalTitle,
  type VoiceRehearsalCue,
} from './speaking-as-avatar';

/** The portrait's words — since v4 `df8d4d70a` a `qt-tooltip`, not a native `title`. */
function tipOf(r: { fixture: ComponentFixture<SpeakingAsAvatar>; cue: HTMLElement }): string | null {
  return tooltipContentOf(r.fixture, r.cue);
}

/**
 * Client port of v4 `__tests__/unit/app/salon/SpeakingAsAvatar.test.tsx`. Covers
 * the two behaviours the cue promises: it always names + pictures the character
 * the human is speaking as, and it renders bright when the human may type,
 * dimming to near-dark while a reply is in flight (v4 Bug 46 cue). v5 expresses
 * the brightness gate as the `.qt-speaking-as-avatar-dim` modifier rather than
 * v4's tailwind `opacity-60 brightness-50`.
 */
describe('SpeakingAsAvatar', () => {
  function render(inputs: {
    name: string;
    avatarUrl?: string | null;
    canType: boolean;
    voiceRehearsal?: boolean;
  }): { fixture: ComponentFixture<SpeakingAsAvatar>; cue: HTMLElement } {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [SpeakingAsAvatar] });
    const fixture = TestBed.createComponent(SpeakingAsAvatar);
    fixture.componentRef.setInput('name', inputs.name);
    if (inputs.avatarUrl !== undefined) fixture.componentRef.setInput('avatarUrl', inputs.avatarUrl);
    fixture.componentRef.setInput('canType', inputs.canType);
    if (inputs.voiceRehearsal !== undefined) {
      fixture.componentRef.setInput('voiceRehearsal', inputs.voiceRehearsal);
    }
    fixture.detectChanges();
    const cue = fixture.nativeElement.querySelector('.qt-speaking-as-avatar') as HTMLElement;
    return { fixture, cue };
  }

  it('names and pictures the character being spoken as, bright when the human may type', () => {
    const { fixture, cue } = render({
      name: 'Charlie',
      avatarUrl: '/files/charlie.webp',
      canType: true,
    });

    expect(cue.getAttribute('aria-label')).toBe('Speaking as Charlie');
    expect(tooltipContentOf(fixture, cue)).toBe('Speaking as Charlie');
    expect(cue.classList.contains('qt-speaking-as-avatar-dim')).toBe(false);

    const img = fixture.nativeElement.querySelector('img') as HTMLImageElement;
    expect(img.getAttribute('alt')).toBe('Charlie');
    expect(img.getAttribute('src')).toBe('/files/charlie.webp');
  });

  it('dims to near-dark while a reply is in flight', () => {
    const { fixture, cue } = render({ name: 'Charlie', canType: false });

    expect(cue.getAttribute('aria-label')).toBe('Speaking as Charlie, waiting for the room');
    expect(tooltipContentOf(fixture, cue)).toBe('Speaking as Charlie — waiting for the room');
    expect(cue.classList.contains('qt-speaking-as-avatar-dim')).toBe(true);
  });

  it('falls back to the initial when there is no portrait', () => {
    const { fixture } = render({ name: 'Charlie', avatarUrl: null, canType: true });
    expect(fixture.nativeElement.querySelector('img')).toBeNull();
    expect(fixture.nativeElement.textContent?.trim()).toBe('C');
  });
});

/**
 * The In Their Own Words cue (v4 `686954937`, made a MODE by `07b8f0209`): an
 * armed seat wears a quill badge and says, in its title, what a typed line will
 * actually do — which since `07b8f0209` depends on WHICH mode is armed. The
 * `aria-label` is deliberately UNCHANGED — v4 leaves it alone and marks the badge
 * `aria-hidden`.
 */
describe('SpeakingAsAvatar — the voice-rehearsal cue', () => {
  function render(inputs: {
    name: string;
    canType: boolean;
    voiceRehearsal?: VoiceRehearsalCue | null;
  }): { fixture: ComponentFixture<SpeakingAsAvatar>; cue: HTMLElement } {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [SpeakingAsAvatar] });
    const fixture = TestBed.createComponent(SpeakingAsAvatar);
    fixture.componentRef.setInput('name', inputs.name);
    fixture.componentRef.setInput('canType', inputs.canType);
    if (inputs.voiceRehearsal !== undefined) {
      fixture.componentRef.setInput('voiceRehearsal', inputs.voiceRehearsal);
    }
    fixture.detectChanges();
    return { fixture, cue: fixture.nativeElement.querySelector('.qt-speaking-as-avatar') };
  }

  it('draws no badge when the feature is not armed', () => {
    expect(
      render({ name: 'Charlie', canType: true }).cue.querySelector(
        '.qt-speaking-as-avatar-voice-badge',
      ),
    ).toBeNull();
    expect(
      render({ name: 'Charlie', canType: true, voiceRehearsal: null }).cue.querySelector(
        '.qt-speaking-as-avatar-voice-badge',
      ),
    ).toBeNull();
  });

  for (const mode of ['ask', 'always'] as const) {
    it(`draws the quill badge, hidden from assistive tech, when armed (${mode})`, () => {
      const { cue } = render({ name: 'Charlie', canType: true, voiceRehearsal: mode });
      const badge = cue.querySelector('.qt-speaking-as-avatar-voice-badge');
      expect(badge).not.toBeNull();
      expect(badge?.getAttribute('aria-hidden')).toBe('true');
      // v5's icons are CSS masks on a `data-icon` span, not inline SVG.
      expect(badge?.querySelector('[data-icon="thinking"]')).not.toBeNull();
    });

    it(`leaves the aria-label alone (${mode}; v4 does not touch it)`, () => {
      expect(
        render({ name: 'Charlie', canType: true, voiceRehearsal: mode }).cue.getAttribute(
          'aria-label',
        ),
      ).toBe('Speaking as Charlie');
      expect(
        render({ name: 'Charlie', canType: false, voiceRehearsal: mode }).cue.getAttribute(
          'aria-label',
        ),
      ).toBe('Speaking as Charlie, waiting for the room');
    });
  }

  it('always: says the draft goes to the character, outranking BOTH other titles', () => {
    const armed =
      'Speaking as Charlie — your draft goes to Charlie to say in their own words first';
    expect(
      tipOf(render({ name: 'Charlie', canType: true, voiceRehearsal: 'always' })),
    ).toBe(armed);
    // …even when the floor is not the human's.
    expect(
      tipOf(render({ name: 'Charlie', canType: false, voiceRehearsal: 'always' })),
    ).toBe(armed);
  });

  it('ask: says the draft opens for review, outranking BOTH other titles', () => {
    const armed =
      'Speaking as Charlie — your draft opens for review; send it as written or have Charlie restate it';
    expect(
      tipOf(render({ name: 'Charlie', canType: true, voiceRehearsal: 'ask' })),
    ).toBe(armed);
    expect(
      tipOf(render({ name: 'Charlie', canType: false, voiceRehearsal: 'ask' })),
    ).toBe(armed);
  });

  /**
   * ⚠ The #97/#107/#108 family — a class name that no rule defines renders as
   * nothing at all — has NO automated guard at this layer, and that was
   * MEASURED, not assumed: deleting the `.qt-speaking-as-avatar-voice-badge`
   * rule outright leaves `npm run lint` green (`check-qt-classes` is
   * deliberately narrow — the four utility families, variant forms, and
   * component HOSTS; a bare component class on an ordinary element is out of
   * scope by design, see that script's SCOPE note). A vitest spec cannot read
   * the stylesheet either (`spa-spec-cannot-read-source`: no `?raw`, no
   * `node:fs`), and jsdom runs no cascade.
   *
   * So the badge's own `position: absolute` and the wrapper's `position:
   * relative` — which v4 spells as inline `absolute` / `relative` utilities and
   * v5 must spell as real declarations — are asserted against a REAL cascade by
   * the `salon-impersonation-voice-flow` beat, which is the only place a
   * computed style means anything.
   */
});

/**
 * The ONE wording of the cue, shared by the portrait and the Send button (v4
 * `SpeakingAsAvatar.tsx:21-39`, `07b8f0209`) — its four strings, byte for byte.
 */
describe('voiceRehearsalTitle', () => {
  it('always / portrait', () => {
    expect(voiceRehearsalTitle('always', 'Evangeline', 'portrait')).toBe(
      'Speaking as Evangeline — your draft goes to Evangeline to say in their own words first',
    );
  });

  it('always / send', () => {
    expect(voiceRehearsalTitle('always', 'Evangeline', 'send')).toBe(
      'Sends your draft to Evangeline to say in their own words first',
    );
  });

  it('ask / portrait', () => {
    expect(voiceRehearsalTitle('ask', 'Evangeline', 'portrait')).toBe(
      'Speaking as Evangeline — your draft opens for review; send it as written or have Evangeline restate it',
    );
  });

  it('ask / send', () => {
    expect(voiceRehearsalTitle('ask', 'Evangeline', 'send')).toBe(
      'Opens your draft for review — send it as written or have Evangeline restate it',
    );
  });
});

/**
 * Quick-hide "Salon Images" (v4 `e3937d7aa` `SpeakingAsAvatar.tsx:47-48`): the
 * cue reads the Salon's switch itself — nothing is threaded through the
 * composer — and falls back to the initial.
 */
describe('SpeakingAsAvatar — the Salon Images switch (v4 e3937d7aa)', () => {
  it('shows the initial, not the portrait, while the Salon hides its images', () => {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({
      imports: [SpeakingAsAvatar],
      providers: [{ provide: IMAGES_HIDDEN, useValue: signal(true) }],
    });
    const fixture = TestBed.createComponent(SpeakingAsAvatar);
    fixture.componentRef.setInput('name', 'Charlie');
    fixture.componentRef.setInput('avatarUrl', '/files/charlie.webp');
    fixture.componentRef.setInput('canType', true);
    fixture.detectChanges();
    const root = fixture.nativeElement as HTMLElement;
    expect(root.querySelector('img')).toBeNull();
    expect(root.querySelector('.qt-speaking-as-avatar')?.textContent?.trim()).toBe('C');
    // The cue still names who is speaking.
    expect(root.querySelector('.qt-speaking-as-avatar')?.getAttribute('aria-label')).toBe(
      'Speaking as Charlie',
    );
    TestBed.resetTestingModule();
  });
});

/**
 * The Deus Ex Machina toggle (P4.D385 item 8; v4 `SpeakingAsAvatar.tsx`,
 * `db6190b44` / `df8d4d70a`) and the portrait's move from `title` to a
 * `qt-tooltip`.
 */
describe('SpeakingAsAvatar — the Deus Ex Machina toggle', () => {
  function render(fact: { armed: boolean; disabled?: boolean } | null | undefined) {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [SpeakingAsAvatar] });
    const fixture = TestBed.createComponent(SpeakingAsAvatar);
    fixture.componentRef.setInput('name', 'Charlie');
    fixture.componentRef.setInput('canType', false);
    if (fact !== undefined) fixture.componentRef.setInput('establishedFact', fact);
    let toggles = 0;
    fixture.componentInstance.toggleEstablishedFact.subscribe(() => (toggles += 1));
    fixture.detectChanges();
    const root = fixture.nativeElement as HTMLElement;
    return {
      fixture,
      root,
      toggle: root.querySelector<HTMLButtonElement>('.qt-chat-established-fact-toggle'),
      toggles: () => toggles,
    };
  }

  it('carries no native title anywhere', () => {
    const { root } = render({ armed: false });
    expect(root.querySelector('[title]')).toBeNull();
  });

  it('is absent without the input', () => {
    expect(render(undefined).toggle).toBeNull();
    expect(render(null).toggle).toBeNull();
  });

  it('names itself and reports its state through aria-pressed, both ways', () => {
    const off = render({ armed: false });
    expect(off.toggle!.getAttribute('aria-label')).toBe('Deus Ex Machina');
    expect(off.toggle!.getAttribute('aria-pressed')).toBe('false');
    expect(off.toggle!.querySelector('[data-icon="deus-ex-machina"]')).not.toBeNull();
    expect(render({ armed: true }).toggle!.getAttribute('aria-pressed')).toBe('true');
  });

  it("wears v4's two tooltips", () => {
    const off = render({ armed: false });
    expect(tooltipContentOf(off.fixture, off.toggle)).toBe(
      'Deus Ex Machina — post your next line as established fact (or type /deus)',
    );
    expect(tooltipContentOf(off.fixture, off.toggle)).toBe(ESTABLISHED_FACT_TOGGLE_TIP.disarmed);
    const on = render({ armed: true });
    expect(tooltipContentOf(on.fixture, on.toggle)).toBe(
      'Deus Ex Machina is armed — your next line descends from the rafters as established fact. Click to stand it down.',
    );
  });

  it('emits on click, and is disabled when asked', () => {
    const r = render({ armed: false });
    r.toggle!.click();
    expect(r.toggles()).toBe(1);
    const d = render({ armed: false, disabled: true });
    expect(d.toggle!.disabled).toBe(true);
    expect(render({ armed: false, disabled: false }).toggle!.disabled).toBe(false);
  });

  it('keeps the caret in the editor: its mousedown is default-prevented', () => {
    const { toggle } = render({ armed: false });
    const down = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
    toggle!.dispatchEvent(down);
    expect(down.defaultPrevented).toBe(true);
  });

  it('sits OUTSIDE the dimmed portrait, so it stays usable while a reply streams', () => {
    const { root, toggle } = render({ armed: false });
    const dim = root.querySelector('.qt-speaking-as-avatar-dim');
    expect(dim).not.toBeNull();
    expect(dim!.contains(toggle)).toBe(false);
  });

  it('floors the box width only when there is no picture', () => {
    const bare = render({ armed: false });
    expect(bare.root.firstElementChild!.classList.contains('min-w-24')).toBe(true);
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [SpeakingAsAvatar] });
    const f = TestBed.createComponent(SpeakingAsAvatar);
    f.componentRef.setInput('name', 'Charlie');
    f.componentRef.setInput('avatarUrl', '/files/charlie.webp');
    f.detectChanges();
    expect((f.nativeElement as HTMLElement).firstElementChild!.classList.contains('min-w-24')).toBe(false);
  });
});
