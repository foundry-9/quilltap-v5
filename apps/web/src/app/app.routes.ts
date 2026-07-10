import { Routes } from '@angular/router';

/**
 * The in-shell routes (v5 introduces real routing this round). The startup gate
 * (`App`) still owns the pre-operational states and only mounts `Shell` — which
 * hosts the `<router-outlet>` — once the vault is operational, so these routes
 * only ever match against an operational engine.
 *
 * `/salon` is the conversation list; `/salon/:id` is one conversation;
 * `/characters` is the character roster, `/characters/new` the create form,
 * `/characters/:id` a character's detail and `/characters/:id/edit` its editor;
 * `/settings` is the Settings hall and `/settings/wizard` the provider wizard.
 * Every other path redirects to the Salon.
 */
export const routes: Routes = [
  {
    path: 'salon',
    loadComponent: () => import('./screens/salon/salon-list').then((m) => m.SalonList),
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
