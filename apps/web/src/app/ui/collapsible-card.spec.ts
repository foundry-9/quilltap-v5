import { Component, signal } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it } from 'vitest';

import { CollapsibleCard } from './collapsible-card';

/**
 * The controlled/uncontrolled split (v4 `CollapsibleCard`'s `isOpen` +
 * `onOpenChange` pair, ported for the chat sidebar's single-open accordion).
 */

@Component({
  imports: [CollapsibleCard],
  template: `
    <qt-collapsible-card title="Uncontrolled" [defaultOpen]="defaultOpen()">
      <p class="body">plain</p>
    </qt-collapsible-card>
  `,
})
class UncontrolledHost {
  readonly defaultOpen = signal(false);
}

@Component({
  imports: [CollapsibleCard],
  template: `
    <qt-collapsible-card title="Controlled" [isOpen]="open()" (openChange)="open.set($event)">
      <p class="body">driven</p>
    </qt-collapsible-card>
  `,
})
class ControlledHost {
  readonly open = signal(false);
}

@Component({
  imports: [CollapsibleCard],
  template: `
    <qt-collapsible-card title="Deaf" [isOpen]="open" (openChange)="heard.push($event)">
      <p class="body">deaf</p>
    </qt-collapsible-card>
  `,
})
class DeafHost {
  /** Never updated — proves the card does NOT open itself in controlled mode. */
  readonly open = false;
  readonly heard: boolean[] = [];
}

async function render<T>(host: new () => T): Promise<ComponentFixture<T>> {
  TestBed.configureTestingModule({ imports: [host as never] });
  const fixture = TestBed.createComponent(host);
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
  return fixture;
}

function header(fixture: ComponentFixture<unknown>): HTMLButtonElement {
  return fixture.nativeElement.querySelector('.qt-collapsible-card-header') as HTMLButtonElement;
}

function body(fixture: ComponentFixture<unknown>): HTMLElement | null {
  return fixture.nativeElement.querySelector('.body');
}

describe('CollapsibleCard', () => {
  it('owns its open state when uncontrolled', async () => {
    const fixture = await render(UncontrolledHost);
    expect(body(fixture)).toBeNull();
    expect(header(fixture).getAttribute('aria-expanded')).toBe('false');

    header(fixture).click();
    fixture.detectChanges();
    expect(body(fixture)).not.toBeNull();
    expect(header(fixture).getAttribute('aria-expanded')).toBe('true');

    header(fixture).click();
    fixture.detectChanges();
    expect(body(fixture)).toBeNull();
  });

  it('honours defaultOpen when uncontrolled', async () => {
    TestBed.configureTestingModule({ imports: [UncontrolledHost] });
    const fixture = TestBed.createComponent(UncontrolledHost);
    fixture.componentInstance.defaultOpen.set(true);
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
    expect(body(fixture)).not.toBeNull();
  });

  it('defers to the bound isOpen when controlled', async () => {
    const fixture = await render(ControlledHost);
    expect(body(fixture)).toBeNull();

    // The header click reports the NEXT desired state; the host applies it.
    header(fixture).click();
    fixture.detectChanges();
    expect(fixture.componentInstance.open()).toBe(true);
    expect(body(fixture)).not.toBeNull();

    header(fixture).click();
    fixture.detectChanges();
    expect(fixture.componentInstance.open()).toBe(false);
    expect(body(fixture)).toBeNull();
  });

  it('never opens itself in controlled mode when the parent ignores openChange', async () => {
    const fixture = await render(DeafHost);
    header(fixture).click();
    fixture.detectChanges();
    expect(fixture.componentInstance.heard).toEqual([true]);
    expect(body(fixture)).toBeNull();
  });
});
