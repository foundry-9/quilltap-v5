import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import type { ParticipantDetail } from '../../core/core-contract';
import { ParticipantCard, USER_IMPERSONATION_VALUE } from './participant-card';

/**
 * The controls P4.9E1B restored to the cast card (v4
 * `components/chat/ParticipantCard.tsx:392-597`): the Controlled-By select, the
 * system-prompt select and its rebuild button, the talkativeness slider, the
 * four-state status select, and Remove. Each assertion is about what the card
 * REPORTS, since the writes themselves are the Salon's.
 */

function participant(over: Partial<ParticipantDetail> = {}): ParticipantDetail {
  return {
    id: 'p-1',
    type: 'CHARACTER',
    displayOrder: 0,
    isActive: true,
    controlledBy: 'llm',
    status: 'active',
    character: {
      id: 'char-1',
      name: 'Bram',
      title: null,
      avatarUrl: null,
      defaultImageId: null,
      defaultImage: null,
      talkativeness: 0.5,
    },
    connectionProfile: null,
    imageProfile: null,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...over,
  };
}

function mount(
  inputs: Record<string, unknown> = {},
): ComponentFixture<ParticipantCard> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [ParticipantCard] });
  const fixture = TestBed.createComponent(ParticipantCard);
  fixture.componentRef.setInput('participant', participant());
  for (const [key, value] of Object.entries(inputs)) {
    fixture.componentRef.setInput(key, value);
  }
  fixture.detectChanges();
  return fixture;
}

function selects(fixture: ComponentFixture<unknown>): HTMLSelectElement[] {
  return [...fixture.nativeElement.querySelectorAll('select')] as HTMLSelectElement[];
}

function selectByLabel(fixture: ComponentFixture<unknown>, label: string): HTMLSelectElement {
  return selects(fixture).find((s) => (s.getAttribute('aria-label') ?? '').includes(label))!;
}

function change(el: HTMLSelectElement, value: string): void {
  el.value = value;
  el.dispatchEvent(new Event('change'));
}

const PROFILES = [
  { id: 'prof-1', name: 'Everyday', provider: 'anthropic', modelName: 'claude-sonnet' },
  { id: 'prof-2', name: 'claude-sonnet', provider: 'anthropic', modelName: 'claude-sonnet' },
];

describe('ParticipantCard — the restored cast controls', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('shows the profile select with the user option and every backend', () => {
    const fixture = mount({ connectionProfiles: PROFILES });
    const select = selectByLabel(fixture, 'Connection profile for Bram');
    const options = [...select.options].map((o) => `${o.value}|${o.textContent?.trim()}`);
    expect(options).toEqual([
      '|Select a provider...',
      `${USER_IMPERSONATION_VALUE}|User (you type)`,
      // v4 :415-420 — "name — model", collapsed when the two are the same.
      'prof-1|Everyday — claude-sonnet',
      'prof-2|claude-sonnet',
    ]);
  });

  it('shows the user sentinel as the value for a user-controlled seat (v4 :376)', () => {
    const fixture = mount({ connectionProfiles: PROFILES });
    fixture.componentRef.setInput('participant', participant({ controlledBy: 'user' }));
    fixture.detectChanges();
    expect(selectByLabel(fixture, 'Connection profile for Bram').value).toBe(
      USER_IMPERSONATION_VALUE,
    );
  });

  it('reports the user option as {profileId: null, controlledBy: user}', () => {
    const fixture = mount({ connectionProfiles: PROFILES });
    const seen: unknown[] = [];
    fixture.componentInstance.connectionProfileChange.subscribe((e) => seen.push(e));
    change(selectByLabel(fixture, 'Connection profile for Bram'), USER_IMPERSONATION_VALUE);
    expect(seen).toEqual([{ participantId: 'p-1', profileId: null, controlledBy: 'user' }]);
  });

  it('reports a chosen backend as {profileId, controlledBy: llm}', () => {
    const fixture = mount({ connectionProfiles: PROFILES });
    const seen: unknown[] = [];
    fixture.componentInstance.connectionProfileChange.subscribe((e) => seen.push(e));
    change(selectByLabel(fixture, 'Connection profile for Bram'), 'prof-2');
    expect(seen).toEqual([{ participantId: 'p-1', profileId: 'prof-2', controlledBy: 'llm' }]);
  });

  it('renders the system-prompt select only when the character has named prompts', () => {
    const bare = mount({});
    expect(selects(bare).some((s) => (s.getAttribute('aria-label') ?? '').includes('System prompt'))).toBe(
      false,
    );
    // The rebuild button is offered either way (v4 :455-469).
    expect(bare.nativeElement.querySelector('[aria-label="Rebuild system prompt for Bram"]')).toBeTruthy();

    const withPrompts = mount({});
    withPrompts.componentRef.setInput(
      'participant',
      participant({
        character: {
          ...participant().character!,
          systemPrompts: [{ id: 'sp-1', name: 'Terse', isDefault: true }],
        },
      }),
    );
    withPrompts.detectChanges();
    const select = selectByLabel(withPrompts, 'System prompt for Bram');
    expect([...select.options].map((o) => o.textContent?.trim())).toEqual([
      'Use default prompt',
      'Terse (Default)',
    ]);
  });

  it('withholds the whole system-prompt row from a user-controlled character (v4 :438)', () => {
    const fixture = mount({});
    fixture.componentRef.setInput('participant', participant({ controlledBy: 'user' }));
    fixture.detectChanges();
    expect(
      fixture.nativeElement.querySelector('[aria-label="Rebuild system prompt for Bram"]'),
    ).toBeNull();
  });

  it('reports "use the default prompt" as an explicit null', () => {
    const fixture = mount({});
    fixture.componentRef.setInput(
      'participant',
      participant({
        selectedSystemPromptId: 'sp-1',
        character: {
          ...participant().character!,
          systemPrompts: [{ id: 'sp-1', name: 'Terse' }],
        },
      }),
    );
    fixture.detectChanges();
    const seen: Array<{ participantId: string; promptId: string | null }> = [];
    fixture.componentInstance.systemPromptChange.subscribe((e) => seen.push(e));
    change(selectByLabel(fixture, 'System prompt for Bram'), '');
    expect(seen).toEqual([{ participantId: 'p-1', promptId: null }]);
  });

  it('the rebuild button reports the participant id', () => {
    const fixture = mount({});
    const seen: string[] = [];
    fixture.componentInstance.rebuildSystemPrompt.subscribe((id) => seen.push(id));
    (
      fixture.nativeElement.querySelector(
        '[aria-label="Rebuild system prompt for Bram"]',
      ) as HTMLButtonElement
    ).click();
    expect(seen).toEqual(['p-1']);
  });

  it('seeds the slider from the per-chat override, falling back to the character', () => {
    const fromCharacter = mount({});
    expect(
      (fromCharacter.nativeElement.querySelector('input[type="range"]') as HTMLInputElement).value,
    ).toBe('0.5');

    const overridden = mount({});
    overridden.componentRef.setInput('participant', participant({ talkativeness: 0.8 }));
    overridden.detectChanges();
    expect(
      (overridden.nativeElement.querySelector('input[type="range"]') as HTMLInputElement).value,
    ).toBe('0.8');
  });

  it('the slider reports every step and moves its own label (the write is debounced upstream)', () => {
    const fixture = mount({});
    const seen: Array<{ participantId: string; value: number }> = [];
    fixture.componentInstance.talkativenessChange.subscribe((e) => seen.push(e));
    const slider = fixture.nativeElement.querySelector('input[type="range"]') as HTMLInputElement;
    slider.value = '0.7';
    slider.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    slider.value = '0.9';
    slider.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(seen).toEqual([
      { participantId: 'p-1', value: 0.7 },
      { participantId: 'p-1', value: 0.9 },
    ]);
    expect((fixture.nativeElement.textContent ?? '').replace(/\s+/g, ' ')).toContain('90%');
  });

  it('the operator’s own seat gets a disabled N/A slider (v4 :490-506)', () => {
    const fixture = mount({ isUserParticipant: true });
    const slider = fixture.nativeElement.querySelector('input[type="range"]') as HTMLInputElement;
    expect(slider.disabled).toBe(true);
    expect((fixture.nativeElement.textContent ?? '').replace(/\s+/g, '')).toContain(
      'TalkativenessN/A',
    );
  });

  it('offers three of the four statuses, and reports the chosen one', () => {
    const fixture = mount({});
    const select = selectByLabel(fixture, 'Participation status for Bram');
    expect([...select.options].map((o) => o.value)).toEqual(['active', 'silent', 'absent']);
    const seen: Array<{ participantId: string; status: string }> = [];
    fixture.componentInstance.statusChange.subscribe((e) => seen.push(e));
    change(select, 'absent');
    expect(seen).toEqual([{ participantId: 'p-1', status: 'absent' }]);
  });

  it('offers Remove for a character, never for the operator’s seat, never as the last one', () => {
    const removable = mount({ canRemove: true });
    const button = removable.nativeElement.querySelector(
      '[aria-label="Remove Bram from chat"]',
    ) as HTMLButtonElement;
    expect(button).toBeTruthy();
    const seen: string[] = [];
    removable.componentInstance.remove.subscribe((id) => seen.push(id));
    button.click();
    expect(seen).toEqual(['p-1']);

    expect(
      mount({ canRemove: false }).nativeElement.querySelector('[aria-label="Remove Bram from chat"]'),
    ).toBeNull();
    expect(
      mount({ canRemove: true, isUserParticipant: true }).nativeElement.querySelector(
        '[aria-label="Remove Bram from chat"]',
      ),
    ).toBeNull();
  });

  it('disables Remove while a response is generating (v4 :588)', () => {
    const fixture = mount({ canRemove: true, isGenerating: true });
    const button = fixture.nativeElement.querySelector(
      '[aria-label="Remove Bram from chat"]',
    ) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
  });
});

// ===========================================================================
// P4.D64 — the archived seat's badge (v4 `ParticipantCard.tsx` at `d553f72a`)
// ===========================================================================

describe('ParticipantCard — the Archived badge (P4.D64)', () => {
  afterEach(() => {
    TestBed.resetTestingModule();
  });

  function badges(fixture: ComponentFixture<ParticipantCard>): string[] {
    return ([...fixture.nativeElement.querySelectorAll('.qt-badge-absent')] as HTMLElement[]).map(
      (el) => el.textContent?.trim() ?? '',
    );
  }

  function mountWith(over: Partial<ParticipantDetail>): ComponentFixture<ParticipantCard> {
    TestBed.resetTestingModule();
    TestBed.configureTestingModule({ imports: [ParticipantCard] });
    const fixture = TestBed.createComponent(ParticipantCard);
    fixture.componentRef.setInput('participant', participant(over));
    fixture.detectChanges();
    return fixture;
  }

  it('badges an archived seat, reusing qt-badge-absent as v4 does', () => {
    const fixture = mountWith({
      character: { ...participant().character!, archivedAt: '2026-08-01T00:00:00.000Z' },
    });
    expect(badges(fixture)).toEqual(['Archived']);
    const badge = fixture.nativeElement.querySelector('.qt-badge-absent') as HTMLElement;
    expect(badge.getAttribute('title')).toBe(
      'Resting in the archive — rehydrate them from their character page to let them speak again',
    );
  });

  it('shows Absent AND Archived together, in that order', () => {
    // An archived seat is normally flipped absent too, and v4 renders BOTH —
    // the order matters because Archived comes after Absent (`:383-393`).
    const fixture = mountWith({
      status: 'absent',
      character: { ...participant().character!, archivedAt: '2026-08-01T00:00:00.000Z' },
    });
    expect(badges(fixture)).toEqual(['Absent', 'Archived']);
  });

  it('badges nothing for a live seat', () => {
    const fixture = mountWith({});
    expect(badges(fixture)).toEqual([]);
  });
});
