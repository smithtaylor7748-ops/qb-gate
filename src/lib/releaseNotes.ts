/**
 * 更新弹窗里的「更新内容」：把 `update.json` 的 `notes` 排成几块。
 *
 * 那段文字来自仓库里的 `.github/release-notes.md`（`scripts/update-manifest.mjs` 抽出中文那一节），
 * 写法约定只有三种行：`## ` 开头是小标题、`- ` 开头是一条、其余是一段话。
 *
 * **不引 Markdown 渲染器，也不 `dangerouslySetInnerHTML`** —— 这段文字是从网上取回来的，
 * 只当纯文本排版：`**粗体**`、`` `代码` ``、`[文字](链接)` 这几种标记剥掉只留文字，
 * 链接不做成可点的（「在 GitHub 上查看」那一个入口够了）。
 */
export type NoteBlock =
  | { kind: "heading"; text: string }
  | { kind: "item"; text: string }
  | { kind: "text"; text: string };

function plain(s: string): string {
  return s
    .replace(/\*\*(.+?)\*\*/g, "$1")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .trim();
}

export function parseNotes(notes: string): NoteBlock[] {
  const out: NoteBlock[] = [];
  for (const raw of notes.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith("<!--") || /^-{3,}$/.test(line)) continue;
    if (line.startsWith("#")) {
      const text = plain(line.replace(/^#+\s*/, ""));
      if (text) out.push({ kind: "heading", text });
    } else if (/^[-*]\s+/.test(line)) {
      const text = plain(line.replace(/^[-*]\s+/, ""));
      if (text) out.push({ kind: "item", text });
    } else {
      out.push({ kind: "text", text: plain(line) });
    }
  }
  return out;
}
