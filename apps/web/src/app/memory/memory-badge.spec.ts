import { ChangeDetectionStrategy, Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { MemoryBadge } from './memory-badge';

@Component({
  selector: 'qt-host',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [MemoryBadge],
  template: `<a href="#x" (click)="navigated = true"
    ><qt-memory-badge chatId="c1" [count]="3" (reextract)="got.push($event)"
  /></a>`,
})
class Host {
  navigated = false;
  got: string[] = [];
}

describe('MemoryBadge (v4 ChatCard.tsx:267-283)', () => {
  it('renders the count with v4 aria-label bytes, at zero too', () => {
    const f = TestBed.createComponent(MemoryBadge);
    f.componentRef.setInput('chatId', 'c1');
    f.componentRef.setInput('count', 0);
    f.detectChanges();
    const b = (f.nativeElement as HTMLElement).querySelector('button')!;
    expect(b.getAttribute('aria-label')).toBe('0 memories — delete and re-extract');
    expect(b.textContent).toContain('0');
  });

  it('emits the chat id and keeps the click from navigating the card link', () => {
    const f = TestBed.createComponent(Host);
    f.detectChanges();
    const b = (f.nativeElement as HTMLElement).querySelector('button')!;
    const ev = new MouseEvent('click', { bubbles: true, cancelable: true });
    b.dispatchEvent(ev);
    expect(f.componentInstance.got).toEqual(['c1']);
    expect(f.componentInstance.navigated).toBe(false);
    expect(ev.defaultPrevented).toBe(true);
  });
});
