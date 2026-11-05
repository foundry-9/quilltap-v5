import { Component, TemplateRef, signal, viewChild } from '@angular/core';
import { provideRouter } from '@angular/router';
import { TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { Subject } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { PageToolbar } from './page-toolbar';
import { PageToolbarService } from './page-toolbar.service';

/**
 * The premise of v4 `3c41cf3da`'s `.qt-page-toolbar-left:empty { flex: 0 1
 * auto; }` (P4.D383 item 11): with no page content, the left section must
 * match `:empty`, so the centre search bar is not pushed off-centre by an
 * empty flex-1 box. v5 renders the slot through an `@if` INSIDE the div, which
 * leaves Angular's anchor COMMENT there — `:empty` ignores comments, but a
 * whitespace text node would defeat it. Measured here, both ways.
 */
@Component({
  imports: [PageToolbar],
  template: `
    <qt-page-toolbar />
    <ng-template #left><span class="test-left-slot">breadcrumb</span></ng-template>
  `,
})
class Host {
  readonly left = viewChild.required<TemplateRef<unknown>>('left');
}

describe('PageToolbar left section :empty', () => {
  beforeEach(() => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response(JSON.stringify({}), { status: 200 })),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    TestBed.resetTestingModule();
  });

  function render() {
    TestBed.configureTestingModule({
      imports: [Host],
      providers: [
        provideRouter([]),
        provideTanStackQuery(new QueryClient()),
        {
          provide: CoreClient,
          useValue: {
            events$: new Subject(),
            connection: signal('idle'),
            resyncCounter: signal(0),
            listAutonomousRooms: async () => [],
          },
        },
      ],
    });
    const fixture = TestBed.createComponent(Host);
    fixture.detectChanges();
    const left = () =>
      (fixture.nativeElement as HTMLElement).querySelector('.qt-page-toolbar-left') as HTMLElement;
    return { fixture, left };
  }

  it('matches :empty with no left content', () => {
    const { left } = render();
    expect(left().matches(':empty')).toBe(true);
  });

  it('does not match :empty with left content, and matches again once cleared', () => {
    const { fixture, left } = render();
    const svc = TestBed.inject(PageToolbarService);
    svc.setLeftContent(fixture.componentInstance.left());
    fixture.detectChanges();
    expect(left().querySelector('.test-left-slot')).not.toBeNull();
    expect(left().matches(':empty')).toBe(false);
    svc.setLeftContent(null);
    fixture.detectChanges();
    expect(left().matches(':empty')).toBe(true);
  });
});
