/**
 * Tier-1 oracle — v4's help-guide tables and pure functions (the P4.9I2B lane).
 *
 * The Angular Help family transcribes three static tables and two pure
 * functions from v4's client. A 68-slug table and a seven-entry path map are
 * exactly what a hand-transcription drops a row of silently, so none of them
 * ships on inspection: this recorder RUNS v4's real modules and writes what
 * they actually produce, and the parity specs read the result.
 *
 * Four sections, none of which reimplements anything:
 *
 *  A. `HELP_CATEGORIES` / `URL_CATEGORY_MAP` / `EXCLUDED_DOCUMENTS`, serialised
 *     straight off `lib/help-guide/categories.ts`.
 *  B. 32 urls through v4's REAL `getCategoryForUrl` — the eight bare paths, all
 *     seven `?tab=` arms, the unknown-tab fallback, and the edges the v4 suite
 *     never asks about (`/settingsish` prefix-matching `/settings`,
 *     `/?tab=system` losing to the exact-root rule, an empty `?tab=`).
 *  C. 35 urls through v4's REAL `labelFromUrl`
 *     (`components/help-chat/hooks/useHelpChatStreaming.ts`) — the seven
 *     `pathNames`, both Capitalisation rules, and the quirks: `+` decoding to a
 *     space, a leading hyphen surviving in `tab` but producing a DOUBLE arrow
 *     space in `section`, an empty `tab` skipped as falsy.
 *  D. `HelpEntityPicker`'s `PARAM_ROUTES`, probed through the module's exported
 *     `hasParamSegments` / `findParamRoute` (the table itself is private): each
 *     row's regex source, api url, response key, `buildUrl` substitution and
 *     all three `getLabel` fallbacks, plus the negative arms (case, a missing
 *     leading slash, `:id2`).
 *
 * `WELCOME_LINKS` is module-private with no accessor, so section E recovers it
 * by rendering the real `HelpWelcomeCard` to static markup.
 *
 * ## Regenerating
 *
 * jest ignores paths outside the checkout, so this file is COPIED into a v4
 * worktree pinned at the baseline (drift-ledger §5.1):
 *
 * ```bash
 * V5=~/source/quilltap-v5
 * PIN=/tmp/qt-v4-pin-p49i2b-d883a5ee1
 * git -C ~/source/quilltap-server worktree add --detach "$PIN" d883a5ee1
 * ln -sfn ~/source/quilltap-server/node_modules "$PIN/node_modules"
 * cp $V5/apps/web/oracle/help-guide-capture.test.tsx \
 *    "$PIN/__tests__/unit/zz-p49i2b-capture.test.tsx"
 * export PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH
 * (cd "$PIN" && P49I2B_OUT=$V5/apps/web/src/app/help/__fixtures__ \
 *    npx jest --watchman=false __tests__/unit/zz-p49i2b-capture.test.tsx)
 * git -C ~/source/quilltap-server worktree remove --force "$PIN"
 * ```
 *
 * Then `npm test` in `apps/web`. Fix the PORT, never the recorded JSON.
 */
import fs from 'fs'
import path from 'path'
import React from 'react'
import { renderToStaticMarkup } from 'react-dom/server'

import {
  HELP_CATEGORIES,
  URL_CATEGORY_MAP,
  EXCLUDED_DOCUMENTS,
  getCategoryForUrl,
} from '@/lib/help-guide/categories'
import { labelFromUrl } from '@/components/help-chat/hooks/useHelpChatStreaming'
import { hasParamSegments, findParamRoute } from '@/components/help-chat/HelpEntityPicker'
import { HelpWelcomeCard } from '@/components/help-chat/HelpWelcomeCard'

const OUT = process.env.P49I2B_OUT || '/tmp/p49i2b-capture'

/** Section B — urls covering every arm of `getCategoryForUrl`. */
const CATEGORY_URLS = [
  '/',
  '',
  '/something',
  '/unknown-page',
  '/aurora',
  '/aurora/123/edit',
  '/salon',
  '/salon/abc',
  '/prospero',
  '/prospero/p1/files',
  '/files',
  '/files/folder/x',
  '/profile',
  '/setup',
  '/setup/wizard',
  '/settings',
  '/settings?tab=system',
  '/settings?tab=templates',
  '/settings?tab=images',
  '/settings?tab=memory',
  '/settings?tab=appearance',
  '/settings?tab=chat',
  '/settings?tab=providers',
  '/settings?tab=unknown',
  '/settings?tab=system&foo=bar',
  '/settings?foo=bar',
  '/settings?tab=',
  '/settings/',
  '/?tab=system',
  '/settingsish',
  '/aurora?tab=chat',
  '/salon?tab=providers',
]

/** Section C — urls covering every arm of `labelFromUrl`. */
const LABEL_URLS = [
  '/settings',
  '/aurora',
  '/salon',
  '/prospero',
  '/profile',
  '/files',
  '/setup',
  '/unknown',
  '',
  '/',
  '/settings?tab=chat',
  '/settings?tab=appearance',
  '/settings?tab=system',
  '/settings?tab=my-tab',
  '/settings?tab=chat&section=dangerous-content',
  '/settings?tab=system&section=data-management',
  '/settings?tab=appearance&section=theme-color-palette',
  '/settings?section=general',
  '/settings?section=appearance',
  '/settings?',
  '/settings?other=value&tab=chat',
  '/settings?section=test&tab=chat',
  '/aurora?tab=browse&section=sort-options',
  '/salon?tab=settings',
  '/profile?section=account-security',
  '/settings?tab=Chat',
  '/settings?tab=a-b-c',
  '/settings?tab=my_tab',
  '/settings?tab=&section=x',
  '/settings?tab=multi+word',
  '/settings?tab=-lead',
  '/settings?section=-lead-ing',
  '/aurora/123?tab=edit',
  '/setupwizard',
  '/settings?tab=chat&section=',
]

/** Section D — url templates covering all three PARAM_ROUTES + the negatives. */
const PARAM_URLS = [
  '/aurora/:id',
  '/aurora/:id/edit',
  '/salon/:id',
  '/salon/:id/settings',
  '/prospero/:id',
  '/prospero/:id/files',
  '/aurora/:id2',
  '/aurora/123',
  '/settings/:id',
  '/files/:id',
  '/aurora',
  '',
  '/AURORA/:id',
  'aurora/:id',
]

describe('P4.9I2B v4 capture', () => {
  it('records the tables and vectors', () => {
    fs.mkdirSync(OUT, { recursive: true })

    const categoryVectors = CATEGORY_URLS.map((url) => ({ url, categoryId: getCategoryForUrl(url) }))
    const labelVectors = LABEL_URLS.map((url) => ({ url, label: labelFromUrl(url) }))
    const paramVectors = PARAM_URLS.map((url) => {
      const route = findParamRoute(url)
      return {
        url,
        hasParamSegments: hasParamSegments(url),
        entityLabel: route ? route.entityLabel : null,
        apiUrl: route ? route.apiUrl : null,
        responseKey: route ? route.responseKey : null,
        pattern: route ? route.pattern.source : null,
        builtUrl: route ? route.buildUrl(url, 'ID-42') : null,
        labelNamed: route ? route.getLabel({ name: 'Nom', title: 'Titre' }) : null,
        labelEmpty: route ? route.getLabel({}) : null,
        labelBlank: route ? route.getLabel({ name: '', title: '' }) : null,
        id: route ? route.getId({ id: 'x-1' }) : null,
      }
    })

    // Section E — WELCOME_LINKS is module-private: recover it from the card.
    const welcomeHtml = renderToStaticMarkup(
      React.createElement(HelpWelcomeCard, { onOpenDocument: () => undefined }),
    )

    fs.writeFileSync(
      path.join(OUT, 'help-guide-tables.json'),
      JSON.stringify({ HELP_CATEGORIES, URL_CATEGORY_MAP, EXCLUDED_DOCUMENTS }, null, 2) + '\n',
    )
    fs.writeFileSync(
      path.join(OUT, 'help-guide-vectors.json'),
      JSON.stringify({ getCategoryForUrl: categoryVectors }, null, 2) + '\n',
    )
    fs.writeFileSync(
      path.join(OUT, 'label-from-url-vectors.json'),
      JSON.stringify({ labelFromUrl: labelVectors }, null, 2) + '\n',
    )
    fs.writeFileSync(
      path.join(OUT, 'param-routes-vectors.json'),
      JSON.stringify({ paramRoutes: paramVectors }, null, 2) + '\n',
    )
    fs.writeFileSync(
      path.join(OUT, 'welcome-card.json'),
      JSON.stringify({ html: welcomeHtml }, null, 2) + '\n',
    )

    expect(categoryVectors.length).toBeGreaterThanOrEqual(20)
  })
})
