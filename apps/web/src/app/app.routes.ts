import { Routes } from '@angular/router';

/**
 * The in-shell routes (v5 introduces real routing this round). The startup gate
 * (`App`) still owns the pre-operational states and only mounts `Shell` — which
 * hosts the `<router-outlet>` — once the vault is operational, so these routes
 * only ever match against an operational engine.
 *
 * `/salon` is the conversation list; `/salon/:id` is one conversation. Every
 * other path redirects to the Salon (the foundation's single real vertical).
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
  { path: '', redirectTo: 'salon', pathMatch: 'full' },
  { path: '**', redirectTo: 'salon' },
];
