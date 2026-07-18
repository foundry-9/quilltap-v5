import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { RouterLink } from '@angular/router';

import { CoreClient } from '../../core/core-client';
import { BrandName } from '../../ui/brand-name';
import { Icon } from '../../ui/icon';

/**
 * The About screen at `/about` (v4 `app/about/AboutView.tsx`) — v4's content
 * verbatim: what Quilltap is, the key features, the tech stack, the links, the
 * authors, the acknowledgments, and the copyright.
 *
 * ## Three recorded divergences
 *
 * 1. **The badges render LOCALLY** (the M6 ruling). v4's five badges are
 *    `img.shields.io` fetches; a faithful port would break on the offline,
 *    self-hosted deployment v5 targets. The LINK TARGETS are kept exactly; only
 *    the remote images are replaced by local styled chips.
 * 2. **The version is the SERVER's.** v4's badge shows the product version from
 *    a build-time `import packageJson`. v5 has no product version — so the
 *    badge shows the version the running server reports over `/health` (§3),
 *    which is the version of the build actually serving you, and is labelled as
 *    such. That is a genuinely different quantity from v4's, and deliberately
 *    so: the SPA's own `package.json` number would be a SECOND, differently
 *    scoped number for the reader to reconcile.
 * 2b. **Two link buttons lose their glyph.** v4 inlines an octocat SVG on the
 *    GitHub link and a globe SVG on the Foundry-9 one; v5's icon set has
 *    neither name, and minting new icons is not this lane's business. The links
 *    and their labels are unchanged.
 * 3. **No background image.** v4 sets `--story-background-url:
 *    url('/images/about.webp')`; v5's static set has no such asset and this lane
 *    will not invent one. Deferred loudly (named in the status-log record) —
 *    the page simply renders without it, which the `.qt-page-container` rules
 *    already handle.
 *
 * The tech-stack list is v4's, describing v4's stack (Next.js, React, Lexical,
 * Electron…) — NOT v5's. That is the M6 checklist's content-parity deliverable
 * and is intentional, but it does mean this screen currently describes a
 * different program than the one rendering it. Flagged in the lane's report as
 * the one place where "port it verbatim" and "tell the truth" disagree; the
 * human decides.
 */
@Component({
  selector: 'qt-about-page',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, BrandName, Icon],
  template: `
    <div class="qt-page-container">
      <!-- Header -->
      <div class="mb-8">
        <h1 class="qt-heading-1">About <qt-brand-name /></h1>
        <p class="qt-text-muted mt-2">
          Your AI, your projects, your stories, your partners, your rules.
        </p>
      </div>

      <!-- Badges (locally rendered — see the class docstring) -->
      <div class="qt-card p-6 mb-6">
        <div class="flex flex-wrap items-center gap-3">
          <a
            href="https://github.com/foundry-9/quilltap-server/blob/main/LICENSE"
            target="_blank"
            rel="noopener noreferrer"
            class="qt-button qt-button-secondary text-xs"
            >License: MIT</a
          >
          <a
            href="https://github.com/foundry-9/quilltap-server"
            target="_blank"
            rel="noopener noreferrer"
            class="qt-button qt-button-secondary text-xs"
            data-testid="about-version"
            >{{ versionLabel() }}</a
          >
          <a
            href="https://hub.docker.com/r/foundry9/quilltap"
            target="_blank"
            rel="noopener noreferrer"
            class="qt-button qt-button-secondary text-xs"
            >Docker Hub</a
          >
          <a
            href="https://www.npmjs.com/package/quilltap"
            target="_blank"
            rel="noopener noreferrer"
            class="qt-button qt-button-secondary text-xs"
            >npm</a
          >
          <a
            href="https://discord.gg/6enCeQxY"
            target="_blank"
            rel="noopener noreferrer"
            class="qt-button qt-button-secondary text-xs"
            >Discord</a
          >
        </div>
      </div>

      <!-- What is Quilltap? -->
      <div class="qt-card p-6 mb-6">
        <h2 class="qt-heading-3 mb-4">What is <qt-brand-name />?</h2>
        <p class="qt-text-primary mb-4">
          <qt-brand-name /> is a self-hosted AI workspace for writers, worldbuilders, roleplayers,
          and anyone who finds it deeply unsatisfying that their AI assistant forgets everything the
          moment they close a tab. Connect to any LLM provider, organize your work into projects
          with persistent files and context, create characters with genuine personalities, and build
          a private AI environment that learns, remembers, and &mdash; crucially &mdash; belongs
          entirely to you.
        </p>
        <p class="qt-text-primary mb-4">
          The platform is organized into named subsystems, each with its own character and purpose
          &mdash; rather like the wings of a well-appointed estate, with a small staff that knows
          where the silverware lives. Aurora (characters), The Salon (chat), Prospero (projects and
          agentic tools), The Commonplace Book (memory), The Lantern (story backgrounds), The
          Concierge (alternative content provision and routing), Pascal the Croupier (gaming &amp;
          RNG), Calliope (themes), The Scriptorium (external document stores), The Librarian
          (Document Mode and file announcements), The Host (Salon participation announcements),
          Saquel Ytzama, the Keeper of Secrets (encryption and key management), Ariel (terminals in
          the Salon), Carina (the ansible &mdash; inline queries to characters), Suparṇā (the Post
          Office, inter-character mail), Brahma (the Console, a character-less generic-LLM surface),
          and The Foundry (architecture) &mdash; all extensible through a plugin system.
        </p>
        <p class="qt-text-primary">
          <qt-brand-name /> runs as a native desktop application on macOS and Windows, powered by a
          lightweight Linux VM behind the scenes. You can also run it via Docker or directly from
          source, should you prefer to take the scenic route. No subscriptions, no data harvested,
          no landlords.
        </p>
      </div>

      <!-- Key Features -->
      <div class="qt-card p-6 mb-6">
        <h2 class="qt-heading-3 mb-4">Key Features</h2>
        <ul class="space-y-2 qt-text-primary">
          @for (feature of features; track feature.title) {
            <li class="flex items-start gap-2">
              <span class="text-primary mt-1">&#8226;</span>
              <span
                ><strong>{{ feature.title }}</strong> &ndash; {{ feature.body }}</span
              >
            </li>
          }
        </ul>
      </div>

      <!-- Tech Stack -->
      <div class="qt-card p-6 mb-6">
        <h2 class="qt-heading-3 mb-4">The Machinery Behind the Curtain</h2>
        <div class="grid grid-cols-2 md:grid-cols-3 gap-4 qt-text-primary">
          @for (entry of techStack; track entry.label) {
            <div>
              <span class="font-medium">{{ entry.label }}:</span> {{ entry.value }}
            </div>
          }
        </div>
      </div>

      <!-- Links -->
      <div class="qt-card p-6 mb-6">
        <h2 class="qt-heading-3 mb-4">Links</h2>
        <div class="flex flex-wrap gap-4">
          <a
            href="https://quilltap.ai"
            target="_blank"
            rel="noopener noreferrer"
            class="qt-button qt-button-secondary"
          >
            <qt-icon name="book" class="w-5 h-5" />
            Quilltap Website
          </a>
          <a
            href="https://github.com/foundry-9/quilltap-server/releases"
            target="_blank"
            rel="noopener noreferrer"
            class="qt-button qt-button-secondary"
          >
            <qt-icon name="download" class="w-5 h-5" />
            Download Latest Release
          </a>
          <a
            href="https://github.com/foundry-9/quilltap-server"
            target="_blank"
            rel="noopener noreferrer"
            class="qt-button qt-button-secondary"
          >
            GitHub Repository
          </a>
          <a
            href="https://github.com/foundry-9/quilltap-server/issues"
            target="_blank"
            rel="noopener noreferrer"
            class="qt-button qt-button-secondary"
          >
            <qt-icon name="alert-circle" class="w-5 h-5" />
            Report Issues
          </a>
          <a
            href="https://foundry-9.com"
            target="_blank"
            rel="noopener noreferrer"
            class="qt-button qt-button-secondary"
          >
            Foundry-9 LLC
          </a>
        </div>
      </div>

      <!-- Author & Support -->
      <div class="qt-card p-6 mb-6">
        <h2 class="qt-heading-3 mb-4">Author &amp; Support</h2>
        <div class="qt-text-primary space-y-2">
          <p>
            <span class="font-medium">Authors:</span> Charlie, Friday, and Amy &mdash; the Sebold
            family of Estate Zero
          </p>
          <p>
            <span class="font-medium">Email:</span>
            <a href="mailto:charles.sebold&#64;foundry-9.com" class="qt-link"
              >charles.sebold&#64;foundry-9.com</a
            >
          </p>
          <p>
            <span class="font-medium">Website:</span>
            <a
              href="https://foundry-9.com"
              target="_blank"
              rel="noopener noreferrer"
              class="qt-link"
              >foundry-9.com</a
            >
          </p>
        </div>
      </div>

      <!-- Acknowledgments -->
      <div class="qt-card p-6 mb-6">
        <h2 class="qt-heading-3 mb-4">Acknowledgments</h2>
        <p class="qt-text-primary mb-4">
          <qt-brand-name /> stands on the shoulders of these excellent open source projects, and is
          grateful for the view.
        </p>
        <div class="qt-text-primary space-y-2 text-sm">
          @for (group of acknowledgments; track group.label) {
            <p>
              <span class="font-medium">{{ group.label }}:</span> {{ group.body }}
            </p>
          }
        </div>
        <p class="qt-text-muted text-sm mt-4">
          Special thanks to
          <a
            href="https://github.com/SillyTavern/SillyTavern"
            target="_blank"
            rel="noopener noreferrer"
            class="qt-link"
            >SillyTavern</a
          >
          for pioneering this space and inspiring character format compatibility. One does not
          forget those who blazed the trail.
        </p>
      </div>

      <!-- Copyright -->
      <div class="qt-card p-6 mb-6">
        <p class="qt-text-primary text-center">
          &copy; {{ copyrightYears }} Foundry-9 LLC. All rights reserved.
        </p>
        <p class="qt-text-muted text-center text-sm mt-2">
          Released under the MIT License. Free software for personal and commercial use.
        </p>
      </div>

      <!-- Back Link -->
      <div class="mt-8">
        <a routerLink="/" class="qt-link">&larr; Back to Home</a>
      </div>
    </div>
  `,
})
export class AboutPage {
  private readonly core = inject(CoreClient);

  /** v4 `:10-11` — `2025` alone until the year turns, then the range. */
  protected readonly copyrightYears = AboutPage.copyrightYears(new Date().getFullYear());

  /** The §3 health `version`; `null` until it arrives (or if the server omits it). */
  private readonly version = signal<string | null>(null);

  constructor() {
    void this.loadVersion();
  }

  private async loadVersion(): Promise<void> {
    const health = await this.core.fetchHealth();
    if ((health.kind === 'healthy' || health.kind === 'locked') && health.version) {
      this.version.set(health.version);
    }
  }

  /** v4's badge reads `version-<v>`; v5 says WHOSE version it is. */
  protected versionLabel(): string {
    const v = this.version();
    return v ? `Server version ${v}` : 'Server version unknown';
  }

  static copyrightYears(currentYear: number): string {
    return currentYear > 2025 ? `2025-${currentYear}` : '2025';
  }

  /** v4 `:122-218`, verbatim. */
  protected readonly features = [
    {
      title: 'Native desktop app',
      body: 'macOS (Lima/VZ) and Windows (WSL2) installers with branded splash screen, data directory management, and automatic VM lifecycle',
    },
    {
      title: 'Docker runtime',
      body: 'toggle between VM and Docker from the splash screen, or run standalone via Docker Hub',
    },
    {
      title: 'Aurora – Characters',
      body: 'detailed profiles with pronouns, aliases, clothing records, personalities, and multi-character turn management',
    },
    {
      title: 'The Salon – Chat',
      body: 'chat interface with tool palette, agent mode, server-side rendering, and embedded tool messages',
    },
    {
      title: 'Prospero – Projects',
      body: 'projects with files, folders, semantic search, agent mode, and custom instructions',
    },
    {
      title: 'The Commonplace Book – Memory',
      body: 'long-term memory with semantic recall, memory gate, proactive recall, and deduplication',
    },
    {
      title: 'The Lantern – Story Backgrounds',
      body: 'AI-generated atmospheric background images derived from chat context',
    },
    {
      title: 'The Concierge – Alternative Content Provision and Routing',
      body: 'content classification with detection, auto-routing to uncensored providers, and quick-hide integration',
    },
    {
      title: 'Pascal the Croupier – Gaming',
      body: 'persistent chat state, dice rolls, coin flips, inventories, stats, and game tracking',
    },
    {
      title: 'Calliope – Themes',
      body: "six bundled themes (Art Deco, Earl Grey, Great Estate, Madman's Box, Old School, Rains) plus a Default, with live switching, declarative .qtap-theme bundles, and signed remote registries",
    },
    {
      title: 'The Scriptorium – Document Stores',
      body: 'mountable external knowledge sources that characters can read, search, and (in Document Mode) write back to',
    },
    {
      title: 'The Librarian – Document Mode',
      body: "co-authoring on real files in the Scriptorium with open/save/rename/delete announcements posted into the chat on the Librarian's behalf",
    },
    {
      title: 'The Host – Salon Etiquette',
      body: 'synthetic chat announcements when characters join, leave, or change participation status, so everyone in the room knows who is actually present',
    },
    {
      title: 'Autonomous Rooms (Enclaves)',
      body: 'private character-to-character salons that run without a human in the loop, bounded by configurable budgets, with cron scheduling, pacing milestones, and post-creation editing',
    },
    {
      title: 'Saquel Ytzama – Keeper of Secrets',
      body: 'SQLCipher-encrypted databases, the Pepper Vault for API keys, instance locking, the .dbkey covenant, and the auto-lock idle timer',
    },
    {
      title: 'Ariel – Terminals',
      body: 'live PTY shell sessions hosted directly inside a Salon chat, with character-readable scrollback and a dedicated Terminal Mode pane',
    },
    {
      title: 'Carina – The Ansible',
      body: 'inline queries to a designated character via @Name: / @Name? markup or the ask_carina tool, with the answer dropped straight into the scene — public or whispered',
    },
    {
      title: 'Suparṇā – The Post Office',
      body: "inter-character mail delivered to a character's vault Mail/ folder, with anti-hijack safeguards in multi-character chats and a delivery announcement when new letters arrive",
    },
    {
      title: 'Brahma – The Console',
      body: 'a character-less, memory-free generic-LLM surface for plain model chat and read-only SQL inspection, reachable from any Salon as the @Brahma answerer',
    },
    {
      title: 'The Foundry – Architecture',
      body: 'unified settings hub, plugin system for themes, providers, templates, tools, search, and storage',
    },
    {
      title: 'Multi-provider support',
      body: 'Anthropic, OpenAI, Google Gemini, Grok, Ollama, OpenRouter, and OpenAI-compatible APIs',
    },
    {
      title: 'LLM tools',
      body: 'web search, image generation, file management, agent mode, MCP connector, custom tool plugins',
    },
    {
      title: 'Database protection',
      body: 'automatic integrity checks, WAL checkpoints, and physical backups with tiered retention',
    },
    {
      title: 'Secure by design',
      body: 'AES-256-GCM encrypted API keys, all data stays on your infrastructure, no external dependencies',
    },
  ];

  /** v4 `:224-267`, verbatim — v4's stack, not v5's (see the class docstring). */
  protected readonly techStack = [
    { label: 'Runtime', value: 'Node.js 24+' },
    { label: 'Framework', value: 'Next.js 16+' },
    { label: 'UI', value: 'React 19' },
    { label: 'Language', value: 'TypeScript 5.9+' },
    { label: 'Database', value: 'SQLite + SQLCipher' },
    { label: 'Editor', value: 'Lexical' },
    { label: 'Data Fetching', value: 'TanStack Query' },
    { label: 'File Manager', value: 'SVAR' },
    { label: 'Desktop', value: 'Electron' },
    { label: 'Styling', value: 'Tailwind CSS 4+' },
    { label: 'Validation', value: 'Zod' },
    { label: 'macOS VM', value: 'Lima / VZ' },
    { label: 'Windows VM', value: 'WSL2' },
    { label: 'Containers', value: 'Docker' },
  ];

  /** v4 `:362-369`, verbatim. */
  protected readonly acknowledgments = [
    {
      label: 'Core',
      body: 'React, Next.js, TypeScript, better-sqlite3-multiple-ciphers (SQLCipher), Zod, Ajv, @tanstack/react-query',
    },
    {
      label: 'Editor',
      body: 'Lexical (and the @lexical family — rich-text, markdown, list, code, table, link, history, selection, clipboard, react)',
    },
    {
      label: 'AI & LLM',
      body: 'OpenAI SDK, Anthropic SDK, Google GenAI SDK, OpenRouter SDK, Model Context Protocol SDK',
    },
    {
      label: 'Markdown & Documents',
      body: 'unified, remark-parse, remark-gfm, remark-rehype, rehype-stringify, rehype-highlight, react-markdown, react-syntax-highlighter, mammoth, pdf-parse, PDF.js, yaml, MessagePack',
    },
    {
      label: 'UI & Interaction',
      body: 'Tailwind CSS, dnd-kit, @tanstack/react-virtual, @svar-ui/react-filemanager, sharp, Lucide Icons',
    },
    { label: 'Filesystem & Archives', body: 'chokidar, tar, yauzl, semver' },
    { label: 'Desktop & Infrastructure', body: 'Electron, Lima, Docker' },
    { label: 'Testing', body: 'Jest, Playwright, Storybook, Testing Library' },
  ];
}
