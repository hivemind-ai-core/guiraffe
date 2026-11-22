import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from 'react';
import {
  ActionIcon,
  AppShell,
  Center,
  Group,
  Kbd,
  Loader,
  MantineProvider,
  NavLink,
  Text,
  Title,
  createTheme,
} from '@mantine/core';
import { Notifications } from '@mantine/notifications';
import { Spotlight, spotlight, type SpotlightActionData } from '@mantine/spotlight';
import { register } from './benchHooks';
import { ROUTES, type RouteId } from './routes';
import { Dashboard } from './pages/Dashboard';
import { FormPage } from './pages/FormPage';

// Heavy routes are code-split; they load on first navigation.
const GridPage = lazy(() => import('./pages/GridPage'));
const DocsPage = lazy(() => import('./pages/DocsPage'));
const ChatPage = lazy(() => import('./pages/ChatPage'));

const theme = createTheme({ primaryColor: 'indigo', defaultRadius: 'sm' });

type Scheme = 'light' | 'dark';

const isMac = navigator.userAgent.includes('Mac');

function Page({ route }: { route: RouteId }) {
  switch (route) {
    case '/':
      return <Dashboard />;
    case '/grid':
      return <GridPage />;
    case '/docs':
      return <DocsPage />;
    case '/chat':
      return <ChatPage />;
    case '/form':
      return <FormPage />;
  }
}

export function App() {
  const [route, setRoute] = useState<RouteId>('/');
  const [scheme, setScheme] = useState<Scheme>('light');
  const toggleTheme = useCallback(() => setScheme((s) => (s === 'light' ? 'dark' : 'light')), []);

  useEffect(() => {
    register('app', { navigate: setRoute, toggleTheme });
    return () => register('app', undefined);
  }, [toggleTheme]);

  const actions = useMemo<SpotlightActionData[]>(
    () => [
      ...ROUTES.map((r) => ({
        id: `go${r.id}`,
        label: `Go to ${r.title}`,
        onClick: () => setRoute(r.id),
      })),
      { id: 'toggle-theme', label: 'Toggle theme', onClick: toggleTheme },
    ],
    [toggleTheme],
  );

  const title = ROUTES.find((r) => r.id === route)?.title ?? '';

  return (
    <MantineProvider theme={theme} forceColorScheme={scheme}>
      <Notifications position="top-right" />
      <Spotlight
        actions={actions}
        shortcut={['mod + K']}
        nothingFound="No commands"
        searchProps={{ placeholder: 'Type a command…' }}
      />
      <AppShell header={{ height: 52 }} navbar={{ width: 200, breakpoint: 0 }} padding="md">
        <AppShell.Header>
          <Group h="100%" px="md" justify="space-between">
            <Group gap="md">
              <Title order={4}>Compare: Electron</Title>
              <Text c="dimmed">/</Text>
              <Text fw={500}>{title}</Text>
            </Group>
            <Group gap="sm">
              <Text size="sm" c="dimmed" onClick={() => spotlight.open()} style={{ cursor: 'pointer' }}>
                <Kbd>{isMac ? '⌘' : 'Ctrl'}</Kbd> + <Kbd>K</Kbd>
              </Text>
              <ActionIcon
                variant="default"
                size="lg"
                aria-label="Toggle theme"
                onClick={toggleTheme}
              >
                {scheme === 'light' ? '☾' : '☀'}
              </ActionIcon>
            </Group>
          </Group>
        </AppShell.Header>
        <AppShell.Navbar p="xs">
          {ROUTES.map((r) => (
            <NavLink
              key={r.id}
              label={r.title}
              active={r.id === route}
              onClick={() => setRoute(r.id)}
            />
          ))}
        </AppShell.Navbar>
        <AppShell.Main className="main">
          <Suspense
            fallback={
              <Center h="100%">
                <Loader />
              </Center>
            }
          >
            <Page route={route} />
          </Suspense>
        </AppShell.Main>
      </AppShell>
    </MantineProvider>
  );
}
