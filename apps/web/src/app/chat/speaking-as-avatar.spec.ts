import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { SpeakingAsAvatar } from './speaking-as-avatar';

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
    expect(cue.getAttribute('title')).toBe('Speaking as Charlie');
    expect(cue.classList.contains('qt-speaking-as-avatar-dim')).toBe(false);

    const img = fixture.nativeElement.querySelector('img') as HTMLImageElement;
    expect(img.getAttribute('alt')).toBe('Charlie');
    expect(img.getAttribute('src')).toBe('/files/charlie.webp');
  });

  it('dims to near-dark while a reply is in flight', () => {
    const { cue } = render({ name: 'Charlie', canType: false });

    expect(cue.getAttribute('aria-label')).toBe('Speaking as Charlie, waiting for the room');
    expect(cue.getAttribute('title')).toBe('Speaking as Charlie — waiting for the room');
    expect(cue.classList.contains('qt-speaking-as-avatar-dim')).toBe(true);
  });

  it('falls back to the initial when there is no portrait', () => {
    const { fixture } = render({ name: 'Charlie', avatarUrl: null, canType: true });
    expect(fixture.nativeElement.querySelector('img')).toBeNull();
    expect(fixture.nativeElement.textContent?.trim()).toBe('C');
  });
});

/**
 * The In Their Own Words cue (v4 `686954937`): an armed seat wears a quill badge
 * and says, in its title, what a typed line will actually do. The `aria-label` is
 * deliberately UNCHANGED — v4 leaves it alone and marks the badge `aria-hidden`.
 */
describe('SpeakingAsAvatar — the voice-rehearsal cue', () => {
  function render(inputs: {
    name: string;
    canType: boolean;
    voiceRehearsal?: boolean;
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
    const { cue } = render({ name: 'Charlie', canType: true });
    expect(cue.querySelector('.qt-speaking-as-avatar-voice-badge')).toBeNull();
  });

  it('draws the quill badge, hidden from assistive tech, when armed', () => {
    const { cue } = render({ name: 'Charlie', canType: true, voiceRehearsal: true });
    const badge = cue.querySelector('.qt-speaking-as-avatar-voice-badge');
    expect(badge).not.toBeNull();
    expect(badge?.getAttribute('aria-hidden')).toBe('true');
    // v5's icons are CSS masks on a `data-icon` span, not inline SVG.
    expect(badge?.querySelector('[data-icon="thinking"]')).not.toBeNull();
  });

  it('says what the send will do, outranking BOTH of the other titles', () => {
    const armed = 'Speaking as Charlie — your draft goes to Charlie to say in their own words first';
    expect(render({ name: 'Charlie', canType: true, voiceRehearsal: true }).cue.getAttribute('title')).toBe(armed);
    // …even when the floor is not the human's.
    expect(render({ name: 'Charlie', canType: false, voiceRehearsal: true }).cue.getAttribute('title')).toBe(armed);
  });

  it('leaves the aria-label alone (v4 does not touch it)', () => {
    const { cue } = render({ name: 'Charlie', canType: true, voiceRehearsal: true });
    expect(cue.getAttribute('aria-label')).toBe('Speaking as Charlie');
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
