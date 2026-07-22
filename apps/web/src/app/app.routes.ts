import { Routes } from '@angular/router';

import { workspaceRedirectGuard } from './workspace/workspace-redirect.guard';

/**
 * The in-shell routes (v5 introduces real routing this round). The startup gate
 * (`App`) still owns the pre-operational states and only mounts `Shell` — which
 * hosts the `<router-outlet>` — once the vault is operational, so these routes
 * only ever match against an operational engine.
 *
 * `/` is the Home dashboard (welcome + quick actions + the three-column grid);
 * `/salon` is the conversation list; `/salon/new` is the New-Chat form;
 * `/salon/:id` is one conversation;
 * `/characters` is the character roster, `/characters/new` the create form,
 * `/characters/:id` a character's detail and `/characters/:id/edit` its editor;
 * `/settings` is the Settings hall and `/settings/wizard` the provider wizard;
 * `/scenarios` is the general (instance-wide) scenarios page;
 * `/custom-tools` is Pascal's Workbench (library ↔ editor in place, deep-linked
 * through `?mount=`/`?path=`/`?new=1` — v4's own no-workspace fallback);
 * `/about` is the static About screen and `/profile` the account screen, both
 * reached from the shell footer's user menu.
 * `/generate-image` is the standalone image-generation screen.
 * `/photos` is the My Photos gallery — the deduped roll-up of every `photos/`
 * folder across the instance.
 * Every other path redirects to the Salon.
 *
 * **P4.9J1 (the tabbed workspace, v4's default shell):** `/workspace` renders
 * the two-pane host. When the workspace-tabs flag is ON (the default), the
 * legacy surface routes carry a `workspaceRedirectGuard` that redirects into
 * `/workspace?open=…` with the v4 param names; when the flag is off every guard
 * is a no-op and the old routes render as before.
 *
 * **P4.d16 (v4 `8d86847a`, "deep links that escaped the tabbed workspace"):**
 * the routes that used to render the legacy full-page shell now redirect too —
 * the salon list (`open=salon-list`), the terminal popout (the Salon tab plus a
 * child terminal tab), and the detail routes `/prospero/:id`,
 * `/scriptorium/:id`, `/characters/groups/:id` and `/characters/:id`, each
 * carrying the id that drills its list tab into the target. The old docstring
 * called those exclusions "v4-faithful"; v4 moved, and so has v5.
 *
 * `/salon/new` redirects too, into a v5-only `salon-new` tab hosting the
 * New-Chat screen — v4 pops a modal there, and v5 never ported it (the standing
 * no-modal divergence; P4.d16 tier 2). The only remaining pass-through is
 * `/settings/wizard?mode=setup` (the fresh-instance handoff, a documented
 * divergence — see the guard's docstring).
 */
export const routes: Routes = [
  {
    path: 'salon',
    canActivate: [workspaceRedirectGuard('salon-list')],
    loadComponent: () => import('./screens/salon/salon-list').then((m) => m.SalonList),
  },
  {
    // v4 pops its NewChatModal here (`open=new-chat`); v5 has no modal, so the
    // New-Chat SCREEN is hosted as a v5-only `salon-new` tab carrying the same
    // three seeds. See the tab kind's contract comment.
    path: 'salon/new',
    canActivate: [
      workspaceRedirectGuard('salon-new', (r) => ({
        characterId: r.queryParamMap.get('characterId'),
        projectId: r.queryParamMap.get('projectId'),
        autonomous: r.queryParamMap.get('autonomous'),
      })),
    ],
    loadComponent: () => import('./screens/new-chat/new-chat-page').then((m) => m.NewChatPage),
  },
  {
    // The pop-out opens the Salon tab PLUS a child terminal tab (the Salon view
    // is the portal source for the live PTY) — the intent layer owns that
    // two-step; the guard just carries both ids.
    path: 'salon/:id/terminal/:sessionId',
    canActivate: [
      workspaceRedirectGuard('terminal', (r) => ({
        chatId: r.paramMap.get('id'),
        sessionId: r.paramMap.get('sessionId'),
      })),
    ],
    loadComponent: () => import('./screens/salon/terminal-popout').then((m) => m.TerminalPopout),
  },
  {
    path: 'salon/:id',
    canActivate: [workspaceRedirectGuard('salon', (r) => ({ chatId: r.paramMap.get('id') }))],
    loadComponent: () =>
      import('./screens/salon/salon-conversation').then((m) => m.SalonConversation),
  },
  {
    path: 'characters',
    canActivate: [workspaceRedirectGuard('aurora')],
    loadComponent: () =>
      import('./screens/characters/list/characters-list').then((m) => m.CharactersList),
  },
  {
    path: 'characters/new',
    canActivate: [workspaceRedirectGuard('character-new')],
    loadComponent: () =>
      import('./screens/characters/new/new-character').then((m) => m.NewCharacter),
  },
  {
    // v4 `/aurora/groups/:id` — the Characters tab drilled into that group.
    path: 'characters/groups/:id',
    canActivate: [workspaceRedirectGuard('aurora', (r) => ({ groupId: r.paramMap.get('id') }))],
    loadComponent: () => import('./screens/groups/group-editor').then((m) => m.GroupEditor),
  },
  {
    path: 'characters/:id/edit',
    canActivate: [
      workspaceRedirectGuard('character-edit', (r) => ({
        characterId: r.paramMap.get('id'),
        tab: r.queryParamMap.get('tab'),
      })),
    ],
    loadComponent: () =>
      import('./screens/characters/edit/character-edit').then((m) => m.CharacterEdit),
  },
  {
    // v5's `/characters/:id` IS v4's `/aurora/:id/view` (no `/view` suffix).
    // `?tab=` is preserved and `?action=chat` forwarded — the intent layer turns
    // the latter into the new-chat funnel, v4's modal-pop arm.
    path: 'characters/:id',
    canActivate: [
      workspaceRedirectGuard('character-view', (r) => ({
        characterId: r.paramMap.get('id'),
        tab: r.queryParamMap.get('tab'),
        action: r.queryParamMap.get('action'),
      })),
    ],
    loadComponent: () =>
      import('./screens/characters/view/character-detail').then((m) => m.CharacterDetail),
  },
  {
    path: 'files',
    canActivate: [workspaceRedirectGuard('files')],
    loadComponent: () => import('./screens/files/files-browser').then((m) => m.FilesBrowser),
  },
  {
    path: 'prospero',
    canActivate: [workspaceRedirectGuard('prospero')],
    loadComponent: () => import('./screens/prospero/prospero-list').then((m) => m.ProsperoList),
  },
  {
    path: 'prospero/:id',
    canActivate: [
      workspaceRedirectGuard('prospero', (r) => ({ projectId: r.paramMap.get('id') })),
    ],
    loadComponent: () =>
      import('./screens/prospero/project-detail').then((m) => m.ProjectDetailScreen),
  },
  {
    path: 'scenarios',
    canActivate: [workspaceRedirectGuard('scenarios')],
    loadComponent: () => import('./screens/scenarios/scenarios-page').then((m) => m.ScenariosPage),
  },
  {
    path: 'scriptorium',
    canActivate: [workspaceRedirectGuard('scriptorium')],
    loadComponent: () =>
      import('./screens/scriptorium/scriptorium-list').then((m) => m.ScriptoriumList),
  },
  {
    path: 'scriptorium/:id',
    canActivate: [workspaceRedirectGuard('scriptorium', (r) => ({ storeId: r.paramMap.get('id') }))],
    loadComponent: () => import('./screens/scriptorium/store-detail').then((m) => m.StoreDetail),
  },
  {
    path: 'custom-tools',
    canActivate: [
      workspaceRedirectGuard('custom-tools', (r) => ({
        mount: r.queryParamMap.get('mount'),
        path: r.queryParamMap.get('path'),
        new: r.queryParamMap.get('new'),
      })),
    ],
    loadComponent: () =>
      import('./screens/custom-tools/custom-tools-page').then((m) => m.CustomToolsPage),
  },
  {
    path: 'settings/wizard',
    // The fresh-instance handoff `?mode=setup` (shell first-run gate) passes
    // through and renders standalone — the frozen contract has no wizard tab
    // payload to carry `mode` (p4.9j3 item 4; a documented divergence). Without
    // `mode=setup`, the Providers-tab re-entry redirects into the workspace tab.
    canActivate: [
      workspaceRedirectGuard(
        'settings-wizard',
        undefined,
        (r) => r.queryParamMap.get('mode') === 'setup',
      ),
    ],
    loadComponent: () =>
      import('./screens/settings/wizard/wizard-screen').then((m) => m.WizardScreen),
  },
  {
    path: 'settings',
    canActivate: [
      workspaceRedirectGuard('settings', (r) => ({
        tab: r.queryParamMap.get('tab'),
        section: r.queryParamMap.get('section'),
      })),
    ],
    loadComponent: () => import('./screens/settings/settings').then((m) => m.Settings),
  },
  // === P4.9c (lane C) ===
  {
    path: 'about',
    canActivate: [workspaceRedirectGuard('about')],
    loadComponent: () => import('./screens/about/about-page').then((m) => m.AboutPage),
  },
  {
    path: 'profile',
    canActivate: [workspaceRedirectGuard('profile')],
    loadComponent: () => import('./screens/profile/profile-page').then((m) => m.ProfilePage),
  },
  // === end P4.9c ===
  {
    path: 'generate-image',
    canActivate: [workspaceRedirectGuard('generate-image')],
    loadComponent: () =>
      import('./screens/generate-image/generate-image-page').then((m) => m.GenerateImagePage),
  },
  // === P4.9a: the My Photos vertical (lane A, append-only) ===
  {
    path: 'photos',
    canActivate: [workspaceRedirectGuard('photos')],
    loadComponent: () => import('./screens/photos/photos-page').then((m) => m.PhotosPage),
  },
  // === end P4.9a ===
  // === P4.9J1: the tabbed workspace (v4's default shell) ===
  {
    path: 'workspace',
    loadComponent: () =>
      import('./workspace/chrome/workspace-host').then((m) => m.WorkspaceHost),
  },
  // === end P4.9J1 ===
  {
    path: '',
    pathMatch: 'full',
    canActivate: [workspaceRedirectGuard('home')],
    loadComponent: () => import('./screens/home/home-page').then((m) => m.HomePage),
  },
  { path: '**', redirectTo: 'salon' },
];
