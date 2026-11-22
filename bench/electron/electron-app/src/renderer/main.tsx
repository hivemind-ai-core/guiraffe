import '@mantine/core/styles.css';
import '@mantine/notifications/styles.css';
import '@mantine/spotlight/styles.css';
import './styles.css';

import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import { runBench } from './bench';

const root = document.getElementById('root');
if (root) {
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}

void window.compare.getConfig().then((config) => {
  if (config.scenario) {
    // A bench run with an uncaught error is invalid: fail loudly instead of measuring it.
    window.addEventListener('error', (e) => window.compare.fail(`uncaught: ${e.message}`));
    window.addEventListener('unhandledrejection', (e) =>
      window.compare.fail(`unhandled rejection: ${String(e.reason)}`),
    );
    runBench(config).catch((err: unknown) => {
      window.compare.fail(err instanceof Error ? `${err.message}\n${err.stack}` : String(err));
    });
  }
});
