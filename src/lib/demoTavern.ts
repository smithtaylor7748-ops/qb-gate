/**
 * 酒馆那几页的演示数据（`npm run demo` 与 `test:ui` 吃这一份，2026-09-29）：软件页 SillyTavern 卡的版本清单，
 * 账户页「酒馆」弹窗的控制台、角色卡、世界书、扩展、回收站（第三轮之前都在软件页的卡上）。
 *
 * 数字、名字全是编的；删除 / 恢复 / 导入只改这里的内存状态，什么都不写。
 * 演示专用的开关（真实面板不读）：`qb-demo-tavern-console-error` = `1` → 控制台里是一次「端口被占」的失败启动。
 * 只被 `demo.ts` 引用，正式包里整个摇掉。
 */
import type { BookEntry } from "./generated/BookEntry";
import type { CharacterCard } from "./generated/CharacterCard";
import type { CharacterDetail } from "./generated/CharacterDetail";
import type { ConsoleChunk } from "./generated/ConsoleChunk";
import type { ExtensionCheck } from "./generated/ExtensionCheck";
import type { ImportReport } from "./generated/ImportReport";
import type { TavernExtension } from "./generated/TavernExtension";
import type { TavernExtensionList } from "./generated/TavernExtensionList";
import type { TavernRelease } from "./generated/TavernRelease";
import type { TavernReleaseNote } from "./generated/TavernReleaseNote";
import type { TrashEntry } from "./generated/TrashEntry";
import type { WorldDetail } from "./generated/WorldDetail";
import type { WorldInfo } from "./generated/WorldInfo";

type Fixture = (args?: Record<string, unknown>) => unknown;

export interface DemoTavernHooks {
  home: string;
  /** 面板装的那份现在是哪一版、留着的是哪一版。 */
  get: () => { managed: boolean; version: string; previous: string | null };
  set: (s: {
    managed: boolean;
    version: string;
    previous: string | null;
  }) => void;
  noNode: () => boolean;
  flag: (key: string) => boolean;
}

// ------------------------------------------------------------------ 版本

const RELEASES: TavernReleaseNote[] = [
  {
    tag: "1.18.1",
    version: "1.18.1",
    date: "2026-09-14",
    notes:
      "SillyTavern 1.18.1\n\nBackends\n• Anthropic: 支持更多模型（演示）。\n• OpenAI: 模型清单刷新（演示）。\n\nImprovements\n• 世界书：排序支持升序与降序（演示）。",
    page_url: "https://github.com/SillyTavern/SillyTavern/releases/tag/1.18.1",
  },
  {
    tag: "1.18.0",
    version: "1.18.0",
    date: "2026-05-03",
    notes: "SillyTavern 1.18.0\n\n• 演示用的更新说明。",
    page_url: "https://github.com/SillyTavern/SillyTavern/releases/tag/1.18.0",
  },
  {
    tag: "1.17.0",
    version: "1.17.0",
    date: "2026-03-28",
    notes: "SillyTavern 1.17.0\n\n• 从这一版起要 Node.js 20（演示）。",
    page_url: "https://github.com/SillyTavern/SillyTavern/releases/tag/1.17.0",
  },
  {
    tag: "1.16.0",
    version: "1.16.0",
    date: "2026-03-25",
    notes: "",
    page_url: "https://github.com/SillyTavern/SillyTavern/releases/tag/1.16.0",
  },
];

function releaseOf(tag: string, noNode: boolean): TavernRelease {
  if (!/^v?\d+\.\d+\.\d+$/.test(tag.trim()))
    throw new Error(
      `「${tag}」不是 SillyTavern 的正式版标签（长这样：1.19.0）`,
    );
  const t = tag.trim();
  return {
    tag: t,
    version: t.replace(/^v/, ""),
    node_engine: ">= 20",
    node_ok: !noNode,
    page_url: `https://github.com/SillyTavern/SillyTavern/releases/tag/${t}`,
    archive_url: `https://github.com/SillyTavern/SillyTavern/archive/refs/tags/${t}.zip`,
  };
}

// ------------------------------------------------------------------ 控制台

const OK_LOG = [
  "Node version: v24.14.1. Running in production environment. Server directory: C:\\Users\\demo\\SillyTavern",
  "",
  "SillyTavern 1.18.0",
  "",
  "\u001b[32mIPv4 support detected\u001b[39m",
  "Warning: failed to start server on IPv6",
  "SillyTavern is listening on\u001b[32m IPv4: 127.0.0.1:8000\u001b[39m",
  "",
  "============================================",
  "",
  "Go to: \u001b[34mhttp://127.0.0.1:8000/\u001b[39m to open SillyTavern",
  "",
  "============================================",
];

const ERROR_LOG = [
  "Node version: v24.14.1. Running in production environment.",
  "SillyTavern 1.18.0",
  "Warning: failed to start server on IPv4",
  '\u001b[31mAddress 127.0.0.1:8000 is already in use. Another SillyTavern instance may already be running. Stop the other process or change "port" in config.yaml.\u001b[39m',
  "\u001b[31mBoth IPv6 and IPv4 are disabled or not detected\u001b[39m",
];

function consoleChunk(
  home: string,
  flag: (k: string) => boolean,
  args?: Record<string, unknown>,
): ConsoleChunk {
  const error = flag("qb-demo-tavern-console-error");
  const previous = !!args?.previous;
  const lines = previous ? OK_LOG.slice(0, 6) : error ? ERROR_LOG : OK_LOG;
  const run = previous ? "demo-run-0" : error ? "demo-run-err" : "demo-run-1";
  const same = args?.run === run;
  // 演示里的「实时」：每读一次多给几行，直到读完。
  const from = same ? Number(args?.offset ?? 0) : 0;
  const to = Math.min(lines.length, from + 4);
  const chunk = lines.slice(from, to);
  const hints =
    error && chunk.some((l) => l.includes("already in use"))
      ? [
          {
            id: "port-in-use",
            title: "酒馆端口被占了",
            detail:
              "别的程序（多半是另一份酒馆）占着这个端口。关掉它；或者换一个端口 —— 它自己 config.yaml 里的 port 与面板这边「位置与端口」里的酒馆端口要一起改。",
            line: "Address 127.0.0.1:8000 is already in use. Another SillyTavern instance may already be running.",
          },
        ]
      : [];
  return {
    exists: true,
    run,
    started_at: previous ? "2026-09-28 21:10:03" : "2026-09-29 20:12:44",
    offset: to,
    lines: chunk,
    truncated: false,
    more: to < lines.length,
    path: `${home}\\AppData\\Local\\ClaudeIpGate\\plugins\\sillytavern-console${previous ? ".prev" : ""}.log`,
    running: !previous && !error && to >= lines.length,
    hints,
  };
}

// ------------------------------------------------------------------ 角色卡

const BOOK: BookEntry[] = [
  {
    uid: "0",
    comment: "馆藏",
    keys: ["书", "藏书"],
    secondary: [],
    content: "图书馆藏书十万册，三楼是古籍区，只有林澈有钥匙。",
    constant: false,
    disabled: false,
    order: 100,
  },
  {
    uid: "1",
    comment: "北门",
    keys: ["门"],
    secondary: ["夜里"],
    content: "北门夜里十点上锁。",
    constant: true,
    disabled: false,
    order: 200,
  },
];

function card(
  file: string,
  name: string,
  extra: Partial<CharacterCard> = {},
): CharacterCard {
  return {
    file,
    name,
    creator: "demo",
    version: "1.0",
    spec: "v2",
    tags: [],
    size: 480_000,
    modified: "2026-09-20 21:04",
    chats: 0,
    book_entries: null,
    problem: null,
    ...extra,
  };
}

// ------------------------------------------------------------------ 世界书

function world(file: string, entries: number, enabled: number): WorldInfo {
  return {
    file,
    name: file.replace(/\.json$/, ""),
    entries,
    enabled,
    size: 12_000 + entries * 300,
    modified: "2026-09-18 10:22",
    problem: null,
  };
}

// ------------------------------------------------------------------ 扩展

function ext(
  home: string,
  name: string,
  extra: Partial<TavernExtension> = {},
): TavernExtension {
  return {
    name,
    scope: "user",
    path: `${home}\\SillyTavern\\data\\default-user\\extensions\\${name}`,
    display_name: name,
    version: "1.0.0",
    author: "demo",
    home_page: "",
    has_manifest: true,
    git: true,
    remote: `https://github.com/example/${name}`,
    branch: "main",
    commit: "a1b2c3d",
    disabled: false,
    modified: "2026-09-10 12:00",
    ...extra,
  };
}

// ------------------------------------------------------------------ 组装

export function makeTavernFixtures(
  h: DemoTavernHooks,
): Record<string, Fixture> {
  const chars: CharacterCard[] = [
    card("林澈.png", "林澈", {
      tags: ["原创", "图书馆", "温和"],
      chats: 3,
      book_entries: 2,
    }),
    card("Seraphina.png", "Seraphina", {
      spec: "v3",
      creator: "example",
      version: "2.1",
      tags: ["fantasy"],
      chats: 12,
    }),
    card("旧卡.png", "旧卡", { spec: "v1", creator: "", version: "" }),
    card("broken.png", "broken", {
      spec: "",
      creator: "",
      version: "",
      problem: "这张 PNG 里没有角色卡数据（没有 chara / ccv3 文本块）",
    }),
  ];
  const worlds: WorldInfo[] = [
    world("图书馆.json", 2, 2),
    world("大陆地理.json", 48, 41),
    { ...world("残缺.json", 0, 0), problem: "JSON 读不懂：EOF while parsing" },
  ];
  const exts: TavernExtension[] = [
    ext(h.home, "st-memory-plus", {
      display_name: "记忆增强（演示）",
      version: "2.3.1",
      home_page: "https://github.com/example/st-memory-plus",
    }),
    ext(h.home, "quick-replies-pro", {
      display_name: "快捷回复 Pro（演示）",
      disabled: true,
    }),
    ext(h.home, "global-theme", {
      scope: "global",
      display_name: "全局主题（演示）",
      path: `${h.home}\\SillyTavern\\public\\scripts\\extensions\\third-party\\global-theme`,
      git: false,
      remote: "",
      branch: "",
      commit: "",
    }),
    ext(h.home, "leftover-folder", {
      display_name: "leftover-folder",
      has_manifest: false,
      git: false,
      remote: "",
      branch: "",
      commit: "",
      version: "",
      author: "",
    }),
  ];
  const trash: TrashEntry[] = [];
  // 被挪进回收站的东西本身，恢复时放回原处。
  const held = new Map<string, () => void>();
  let seq = 0;

  const put = (
    kind: string,
    name: string,
    originals: string[],
    size: number,
    restore: () => void,
  ): TrashEntry => {
    seq += 1;
    const id = `20260929-2015${String(seq).padStart(2, "0")}-${kind}-1`;
    const entry: TrashEntry = {
      id,
      kind,
      name,
      originals,
      deleted_at: "2026-09-29 20:15",
      size,
    };
    trash.unshift(entry);
    held.set(id, restore);
    return entry;
  };

  const importReport = (
    files: unknown,
    accept: RegExp,
    add: (name: string) => void,
    why: string,
  ): ImportReport => {
    const list = Array.isArray(files) ? files.map(String) : [];
    const report: ImportReport = { imported: [], skipped: [] };
    for (const f of list) {
      const name = f.split(/[\\/]/).pop() ?? f;
      if (!accept.test(name)) {
        report.skipped.push(`${name} —— ${why}`);
        continue;
      }
      add(name);
      report.imported.push(name);
    }
    return report;
  };

  return {
    tavern_releases: () => RELEASES,
    tavern_release: (args) => releaseOf(String(args?.tag ?? ""), h.noNode()),
    tavern_swap_previous: (args) => {
      const s = h.get();
      if (!s.managed || !s.previous)
        throw new Error("没有留着的那一版（sillytavern.previous 不在）。");
      h.set({ managed: true, version: s.previous, previous: s.version });
      return `演示：SillyTavern ${s.previous} 已换好；从 ${s.version} 换回了 ${s.previous}；${
        args?.carryData
          ? "带过去了现在的数据（1234 个文件）"
          : "用的是留着那份自己的数据"
      }`;
    },
    tavern_console: (args) => consoleChunk(h.home, h.flag, args),

    // ⚠ 列表一律回副本：回同一个被原地改过的数组，查询缓存的结构共享会判成「没变」，界面不重画。
    tavern_characters: () => chars.map((c) => ({ ...c })),
    tavern_character_detail: (args): CharacterDetail => {
      const file = String(args?.file ?? "");
      const c = chars.find((x) => x.file === file);
      if (!c) throw new Error(`路径不存在: ${file}`);
      const lin = c.name === "林澈";
      return {
        card: c,
        description: lin
          ? "林澈是城东图书馆的管理员，说话慢条斯理，记得每一本书放在哪。"
          : "演示用的角色描述。",
        personality: lin ? "温和、机敏、有点较真" : "",
        scenario: lin ? "深夜的图书馆，只剩你们两个人。" : "",
        first_mes: lin
          ? "林澈从一排排书架间抬起头：「这么晚了，还要借书吗？」"
          : "你好。",
        mes_example: "",
        creator_notes: lin ? "演示卡，适合测试长篇回复。" : "",
        system_prompt: "",
        post_history_instructions: "",
        alternate_greetings: lin
          ? ["「欢迎回来。」", "「今天想看点什么？」"]
          : [],
        book_name: lin ? "馆藏" : "",
        book: lin ? BOOK : [],
        image: null,
      };
    },
    tavern_character_thumbnail: () => null,
    tavern_characters_import: (args) =>
      importReport(
        args?.files,
        /\.png$/i,
        (name) =>
          chars.push(
            card(name, name.replace(/\.png$/i, ""), {
              modified: "2026-09-29 20:20",
            }),
          ),
        "只收 PNG 角色卡；JSON 卡、.charx 请在酒馆里导入（它会配一张图）",
      ),
    tavern_character_delete: (args) => {
      const file = String(args?.file ?? "");
      const i = chars.findIndex((x) => x.file === file);
      if (i < 0) throw new Error(`路径不存在: ${file}`);
      const [c] = chars.splice(i, 1);
      const originals = [
        `${h.home}\\SillyTavern\\data\\default-user\\characters\\${c.file}`,
      ];
      if (args?.withChats && c.chats)
        originals.push(
          `${h.home}\\SillyTavern\\data\\default-user\\chats\\${c.file.replace(/\.png$/i, "")}`,
        );
      return put("character", c.name, originals, c.size, () => {
        chars.push(c);
        chars.sort((a, b) => a.name.localeCompare(b.name));
      });
    },

    tavern_worlds: () => worlds.map((w) => ({ ...w })),
    tavern_world_detail: (args): WorldDetail => {
      const file = String(args?.file ?? "");
      const w = worlds.find((x) => x.file === file);
      if (!w) throw new Error(`路径不存在: ${file}`);
      if (w.problem) throw new Error(w.problem);
      const entries: BookEntry[] =
        w.name === "图书馆"
          ? BOOK
          : Array.from({ length: w.entries }, (_, i) => ({
              uid: String(i),
              comment: `地区 ${i + 1}`,
              keys: [`地名${i + 1}`],
              secondary: [],
              content: `第 ${i + 1} 个地区的设定（演示）。`,
              constant: i === 0,
              disabled: i >= w.enabled,
              order: 100 + i,
            }));
      return { info: w, entries };
    },
    tavern_worlds_import: (args) =>
      importReport(
        args?.files,
        /\.json$/i,
        (name) => worlds.push(world(name, 1, 1)),
        "只收 .json 世界书",
      ),
    tavern_world_delete: (args) => {
      const file = String(args?.file ?? "");
      const i = worlds.findIndex((x) => x.file === file);
      if (i < 0) throw new Error(`路径不存在: ${file}`);
      const [w] = worlds.splice(i, 1);
      return put(
        "world",
        w.name,
        [`${h.home}\\SillyTavern\\data\\default-user\\worlds\\${w.file}`],
        w.size,
        () => worlds.push(w),
      );
    },

    tavern_extensions: (): TavernExtensionList => ({
      git: "C:\\Program Files\\Git\\cmd\\git.exe",
      items: exts.map((e) => ({ ...e })),
    }),
    tavern_extension_check: (args): ExtensionCheck => {
      const name = String(args?.name ?? "");
      const e = exts.find((x) => x.name === name);
      if (!e?.git)
        return {
          name,
          scope: e?.scope ?? "user",
          behind: null,
          detail: "不是 git 克隆来的，面板没法检查更新（它多半是解压进来的）",
        };
      const behind = name === "st-memory-plus" ? 2 : 0;
      return {
        name,
        scope: e.scope,
        behind,
        detail: behind ? `远端多了 ${behind} 个提交` : "已经是最新的",
      };
    },
    tavern_extension_update: (args) => {
      const e = exts.find((x) => x.name === String(args?.name ?? ""));
      if (!e) throw new Error("路径不存在");
      e.commit = "f00dbab";
      return "已更新到 f00dbab（main）";
    },
    tavern_extension_install: (args) => {
      const url = String(args?.url ?? "").trim();
      const m =
        /^https:\/\/[^/@\s]+\/(?:[^/\s]+\/)*([^/\s]+?)(?:\.git)?\/?$/.exec(url);
      if (!m) throw new Error("只收 https:// 开头的仓库地址");
      if (exts.some((x) => x.name === m[1]))
        throw new Error(`已经有一个叫 ${m[1]} 的扩展了`);
      exts.push(ext(h.home, m[1], { remote: url }));
      return `装好了 ${m[1]}；刷新一下酒馆的页面就能看到`;
    },
    tavern_extension_delete: (args) => {
      const name = String(args?.name ?? "");
      const i = exts.findIndex((x) => x.name === name);
      if (i < 0) throw new Error(`路径不存在: ${name}`);
      const [e] = exts.splice(i, 1);
      return put(
        "extension",
        `${e.scope === "global" ? "（全局）" : ""}${e.name}`,
        [e.path],
        2_300_000,
        () => exts.push(e),
      );
    },

    tavern_trash: () => trash.map((t) => ({ ...t })),
    tavern_trash_restore: (args) => {
      const id = String(args?.id ?? "");
      const i = trash.findIndex((x) => x.id === id);
      if (i < 0) throw new Error(`路径不存在: 回收站里没有 ${id}`);
      const [entry] = trash.splice(i, 1);
      held.get(id)?.();
      held.delete(id);
      return { ...entry, size: 0 };
    },
    tavern_trash_purge: (args) => {
      const id = args?.id ? String(args.id) : null;
      const n = id ? trash.filter((x) => x.id === id).length : trash.length;
      for (let i = trash.length - 1; i >= 0; i--)
        if (!id || trash[i].id === id) {
          held.delete(trash[i].id);
          trash.splice(i, 1);
        }
      return n;
    },
  };
}
