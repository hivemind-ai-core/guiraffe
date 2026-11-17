import { memo, use, useMemo } from 'react';
import ReactMarkdown, { type Options } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeShikiFromHighlighter from '@shikijs/rehype/core';
import { createHighlighterCore, type HighlighterCore } from 'shiki/core';
import { createJavaScriptRegexEngine } from 'shiki/engine/javascript';

// One highlighter for the whole app, created on first use (Document or Chat).
// Fine-grained bundle: only the three languages and two themes the content uses.
let highlighterPromise: Promise<HighlighterCore> | null = null;
export function getHighlighter(): Promise<HighlighterCore> {
  highlighterPromise ??= createHighlighterCore({
    themes: [import('shiki/themes/github-light.mjs'), import('shiki/themes/github-dark.mjs')],
    langs: [
      import('shiki/langs/rust.mjs'),
      import('shiki/langs/typescript.mjs'),
      import('shiki/langs/python.mjs'),
    ],
    engine: createJavaScriptRegexEngine(),
  });
  return highlighterPromise;
}

const remarkPlugins: Options['remarkPlugins'] = [remarkGfm];

/** Markdown with GFM and Shiki highlighting (dual light/dark themes, switched via CSS). */
export const Markdown = memo(function Markdown({ text }: { text: string }) {
  // Suspends until the highlighter is ready; callers provide a Suspense boundary.
  const highlighter = use(getHighlighter());
  const rehypePlugins = useMemo<Options['rehypePlugins']>(
    () => [
      [
        rehypeShikiFromHighlighter,
        highlighter,
        {
          themes: { light: 'github-light', dark: 'github-dark' },
          defaultColor: 'light',
          fallbackLanguage: 'text',
        },
      ],
    ],
    [highlighter],
  );
  return (
    <div className="markdown">
      <ReactMarkdown remarkPlugins={remarkPlugins} rehypePlugins={rehypePlugins}>
        {text}
      </ReactMarkdown>
    </div>
  );
});
