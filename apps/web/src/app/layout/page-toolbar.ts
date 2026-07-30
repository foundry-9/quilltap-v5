import { NgTemplateOutlet } from '@angular/common';
import { ChangeDetectionStrategy, Component, inject } from '@angular/core';

import { AutonomousRoomBadges } from '../autonomous/autonomous-room-badges';
import { SearchBar } from '../search/search-bar';
import { NavContentWidthToggle } from './nav-content-width-toggle';
import { PageToolbarService } from './page-toolbar.service';
import { QueueStatusBadges } from './queue-status-badges';

/**
 * The top page toolbar (v4 `components/layout/page-toolbar.tsx`, dogfood #38):
 * left = the page's slot content; center = the global SearchBar; right, in
 * v4's exact occupant order = AutonomousRoomBadges, QueueStatusBadges, the
 * page's right slot, NavContentWidthToggle. All responsive behavior is CSS
 * (`.qt-page-toolbar*` — ported verbatim long before this component existed).
 *
 * Mounted once in the shell inside `<main>`, above the scroller (v4
 * `app-layout.tsx:113`); the shell only renders in the operational state, so
 * v4's setup/unlock/no-session exclusions hold structurally.
 */
@Component({
  selector: 'qt-page-toolbar',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [NgTemplateOutlet, AutonomousRoomBadges, NavContentWidthToggle, QueueStatusBadges, SearchBar],
  template: `
    <div class="qt-page-toolbar">
      <!-- Left section: page-specific content -->
      <div class="qt-page-toolbar-left">
        @if (toolbar.leftContent(); as left) {
          <ng-container [ngTemplateOutlet]="left" />
        }
      </div>

      <!-- Center section: search bar -->
      <div class="qt-page-toolbar-center">
        <qt-search-bar />
      </div>

      <!-- Right section: autonomous rooms + queue status + page-specific content + full-width toggle -->
      <div class="qt-page-toolbar-right">
        <qt-autonomous-room-badges />
        <qt-queue-status-badges />
        @if (toolbar.rightContent(); as right) {
          <ng-container [ngTemplateOutlet]="right" />
        }
        <qt-nav-content-width-toggle />
      </div>
    </div>
  `,
})
export class PageToolbar {
  protected readonly toolbar = inject(PageToolbarService);
}
