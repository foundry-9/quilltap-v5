import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  output,
  signal,
} from '@angular/core';

import { CoreClient } from '../../../../core/core-client';
import { Icon } from '../../../../ui/icon';
import { createConnectionProfile, testMessage, updateChatSettings } from '../wizard-api';
import { WizardStore } from '../wizard-state';

/** Wizard step 6 — review, test, and save (v4 `steps/TestConfirmStep.tsx`). */
@Component({
  selector: 'qt-wizard-confirm-step',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="space-y-6">
      <div class="space-y-3">
        <h3 class="qt-text-label">Configuration Summary</h3>
        <div class="border qt-border-default rounded-lg divide-y qt-divide-default">
          <div class="flex items-center justify-between p-3">
            <div class="flex-1">
              <span class="qt-text-label">Chat</span>
              <p class="qt-text-small qt-text-muted">
                {{ chatProviderName() }} — {{ store.chatConfig().model }}
              </p>
            </div>
            @switch (store.testResults()['chat']) {
              @case ('testing') {
                <span class="qt-spinner-sm"></span>
              }
              @case ('success') {
                <qt-icon name="check" class="w-5 h-5 qt-text-success" />
              }
              @case ('error') {
                <div class="flex items-center gap-2">
                  <qt-icon name="close" class="w-5 h-5 qt-text-destructive flex-shrink-0" />
                  @if (testError()) {
                    <span class="qt-text-xs qt-text-destructive">{{ testError() }}</span>
                  }
                </div>
              }
              @default {
                <span class="w-3 h-3 rounded-full qt-bg-muted inline-block"></span>
              }
            }
          </div>
          <div class="flex items-center justify-between p-3">
            <div class="flex-1">
              <span class="qt-text-label">Cheap LLM</span>
              <p class="qt-text-small qt-text-muted">
                {{
                  store.cheapConfig().strategy === 'PROVIDER_CHEAPEST'
                    ? 'Auto (provider selects cheapest model)'
                    : 'Manual — ' +
                      (cheapProviderName() || store.cheapConfig().provider) +
                      ' — ' +
                      store.cheapConfig().model
                }}
              </p>
            </div>
          </div>
          <div class="flex items-center justify-between p-3">
            <div class="flex-1">
              <span class="qt-text-label">Embeddings</span>
              <p class="qt-text-small qt-text-muted">Skipped</p>
            </div>
          </div>
          <div class="flex items-center justify-between p-3">
            <div class="flex-1">
              <span class="qt-text-label">Images</span>
              <p class="qt-text-small qt-text-muted">Skipped</p>
            </div>
          </div>
        </div>
      </div>

      <div class="flex items-center gap-3">
        <button
          type="button"
          class="qt-button-secondary"
          [disabled]="store.saving() || testing()"
          (click)="testAll()"
        >
          {{ testing() ? 'Testing...' : 'Test All' }}
        </button>
        @if (store.testResults()['chat'] === 'success') {
          <span class="qt-text-small qt-text-success">All tests passed</span>
        }
      </div>

      @if (store.error()) {
        <div class="qt-alert-error">{{ store.error() }}</div>
      }

      <div class="flex items-center gap-3">
        <button
          type="button"
          class="qt-button-primary"
          [disabled]="store.saving() || saveSuccess()"
          (click)="save()"
        >
          {{ store.saving() ? 'Saving...' : saveSuccess() ? 'Saved' : 'Save & Complete' }}
        </button>
      </div>

      @if (saveSuccess()) {
        <div class="space-y-4">
          <div class="qt-alert-info">
            <p class="qt-text-small">
              Configuration saved successfully. Your AI workspace is ready to use. You can adjust
              any of these settings later in The Foundry.
            </p>
          </div>
          <button type="button" class="qt-button-primary w-full py-2" (click)="complete.emit()">
            Continue to Quilltap
          </button>
        </div>
      }
    </div>
  `,
})
export class TestConfirmStep {
  protected readonly store = inject(WizardStore);
  private readonly core = inject(CoreClient);

  readonly complete = output<void>();

  protected readonly testing = signal(false);
  protected readonly testError = signal<string | null>(null);
  protected readonly saveSuccess = signal(false);

  protected readonly chatProviderName = computed(
    () =>
      this.store.providerById(this.store.chatConfig().provider)?.displayName ??
      this.store.chatConfig().provider,
  );
  protected readonly cheapProviderName = computed(() => {
    const c = this.store.cheapConfig();
    return c.strategy === 'USER_DEFINED' && c.provider
      ? this.store.providerById(c.provider)?.displayName
      : null;
  });

  protected async testAll(): Promise<void> {
    this.testError.set(null);
    this.testing.set(true);
    this.store.setTestResult('chat', 'testing');
    try {
      const chat = this.store.chatConfig();
      const result = await testMessage(this.core, {
        provider: chat.provider,
        apiKeyId: this.store.apiKeys()[chat.provider]?.apiKeyId,
        baseUrl: this.store.baseUrls()[chat.provider],
        modelName: chat.model,
      });
      this.store.setTestResult('chat', result.success ? 'success' : 'error');
      if (!result.success) {
        this.testError.set(result.error || 'Chat test failed');
      }
    } catch (err) {
      this.store.setTestResult('chat', 'error');
      this.testError.set(err instanceof Error ? err.message : 'Chat test failed');
    } finally {
      this.testing.set(false);
    }
  }

  protected async save(): Promise<void> {
    this.store.saving.set(true);
    this.store.error.set(null);
    this.saveSuccess.set(false);
    try {
      const chat = this.store.chatConfig();
      await createConnectionProfile(this.core, {
        name: `${this.chatProviderName()} - ${chat.model}`,
        provider: chat.provider,
        apiKeyId: this.store.apiKeys()[chat.provider]?.apiKeyId,
        baseUrl: this.store.baseUrls()[chat.provider],
        modelName: chat.model,
        isDefault: true,
      });

      let cheapProfileId: string | undefined;
      const cheap = this.store.cheapConfig();
      if (cheap.strategy === 'USER_DEFINED' && cheap.provider && cheap.model) {
        const cheapProfile = await createConnectionProfile(this.core, {
          name: `Cheap - ${cheap.provider} - ${cheap.model}`,
          provider: cheap.provider,
          apiKeyId: this.store.apiKeys()[cheap.provider]?.apiKeyId,
          baseUrl: this.store.baseUrls()[cheap.provider],
          modelName: cheap.model,
          isCheap: true,
        });
        cheapProfileId = cheapProfile.id;
      }

      await updateChatSettings(this.core, { strategy: cheap.strategy, profileId: cheapProfileId });
      this.saveSuccess.set(true);
    } catch (err) {
      this.store.error.set(err instanceof Error ? err.message : 'Failed to save configuration');
    } finally {
      this.store.saving.set(false);
    }
  }
}
