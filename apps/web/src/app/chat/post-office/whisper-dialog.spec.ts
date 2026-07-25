import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { WhisperDialog } from './whisper-dialog';

function mount(): ComponentFixture<WhisperDialog> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [WhisperDialog] });
  const fixture = TestBed.createComponent(WhisperDialog);
  fixture.componentRef.setInput('targetName', 'Bram');
  fixture.componentRef.setInput('targetParticipantId', 'p-bram');
  fixture.detectChanges();
  return fixture;
}

function textarea(fixture: ComponentFixture<unknown>): HTMLTextAreaElement {
  return fixture.nativeElement.querySelector('textarea') as HTMLTextAreaElement;
}

function type(fixture: ComponentFixture<unknown>, value: string): void {
  const el = textarea(fixture);
  el.value = value;
  el.dispatchEvent(new Event('input'));
  fixture.detectChanges();
}

function whisperButton(fixture: ComponentFixture<unknown>): HTMLButtonElement {
  return [...fixture.nativeElement.querySelectorAll('.qt-dialog-footer button')].find((b) =>
    (b as HTMLElement).textContent?.includes('Whisper'),
  ) as HTMLButtonElement;
}

describe('WhisperDialog (v4 components/chat/WhisperDialog.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('names the target in the title, the blurb and the placeholder', () => {
    const fixture = mount();
    const text = (fixture.nativeElement.textContent ?? '').replace(/\s+/g, ' ');
    expect(text).toContain('Whisper to Bram');
    expect(text).toContain('only be visible to you and Bram');
    expect(textarea(fixture).placeholder).toBe('Write a private message to Bram...');
  });

  it('reports the trimmed line with its target, then closes AT ONCE (v4’s sequencing)', () => {
    const fixture = mount();
    const seen: string[] = [];
    const sends: Array<{ targetParticipantId: string; content: string }> = [];
    fixture.componentInstance.send.subscribe((e) => {
      sends.push(e);
      seen.push('send');
    });
    fixture.componentInstance.close.subscribe(() => seen.push('close'));

    type(fixture, '  Meet me on the ridge.  ');
    whisperButton(fixture).click();

    expect(sends).toEqual([{ targetParticipantId: 'p-bram', content: 'Meet me on the ridge.' }]);
    // The line goes out BEFORE the close, and the close is not deferred — the
    // operator never waits out the reply.
    expect(seen).toEqual(['send', 'close']);
  });

  it('will not send an empty or whitespace-only whisper', () => {
    const fixture = mount();
    const send = vi.fn();
    fixture.componentInstance.send.subscribe(send);
    expect(whisperButton(fixture).disabled).toBe(true);
    type(fixture, '   ');
    expect(whisperButton(fixture).disabled).toBe(true);
    whisperButton(fixture).click();
    expect(send).not.toHaveBeenCalled();
  });

  it('sends once even if the commit is driven twice', () => {
    const fixture = mount();
    const send = vi.fn();
    fixture.componentInstance.send.subscribe(send);
    type(fixture, 'psst');
    whisperButton(fixture).click();
    whisperButton(fixture).click();
    expect(send).toHaveBeenCalledTimes(1);
  });

  it('Enter sends, Shift+Enter does not, Escape closes (v4 handleKeyDown)', () => {
    const fixture = mount();
    const send = vi.fn();
    const closed = vi.fn();
    fixture.componentInstance.send.subscribe(send);
    fixture.componentInstance.close.subscribe(closed);
    type(fixture, 'psst');

    textarea(fixture).dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Enter', shiftKey: true, bubbles: true }),
    );
    expect(send).not.toHaveBeenCalled();

    textarea(fixture).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    expect(send).toHaveBeenCalledTimes(1);

    textarea(fixture).dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(closed).toHaveBeenCalled();
  });

  it('a backdrop click closes it', () => {
    const fixture = mount();
    const closed = vi.fn();
    fixture.componentInstance.close.subscribe(closed);
    (fixture.nativeElement.querySelector('.qt-bg-overlay-caption') as HTMLElement).click();
    expect(closed).toHaveBeenCalled();
  });
});
