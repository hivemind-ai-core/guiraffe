export type RouteId = '/' | '/grid' | '/docs' | '/chat' | '/form';

export const ROUTES: { id: RouteId; title: string }[] = [
  { id: '/', title: 'Dashboard' },
  { id: '/grid', title: 'Data grid' },
  { id: '/docs', title: 'Document' },
  { id: '/chat', title: 'Chat' },
  { id: '/form', title: 'Form' },
];
