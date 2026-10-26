import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { ToastService } from '../ui/toast.service';
import { ItemFolderPickerDialog } from './item-folder-picker-dialog';

/** v4 `components/item-browser/ItemFolderPickerDialog.tsx` (`4fbdad186`). */

@Component({
  imports: [ItemFolderPickerDialog],
  template: `
    <qt-item-folder-picker-dialog
      [mode]="mode()"
      subject="&quot;lantern&quot;"
      [folders]="folders()"
      [loading]="loading()"
      rootLabel="Inventory (top level)"
      [createFolder]="createFolder"
      [currentFolder]="current()"
      [working]="working()"
      note="Folders are for your browsing; what anyone carries stays exactly as it was."
      (closed)="log.push('close')"
      (submitted)="log.push('to:' + $event)"
    >
      <p class="extra">extra choice</p>
    </qt-item-folder-picker-dialog>
  `,
})
class Host {
  readonly mode = signal<'move' | 'copy'>('move');
  readonly folders = signal([{ path: 'Arms' }, { path: 'Arms/Bows' }]);
  readonly loading = signal(false);
  readonly current = signal<string | null>('Arms');
  readonly working = signal(false);
  readonly createFolder = vi.fn(async (path: string) => ({ path }));
  readonly log: string[] = [];
}

function render() {
  TestBed.configureTestingModule({ imports: [Host] });
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  const el = fixture.nativeElement as HTMLElement;
  const options = () => [...el.querySelectorAll('[role="option"]')] as HTMLButtonElement[];
  const footer = (text: string) =>
    [...el.querySelectorAll('[qt-modal-footer] button')].find((b) => b.textContent!.trim() === text) as HTMLButtonElement;
  return { fixture, el, host: fixture.componentInstance, options, footer };
}

describe('ItemFolderPickerDialog', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('titles itself by verb and subject, notes the caller’s line, lists the root then the indented tree, projects extras', () => {
    const { el, options } = render();
    expect(el.querySelector('.qt-dialog-title')!.textContent).toBe('Move "lantern" to a folder');
    expect(el.querySelector('.qt-dialog-body p')!.textContent).toBe(
      'Folders are for your browsing; what anyone carries stays exactly as it was.',
    );
    expect(options().map((o) => o.textContent!.replace(/\s+/g, ' ').trim())).toEqual([
      'Inventory (top level)',
      'Arms (here now)',
      'Bows',
    ]);
    expect(options().map((o) => o.style.paddingLeft)).toEqual(['8px', '24px', '40px']);
    expect(el.querySelector('.extra')!.textContent).toBe('extra choice');
  });

  it('a move cannot choose the shared current folder; a copy can', () => {
    const { fixture, host, options } = render();
    const arms = options()[1];
    expect(arms.disabled).toBe(true);
    expect(arms.getAttribute('aria-disabled')).toBe('true');
    expect(arms.title).toBe('Already here');
    host.mode.set('copy');
    fixture.detectChanges();
    expect(options()[1].disabled).toBe(false);
    expect(options()[1].textContent).not.toContain('(here now)');
  });

  it('submits only with a choosable target and not while working; the button says Moving… while working', () => {
    const { fixture, host, options, footer } = render();
    expect(footer('Move').disabled).toBe(true);
    options()[2].click();
    fixture.detectChanges();
    expect(options()[2].getAttribute('aria-selected')).toBe('true');
    footer('Move').click();
    expect(host.log).toEqual(['to:Arms/Bows']);
    host.working.set(true);
    fixture.detectChanges();
    expect(footer('Moving…').disabled).toBe(true);
  });

  it('New folder… opens under the highlighted folder, creates the joined path and highlights the answer', async () => {
    const { fixture, el, host, options, footer } = render();
    options()[2].click();
    fixture.detectChanges();
    footer('New folder…').click();
    fixture.detectChanges();
    expect(footer('New folder…').disabled).toBe(true);
    const input = el.querySelector('input[aria-label="New folder name"]') as HTMLInputElement;
    expect((input.parentElement as HTMLElement).style.paddingLeft).toBe('56px');
    input.value = 'Long';
    input.dispatchEvent(new Event('input'));
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    await fixture.whenStable();
    fixture.detectChanges();
    expect(host.createFolder).toHaveBeenCalledWith('Arms/Bows/Long');
    expect(el.querySelector('input[aria-label="New folder name"]')).toBeNull();
  });

  it('with nothing highlighted the input sits at the end; a failed create toasts its words', async () => {
    const { fixture, el, host, footer } = render();
    footer('New folder…').click();
    fixture.detectChanges();
    const list = el.querySelector('[role="listbox"]')!;
    expect(list.lastElementChild!.querySelector('input')).not.toBeNull();
    host.createFolder.mockRejectedValueOnce(new Error('That folder already exists'));
    const input = el.querySelector('input[aria-label="New folder name"]') as HTMLInputElement;
    input.value = 'Arms';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    (input.nextElementSibling as HTMLButtonElement).click();
    await fixture.whenStable();
    expect(TestBed.inject(ToastService).toasts().at(-1)?.message).toBe('That folder already exists');
  });

  it('Escape closes unless working; Loading folders… while the tree loads', () => {
    const { fixture, el, host } = render();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    host.working.set(true);
    fixture.detectChanges();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(host.log).toEqual(['close']);
    host.loading.set(true);
    fixture.detectChanges();
    expect(el.querySelector('.qt-dialog-body')!.textContent).toContain('Loading folders…');
  });
});
