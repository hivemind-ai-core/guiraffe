import { memo, Suspense, useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { Button, Group, Text, Textarea } from '@mantine/core';
import { useVirtualizer } from '@tanstack/react-virtual';
import chat from '@shared-fixtures/chat.json';
import { Markdown } from '../markdown';
import { register } from '../benchHooks';

interface Message {
  id: number;
  role: 'user' | 'assistant';
  text: string;
}

const TOKEN_MS = 20;

/** "Split after each space": "a b c" → ["a ", "b ", "c"]. */
export function tokenize(text: string): string[] {
  return text.split(/(?<= )/);
}

const REPLY_TOKENS = tokenize(chat.reply);
const HISTORY: Message[] = chat.history.map((m, i) => ({
  id: i,
  role: m.role === 'user' ? 'user' : 'assistant',
  text: m.text,
}));

const MessageView = memo(function MessageView({ message }: { message: Message }) {
  return (
    <div className="chat-msg">
      <div className={`chat-bubble ${message.role}`}>
        {message.role === 'user' ? (
          <Text style={{ whiteSpace: 'pre-wrap' }} py={8}>
            {message.text}
          </Text>
        ) : (
          <Suspense fallback={<Text style={{ whiteSpace: 'pre-wrap' }}>{message.text}</Text>}>
            <Markdown text={message.text} />
          </Suspense>
        )}
      </div>
    </div>
  );
});

export default function ChatPage() {
  const [messages, setMessages] = useState<Message[]>(HISTORY);
  const [draft, setDraft] = useState('');
  const [streaming, setStreaming] = useState(false);
  const timer = useRef<ReturnType<typeof setInterval> | null>(null);
  const tokens = useRef(0);
  const nextId = useRef(HISTORY.length);
  const scrollRef = useRef<HTMLDivElement>(null);

  const virtualizer = useVirtualizer({
    count: messages.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 140,
    getItemKey: (i) => messages[i].id,
    overscan: 4,
  });

  const stop = useCallback(() => {
    if (timer.current !== null) clearInterval(timer.current);
    timer.current = null;
    setStreaming(false);
  }, []);

  const send = useCallback(
    (text: string) => {
      const trimmed = text.trim();
      if (!trimmed || timer.current !== null) return;
      const userMsg: Message = { id: nextId.current++, role: 'user', text: trimmed };
      const replyId = nextId.current++;
      setMessages((prev) => [...prev, userMsg, { id: replyId, role: 'assistant', text: '' }]);
      setDraft('');
      setStreaming(true);
      let k = 0;
      timer.current = setInterval(() => {
        if (k >= REPLY_TOKENS.length) {
          stop();
          return;
        }
        const token = REPLY_TOKENS[k++];
        tokens.current++;
        setMessages((prev) => {
          const last = prev[prev.length - 1];
          if (last.id !== replyId) return prev;
          return [...prev.slice(0, -1), { ...last, text: last.text + token }];
        });
      }, TOKEN_MS);
    },
    [stop],
  );

  useEffect(() => () => stop(), [stop]);

  // Stick to the bottom: on mount, and as messages arrive / grow.
  useLayoutEffect(() => {
    virtualizer.scrollToIndex(messages.length - 1, { align: 'end' });
  }, [messages, virtualizer]);

  useEffect(() => {
    register('chat', { send, tokensStreamed: () => tokens.current });
    return () => register('chat', undefined);
  }, [send]);

  return (
    <div className="page">
      <div className="chat-scroll" ref={scrollRef}>
        <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
          {virtualizer.getVirtualItems().map((item) => (
            <div
              key={item.key}
              data-index={item.index}
              ref={virtualizer.measureElement}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                transform: `translateY(${item.start}px)`,
              }}
            >
              <MessageView message={messages[item.index]} />
            </div>
          ))}
        </div>
      </div>
      <Group align="flex-end" gap="sm">
        <Textarea
          style={{ flex: 1 }}
          placeholder="Send a message…"
          autosize
          minRows={1}
          maxRows={5}
          value={draft}
          onChange={(e) => setDraft(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault();
              send(draft);
            }
          }}
        />
        {streaming ? (
          <Button color="red" variant="light" onClick={stop}>
            Stop
          </Button>
        ) : (
          <Button onClick={() => send(draft)}>Send</Button>
        )}
      </Group>
    </div>
  );
}
