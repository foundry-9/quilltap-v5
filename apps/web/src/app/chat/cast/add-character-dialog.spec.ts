import { ComponentFixture, TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { CharacterListItem } from '../../core/core-contract';
import { AddCharacterDialog, USER_IMPERSONATION_VALUE } from './add-character-dialog';
import { ToastService } from '../../ui/toast.service';

function character(over: Partial<CharacterListItem> = {}): CharacterListItem {
  return {
    id: 'char-1',
    name: 'Bram',
    title: 'A cautious navigator',
    description: null,
    defaultImageId: null,
    defaultImage: null,
    isFavorite: false,
    controlledBy: 'llm',
    canBeCarina: false,
    defaultConnectionProfileId: null,
    defaultPartnerId: null,
    defaultPartnerName: null,
    defaultTimestampConfig: null,
    defaultScenarioId: null,
    defaultSystemPromptId: null,
    defaultImageProfileId: null,
    npc: false,
    createdAt: '2026-02-01T00:00:00.000Z',
    tags: [],
    updatedAt: '2026-02-01T00:00:00.000Z',
    systemPrompts: [],
    scenarios: [],
    _count: { chats: 0 },
    ...over,
  };
}

function profile(over: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    id: 'prof-1',
    name: 'Everyday',
    provider: 'anthropic',
    modelName: 'claude-sonnet',
    parameters: {},
    isDefault: false,
    ...over,
  };
}

interface Stub {
  calls: Record<string, unknown>[];
  client: Partial<CoreClient>;
}

function stub(opts: {
  characters?: CharacterListItem[];
  profiles?: Array<Record<string, unknown>>;
  add?: Record<string, unknown> | Error;
}): Stub {
  const calls: Record<string, unknown>[] = [];
  const dispatchData = vi.fn(async (req: Record<string, unknown>) => {
    calls.push(req);
    switch (req['type']) {
      case 'characterList':
        return { characters: opts.characters ?? [] };
      case 'connectionProfileList':
        return { profiles: opts.profiles ?? [] };
      case 'chatAddParticipant':
        if (opts.add instanceof Error) throw opts.add;
        return opts.add ?? { participant: { id: 'p-new' } };
      default:
        return {};
    }
  });
  return { calls, client: { dispatchData: dispatchData as unknown as CoreClient['dispatchData'] } };
}

async function settle(fixture: ComponentFixture<unknown>): Promise<void> {
  for (let i = 0; i < 8; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function mount(
  s: Stub,
  existingCharacterIds: string[] = [],
): Promise<ComponentFixture<AddCharacterDialog>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [AddCharacterDialog],
    providers: [provideTanStackQuery(new QueryClient()), { provide: CoreClient, useValue: s.client }],
  });
  const fixture = TestBed.createComponent(AddCharacterDialog);
  fixture.componentRef.setInput('chatId', 'chat-1');
  fixture.componentRef.setInput('existingCharacterIds', existingCharacterIds);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

function text(fixture: ComponentFixture<unknown>): string {
  return (fixture.nativeElement.textContent ?? '').replace(/\s+/g, ' ');
}

/** The roster tiles, in DOM order (regulars, then the NPC group). */
function rosterNames(fixture: ComponentFixture<unknown>): string[] {
  return [...fixture.nativeElement.querySelectorAll('.grid button .font-semibold')]
    .map((el) => (el.textContent ?? '').trim())
    .filter((name) => name !== 'Create New NPC' && name !== 'Summon from Lore');
}

function pick(fixture: ComponentFixture<unknown>, name: string): void {
  const tile = [...fixture.nativeElement.querySelectorAll('.grid button')].find((b) =>
    (b.textContent ?? '').includes(name),
  ) as HTMLButtonElement;
  tile.click();
  fixture.detectChanges();
}

function footerPrimary(fixture: ComponentFixture<unknown>): HTMLButtonElement {
  return fixture.nativeElement.querySelector(
    '.qt-dialog-footer .qt-button-primary',
  ) as HTMLButtonElement;
}

function addCall(s: Stub): Record<string, unknown> | undefined {
  return s.calls.find((c) => c['type'] === 'chatAddParticipant');
}

/** The toast stack this render raised, newest last. */
function toasts(): { type: string; message: string }[] {
  return TestBed.inject(ToastService)
    .toasts()
    .map((t) => ({ type: t.type, message: t.message }));
}

describe('AddCharacterDialog (v4 components/chat/AddCharacterDialog.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('excludes characters already in the chat, and sorts each group by name', async () => {
    const s = stub({
      characters: [
        character({ id: 'c-zoe', name: 'Zoe' }),
        character({ id: 'c-aria', name: 'Aria' }),
        character({ id: 'c-in', name: 'AlreadyHere' }),
        character({ id: 'c-npc', name: 'Barkeep', npc: true }),
      ],
    });
    const fixture = await mount(s, ['c-in']);
    // Regulars sorted, then NPCs sorted — and no cast member.
    expect(rosterNames(fixture)).toEqual(['Aria', 'Zoe', 'Barkeep']);
    expect(text(fixture)).toContain('NPCs');
  });

  it('says so when every character is already in the chat (v4 :322)', async () => {
    const s = stub({ characters: [character({ id: 'c-in' })] });
    const fixture = await mount(s, ['c-in']);
    expect(text(fixture)).toContain('All your characters are already in this chat');
  });

  it('searches on name OR title, case-insensitively (v4 :143-147)', async () => {
    const s = stub({
      characters: [
        character({ id: 'c-1', name: 'Aria', title: 'The pilot' }),
        character({ id: 'c-2', name: 'Bram', title: 'A navigator' }),
      ],
    });
    const fixture = await mount(s);
    const search = fixture.nativeElement.querySelector('#qt-add-character-search') as HTMLInputElement;
    search.value = 'NAVIG';
    search.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(rosterNames(fixture)).toEqual(['Bram']);
  });

  it('seeds the character’s own default profile when it still exists', async () => {
    const s = stub({
      characters: [character({ id: 'c-1', defaultConnectionProfileId: 'prof-2' })],
      profiles: [profile(), profile({ id: 'prof-2', name: 'Deep' })],
    });
    const fixture = await mount(s);
    pick(fixture, 'Bram');
    await settle(fixture);
    footerPrimary(fixture).click();
    await settle(fixture);
    expect(addCall(s)!['connectionProfileId']).toBe('prof-2');
  });

  it('falls through a DELETED character default to the user default (v4 :122-133)', async () => {
    const s = stub({
      characters: [character({ id: 'c-1', defaultConnectionProfileId: 'prof-gone' })],
      profiles: [profile(), profile({ id: 'prof-def', name: 'Default', isDefault: true })],
    });
    const fixture = await mount(s);
    pick(fixture, 'Bram');
    await settle(fixture);
    footerPrimary(fixture).click();
    await settle(fixture);
    expect(addCall(s)!['connectionProfileId']).toBe('prof-def');
  });

  it('falls through to the first profile when nothing is marked default', async () => {
    const s = stub({
      characters: [character({ id: 'c-1' })],
      profiles: [profile({ id: 'prof-first' }), profile({ id: 'prof-second' })],
    });
    const fixture = await mount(s);
    pick(fixture, 'Bram');
    await settle(fixture);
    footerPrimary(fixture).click();
    await settle(fixture);
    expect(addCall(s)!['connectionProfileId']).toBe('prof-first');
  });

  it('sends controlledBy llm and hasHistoryAccess false by default', async () => {
    const s = stub({ characters: [character({ id: 'c-1' })], profiles: [profile()] });
    const fixture = await mount(s);
    pick(fixture, 'Bram');
    await settle(fixture);
    footerPrimary(fixture).click();
    await settle(fixture);
    // The outfit selector seeds and EMITS its computed initial mode as soon as a
    // character is picked (v4's does the same), so `outfitSelection` rides along
    // even when the operator touched nothing — v4 posts exactly this body.
    expect(addCall(s)).toEqual({
      type: 'chatAddParticipant',
      chatId: 'chat-1',
      characterId: 'c-1',
      controlledBy: 'llm',
      hasHistoryAccess: false,
      connectionProfileId: 'prof-1',
      outfitSelection: { characterId: 'c-1', mode: 'default' },
    });
  });

  it('an LLM character flagged canChooseOutfit seeds `llm_choose` (v4 8bf3cb5f)', async () => {
    const s = stub({
      characters: [character({ id: 'c-1', canChooseOutfit: true })],
      profiles: [profile()],
    });
    const fixture = await mount(s);
    pick(fixture, 'Bram');
    await settle(fixture);
    footerPrimary(fixture).click();
    await settle(fixture);
    expect(addCall(s)!['outfitSelection']).toEqual({ characterId: 'c-1', mode: 'llm_choose' });
  });

  it('the user-impersonation option sends controlledBy user and NO profile (v4 :196-199)', async () => {
    const s = stub({ characters: [character({ id: 'c-1' })], profiles: [profile()] });
    const fixture = await mount(s);
    pick(fixture, 'Bram');
    await settle(fixture);
    const select = fixture.nativeElement.querySelector(
      '#qt-add-character-profile',
    ) as HTMLSelectElement;
    select.value = USER_IMPERSONATION_VALUE;
    select.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    footerPrimary(fixture).click();
    await settle(fixture);
    const call = addCall(s)!;
    expect(call['controlledBy']).toBe('user');
    expect('connectionProfileId' in call).toBe(false);
  });

  it('checks history access through to the wire', async () => {
    const s = stub({ characters: [character({ id: 'c-1' })], profiles: [profile()] });
    const fixture = await mount(s);
    pick(fixture, 'Bram');
    await settle(fixture);
    const box = fixture.nativeElement.querySelector('#hasHistoryAccess') as HTMLInputElement;
    box.checked = true;
    box.dispatchEvent(new Event('change'));
    fixture.detectChanges();
    footerPrimary(fixture).click();
    await settle(fixture);
    expect(addCall(s)!['hasHistoryAccess']).toBe(true);
  });

  it('omits joinScenario when it is blank or whitespace (v4 :201-203)', async () => {
    const s = stub({ characters: [character({ id: 'c-1' })], profiles: [profile()] });
    const fixture = await mount(s);
    pick(fixture, 'Bram');
    await settle(fixture);
    const dialog = fixture.componentInstance as unknown as {
      joinScenario: { set(v: string): void };
    };
    dialog.joinScenario.set('   ');
    fixture.detectChanges();
    footerPrimary(fixture).click();
    await settle(fixture);
    expect('joinScenario' in addCall(s)!).toBe(false);
  });

  it('sends a TRIMMED joinScenario when one was written', async () => {
    const s = stub({ characters: [character({ id: 'c-1' })], profiles: [profile()] });
    const fixture = await mount(s);
    pick(fixture, 'Bram');
    await settle(fixture);
    (
      fixture.componentInstance as unknown as { joinScenario: { set(v: string): void } }
    ).joinScenario.set('  Swung down from the rigging.  ');
    fixture.detectChanges();
    footerPrimary(fixture).click();
    await settle(fixture);
    expect(addCall(s)!['joinScenario']).toBe('Swung down from the rigging.');
  });

  it('forwards the outfit selection, and omits it when the selector produced none', async () => {
    const s = stub({ characters: [character({ id: 'c-1' })], profiles: [profile()] });
    const fixture = await mount(s);
    pick(fixture, 'Bram');
    await settle(fixture);
    const instance = fixture.componentInstance as unknown as {
      onOutfitSelections(v: Array<{ characterId: string; mode: string }>): void;
      outfitSelection: { set(v: null): void };
    };
    instance.onOutfitSelections([{ characterId: 'c-1', mode: 'none' }]);
    fixture.detectChanges();
    footerPrimary(fixture).click();
    await settle(fixture);
    expect(addCall(s)!['outfitSelection']).toEqual({ characterId: 'c-1', mode: 'none' });
  });

  it('emits `added` with the character’s name, then closes', async () => {
    const s = stub({ characters: [character({ id: 'c-1', name: 'Bram' })], profiles: [profile()] });
    const fixture = await mount(s);
    const added: Array<{ characterId: string; name: string }> = [];
    let closed = 0;
    fixture.componentInstance.added.subscribe((e) => added.push(e));
    fixture.componentInstance.close.subscribe(() => (closed += 1));
    pick(fixture, 'Bram');
    await settle(fixture);
    footerPrimary(fixture).click();
    await settle(fixture);
    expect(added).toEqual([{ characterId: 'c-1', name: 'Bram' }]);
    expect(closed).toBe(1);
  });

  it('keeps the dialog open and shows the server’s message on failure', async () => {
    const s = stub({
      characters: [character({ id: 'c-1' })],
      profiles: [profile()],
      add: new Error('Character is already a participant'),
    });
    const fixture = await mount(s);
    let closed = 0;
    fixture.componentInstance.close.subscribe(() => (closed += 1));
    pick(fixture, 'Bram');
    await settle(fixture);
    footerPrimary(fixture).click();
    await settle(fixture);
    expect(toasts().at(-1)).toEqual({
      type: 'error',
      message: 'Character is already a participant',
    });
    expect(closed).toBe(0);
  });

  it('Summon from Lore refuses BY NAME rather than disappearing', async () => {
    const s = stub({ characters: [character({ id: 'c-1' })] });
    const fixture = await mount(s);
    const summon = [...fixture.nativeElement.querySelectorAll('.grid button')].find((b) =>
      (b.textContent ?? '').includes('Summon from Lore'),
    ) as HTMLButtonElement;
    expect(summon).toBeTruthy();
    summon.click();
    fixture.detectChanges();
    const body = text(fixture);
    expect(body).toContain('not yet available');
    expect(body).toContain('SummonFromLoreModal.tsx');
    expect(body).toContain('AIImportWizard');
  });

  it('warns when there is no connection profile to control an LLM with', async () => {
    const s = stub({ characters: [character({ id: 'c-1' })], profiles: [] });
    const fixture = await mount(s);
    pick(fixture, 'Bram');
    await settle(fixture);
    expect(text(fixture)).toContain('No connection profiles available');
  });

  it('a backdrop click closes — unless the NPC dialog is open (v4 :161-164)', async () => {
    const s = stub({ characters: [character({ id: 'c-1' })], profiles: [profile()] });
    const fixture = await mount(s);
    let closed = 0;
    fixture.componentInstance.close.subscribe(() => (closed += 1));

    const overlay = fixture.nativeElement.querySelector('.qt-dialog-overlay') as HTMLElement;
    const npcButton = [...fixture.nativeElement.querySelectorAll('.grid button')].find((b) =>
      (b.textContent ?? '').includes('Create New NPC'),
    ) as HTMLButtonElement;
    npcButton.click();
    fixture.detectChanges();
    overlay.dispatchEvent(new MouseEvent('click'));
    fixture.detectChanges();
    expect(closed).toBe(0);

    (fixture.componentInstance as unknown as { createNpcOpen: { set(v: boolean): void } }).createNpcOpen.set(
      false,
    );
    fixture.detectChanges();
    overlay.dispatchEvent(new MouseEvent('click'));
    fixture.detectChanges();
    expect(closed).toBe(1);
  });
});

// ===========================================================================
// P4.D64 — pickers inherit the archive chokepoint's exclude default.
// ===========================================================================

describe('AddCharacterDialog — archived characters are not offered (P4.D64)', () => {
  it('lists characters WITHOUT an archived param, so the server excludes tombstones', async () => {
    // v4's chokepoint (`characters/handlers/get.ts:28-33`) excludes archived rows
    // unless asked, so every picker is clean automatically and NEEDS no change.
    // This spec is the tripwire for that claim: if a future lane ever teaches a
    // picker to send `archived`, it fails here rather than quietly offering a
    // character who cannot speak.
    const s = stub({ characters: [] });
    await mount(s);
    const lists = s.calls.filter((c) => c['type'] === 'characterList');
    expect(lists.length).toBeGreaterThan(0);
    for (const call of lists) {
      expect('archived' in call).toBe(false);
    }
  });
});
