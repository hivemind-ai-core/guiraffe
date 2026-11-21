// Scripted bench scenarios (see examples/compare/README.md, "Bench mode").
import { flushSync } from 'react-dom';
import type { BenchConfig, BenchReport } from '../shared/bridge';
import { waitFor } from './benchHooks';

const nextFrame = () => new Promise<number>((resolve) => requestAnimationFrame(resolve));
const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));
const epochNow = () => performance.timeOrigin + performance.now();

/** Continuous rAF chain recording callback entry, like GPUI's recorder. */
function recordFrames(): { stop(): number[] } {
  const intervals: number[] = [];
  let last: number | null = null;
  let running = true;
  const tick = () => {
    if (!running) return;
    const t = performance.now();
    if (last !== null) intervals.push(t - last);
    last = t;
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
  return {
    stop() {
      running = false;
      return intervals;
    },
  };
}

/** Apply a state update, commit it synchronously, and wait until it has been presented. */
async function settled(update: () => void): Promise<number> {
  const t0 = performance.now();
  flushSync(update);
  await nextFrame();
  await nextFrame();
  return performance.now() - t0;
}

/** Run `step` once per frame for `ms` milliseconds. */
async function eachFrameFor(ms: number, step: () => void): Promise<void> {
  const end = performance.now() + ms;
  while (performance.now() < end) {
    step();
    await nextFrame();
  }
}

async function openGrid() {
  const app = await waitFor('app');
  app.navigate('/grid');
  const grid = await waitFor('grid');
  await nextFrame();
  await nextFrame();
  return grid;
}

async function flingGrid(ms: number): Promise<void> {
  const grid = await waitFor('grid');
  const el = grid.scrollEl;
  await eachFrameFor(ms, () => {
    if (el.scrollTop + el.clientHeight >= el.scrollHeight - 1) el.scrollTop = 0;
    else el.scrollTop += el.clientHeight;
  });
}

const CHAT_PROMPT = 'Explain the render pipeline.';

export async function runBench(config: BenchConfig): Promise<void> {
  const scenario = config.scenario;
  await new Promise<void>((resolve) => window.compare.onShown(resolve));
  await nextFrame();
  await nextFrame();
  const firstFrame = epochNow();

  const report: BenchReport = {
    first_frame_epoch_ms: firstFrame,
    frames_ms: [],
    timings_ms: {},
    info: {},
  };

  switch (scenario) {
    case 'startup':
      break;

    case 'idle': {
      const app = await waitFor('app');
      app.navigate('/form');
      await sleep(config.idleSecs * 1000);
      report.info.idle_secs = config.idleSecs;
      break;
    }

    case 'grid-fling': {
      const grid = await openGrid();
      await sleep(500);
      const el = grid.scrollEl;
      const rec = recordFrames();
      const t0 = performance.now();
      for (let page = 0; page < 200; page++) {
        el.scrollTop += el.clientHeight;
        await nextFrame();
      }
      report.timings_ms.total_ms = [performance.now() - t0];
      report.frames_ms = rec.stop();
      report.info.rows = grid.rowCount();
      break;
    }

    case 'sort': {
      const grid = await openGrid();
      await sleep(500);
      const sortMs: number[] = [];
      const unsortMs: number[] = [];
      for (let i = 0; i < 5; i++) {
        sortMs.push(await settled(() => grid.setSorting([{ id: 'amount', desc: true }])));
        unsortMs.push(await settled(() => grid.setSorting([])));
      }
      report.timings_ms.sort_ms = sortMs;
      report.timings_ms.unsort_ms = unsortMs;
      report.info.rows = grid.rowCount();
      break;
    }

    case 'filter': {
      const grid = await openGrid();
      await sleep(500);
      const filterMs: number[] = [];
      const clearMs: number[] = [];
      let matched = 0;
      for (let i = 0; i < 5; i++) {
        filterMs.push(await settled(() => grid.setFilter('ada')));
        matched = grid.rowCount();
        clearMs.push(await settled(() => grid.setFilter('')));
      }
      report.timings_ms.filter_ms = filterMs;
      report.timings_ms.clear_ms = clearMs;
      report.info.rows = grid.rowCount();
      report.info.filtered_rows = matched;
      break;
    }

    case 'theme': {
      const app = await waitFor('app');
      await sleep(500);
      const themeMs: number[] = [];
      for (let i = 0; i < 10; i++) themeMs.push(await settled(() => app.toggleTheme()));
      report.timings_ms.theme_ms = themeMs;
      break;
    }

    case 'chat': {
      const app = await waitFor('app');
      app.navigate('/chat');
      const chat = await waitFor('chat');
      await sleep(500);
      const rec = recordFrames();
      chat.send(CHAT_PROMPT);
      await sleep(10_000);
      report.frames_ms = rec.stop();
      report.info.tokens = chat.tokensStreamed();
      break;
    }

    case 'resize': {
      const rec = recordFrames();
      for (let k = 0; k < 120; k++) {
        const width = 1000 + 280 * Math.abs(Math.sin(k / 10));
        window.compare.setContentSize(width, 800);
        await nextFrame();
      }
      report.frames_ms = rec.stop();
      break;
    }

    case 'load': {
      window.compare.phase('grid');
      await openGrid();
      const gridRec = recordFrames();
      await flingGrid(10_000);
      const gridFrames = gridRec.stop();

      window.compare.phase('chat');
      const app = await waitFor('app');
      app.navigate('/chat');
      const chat = await waitFor('chat');
      const chatRec = recordFrames();
      chat.send(CHAT_PROMPT);
      await sleep(10_000);
      const chatFrames = chatRec.stop();

      report.frames_ms = [...gridFrames, ...chatFrames];
      report.timings_ms.grid_frames_ms = gridFrames;
      report.timings_ms.chat_frames_ms = chatFrames;
      report.info.tokens = chat.tokensStreamed();
      break;
    }

    default:
      throw new Error(`unknown bench scenario: ${scenario}`);
  }

  window.compare.report(report);
}
