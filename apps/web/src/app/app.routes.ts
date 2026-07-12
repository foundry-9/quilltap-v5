import { Routes } from '@angular/router';

/**
 * The in-shell routes (v5 introduces real routing this round). The startup gate
 * (`App`) still owns the pre-operational states and only mounts `Shell` — which
 * hosts the `<router-outlet>` — once the vault is operational, so these routes
 * only ever match against an operational engine.
 *
 * `/salon` is the conversation list; `/salon/new` is the New-Chat form;
 * `/salon/:id` is one conversation;
 * `/characters` is the character roster, `/characters/new` the create form,
 * `/characters/:id` a character's detail and `/characters/:id/edit` its editor;
 * `/settings` is the Settings hall and `/settings/wizard` the provider wizard;
 * `/scenarios` is the general (instance-wide) scenarios page.
 * Every other path redirects to the Salon.
 */
export const routes: Routes = [
  {
    path: 'salon',
    loadComponent: () => import('./screens/salon/salon-list').then((m) => m.SalonList),
  },
  {
    path: 'salon/new',
    loadComponent: () => import('./screens/new-chat/new-chat-page').then((m) => m.NewChatPage),
  },
  {
    path: 'salon/:id',
    loadComponent: () =>
      import('./screens/salon/salon-conversation').then((m) => m.SalonConversation),
  },
  {
    path: 'characters',
    loadComponent: () =>
      import('./screens/characters/list/characters-list').then((m) => m.CharactersList),
  },
  {
    path: 'characters/new',
    loadComponent: () =>
      import('./screens/characters/new/new-character').then((m) => m.NewCharacter),
  },
  {
    path: 'characters/groups/:id',
    loadComponent: () => import('./screens/groups/group-editor').then((m) => m.GroupEditor),
  },
  {
    path: 'characters/:id/edit',
    loadComponent: () =>
      import('./screens/characters/edit/character-edit').then((m) => m.CharacterEdit),
  },
  {
    path: 'characters/:id',
    loadComponent: () =>
      import('./screens/characters/view/character-detail').then((m) => m.CharacterDetail),
  },
  {
    path: 'prospero',
    loadComponent: () => import('./screens/prospero/prospero-list').then((m) => m.ProsperoList),
  },
  {
    path: 'prospero/:id',
    loadComponent: () =>
      import('./screens/prospero/project-detail').then((m) => m.ProjectDetailScreen),
  },
  {
    path: 'scenarios',
    loadComponent: () =>
      import('./screens/scenarios/scenarios-page').then((m) => m.ScenariosPage),
  },
  {
    path: 'settings/wizard',
    loadComponent: () =>
      import('./screens/settings/wizard/wizard-screen').then((m) => m.WizardScreen),
  },
  {
    path: 'settings',
    loadComponent: () => import('./screens/settings/settings').then((m) => m.Settings),
  },
  { path: '', redirectTo: 'salon', pathMatch: 'full' },
  { path: '**', redirectTo: 'salon' },
];
