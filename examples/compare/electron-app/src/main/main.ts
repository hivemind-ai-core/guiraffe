import { app, BrowserWindow, ipcMain } from 'electron';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { IPC, type BenchConfig, type BenchReport } from '../shared/bridge';
import { generateRows } from '../shared/data';

/** Unix epoch ms of this (main) process start: /proc/self/stat field 22 + btime. */
function processStartEpochMs(): number | null {
  try {
    const stat = readFileSync('/proc/self/stat', 'utf8');
    // Fields after the parenthesised comm start at field 3.
    const rest = stat.slice(stat.lastIndexOf(')') + 2).split(' ');
    const startTicks = Number(rest[22 - 3]);
    const btimeLine = readFileSync('/proc/stat', 'utf8')
      .split('\n')
      .find((l) => l.startsWith('btime '));
    if (!btimeLine) return null;
    const btime = Number(btimeLine.split(/\s+/)[1]);
    const CLK_TCK = 100;
    return (btime + startTicks / CLK_TCK) * 1000;
  } catch {
    return null;
  }
}

// Captured as early as possible.
const processStart = processStartEpochMs();

function argValue(name: string): string | undefined {
  const i = process.argv.indexOf(name);
  if (i >= 0 && i + 1 < process.argv.length) return process.argv[i + 1];
  const eq = process.argv.find((a) => a.startsWith(`${name}=`));
  return eq?.slice(name.length + 1);
}

const scenario = argValue('--bench') ?? process.env.COMPARE_BENCH ?? null;
const idleSecs = Number(process.env.BENCH_IDLE_SECS ?? 95);

if (process.env.COMPARE_NO_SANDBOX === '1') {
  // Only for environments where Chromium's sandbox can't start (e.g. no suid chrome-sandbox
  // and no unprivileged user namespaces). The harness opts in explicitly.
  app.commandLine.appendSwitch('no-sandbox');
}

if (process.argv.includes('--print-row0')) {
  const [row] = generateRows(1);
  process.stdout.write(`${JSON.stringify(row)}\n`);
  app.exit(0);
} else {
  start();
}

function start(): void {
  app.enableSandbox();

  let win: BrowserWindow | null = null;

  ipcMain.handle(IPC.config, (): BenchConfig => ({ scenario, idleSecs }));

  ipcMain.on(IPC.setSize, (_e, width: number, height: number) => {
    win?.setContentSize(Math.round(width), Math.round(height));
  });

  ipcMain.on(IPC.phase, (_e, name: string) => {
    process.stdout.write(`BENCH_PHASE ${name}\n`);
  });

  ipcMain.on(IPC.report, (_e, report: BenchReport) => {
    const firstFrameSince =
      processStart === null ? null : report.first_frame_epoch_ms - processStart;
    const out = {
      app: 'electron',
      scenario,
      first_frame_epoch_ms: report.first_frame_epoch_ms,
      process_start_epoch_ms: processStart,
      first_frame_since_process_start_ms: firstFrameSince,
      frames_ms: report.frames_ms,
      timings_ms: report.timings_ms,
      info: report.info,
    };
    process.stdout.write(`BENCH_JSON ${JSON.stringify(out)}\n`, () => app.exit(0));
  });

  ipcMain.on(IPC.fail, (_e, message: string) => {
    process.stderr.write(`BENCH_FAIL ${message}\n`);
    app.exit(1);
  });

  app.whenReady().then(() => {
    win = new BrowserWindow({
      width: 1280,
      height: 800,
      useContentSize: true,
      title: 'Compare: Electron',
      show: false,
      autoHideMenuBar: true,
      webPreferences: {
        preload: join(__dirname, '../preload/preload.cjs'),
        contextIsolation: true,
        nodeIntegration: false,
        sandbox: true,
        devTools: false,
        backgroundThrottling: false,
        spellcheck: false,
      },
    });
    win.setMenu(null);
    win.once('ready-to-show', () => {
      win?.show();
      win?.webContents.send(IPC.shown);
    });
    win.webContents.on('render-process-gone', (_e, details) => {
      process.stderr.write(`renderer gone: ${details.reason}\n`);
      if (scenario) app.exit(1);
    });
    void win.loadFile(join(__dirname, '../renderer/index.html'));
  });

  app.on('window-all-closed', () => app.quit());
}
