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

  /**
   * v4 `5fdd7bed` freshened the Key Features list for the 4.8.0 release. Every
   * named feature is ported in v5, so the sentences are true here too — pinned
   * because static template prose has no other guard against silent rot.
   */
  it('carries the 4.8.0 release-freshness sweep of the feature list', async () => {
    const fixture = await render({ kind: 'healthy', version: '0.0.28' });
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';

    expect(text).toContain('with filesystem document stores bound through to the container');
    expect(text).toContain('The Workspace');
    expect(text).toContain('a two-pane shell of kept-alive tabs');
    expect(text).toContain('and archiving into an encrypted bundle you can rehydrate later');
    expect(text).toContain('persistent state across four registers');
    expect(text).toContain('a visual workbench to build them');
    expect(text).toContain(
      'the Almanack: a full system report on what this instance can actually do',
    );
    // The Workspace bullet is inserted BEFORE Aurora, as v4 places it. (The
    // intro paragraph's "Aurora (characters)" is a different string, so the
    // titled bullet is the one being ordered here.)
    expect(text.indexOf('The Workspace')).toBeLessThan(text.indexOf('Aurora – Characters'));
  });

  /**
   * v4 `8440b6391` (the 4.9.0 documentation-freshness sweep) added DeepSeek,
   * Z.AI and NanoGPT to the provider sentence and a new "Live interface"
   * bullet between "LLM tools" and "Database protection". v4's JSX spells the
   * dashes and quotes as HTML entities (`&mdash;`, `&ldquo;`/`&rdquo;`); v5
   * stores plain strings, so the RENDERED characters are what is carried.
   *
   * The bullet says "socket" while v5 pushes the same hints over SSE — the
   * f3892158d round's locked mechanism divergence. The copy is v4's, verbatim.
   */
  it('carries the 4.9.0 provider list and the Live interface bullet', async () => {
    const fixture = await render({ kind: 'healthy', version: '0.0.28' });
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';

    expect(text).toContain(
      'Anthropic, OpenAI, Google Gemini, Grok, DeepSeek, Z.AI, NanoGPT, Ollama, OpenRouter, and OpenAI-compatible APIs',
    );
    expect(text).toContain('Live interface');
    expect(text).toContain(
      'a single multiplexed socket tells every open tab the moment something changes \u2014 queued errands, autonomous rooms, generated backdrops \u2014 so screens refresh themselves rather than asking again every few seconds, and every \u201C4m ago\u201D in the house turns over on the same tick',
    );
    // v4 slots it between "LLM tools" and "Database protection".
    expect(text.indexOf('LLM tools')).toBeLessThan(text.indexOf('Live interface'));
    expect(text.indexOf('Live interface')).toBeLessThan(text.indexOf('Database protection'));
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
    // v4 `7fb668263` moved the Discord invite (the P4.D134 rider).
    expect(hrefs).toContain('https://discord.gg/fnTPEZDE4');
    expect(hrefs).toContain('https://quilltap.ai');
    expect(hrefs).toContain('mailto:charles.sebold@foundry-9.com');
    expect(hrefs.every((h) => !h?.includes('shields.io'))).toBe(true);
  });

  it('describes the three back ends and names no VM (v4 `1560bd43b`)', async () => {
    const fixture = await render({ kind: 'healthy', version: '0.0.28' });
    const text = (fixture.nativeElement as HTMLElement).textContent ?? '';

    // The retirement: not one of these words survives anywhere on the page.
    for (const gone of ['Lima', 'WSL2', 'WSL', 'VZ', 'macOS VM', 'Windows VM']) {
      expect(text).not.toContain(gone);
    }

    // What replaced them, byte-for-byte from v4's post-`1560bd43b` AboutView.
    expect(text).toContain(
      'runs as a native desktop application on macOS, Windows, and Linux.',
    );
    expect(text).toContain(
      'macOS, Windows, and Linux installers with branded splash screen, data directory ' +
        'management, and managed updates, fronting the back end of your choosing: Direct (the ' +
        'server inside Electron), Docker, or Remote (any Quilltap URL that will have you)',
    );
    expect(text).toContain(
      'the sandboxed option: chosen from the splash screen or run standalone via Docker Hub, ' +
        'with filesystem document stores bound through to the container',
    );
    expect(text).toContain('Electron, Docker');
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
