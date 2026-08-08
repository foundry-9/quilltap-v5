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
  }): { fixture: ComponentFixture<SpeakingAsAvatar>; cue: HTMLElement } {
    TestBed.configureTestingModule({ imports: [SpeakingAsAvatar] });
    const fixture = TestBed.createComponent(SpeakingAsAvatar);
    fixture.componentRef.setInput('name', inputs.name);
    if (inputs.avatarUrl !== undefined) fixture.componentRef.setInput('avatarUrl', inputs.avatarUrl);
    fixture.componentRef.setInput('canType', inputs.canType);
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
