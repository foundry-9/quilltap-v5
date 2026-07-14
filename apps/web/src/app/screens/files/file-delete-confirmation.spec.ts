import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import type { FileAssociations } from '../../core/core-contract';
import { FileDeleteConfirmation } from './file-delete-confirmation';

/**
 * The presentational delete-associations dialog (v4
 * `components/files/FileDeleteConfirmation.tsx`). v4's UI offers ONE action —
 * "Delete Anyway" → the parent's dissociate dispatch (`?dissociate=true`). There
 * is no separate "force" affordance in v4's FileBrowser (the `force` query param
 * exists server-side but no v4 client surface sends it), so this component emits
 * a single `confirm`; the parent owns the flag choice (`FilesBrowser` uses
 * `dissociate:true`, matching v4).
 */

const ASSOCIATIONS: FileAssociations = {
  characters: [
    { id: 'c1', name: 'Lorian', usage: 'avatar' },
    { id: 'c2', name: 'Riya', usage: '' },
  ],
  messages: [
    { chatId: 'ch1', chatName: 'A Midnight Correspondence', messageId: 'm1' },
    { chatId: 'ch2', chatName: 'The Second Salon', messageId: 'm2' },
  ],
};

function render(
  inputs: { filename?: string; associations?: FileAssociations; isDeleting?: boolean } = {},
): { fixture: ComponentFixture<FileDeleteConfirmation>; component: FileDeleteConfirmation } {
  TestBed.configureTestingModule({ imports: [FileDeleteConfirmation] });
  const fixture = TestBed.createComponent(FileDeleteConfirmation);
  fixture.componentRef.setInput('filename', inputs.filename ?? 'portrait.png');
  fixture.componentRef.setInput('associations', inputs.associations ?? ASSOCIATIONS);
  if (inputs.isDeleting !== undefined) fixture.componentRef.setInput('isDeleting', inputs.isDeleting);
  fixture.detectChanges();
  return { fixture, component: fixture.componentInstance };
}

function dialogButton(
  fixture: ComponentFixture<FileDeleteConfirmation>,
  label: string,
): HTMLButtonElement {
  const buttons = Array.from(
    fixture.nativeElement.querySelectorAll('[role=dialog] button'),
  ) as HTMLButtonElement[];
  const match = buttons.find((b) => b.textContent?.trim().includes(label));
  if (!match) throw new Error(`No dialog button matching "${label}"`);
  return match;
}

describe('FileDeleteConfirmation', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders the itemized payload: filename, characters (with usage), messages by chat', () => {
    const { fixture } = render();
    const dialog = fixture.nativeElement.querySelector('[role=dialog]') as HTMLElement;
    expect(dialog).not.toBeNull();
    expect(dialog.textContent).toContain('This file is in use');
    expect(dialog.textContent).toContain('portrait.png');

    // Characters section, with the usage rider only when present.
    expect(dialog.textContent).toContain('Used by characters:');
    expect(dialog.textContent).toContain('Lorian');
    expect(dialog.textContent).toContain('— avatar');
    expect(dialog.textContent).toContain('Riya');

    // Messages section, listed by chat name.
    expect(dialog.textContent).toContain('Attached to messages in:');
    expect(dialog.textContent).toContain('A Midnight Correspondence');
    expect(dialog.textContent).toContain('The Second Salon');
  });

  it('hides the characters section when there are no characters', () => {
    const { fixture } = render({
      associations: { characters: [], messages: ASSOCIATIONS.messages },
    });
    const dialog = fixture.nativeElement.querySelector('[role=dialog]') as HTMLElement;
    expect(dialog.textContent).not.toContain('Used by characters:');
    expect(dialog.textContent).toContain('Attached to messages in:');
  });

  it('hides the messages section when there are no messages', () => {
    const { fixture } = render({
      associations: { characters: ASSOCIATIONS.characters, messages: [] },
    });
    const dialog = fixture.nativeElement.querySelector('[role=dialog]') as HTMLElement;
    expect(dialog.textContent).toContain('Used by characters:');
    expect(dialog.textContent).not.toContain('Attached to messages in:');
  });

  it('emits confirm when "Delete Anyway" is clicked', () => {
    const { fixture, component } = render();
    let fired = 0;
    component.confirm.subscribe(() => (fired += 1));
    dialogButton(fixture, 'Delete Anyway').click();
    expect(fired).toBe(1);
  });

  it('emits cancel when "Cancel" is clicked and on the modal close', () => {
    const { fixture, component } = render();
    let cancelled = 0;
    component.cancel.subscribe(() => (cancelled += 1));
    dialogButton(fixture, 'Cancel').click();
    expect(cancelled).toBe(1);
  });

  it('disables both actions and relabels the confirm while deleting', () => {
    const { fixture } = render({ isDeleting: true });
    const cancel = dialogButton(fixture, 'Cancel');
    const confirm = dialogButton(fixture, 'Deleting...');
    expect(cancel.disabled).toBe(true);
    expect(confirm.disabled).toBe(true);
    expect(confirm.textContent?.trim()).toBe('Deleting...');
  });
});
