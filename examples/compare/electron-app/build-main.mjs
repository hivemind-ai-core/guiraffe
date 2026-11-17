// Bundles the Electron main process and the (sandboxed, so single-file CJS) preload.
import { build } from 'esbuild';

const common = {
  bundle: true,
  platform: 'node',
  target: 'node22',
  format: 'cjs',
  external: ['electron'],
  minify: true,
  logLevel: 'info',
};

await build({ ...common, entryPoints: ['src/main/main.ts'], outfile: 'out/main/main.cjs' });
await build({ ...common, entryPoints: ['src/main/preload.ts'], outfile: 'out/preload/preload.cjs' });
