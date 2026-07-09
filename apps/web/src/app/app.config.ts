import { ApplicationConfig, provideBrowserGlobalErrorListeners } from '@angular/core';
import { provideRouter } from '@angular/router';
import { QueryClient, provideTanStackQuery } from '@tanstack/angular-query-experimental';

import { routes } from './app.routes';

/**
 * TanStack Query (Angular adapter) is the server-state layer over
 * `CoreClient.dispatch` (v4's ~52 useQuery/useMutation sites port 1:1). The live
 * SSE stream is deliberately NOT a Query concern — it rides the stream reducer.
 */
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // Local single-user app: data changes only through this client.
      staleTime: 5_000,
      refetchOnWindowFocus: false,
      retry: 1,
    },
  },
});

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideRouter(routes),
    provideTanStackQuery(queryClient),
  ],
};
