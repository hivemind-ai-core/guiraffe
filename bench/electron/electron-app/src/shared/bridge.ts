// The minimal IPC surface the preload exposes as `window.compare`.

export interface BenchConfig {
  /** Scenario name, or null when not in bench mode. */
  scenario: string | null;
  idleSecs: number;
}

export interface BenchReport {
  first_frame_epoch_ms: number;
  frames_ms: number[];
  timings_ms: Record<string, number[]>;
  info: Record<string, unknown>;
}

export interface CompareBridge {
  getConfig(): Promise<BenchConfig>;
  /** Resolves once main has shown the window (on `ready-to-show`). */
  onShown(cb: () => void): void;
  setContentSize(width: number, height: number): void;
  phase(name: string): void;
  report(report: BenchReport): void;
  fail(message: string): void;
}

export const IPC = {
  config: 'compare:config',
  shown: 'compare:shown',
  setSize: 'compare:set-size',
  phase: 'compare:phase',
  report: 'compare:report',
  fail: 'compare:fail',
} as const;
