import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { RouterLink } from '@angular/router';

import type { CharacterDetail } from '../../../../core/core-contract';
import { Icon } from '../../../../ui/icon';
import { highlightSegments, type HighlightSegment } from '../template-highlighter';

/**
 * The System Prompts tab (v4 `components/SystemPromptsTab.tsx`): a read-only
 * list of `character.systemPrompts`, the default one highlighted, each body
 * rendered with the `{{char}}`/`{{user}}` highlighter — editing lives on the
 * character-edit screen (`/characters/:id/edit?tab=system-prompts`).
 */
@Component({
  selector: 'qt-character-system-prompts-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, Icon],
  template: `
    <div class="space-y-6">
      <div class="flex items-center justify-between flex-wrap gap-2">
        <div>
          <h2 class="qt-heading-4 text-foreground">System Prompts</h2>
          <p class="qt-text-small">
            Named system prompts for this character. The default prompt is used when starting new chats.
          </p>
        </div>
        <a
          [routerLink]="['/characters', characterId(), 'edit']"
          [queryParams]="{ tab: 'system-prompts' }"
          class="qt-button-primary"
        >
          <qt-icon name="pencil" class="w-4 h-4" />
          Edit Prompts
        </a>
      </div>

      @if (character().systemPrompts.length > 0) {
        <div class="space-y-4">
          @for (prompt of character().systemPrompts; track prompt.id) {
            <div
              [class]="
                'rounded-lg border p-4 ' +
                (prompt.isDefault ? 'qt-border-primary/40 qt-bg-primary/5' : 'qt-border-default qt-bg-card')
              "
            >
              <div class="flex items-center gap-2 mb-3">
                <h3 class="qt-text-primary">{{ prompt.name }}</h3>
                @if (prompt.isDefault) {
                  <span class="qt-badge-primary">Default</span>
                }
              </div>
              <pre class="overflow-hidden rounded-md qt-bg-muted/80 p-4 text-sm text-foreground">
                <code class="text-sm whitespace-pre-wrap break-words">
                  @for (seg of segmentsFor(prompt.content); track $index) {
                    @switch (seg.kind) {
                      @case ('char-template') {
                        <span class="px-0.5 rounded border-b-2 qt-badge-chat qt-border-info" [title]="seg.title">{{
                          seg.text
                        }}</span>
                      }
                      @case ('user-template') {
                        <span
                          class="px-0.5 rounded border-b-2 qt-badge-user-character qt-border-success"
                          [title]="seg.title"
                          >{{ seg.text }}</span
                        >
                      }
                      @case ('char-hardcoded') {
                        <span
                          class="px-0.5 rounded border-b-2 border-dashed qt-bg-warning/10 qt-border-warning"
                          [title]="seg.title"
                          >{{ seg.text }}</span
                        >
                      }
                      @case ('user-hardcoded') {
                        <span
                          class="px-0.5 rounded border-b-2 border-dashed qt-bg-warning/10 qt-border-warning"
                          [title]="seg.title"
                          >{{ seg.text }}</span
                        >
                      }
                      @default {
                        <span>{{ seg.text }}</span>
                      }
                    }
                  }
                </code>
              </pre>
            </div>
          }
        </div>
      } @else {
        <div class="rounded-lg border border-dashed qt-border-default qt-bg-muted/30 p-8 text-center">
          <qt-icon name="code" class="mx-auto w-12 h-12 qt-text-secondary/50" />
          <p class="mt-4 qt-text-small">No system prompts defined for this character.</p>
          <a
            [routerLink]="['/characters', characterId(), 'edit']"
            [queryParams]="{ tab: 'system-prompts' }"
            class="mt-4 inline-flex items-center gap-2 qt-label text-primary hover:text-primary/80"
          >
            <qt-icon name="plus" class="w-4 h-4" />
            Add a system prompt
          </a>
        </div>
      }
    </div>
  `,
})
export class CharacterSystemPromptsTab {
  readonly characterId = input.required<string>();
  readonly character = input.required<CharacterDetail>();
  readonly defaultPartnerName = input<string | null>(null);

  protected segmentsFor(content: string): HighlightSegment[] {
    return highlightSegments(content, this.character().name, this.defaultPartnerName());
  }
}
