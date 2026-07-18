import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { afterEach, describe, expect, it } from 'vitest';

import { CoreClient } from '../../core/core-client';
import type { HealthStatus } from '../../core/core-transport';
import { AboutPage } from './about-page';

function stubClient(health: HealthStatus): Partial<CoreClient> {
  return { fetchHealth: () => Promise.resolve(health) } as Partial<CoreClient>;
}

async function settle(fixture: ComponentFixture<unknown>, ticks = 6): Promise<void> {
  for (let i = 0; i < ticks; i++) {
    await new Promise((r) => setTimeout(r, 0));
    fixture.detectChanges();
  }
}

async function render(health: HealthStatus): Promise<ComponentFixture<AboutPage>> {
  TestBed.configureTestingModule({
    imports: [AboutPage],
    providers: [{ provide: CoreClient, useValue: stubClient(health) }, provideRouter([])],
  });
  const fixture = TestBed.createComponent(AboutPage);
  fixture.detectChanges();
  await settle(fixture);
  return fixture;
}

describe('AboutPage (v4 app/about/AboutView.tsx)', () => {
  afterEach(() => TestBed.resetTestingModule());

  it('renders v4 content: the tagline, the subsystem roll-call, the stack, the authors', async () => {
    const fixture = await render({ kind: 'healthy', version: '0.0.28' });
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';

    expect(text).toContain('Your AI, your projects, your stories, your partners, your rules.');
    expect(text).toContain('What is');
    expect(text).toContain('Key Features');
    expect(text).toContain('The Machinery Behind the Curtain');
    expect(text).toContain('Suparṇā');
    expect(text).toContain('Charlie, Friday, and Amy');
    expect(text).toContain('SillyTavern');
    expect(text).toContain('Foundry-9 LLC. All rights reserved.');
  });

  it('renders the version LOCALLY from the §3 health field', async () => {
    const fixture = await render({ kind: 'healthy', version: '0.0.28' });
    const badge = (fixture.nativeElement as HTMLElement).querySelector(
      '[data-testid="about-version"]',
    );
    expect(badge?.textContent?.trim()).toBe('Server version 0.0.28');
  });

  it('says so plainly when the server reports no version', async () => {
    // A server older than the §3 carry simply omits the field — no crash, no
    // fabricated number.
    const fixture = await render({ kind: 'healthy' });
    const badge = (fixture.nativeElement as HTMLElement).querySelector(
      '[data-testid="about-version"]',
    );
    expect(badge?.textContent?.trim()).toBe('Server version unknown');
  });

  it('fetches NO remote badge images (the ruled offline divergence)', async () => {
    const fixture = await render({ kind: 'healthy', version: '0.0.28' });
    const images = (fixture.nativeElement as HTMLElement).querySelectorAll('img');
    // v4 renders five shields.io <img>s here; a self-hosted, offline v5 must
    // render none. The LINK TARGETS are kept — asserted below.
    expect(images.length).toBe(0);

    const hrefs = Array.from(
      (fixture.nativeElement as HTMLElement).querySelectorAll<HTMLAnchorElement>('a[href]'),
    ).map((a) => a.getAttribute('href'));
    expect(hrefs).toContain('https://github.com/foundry-9/quilltap-server/blob/main/LICENSE');
    expect(hrefs).toContain('https://hub.docker.com/r/foundry9/quilltap');
    expect(hrefs).toContain('https://www.npmjs.com/package/quilltap');
    expect(hrefs).toContain('https://discord.gg/6enCeQxY');
    expect(hrefs).toContain('https://quilltap.ai');
    expect(hrefs).toContain('mailto:charles.sebold@foundry-9.com');
    expect(hrefs.every((h) => !h?.includes('shields.io'))).toBe(true);
  });
});

describe('AboutPage copyright years (v4 AboutView.tsx:10-11)', () => {
  it('is 2025 alone until the year turns, then a range', () => {
    expect(AboutPage.copyrightYears(2025)).toBe('2025');
    expect(AboutPage.copyrightYears(2026)).toBe('2025-2026');
    expect(AboutPage.copyrightYears(2031)).toBe('2025-2031');
    // v4 compares `> 2025`, so an (impossible) earlier clock still reads 2025.
    expect(AboutPage.copyrightYears(2024)).toBe('2025');
  });
});
