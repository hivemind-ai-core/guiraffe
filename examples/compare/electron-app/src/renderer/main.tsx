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
    runBench(config).catch((err: unknown) => {
      window.compare.fail(err instanceof Error ? `${err.message}\n${err.stack}` : String(err));
    });
  }
});
