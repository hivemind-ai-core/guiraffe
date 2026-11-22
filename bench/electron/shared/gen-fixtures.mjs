// Deterministic fixtures shared by both comparison apps (run: node gen-fixtures.mjs).
// Writes document.md (2,000 lines with highlighted code blocks) and chat.json
// (200-message history + a long streamed reply). The 100k-row grid data is generated at
// runtime by the same seeded mulberry32 generator in both apps (see README.md).
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));

function mulberry32(a) {
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return (t ^ (t >>> 14)) >>> 0;
  };
}
const rnd = mulberry32(7);
const pick = (xs) => xs[rnd() % xs.length];
const WORDS =
  "the frame layout render scroll cache index buffer thread queue vector glyph shader window event state store route theme token stream parser block table column row cell sort filter batch commit async future signal effect memo paint atlas texture pixel element view model update notify measure".split(
    " ",
  );
const sentence = (n) => {
  const w = Array.from({ length: n }, () => pick(WORDS));
  w[0] = w[0][0].toUpperCase() + w[0].slice(1);
  return w.join(" ") + ".";
};

const CODE = {
  rust: [
    "fn layout(rows: &[Row], width: f32) -> Vec<Rect> {",
    "    let mut y = 0.0;",
    "    rows.iter()",
    "        .map(|r| {",
    "            let h = r.measure(width);",
    "            let rect = Rect::new(0.0, y, width, h);",
    "            y += h;",
    "            rect",
    "        })",
    "        .collect()",
    "}",
  ],
  typescript: [
    "export function layout(rows: Row[], width: number): Rect[] {",
    "  let y = 0;",
    "  return rows.map((r) => {",
    "    const h = r.measure(width);",
    "    const rect = { x: 0, y, width, height: h };",
    "    y += h;",
    "    return rect;",
    "  });",
    "}",
  ],
  python: [
    "def layout(rows, width):",
    "    y = 0.0",
    "    out = []",
    "    for r in rows:",
    "        h = r.measure(width)",
    "        out.append((0.0, y, width, h))",
    "        y += h",
    "    return out",
  ],
};

// Build whole sections until the next one would not fit in 2,000 lines, then pad.
const lines = ["# Rendering pipeline handbook", ""];
for (let section = 1; ; section++) {
  const s = [`## ${section}. ${sentence(4).slice(0, -1)}`, ""];
  s.push(sentence(14) + " " + sentence(12), "");
  s.push(`- ${sentence(6)}`, `- ${sentence(7)}`, `- **${pick(WORDS)}**: ${sentence(5)}`, "");
  const lang = ["rust", "typescript", "python"][section % 3];
  s.push("```" + lang, ...CODE[lang], "```", "");
  if (section % 4 === 0) {
    s.push("| stage | time | notes |", "|---|---:|---|");
    for (let i = 0; i < 3; i++) s.push(`| ${pick(WORDS)} | ${(rnd() % 900) / 100} ms | ${sentence(3)} |`);
    s.push("");
  }
  s.push(`> ${sentence(10)}`, "");
  if (lines.length + s.length > 2000) break;
  lines.push(...s);
}
while (lines.length < 2000) lines.push(lines.length % 2 ? sentence(9) : "");
writeFileSync(join(here, "document.md"), lines.join("\n") + "\n");

const history = [];
for (let i = 0; i < 200; i++) {
  const user = i % 2 === 0;
  let text = user ? sentence(8 + (rnd() % 10)) : `${sentence(12)} ${sentence(10)}`;
  if (!user && i % 10 === 1) text += "\n\n```rust\n" + CODE.rust.slice(0, 5).join("\n") + "\n```";
  if (!user && i % 6 === 3) text += `\n\n1. ${sentence(5)}\n2. ${sentence(6)}`;
  history.push({ role: user ? "user" : "assistant", text });
}
const reply = [];
for (let p = 0; p < 40; p++) {
  reply.push(`${sentence(10)} ${sentence(9)}`);
  if (p % 8 === 4) reply.push("```rust\n" + CODE.rust.join("\n") + "\n```");
}
writeFileSync(
  join(here, "chat.json"),
  JSON.stringify({ history, reply: reply.join("\n\n") }, null, 1) + "\n",
);
console.log(
  "document.md lines:",
  lines.length,
  "chat messages:",
  history.length,
  "reply tokens:",
  reply.join(" ").split(" ").length,
);
