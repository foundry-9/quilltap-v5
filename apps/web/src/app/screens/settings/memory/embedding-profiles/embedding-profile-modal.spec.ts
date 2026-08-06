import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import type { EmbeddingProviderInfoDto } from '../../../../core/core-contract';
import { EmbeddingProfileModal } from './embedding-profile-modal';

/**
 * Ported case-for-case from v4's `ProfileModal.tsx` behaviour. The load-bearing
 * facts are the REQUEST CONTRACT: BUILTIN pins `modelName='tfidf-bm25-v1'`, a
 * newly-default save opens the "Re-embed Everything?" follow-up and confirming
 * it issues a no-scope reindex, the validity gate, and the refusal-armed
 * fetch-models path surfacing the server's own sentence.
 */

const PROVIDERS: EmbeddingProviderInfoDto[] = [
  { name: 'BUILTIN', displayName: 'Built-in (TF-IDF)', requiresApiKey: false, requiresBaseUrl: false },
  { name: 'OPENAI', displayName: 'OpenAI', requiresApiKey: true, requiresBaseUrl: false },
  { name: 'OLLAMA', displayName: 'Ollama (Local)', requiresApiKey: false, requiresBaseUrl: true },
];

interface Stub {
  client: Partial<CoreClient>;
  created: Record<string, unknown>[];
  updated: Record<string, unknown>[];
  reindexed: { profileId: string; scope?: string }[];
  fetchModels: ReturnType<typeof vi.fn>;
}

function stubClient(fetchModelsImpl?: () => Promise<never>): Stub {
  const created: Record<string, unknown>[] = [];
  const updated: Record<string, unknown>[] = [];
  const reindexed: { profileId: string; scope?: string }[] = [];
  const fetchModels = vi.fn(fetchModelsImpl ?? (async () => []));
  return {
    created,
    updated,
    reindexed,
    fetchModels,
    client: {
      createEmbeddingProfile: (async (payload: Record<string, unknown>) => {
        created.push(payload);
        return { id: 'created-1', ...payload };
      }) as unknown as CoreClient['createEmbeddingProfile'],
      updateEmbeddingProfile: (async (payload: Record<string, unknown>) => {
        updated.push(payload);
        return { id: payload['profileId'], ...payload };
      }) as unknown as CoreClient['updateEmbeddingProfile'],
      reindexEmbeddingProfile: (async (profileId: string, scope?: 'all' | 'mismatched-dim') => {
        reindexed.push({ profileId, scope });
      }) as unknown as CoreClient['reindexEmbeddingProfile'],
      fetchEmbeddingModels: fetchModels as unknown as CoreClient['fetchEmbeddingModels'],
    },
  };
}

interface MountOpts {
  profile?: Record<string, unknown> | null;
  providers?: EmbeddingProviderInfoDto[];
}

async function mount(
  stub: Stub,
  opts: MountOpts = {},
): Promise<{ fixture: ComponentFixture<EmbeddingProfileModal>; saved: number; closed: number }> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [EmbeddingProfileModal],
    providers: [{ provide: CoreClient, useValue: stub.client }],
  });
  const fixture = TestBed.createComponent(EmbeddingProfileModal);
  const counts = { saved: 0, closed: 0 };
  fixture.componentInstance.saved.subscribe(() => (counts.saved += 1));
  fixture.componentInstance.close.subscribe(() => (counts.closed += 1));
  fixture.componentRef.setInput('profile', opts.profile ?? null);
  fixture.componentRef.setInput('apiKeys', []);
  fixture.componentRef.setInput('embeddingModels', {});
  fixture.componentRef.setInput('embeddingProviders', opts.providers ?? PROVIDERS);
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, get saved() { return counts.saved; }, get closed() { return counts.closed; } };
}

async function settle(fixture: ComponentFixture<EmbeddingProfileModal>): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
  fixture.detectChanges();
}

function text(fixture: ComponentFixture<EmbeddingProfileModal>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

function byText(fixture: ComponentFixture<EmbeddingProfileModal>, label: string): HTMLButtonElement {
  const btn = [...(fixture.nativeElement as HTMLElement).querySelectorAll('button')].find(
    (b) => (b as HTMLButtonElement).textContent?.trim() === label,
  );
  if (!btn) throw new Error(`button "${label}" not found`);
  return btn as HTMLButtonElement;
}

function nameInput(fixture: ComponentFixture<EmbeddingProfileModal>): HTMLInputElement {
  return (fixture.nativeElement as HTMLElement).querySelector('input[type="text"]')!;
}

function setInput(el: HTMLInputElement, value: string): void {
  el.value = value;
  el.dispatchEvent(new Event('input'));
}

function tickDefault(fixture: ComponentFixture<EmbeddingProfileModal>): void {
  const cb = (fixture.nativeElement as HTMLElement).querySelector<HTMLInputElement>(
    '#qt-emb-isDefault',
  )!;
  cb.checked = true;
  cb.dispatchEvent(new Event('change'));
  fixture.detectChanges();
}

describe('EmbeddingProfileModal', () => {
  it('pins modelName=tfidf-bm25-v1 for a BUILTIN profile and opens the re-embed follow-up', async () => {
    const stub = stubClient();
    const view = await mount(stub);

    setInput(nameInput(view.fixture), 'My Vocab');
    tickDefault(view.fixture);

    byText(view.fixture, 'Create Profile').click();
    await settle(view.fixture);

    expect(stub.created).toEqual([
      {
        name: 'My Vocab',
        provider: 'BUILTIN',
        apiKeyId: undefined,
        baseUrl: undefined,
        modelName: 'tfidf-bm25-v1',
        dimensions: undefined,
        isDefault: true,
      },
    ]);
    // The list refetch fired, and the re-embed dialog is now showing.
    expect(view.saved).toBe(1);
    expect(text(view.fixture)).toContain('Re-embed Everything?');
    // Not yet dismissed — the follow-up is still open.
    expect(view.closed).toBe(0);
  });

  it('confirming the re-embed issues a no-scope reindex, then dismisses', async () => {
    const stub = stubClient();
    const view = await mount(stub);
    setInput(nameInput(view.fixture), 'My Vocab');
    tickDefault(view.fixture);
    byText(view.fixture, 'Create Profile').click();
    await settle(view.fixture);

    byText(view.fixture, 'Re-embed Everything').click();
    await settle(view.fixture);

    expect(stub.reindexed).toEqual([{ profileId: 'created-1', scope: undefined }]);
    expect(view.closed).toBe(1);
  });

  it('Skip declines the re-embed without a reindex', async () => {
    const stub = stubClient();
    const view = await mount(stub);
    setInput(nameInput(view.fixture), 'My Vocab');
    tickDefault(view.fixture);
    byText(view.fixture, 'Create Profile').click();
    await settle(view.fixture);

    byText(view.fixture, 'Skip').click();
    await settle(view.fixture);

    expect(stub.reindexed).toEqual([]);
    expect(view.closed).toBe(1);
  });

  it('does NOT open the re-embed follow-up when the profile was already default', async () => {
    const stub = stubClient();
    const view = await mount(stub, {
      profile: {
        id: 'p1',
        name: 'Existing',
        provider: 'BUILTIN',
        modelName: 'tfidf-bm25-v1',
        isDefault: true,
      },
    });
    // Editing name only; still default, so this is NOT newly-default.
    setInput(nameInput(view.fixture), 'Existing Renamed');
    byText(view.fixture, 'Update Profile').click();
    await settle(view.fixture);

    expect(stub.updated).toHaveLength(1);
    expect(text(view.fixture)).not.toContain('Re-embed Everything?');
    expect(view.closed).toBe(1);
  });

  it('requires a model for a non-BUILTIN provider (validity gate)', async () => {
    const stub = stubClient();
    const view = await mount(stub);
    // Switch to OPENAI — a model is now required.
    const select = (view.fixture.nativeElement as HTMLElement).querySelector('select')!;
    select.value = 'OPENAI';
    select.dispatchEvent(new Event('change'));
    setInput(nameInput(view.fixture), 'Keyed');
    view.fixture.detectChanges();
    await settle(view.fixture);

    expect(byText(view.fixture, 'Create Profile').disabled).toBe(true);

    // Give it a model → enabled. OPENAI shows a free-text model input (no
    // static models), which is the LAST text input in the form.
    const inputs = [
      ...(view.fixture.nativeElement as HTMLElement).querySelectorAll<HTMLInputElement>(
        'input[type="text"]',
      ),
    ];
    const modelField = inputs[inputs.length - 2]; // model, then dimensions
    setInput(modelField, 'text-embedding-3-small');
    view.fixture.detectChanges();
    expect(byText(view.fixture, 'Create Profile').disabled).toBe(false);
  });

  it('BUILTIN is valid with only a name (no model needed)', async () => {
    const stub = stubClient();
    const view = await mount(stub);
    expect(byText(view.fixture, 'Create Profile').disabled).toBe(true);
    setInput(nameInput(view.fixture), 'Vocab');
    view.fixture.detectChanges();
    expect(byText(view.fixture, 'Create Profile').disabled).toBe(false);
  });

  it('surfaces the server refusal sentence when fetch-models fails', async () => {
    const stub = stubClient(async () => {
      throw new Error('Model fetching for this provider is not available in this build');
    });
    const view = await mount(stub);
    // Switch to OLLAMA (base-URL bearing → the Fetch button appears).
    const select = (view.fixture.nativeElement as HTMLElement).querySelector('select')!;
    select.value = 'OLLAMA';
    select.dispatchEvent(new Event('change'));
    view.fixture.detectChanges();
    await settle(view.fixture);

    byText(view.fixture, 'Fetch Installed Models').click();
    await settle(view.fixture);

    expect(stub.fetchModels).toHaveBeenCalledWith('OLLAMA', undefined);
    expect(text(view.fixture)).toContain(
      'Model fetching for this provider is not available in this build',
    );
  });
});
