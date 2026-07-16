import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { RouterLink } from '@angular/router';

import { ProjectItem } from './project-item';
import type { HomepageProject } from './home.api';

/**
 * The homepage Active Projects section (v4
 * `components/homepage/ProjectsSection.tsx`): header + "View all" into
 * Prospero, the empty arm, and the project rows (§1: max 12, activity-sorted
 * by the server — rendered as sent).
 */
@Component({
  selector: 'qt-projects-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, ProjectItem],
  template: `
    <div class="qt-homepage-section">
      <div class="qt-homepage-section-header">
        <h2 class="qt-homepage-section-title">Active Projects</h2>
        <a routerLink="/prospero" class="qt-homepage-section-link">View all &rarr;</a>
      </div>
      <div class="qt-homepage-section-content">
        @if (projects().length === 0) {
          <div class="text-center py-6 qt-text-secondary">
            <p class="text-sm">No projects yet</p>
            <p class="text-xs">Create a project to organize your chats</p>
          </div>
        } @else {
          @for (project of projects(); track project.id) {
            <qt-project-item [project]="project" />
          }
        }
      </div>
    </div>
  `,
})
export class ProjectsSection {
  readonly projects = input.required<HomepageProject[]>();
}
