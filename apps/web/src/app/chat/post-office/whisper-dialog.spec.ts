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

describe('WhisperDialog — WCAG semantics (P4.D358)', () => {
  afterEach(() => TestBed.resetTestingModule());

  const esc = (): KeyboardEvent =>
    new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });

  it('announces as a dialog named by its title', () => {
    const fixture = mount();
    const dlg = fixture.nativeElement.querySelector('[role="dialog"]') as HTMLElement;
    expect(dlg.getAttribute('aria-modal')).toBe('true');
    expect(dlg.getAttribute('tabindex')).toBe('-1');
    const id = dlg.getAttribute('aria-labelledby')!;
    expect(id).toMatch(/^qt-dialog-\d+$/);
    expect(
      (fixture.nativeElement.querySelector('#' + id) as HTMLElement).textContent?.trim(),
    ).toBe('Whisper to Bram');
  });

  it('a document Escape closes exactly once', () => {
    const fixture = mount();
    const closed = vi.fn();
    fixture.componentInstance.close.subscribe(closed);
    document.dispatchEvent(esc());
    expect(closed).toHaveBeenCalledTimes(1);
  });

  it('an Escape typed in the textarea closes exactly ONCE (the document listener skips it)', () => {
    const fixture = mount();
    const closed = vi.fn();
    fixture.componentInstance.close.subscribe(closed);
    textarea(fixture).dispatchEvent(esc());
    expect(closed).toHaveBeenCalledTimes(1);
  });
});
