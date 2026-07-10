import { Routes } from '@angular/router';

/**
 * The in-shell routes (v5 introduces real routing this round). The startup gate
 * (`App`) still owns the pre-operational states and only mounts `Shell` — which
 * hosts the `<router-outlet>` — once the vault is operational, so these routes
 * only ever match against an operational engine.
 *
 * `/salon` is the conversation list; `/salon/:id` is one conversation;
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
