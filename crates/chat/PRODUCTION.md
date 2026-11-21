# Chat performance

The [long-history interaction test](src/gpui_tests.rs) checks that message insertion registers history rows in one list update per render and that Markdown state is created only for rows the list renders. Reasoning starts parsing when expanded.

Each rendered message retains one Kit text state so its selection can survive scrolling out of view. Each expanded reasoning panel can retain another. The upper bound is the number of messages and reasoning panels seen, so this retention can grow with history traversal. Kit also keeps one highlight result per parsed code block and, with Tree-sitter enabled, one parser and query per literal fence language per thread. Bounded history-wide shaping and query storage remain open.
