// Handles that pages register so the scripted bench can drive the real UI.
// Not used outside bench mode beyond the (cheap) registration itself.

import type { SortingState } from '@tanstack/react-table';
import type { RouteId } from './routes';

export interface AppHandle {
  navigate(route: RouteId): void;
  toggleTheme(): void;
}

export interface GridHandle {
  scrollEl: HTMLDivElement;
  setSorting(s: SortingState): void;
  setFilter(text: string): void;
  rowCount(): number;
}

export interface ChatHandle {
  send(text: string): void;
  tokensStreamed(): number;
}

type Handles = { app?: AppHandle; grid?: GridHandle; chat?: ChatHandle };

export const handles: Handles = {};

type Waiter = () => void;
const waiters: Waiter[] = [];

export function register<K extends keyof Handles>(key: K, handle: Handles[K] | undefined): void {
  handles[key] = handle;
  for (const w of waiters.splice(0)) w();
}

/** Resolves when `handles[key]` is set. */
export function waitFor<K extends keyof Handles>(key: K): Promise<NonNullable<Handles[K]>> {
  return new Promise((resolve) => {
    const check = () => {
      const h = handles[key];
      if (h) resolve(h as NonNullable<Handles[K]>);
      else waiters.push(check);
    };
    check();
  });
}
