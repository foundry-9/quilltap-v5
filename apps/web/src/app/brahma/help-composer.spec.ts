/**
 * HelpComposer — asserted against v4 `HelpChatComposer.tsx`: Enter sends the
 * trimmed content, Shift+Enter does not, `disabled` blocks the send, an empty
 * field never emits, and a send clears the field.
 */

import { ComponentFixture, TestBed } from '@angular/core/testing';
import { afterEach, describe, expect, it } from 'vitest';

import { HelpComposer } from './help-composer';

async function render(): Promise<ComponentFixture<HelpComposer>> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({ imports: [HelpComposer] });
  const fixture = TestBed.createComponent(HelpComposer);
  fixture.detectChanges();
  return fixture;
}

function textarea(fixture: ComponentFixture<HelpComposer>): HTMLTextAreaElement {
  return fixture.nativeElement.querySelector('textarea') as HTMLTextAreaElement;
}

function type(fixture: ComponentFixture<HelpComposer>, value: string): void {
  const el = textarea(fixture);
  el.value = value;
  el.dispatchEvent(new Event('input'));
  fixture.detectChanges();
}

describe('HelpComposer (v4 HelpChatComposer.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('Enter (no shift) sends the trimmed content and clears the field', async () => {
    const fixture = await render();
    const sent: string[] = [];
    fixture.componentInstance.send.subscribe((v) => sent.push(v));

    type(fixture, '  hello engine  ');
    textarea(fixture).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    fixture.detectChanges();

    expect(sent).toEqual(['hello engine']);
    expect(textarea(fixture).value).toBe('');
  });

  it('Shift+Enter does NOT send', async () => {
    const fixture = await render();
    const sent: string[] = [];
    fixture.componentInstance.send.subscribe((v) => sent.push(v));

    type(fixture, 'line one');
    textarea(fixture).dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Enter', shiftKey: true }),
    );
    fixture.detectChanges();

    expect(sent).toEqual([]);
  });

  it('an empty / whitespace-only field never emits', async () => {
    const fixture = await render();
    const sent: string[] = [];
    fixture.componentInstance.send.subscribe((v) => sent.push(v));

    type(fixture, '   ');
    textarea(fixture).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    fixture.detectChanges();

    expect(sent).toEqual([]);
  });

  it('disabled blocks the send and disables the controls', async () => {
    const fixture = await render();
    const sent: string[] = [];
    fixture.componentInstance.send.subscribe((v) => sent.push(v));
    type(fixture, 'ready');

    fixture.componentRef.setInput('disabled', true);
    fixture.detectChanges();

    textarea(fixture).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    fixture.detectChanges();

    expect(sent).toEqual([]);
    expect(textarea(fixture).disabled).toBe(true);
    const button = fixture.nativeElement.querySelector('button') as HTMLButtonElement;
    expect(button.disabled).toBe(true);
  });

  it('honors the placeholder input', async () => {
    const fixture = await render();
    fixture.componentRef.setInput('placeholder', 'Speak to the engine…');
    fixture.detectChanges();
    expect(textarea(fixture).placeholder).toBe('Speak to the engine…');
  });
});
