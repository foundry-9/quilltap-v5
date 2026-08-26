import { Component, TemplateRef, signal, viewChild } from '@angular/core';
import { provideRouter } from '@angular/router';
import { TestBed } from '@angular/core/testing';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';
import { Subject } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../core/core-client';
import { PageToolbar } from './page-toolbar';
import { PageToolbarService } from './page-toolbar.service';

/** A host that supplies slot templates the way a page would. */
@Component({
  imports: [PageToolbar],
  template: `
    <qt-page-toolbar />
    <ng-template #left><span class="test-left-slot">breadcrumb</span></ng-template>
    <ng-template #right><span class="test-right-slot">cost</span></ng-template>
  `,
})
class Host {
  readonly left = viewChild.required<TemplateRef<unknown>>('left');
  readonly right = viewChild.required<TemplateRef<unknown>>('right');
}

describe('PageToolbar (v4 page-toolbar.tsx)', () => {
  beforeEach(() => {
    // The occupants poll (autonomous rooms via CoreClient HTTP, queue badges
    // via fetch) — stub the network so mounting is inert. The chips also sit on
    // TanStack Query and the realtime hub now, so the module needs both a
    // QueryClient and a CoreClient carrying the live-stream surface.
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response(JSON.stringify({}), { status: 200 })),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  function render() {
    TestBed.resetTestingModule();
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
    return fixture;
  }

  it('renders the three sections with v4\'s right-section occupant order', () => {
    const fixture = render();
    const toolbar = fixture.nativeElement.querySelector('.qt-page-toolbar') as HTMLElement;
    expect(toolbar).not.toBeNull();
    expect(toolbar.querySelector('.qt-page-toolbar-left')).not.toBeNull();
    expect(toolbar.querySelector('.qt-page-toolbar-center qt-search-bar')).not.toBeNull();
    const right = toolbar.querySelector('.qt-page-toolbar-right') as HTMLElement;
    const order = Array.from(right.children).map((el) => el.tagName.toLowerCase());
    // v4: AutonomousRoomBadges, QueueStatusBadges, {rightContent}, NavContentWidthToggle.
    expect(order[0]).toBe('qt-autonomous-room-badges');
    expect(order[1]).toBe('qt-queue-status-badges');
    expect(order[order.length - 1]).toBe('qt-nav-content-width-toggle');
  });

  it('renders registered slot templates and clears them again', () => {
    const fixture = render();
    const svc = TestBed.inject(PageToolbarService);
    svc.setLeftContent(fixture.componentInstance.left());
    svc.setRightContent(fixture.componentInstance.right());
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('.qt-page-toolbar-left .test-left-slot')).not.toBeNull();
    const right = fixture.nativeElement.querySelector('.qt-page-toolbar-right') as HTMLElement;
    expect(right.querySelector('.test-right-slot')).not.toBeNull();
    // The right slot sits BETWEEN the queue badges and the width toggle.
    const html = right.innerHTML;
    expect(html.indexOf('qt-queue-status-badges')).toBeLessThan(html.indexOf('test-right-slot'));
    expect(html.indexOf('test-right-slot')).toBeLessThan(html.indexOf('qt-nav-content-width-toggle'));

    svc.clearLeft();
    svc.clearRight();
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('.test-left-slot')).toBeNull();
    expect(fixture.nativeElement.querySelector('.test-right-slot')).toBeNull();
  });
});
