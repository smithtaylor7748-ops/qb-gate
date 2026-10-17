/**
 * 「酒馆」弹窗的「控制台」分页（2026-09-29，使用者选的「实时控制台日志」；第二轮在软件页的 SillyTavern 卡上，
 * 第三轮对调进了账户页酒馆磁贴右上角的弹窗）。
 *
 * 面板起的那份酒馆没有窗口（`CREATE_NO_WINDOW`），它的输出现在进 `state_dir\plugins\sillytavern-console.log`，
 * 这里按偏移量增量读（`tavern_console`）：在跑时每 1.2 秒读一次，没在跑时 5 秒一次（等你从账户页起它）；
 * 分页不在前台、窗口最小化时不读。读的是本机文件，**不联网**。
 *
 * - 颜色：SillyTavern 自己打的 ANSI 颜色码按 `lib/ansi.ts` 解析，映射到主题里的颜色；
 * - 认得出的报错（端口被占、没有 Node、npm 连不上……）在日志上方各给一句能照着做的话（后端 `tavern_console::hints_for`）；
 * - 「上一次」：每次起新的一份时上一份日志留成 `.prev`，起不来的时候最有用。
 *
 * ⛔ 内存里最多留 5000 行，更早的丢掉（文件里还在，路径写在底下）。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { Check, Copy, Eraser } from "lucide-react";

import { api, type ConsoleHint } from "../../../lib/api";
import { parseAnsi, stripAnsi } from "../../../lib/ansi";
import { Button, Checkbox, Pill } from "../../../ui";
import { errorText } from "./common";

const KEEP = 5000;
const FAST_MS = 1200;
const SLOW_MS = 5000;

interface View {
  exists: boolean;
  run: string | null;
  offset: number;
  startedAt: string;
  path: string;
  running: boolean;
  truncated: boolean;
  lines: string[];
  hints: ConsoleHint[];
}

const EMPTY: View = {
  exists: false,
  run: null,
  offset: 0,
  startedAt: "",
  path: "",
  running: false,
  truncated: false,
  lines: [],
  hints: [],
};

export default function Console() {
  const [previous, setPrevious] = useState(false);
  const [view, setView] = useState<View>(EMPTY);
  const [error, setError] = useState("");
  const [follow, setFollow] = useState(true);
  const [loaded, setLoaded] = useState(false);
  const [copied, setCopied] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const cursor = useRef<{ run: string | null; offset: number }>({
    run: null,
    offset: 0,
  });

  /** 读一次；`more` 时立刻接着读。返回下一次该隔多久再读。 */
  const pull = useCallback(async (prev: boolean): Promise<number> => {
    for (let round = 0; round < 8; round++) {
      const c = await api.tavernConsole(
        prev,
        cursor.current.run,
        cursor.current.offset,
      );
      const fresh = c.run !== cursor.current.run;
      cursor.current = { run: c.run, offset: c.offset };
      setView((v) => {
        const base = fresh ? EMPTY : v;
        const lines = [...base.lines, ...c.lines];
        const hints = [...base.hints];
        for (const h of c.hints)
          if (!hints.some((x) => x.id === h.id)) hints.push(h);
        return {
          exists: c.exists,
          run: c.run,
          offset: c.offset,
          startedAt: c.started_at,
          path: c.path,
          running: c.running,
          truncated: fresh ? c.truncated : base.truncated,
          lines: lines.length > KEEP ? lines.slice(lines.length - KEEP) : lines,
          hints,
        };
      });
      if (!c.more) return c.running ? FAST_MS : SLOW_MS;
    }
    return FAST_MS;
  }, []);

  useEffect(() => {
    let alive = true;
    let timer: number | undefined;
    cursor.current = { run: null, offset: 0 };
    setView(EMPTY);
    setLoaded(false);
    setError("");
    const tick = async () => {
      let wait = SLOW_MS;
      if (document.visibilityState !== "hidden") {
        try {
          wait = await pull(previous);
          if (alive) setError("");
        } catch (e) {
          if (alive) setError(errorText(e));
        }
        if (alive) setLoaded(true);
      }
      // 上一次那份不会再变，读完就不用再读了。
      if (alive && !previous)
        timer = window.setTimeout(() => void tick(), wait);
    };
    void tick();
    return () => {
      alive = false;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [previous, pull]);

  useEffect(() => {
    if (!follow) return;
    const el = box.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [view.lines, follow]);

  async function copyAll() {
    try {
      await navigator.clipboard.writeText(view.lines.map(stripAnsi).join("\n"));
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      /* 剪贴板不给用就算了 */
    }
  }

  return (
    <div className="tv-console-wrap" data-testid="tavern-console">
      <div className="tv-toolbar">
        <div className="tv-seg" role="group" aria-label="看哪一次的日志">
          <button
            type="button"
            aria-pressed={!previous}
            onClick={() => setPrevious(false)}
          >
            这一次
          </button>
          <button
            type="button"
            aria-pressed={previous}
            onClick={() => setPrevious(true)}
          >
            上一次
          </button>
        </div>
        {view.exists && !previous && (
          <Pill tone={view.running ? "ok" : "default"}>
            {view.running ? "在跑" : "已停"}
          </Pill>
        )}
        {view.startedAt && (
          <span className="notice">起于 {view.startedAt}</span>
        )}
        <span className="notice">{view.lines.length} 行</span>
        <span className="tv-toolbar-end">
          <Checkbox checked={follow} onChange={setFollow}>
            跟到最后一行
          </Checkbox>
          <Button
            size="sm"
            variant="ghost"
            icon={<Eraser size={12} />}
            disabled={!view.lines.length}
            onClick={() => setView((v) => ({ ...v, lines: [], hints: [] }))}
          >
            清空显示
          </Button>
          <Button
            size="sm"
            variant="ghost"
            icon={copied ? <Check size={12} /> : <Copy size={12} />}
            disabled={!view.lines.length}
            onClick={() => void copyAll()}
          >
            {copied ? "已复制" : "复制全部"}
          </Button>
        </span>
      </div>

      {error && <p className="notice notice--danger mt-2">{error}</p>}

      {view.hints.length > 0 && (
        <ul className="tv-hints mt-2" data-testid="tavern-console-hints">
          {view.hints.map((h) => (
            <li key={h.id} className="tv-hint">
              <strong>{h.title}</strong>
              <span>{h.detail}</span>
              <code className="tv-hint-line">{h.line}</code>
            </li>
          ))}
        </ul>
      )}

      {view.truncated && (
        <p className="notice mt-2">
          日志太长，只从最后一段开始显示；完整的在下面那个文件里。
        </p>
      )}

      <div className="tv-console mt-2" ref={box} role="log" aria-live="off">
        {!loaded ? (
          <span className="notice">读取中…</span>
        ) : !view.exists ? (
          <span className="notice">
            {previous
              ? "没有上一次的日志（面板起过两次酒馆之后才有）。"
              : "面板还没起过酒馆。从账户页的「酒馆」磁贴起一次，它的输出就会出现在这里；你自己双击起的那份，输出在它自己的窗口里。"}
          </span>
        ) : view.lines.length === 0 ? (
          <span className="notice">（还没有输出）</span>
        ) : (
          view.lines.map((line, i) => (
            <div key={i} className="tv-console-line">
              {parseAnsi(line).map((seg, j) => (
                <span
                  key={j}
                  className={
                    `ansi-${seg.tone}` + (seg.bold ? " ansi-bold" : "")
                  }
                >
                  {seg.text || " "}
                </span>
              ))}
            </div>
          ))
        )}
      </div>

      {view.path && (
        <p className="notice mt-1 break-all">
          日志文件：<span className="mono">{view.path}</span>
          （只在本机；酒馆打印的请求内容也会在里面，面板不上传它）
        </p>
      )}
    </div>
  );
}
