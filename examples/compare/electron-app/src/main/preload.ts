import { contextBridge, ipcRenderer } from 'electron';
import { IPC, type BenchReport, type CompareBridge } from '../shared/bridge';

// Registered at preload time so the notification can't race the renderer's bundle.
let shown = false;
const waiters: (() => void)[] = [];
ipcRenderer.once(IPC.shown, () => {
  shown = true;
  for (const cb of waiters.splice(0)) cb();
});

const bridge: CompareBridge = {
  getConfig: () => ipcRenderer.invoke(IPC.config),
  onShown: (cb) => {
    if (shown) cb();
    else waiters.push(cb);
  },
  setContentSize: (width, height) => ipcRenderer.send(IPC.setSize, width, height),
  phase: (name) => ipcRenderer.send(IPC.phase, name),
  report: (report: BenchReport) => ipcRenderer.send(IPC.report, report),
  fail: (message) => ipcRenderer.send(IPC.fail, message),
};

contextBridge.exposeInMainWorld('compare', bridge);
