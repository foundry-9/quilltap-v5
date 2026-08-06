import { provideRouter } from '@angular/router';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { describe, expect, it, vi } from 'vitest';

import { CoreClient } from '../../../../core/core-client';
import type { EmbeddingProfileDto } from '../../../../core/core-contract';
import { EmbeddingProfileList } from './embedding-profile-list';

/**
 * Ported from v4's `ProfileList.tsx`: the MissingApiKeyBadge condition, the
 * single-click refit (BUILTIN → refit, default non-BUILTIN → no-scope reindex),
 * the two-step Matryoshka re-apply and mismatched-reindex confirms with their
 * flipped labels, and the two-step delete.
 */

function profile(over: Partial<EmbeddingProfileDto>): EmbeddingProfileDto {
  return {
    id: 'p1',
    name: 'Profile',
    provider: 'BUILTIN',
    modelName: 'tfidf-bm25-v1',
    isDefault: false,
    ...over,
  };
}

interface Stub {
  client: Partial<CoreClient>;
  refit: ReturnType<typeof vi.fn>;
  reindex: ReturnType<typeof vi.fn>;
  reapply: ReturnType<typeof vi.fn>;
  del: ReturnType<typeof vi.fn>;
}

function stubClient(): Stub {
  const refit = vi.fn(async () => undefined);
  const reindex = vi.fn(async () => undefined);
  const reapply = vi.fn(async () => undefined);
  const del = vi.fn(async () => undefined);
  return {
    refit,
    reindex,
    reapply,
    del,
    client: {
      refitEmbeddingProfile: refit as unknown as CoreClient['refitEmbeddingProfile'],
      reindexEmbeddingProfile: reindex as unknown as CoreClient['reindexEmbeddingProfile'],
      reapplyEmbeddingProfile: reapply as unknown as CoreClient['reapplyEmbeddingProfile'],
      deleteEmbeddingProfile: del as unknown as CoreClient['deleteEmbeddingProfile'],
    },
  };
}

async function mount(
  stub: Stub,
  profiles: EmbeddingProfileDto[],
): Promise<{ fixture: ComponentFixture<EmbeddingProfileList>; changed: number }> {
  TestBed.resetTestingModule();
  TestBed.configureTestingModule({
    imports: [EmbeddingProfileList],
    providers: [{ provide: CoreClient, useValue: stub.client }, provideRouter([])],
  });
  const fixture = TestBed.createComponent(EmbeddingProfileList);
  const counts = { changed: 0 };
  fixture.componentInstance.changed.subscribe(() => (counts.changed += 1));
  fixture.componentRef.setInput('profiles', profiles);
  fixture.detectChanges();
  await settle(fixture);
  return { fixture, get changed() { return counts.changed; } };
}

async function settle(fixture: ComponentFixture<EmbeddingProfileList>): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
  fixture.detectChanges();
}

function text(fixture: ComponentFixture<EmbeddingProfileList>): string {
  return (fixture.nativeElement as HTMLElement).textContent ?? '';
}

function byText(fixture: ComponentFixture<EmbeddingProfileList>, label: string): HTMLButtonElement {
  const btn = [...(fixture.nativeElement as HTMLElement).querySelectorAll('button')].find(
    (b) => (b as HTMLButtonElement).textContent?.trim() === label,
  );
  if (!btn) throw new Error(`button "${label}" not found; have: ${text(fixture)}`);
  return btn as HTMLButtonElement;
}

describe('EmbeddingProfileList', () => {
  it('shows the missing-API-key badge for an OPENAI profile with no key', async () => {
    const stub = stubClient();
    const view = await mount(stub, [profile({ provider: 'OPENAI', modelName: 'm', apiKey: null })]);
    expect(text(view.fixture)).toContain('⚠️ No API Key');
  });

  it('omits the missing-key badge when the OPENAI profile has a key', async () => {
    const stub = stubClient();
    const view = await mount(stub, [
      profile({
        provider: 'OPENAI',
        modelName: 'm',
        apiKey: { id: 'k1', label: 'My Key', provider: 'OPENAI', isActive: true },
      }),
    ]);
    expect(text(view.fixture)).not.toContain('No API Key');
  });

  it('omits the missing-key badge for a BUILTIN profile', async () => {
    const stub = stubClient();
    const view = await mount(stub, [profile({ provider: 'BUILTIN' })]);
    expect(text(view.fixture)).not.toContain('No API Key');
  });

  it('BUILTIN default → Refit Vocabulary calls refit and shows the banner', async () => {
    const stub = stubClient();
    const view = await mount(stub, [profile({ provider: 'BUILTIN', isDefault: true })]);
    byText(view.fixture, 'Refit Vocabulary').click();
    await settle(view.fixture);
    expect(stub.refit).toHaveBeenCalledWith('p1');
    expect(stub.reindex).not.toHaveBeenCalled();
    expect(text(view.fixture)).toContain('Vocabulary refit job queued');
  });

  it('non-BUILTIN default → Re-embed Everything calls a no-scope reindex', async () => {
    const stub = stubClient();
    const view = await mount(stub, [
      profile({ id: 'p2', provider: 'OPENAI', modelName: 'm', isDefault: true }),
    ]);
    byText(view.fixture, 'Re-embed Everything').click();
    await settle(view.fixture);
    // The refit path issues the legacy no-scope reindex (single arg).
    expect(stub.reindex).toHaveBeenCalledWith('p2');
    expect(text(view.fixture)).toContain('Re-embedding everything');
  });

  it('Matryoshka re-apply is a two-step confirm that ends in a reapply', async () => {
    const stub = stubClient();
    const view = await mount(stub, [
      profile({
        id: 'p3',
        provider: 'OPENAI',
        modelName: 'm',
        isDefault: true,
        dimensions: 1536,
        truncateToDimensions: 512,
      }),
    ]);
    byText(view.fixture, 'Re-apply (Matryoshka)').click();
    view.fixture.detectChanges();
    // First click only arms the confirm — nothing enqueued yet.
    expect(stub.reapply).not.toHaveBeenCalled();
    byText(view.fixture, 'Confirm: slice to 512d').click();
    await settle(view.fixture);
    expect(stub.reapply).toHaveBeenCalledWith('p3');
    expect(text(view.fixture)).toContain('sliced to 512d');
  });

  it('Re-embed Mismatched is a two-step confirm ending in a mismatched-dim reindex', async () => {
    const stub = stubClient();
    const view = await mount(stub, [
      profile({ id: 'p4', provider: 'OPENAI', modelName: 'm', isDefault: true, dimensions: 1536 }),
    ]);
    byText(view.fixture, 'Re-embed Mismatched').click();
    view.fixture.detectChanges();
    expect(stub.reindex).not.toHaveBeenCalled();
    byText(view.fixture, 'Confirm: re-embed misfits at 1536d').click();
    await settle(view.fixture);
    expect(stub.reindex).toHaveBeenCalledWith('p4', 'mismatched-dim');
  });

  it('delete is a two-step confirm that emits changed', async () => {
    const stub = stubClient();
    const view = await mount(stub, [profile({ id: 'p5' })]);
    byText(view.fixture, 'Delete').click();
    view.fixture.detectChanges();
    expect(stub.del).not.toHaveBeenCalled();
    byText(view.fixture, 'Delete this profile?').click();
    await settle(view.fixture);
    expect(stub.del).toHaveBeenCalledWith('p5');
    expect(view.changed).toBe(1);
  });

  it('renders the empty state when there are no profiles', async () => {
    const stub = stubClient();
    const view = await mount(stub, []);
    expect(text(view.fixture)).toContain('No embedding profiles yet');
  });
});
