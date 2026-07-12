import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { ProjectDetail, RoleplayTemplateDto } from '../../../core/core-contract';
import { CollapsibleCard } from '../../../ui/collapsible-card';
import { ErrorAlert } from '../../../ui/error-alert';
import { fetchRoleplayTemplates, templateKeys } from '../../settings/templates/templates.api';
import { projectKeys, updateProject } from '../projects.api';

/**
 * The Model Behavior card (v4 `ModelBehaviorCard.tsx`): immediate-save selects.
 * Agent Mode (inherit/enabled/disabled), Answer Confirmation (inherit/ON/OFF),
 * and the Default Roleplay Template picker land LIVE — each PUTs one field on
 * change and surfaces a failed save with v4's fallback microcopy. The template
 * picker fetches the roleplay-templates listing (P4.6p) and binds the project's
 * `defaultRoleplayTemplateId`.
 *
 * DEFERRED LOUDLY: the Default Tool Settings row (v4 `ProjectToolSettingsModal`,
 * needs a tools-listing surface v5 doesn't expose) stays a disabled affordance.
 */
@Component({
  selector: 'qt-project-model-behavior-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CollapsibleCard, ErrorAlert],
  template: `
    <qt-collapsible-card
      title="Model Behavior"
      description="Agent mode, roleplay template & tool defaults"
      icon="cpu"
      [defaultOpen]="defaultOpen()"
    >
      @if (saveError(); as msg) {
        <qt-error-alert [message]="msg" class="mb-3" />
      }

      <div class="space-y-4">
        <div class="p-3 rounded-lg qt-border qt-bg-surface">
          <h4 class="qt-label text-foreground mb-1">Agent Mode</h4>
          <p class="qt-text-xs qt-text-secondary mb-2">
            Default agent mode for chats in this project. Agent mode allows iterative tool use with
            self-correction.
          </p>
          <select
            class="qt-input w-full max-w-xs"
            aria-label="Agent Mode"
            (change)="onAgentMode($event)"
          >
            <option value="inherit" [selected]="agentModeValue() === 'inherit'">
              Inherit from global/character
            </option>
            <option value="enabled" [selected]="agentModeValue() === 'enabled'">
              Enabled by default
            </option>
            <option value="disabled" [selected]="agentModeValue() === 'disabled'">
              Disabled by default
            </option>
          </select>
        </div>

        <div class="p-3 rounded-lg qt-border qt-bg-surface">
          <h4 class="qt-label text-foreground mb-1">Answer Confirmation</h4>
          <p class="qt-text-xs qt-text-secondary mb-2">
            Vet looked-up answers in this project's chats against what the character actually knew
            this turn. On enables it for every chat here; individual chats may still overrule.
          </p>
          <select
            class="qt-input w-full max-w-xs"
            aria-label="Answer Confirmation"
            (change)="onAnswerConfirmation($event)"
          >
            <option value="inherit" [selected]="answerConfirmationValue() === 'inherit'">
              Inherit from global
            </option>
            <option value="ON" [selected]="answerConfirmationValue() === 'ON'">
              Enabled by default
            </option>
            <option value="OFF" [selected]="answerConfirmationValue() === 'OFF'">
              Disabled by default
            </option>
          </select>
        </div>

        <div class="p-3 rounded-lg qt-border qt-bg-surface">
          <h4 class="qt-label text-foreground mb-1">Default Roleplay Template</h4>
          <p class="qt-text-xs qt-text-secondary mb-2">
            The roleplay template applied to new chats started in this project. Leave on inherit to
            use your global default.
          </p>
          <select
            class="qt-input w-full max-w-xs"
            aria-label="Default Roleplay Template"
            [disabled]="savingTemplate()"
            (change)="onRoleplayTemplate($event)"
          >
            <option value="" [selected]="roleplayTemplateValue() === ''">
              Inherit from global default
            </option>
            @for (t of roleplayTemplates(); track t.id) {
              <option [value]="t.id" [selected]="roleplayTemplateValue() === t.id">{{ t.name }}</option>
            }
          </select>
          @if (savingTemplate()) {
            <p class="qt-text-xs qt-text-secondary mt-1">Saving…</p>
          }
        </div>

        <div class="flex items-center justify-between p-3 rounded-lg qt-border qt-bg-surface">
          <div>
            <h4 class="qt-label text-foreground">Default Tool Settings</h4>
            <p class="qt-text-xs qt-text-secondary">All tools enabled</p>
          </div>
          <button
            type="button"
            class="qt-button qt-button-secondary qt-button-sm"
            title="Configuring the tool roster is not yet available"
            disabled
          >
            Configure
          </button>
        </div>
      </div>
    </qt-collapsible-card>
  `,
})
export class ProjectModelBehaviorCard {
  readonly project = input.required<ProjectDetail>();
  readonly defaultOpen = input(false);

  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();
  protected readonly saveError = signal<string | null>(null);
  protected readonly savingTemplate = signal(false);

  private readonly templatesQuery = injectQuery(() => ({
    queryKey: templateKeys.list(),
    queryFn: (): Promise<RoleplayTemplateDto[]> => fetchRoleplayTemplates(this.core),
  }));

  protected readonly roleplayTemplates = computed(() => this.templatesQuery.data() ?? []);
  protected readonly roleplayTemplateValue = computed(
    () => this.project().defaultRoleplayTemplateId ?? '',
  );

  protected readonly agentModeValue = computed(() => {
    const v = this.project().defaultAgentModeEnabled;
    return v === null || v === undefined ? 'inherit' : v ? 'enabled' : 'disabled';
  });

  protected readonly answerConfirmationValue = computed(
    () => this.project().answerConfirmationOverride ?? 'inherit',
  );

  protected async onAgentMode(event: Event): Promise<void> {
    const value = (event.target as HTMLSelectElement).value;
    const enabled = value === 'inherit' ? null : value === 'enabled';
    await this.save({ defaultAgentModeEnabled: enabled }, 'Failed to update agent mode');
  }

  protected async onAnswerConfirmation(event: Event): Promise<void> {
    const value = (event.target as HTMLSelectElement).value;
    const override = value === 'inherit' ? null : (value as 'ON' | 'OFF');
    await this.save(
      { answerConfirmationOverride: override },
      'Failed to update answer confirmation',
    );
  }

  protected async onRoleplayTemplate(event: Event): Promise<void> {
    const value = (event.target as HTMLSelectElement).value;
    this.savingTemplate.set(true);
    try {
      await this.save(
        { defaultRoleplayTemplateId: value || null },
        'Failed to update default roleplay template',
      );
    } finally {
      this.savingTemplate.set(false);
    }
  }

  private async save(patch: Record<string, unknown>, fallback: string): Promise<void> {
    this.saveError.set(null);
    try {
      await updateProject(this.core, this.project().id, patch);
      await this.queryClient.invalidateQueries({ queryKey: projectKeys.detail(this.project().id) });
    } catch (err) {
      this.saveError.set(err instanceof Error ? err.message : fallback);
    }
  }
}
