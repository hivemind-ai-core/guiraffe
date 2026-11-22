# Compare: Electron

The Electron + React side of the GUIraffe comparison. Behaviour follows the contract in
[`../README.md`](../README.md).

Stack: Electron, React 19, Vite, TypeScript, Mantine (core, hooks, form, notifications,
spotlight), TanStack Table v9 + TanStack Virtual, Recharts, react-markdown + remark-gfm +
Shiki (`@shikijs/rehype`, Oniguruma engine, one shared highlighter, dual light/dark themes).
Fixtures are imported from `../shared` (`@shared-fixtures` alias), not copied.

All npm packages are `devDependencies`: the renderer is bundled by Vite and main/preload by
esbuild, so the packaged app contains only `out/`.

## Build

```sh
pnpm install          # also downloads the Electron binary (postinstall: install-electron)
pnpm typecheck        # tsc --noEmit
pnpm build            # vite build → out/renderer, esbuild → out/main, out/preload
pnpm package          # optional: electron-builder --dir → dist/linux-unpacked
```

## Run

```sh
./node_modules/.bin/electron .                    # interactive
./node_modules/.bin/electron . --print-row0       # generator parity check
./node_modules/.bin/electron . --bench sort       # or COMPARE_BENCH=sort
dist/linux-unpacked/compare-electron-app --bench startup   # packaged build
```

Scenarios: `startup idle grid-fling sort filter theme chat resize load`
(`BENCH_IDLE_SECS` sets the idle duration, default 95). Output: optional `BENCH_PHASE <name>`
lines, then one `BENCH_JSON {…}` line on stdout; exit 0. A renderer error, an unknown scenario
or a crashed renderer exits non-zero with `BENCH_FAIL …` on stderr.

The `load` scenario also reports per-phase frame intervals in `timings_ms.grid_frames_ms` and
`timings_ms.chat_frames_ms` (`frames_ms` is both concatenated).

Security settings: `contextIsolation`, `sandbox` on, `nodeIntegration` and devtools off,
`backgroundThrottling: false`. GPU is on (no flags). If Chromium's sandbox can't start on a
machine (no unprivileged user namespaces and no suid `chrome-sandbox`), set
`COMPARE_NO_SANDBOX=1`; it is never disabled unconditionally.

## How bench mode works

- The window is created `show: false`; on `ready-to-show` main shows it and notifies the
  renderer, which waits two `requestAnimationFrame`s and takes
  `performance.timeOrigin + performance.now()` as the first frame.
- `process_start_epoch_ms` is the main process start from `/proc/self/stat` + `btime`.
- Frame intervals come from a continuous rAF chain and are sent to main over IPC at the end;
  main prints `BENCH_JSON` and calls `app.exit(0)`.
- "Settled" timings: `flushSync(update)` then two rAFs.
- `resize`: the renderer asks main to `setContentSize` once per frame.

## Notes

- Shiki's JavaScript regex engine stalled the renderer for up to ~1 s per frame while a
  partial Rust block was streaming in chat. The Oniguruma (WASM) engine, Shiki's default,
  doesn't, so this app uses it. That's why the CSP allows `'wasm-unsafe-eval'`.
