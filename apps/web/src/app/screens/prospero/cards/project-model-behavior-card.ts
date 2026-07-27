import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { ProjectDetail, RoleplayTemplateDto } from '../../../core/core-contract';
import { CollapsibleCard } from '../../../ui/collapsible-card';
import { ErrorAlert } from '../../../ui/error-alert';
import { fetchRoleplayTemplates, templateKeys } from '../../settings/templates/templates.api';
import { ProjectToolSettingsModal } from '../project-tool-settings-modal';
import { projectKeys, updateProject } from '../projects.api';

/**
 * The Model Behavior card (v4 `ModelBehaviorCard.tsx`): immediate-save selects.
 * Agent Mode (inherit/enabled/disabled), Answer Confirmation (inherit/ON/OFF),
 * and the Default Roleplay Template picker land LIVE — each PUTs one field on
 * change and surfaces a failed save with v4's fallback microcopy. The template
 * picker fetches the roleplay-templates listing (P4.6p) and binds the project's
 * `defaultRoleplayTemplateId`.
 *
 * The Default Tool Settings row landed in P4.9E4B (it had been a disabled
 * affordance reading "All tools enabled", waiting on a tool inventory v5 did not
 * expose until P4.9E3B's `toolsList`): the real `toolSummary` and a live
 * Configure that opens `qt-project-tool-settings-modal`. As in v4
 * (`ModelBehaviorCard.tsx:39-40,64-68`), the two arrays are held LOCALLY and
 * updated from the dialog's success, so the summary changes at once without
 * waiting on a refetch.
 */
@Component({
  selector: 'qt-project-model-behavior-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CollapsibleCard, ErrorAlert, ProjectToolSettingsModal],
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
            <p class="qt-text-xs qt-text-secondary">{{ toolSummary() }}</p>
          </div>
          <button
            type="button"
            class="qt-button qt-button-secondary qt-button-sm"
            (click)="showToolSettings.set(true)"
          >
            Configure
          </button>
        </div>
      </div>
    </qt-collapsible-card>

    @if (showToolSettings()) {
      <qt-project-tool-settings-modal
        [projectId]="project().id"
        [disabledTools]="localDisabledTools()"
        [disabledToolGroups]="localDisabledToolGroups()"
        (saved)="onToolSettingsSaved($event)"
        (close)="showToolSettings.set(false)"
      />
    }
  `,
})
export class ProjectModelBehaviorCard {
  readonly project = input.required<ProjectDetail>();
  readonly defaultOpen = input(false);

  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();
  protected readonly saveError = signal<string | null>(null);
  protected readonly savingTemplate = signal(false);
  protected readonly showToolSettings = signal(false);

  /**
   * v4 keeps both arrays in local state seeded from the project (`:39-40`) and
   * replaces them on the dialog's success (`:64-68`), so the summary line updates
   * before any refetch lands. `null` means "not overridden yet" — the project's
   * own values are used until the dialog writes.
   */
  private readonly savedDisabledTools = signal<string[] | null>(null);
  private readonly savedDisabledToolGroups = signal<string[] | null>(null);

  protected readonly localDisabledTools = computed(
    () => this.savedDisabledTools() ?? this.project().defaultDisabledTools ?? [],
  );
  protected readonly localDisabledToolGroups = computed(
    () => this.savedDisabledToolGroups() ?? this.project().defaultDisabledToolGroups ?? [],
  );

  /** v4 `toolSummary` (`:47-62`) — the pluralised counts, or "All tools enabled". */
  protected readonly toolSummary = computed(() => {
    const toolCount = this.localDisabledTools().length;
    const groupCount = this.localDisabledToolGroups().length;
    if (toolCount === 0 && groupCount === 0) return 'All tools enabled';
    const parts: string[] = [];
    if (toolCount > 0) parts.push(`${toolCount} tool${toolCount !== 1 ? 's' : ''} disabled`);
    if (groupCount > 0) parts.push(`${groupCount} group${groupCount !== 1 ? 's' : ''} disabled`);
    return parts.join(', ');
  });

  /** v4 `handleToolSettingsSuccess` (`:64-68`) — adopt, then let the parent refetch. */
  protected async onToolSettingsSaved(next: {
    disabledTools: string[];
    disabledToolGroups: string[];
  }): Promise<void> {
    this.savedDisabledTools.set(next.disabledTools);
    this.savedDisabledToolGroups.set(next.disabledToolGroups);
    await this.queryClient.invalidateQueries({ queryKey: projectKeys.detail(this.project().id) });
  }

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
