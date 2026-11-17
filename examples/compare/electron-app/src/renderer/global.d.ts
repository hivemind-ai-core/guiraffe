import type { CompareBridge } from '../shared/bridge';

declare global {
  interface Window {
    compare: CompareBridge;
  }
}

export {};
