import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { PromptFieldExample } from './prompt-field-example';

/**
 * `qt-prompt-field-example` against v4 `components/prompt-fields/PromptFieldLabel
 * .tsx:41-58` (`export function PromptFieldExample`, `2f4254b42`): the `<p>`
 * with the caller's classes, the literal `Written as: ` lead-in, the example in
 * an `<em>`, and v4's `className = 'text-xs qt-text-secondary'` default.
 *
 * The host must contribute NO box (v4's is a React function component): pinned
 * by asserting `display: contents` on the element, because every adoption site
 * relies on the `<p>` being its container's direct layout child.
 */

@Component({
  imports: [PromptFieldExample],
  template: `
    @if (className(); as c) {
      <qt-prompt-field-example [example]="example()" [className]="c" />
    } @else {
      <qt-prompt-field-example [example]="example()" />
    }
  `,
})
class Host {
  readonly example = signal('an example');
  readonly className = signal<string | undefined>(undefined);
}

function mount(): ComponentFixture<Host> {
  const fixture = TestBed.createComponent(Host);
  fixture.detectChanges();
  return fixture;
}

describe('qt-prompt-field-example', () => {
  it("renders v4's `Written as: <em>…</em>` line with the default classes", () => {
    const fixture = mount();
    const p = fixture.nativeElement.querySelector('p') as HTMLParagraphElement;
    expect(p).toBeTruthy();
    // v4's `className` parameter default, verbatim.
    expect(p.getAttribute('class')).toBe('text-xs qt-text-secondary');
    expect(p.textContent).toBe('Written as: an example');
    const em = p.querySelector('em') as HTMLElement;
    expect(em).toBeTruthy();
    expect(em.textContent).toBe('an example');
    // The lead-in is v4's literal, outside the emphasis.
    expect(p.firstChild?.textContent).toBe('Written as: ');
  });

  it("takes the caller's class override (v4's `className` prop)", () => {
    const fixture = mount();
    fixture.componentInstance.className.set('text-xs qt-text-secondary mt-1');
    fixture.detectChanges();
    const p = fixture.nativeElement.querySelector('p') as HTMLParagraphElement;
    expect(p.getAttribute('class')).toBe('text-xs qt-text-secondary mt-1');
  });

  it('contributes no box of its own — the host is `display: contents`', () => {
    const fixture = mount();
    const host = fixture.nativeElement.querySelector('qt-prompt-field-example') as HTMLElement;
    expect(host.classList.contains('contents')).toBe(true);
  });

  it('tracks the example as it changes', () => {
    const fixture = mount();
    fixture.componentInstance.example.set('a different example');
    fixture.detectChanges();
    const p = fixture.nativeElement.querySelector('p') as HTMLParagraphElement;
    expect(p.textContent).toBe('Written as: a different example');
  });
});
