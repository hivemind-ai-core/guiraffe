import { Suspense } from 'react';
import { Center, Loader, ScrollArea } from '@mantine/core';
import documentMd from '@shared-fixtures/document.md?raw';
import { Markdown } from '../markdown';

export default function DocsPage() {
  return (
    <div className="page">
      <ScrollArea style={{ flex: 1, minHeight: 0 }} type="auto" offsetScrollbars>
        <Suspense
          fallback={
            <Center p="xl">
              <Loader />
            </Center>
          }
        >
          <div style={{ maxWidth: 900 }}>
            <Markdown text={documentMd} />
          </div>
        </Suspense>
      </ScrollArea>
    </div>
  );
}
