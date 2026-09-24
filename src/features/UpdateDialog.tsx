import { useEffect, useState, type ReactNode } from "react";
import { ArrowUpCircle, Download } from "lucide-react";
import { api, type UpdateStatus } from "../lib/api";
import { parseNotes, type NoteBlock } from "../lib/releaseNotes";
import { setSession, useSession } from "../lib/store";
import { endTask, resetTask, useTask } from "../lib/tasks";
import { Button, ExternalLink, Modal, ProgressBar } from "../ui";

/**
 * 「发现新版本」弹窗 + 启动时那一次检查（0.25.3，使用者要的：打开软件时弹窗提醒、一键更新）。
 *
 * **全局只挂一份**（`Shell` 里）。设置页的「检查更新」查到新版时用 [`openUpdateDialog`]
 * 喊它，不自己再挂一个 —— 两份弹窗迟早各说各的。
 *
 * 规矩（CLAUDE.md「应用自己的更新」）：
 * - 只提醒，**不自动装**：装要使用者点「一键更新」；
 * - 代价常驻在按钮上方，不放悬停 —— 装的时候面板会退出，它起的桌面端、对话、酒馆一起关；
 * - 启动检查失败**不打扰人**（没有弹窗、没有 toast），原因在设置页「软件更新」那一行。
 */
const KEY = "update.dialog";

/** 让弹窗显示这份状态（设置页「检查更新」查到新版时调）。 */
export function openUpdateDialog(status: UpdateStatus): void {
  setSession<UpdateStatus | null>(KEY, status);
}

/** 启动检查只在面板进程的生命周期里跑一次：Layout 重挂不再问。 */
let startupChecked = false;

/** 连续的条目收进一个 `<ul>`，读屏才报得出「列表，N 项」。 */
function Notes({ blocks }: { blocks: NoteBlock[] }) {
  const out: ReactNode[] = [];
  let items: string[] = [];
  const flush = (key: string) => {
    if (!items.length) return;
    out.push(
      <ul key={key}>
        {items.map((t, i) => (
          <li key={i}>{t}</li>
        ))}
      </ul>,
    );
    items = [];
  };
  blocks.forEach((b, i) => {
    if (b.kind === "item") {
      items.push(b.text);
      return;
    }
    flush(`list-${i}`);
    out.push(
      b.kind === "heading" ? (
        <h4 key={i}>{b.text}</h4>
      ) : (
        <p key={i}>{b.text}</p>
      ),
    );
  });
  flush("list-end");
  return (
    <div className="qb-update-notes" aria-label="更新内容">
      {out}
    </div>
  );
}

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

function when(iso: string | null): string {
  if (!iso) return "";
  const d = new Date(iso);
  return Number.isNaN(d.getTime())
    ? ""
    : d.toLocaleDateString("zh-CN", {
        year: "numeric",
        month: "long",
        day: "numeric",
      });
}

export default function UpdateDialog() {
  const [status, setStatus] = useSession<UpdateStatus | null>(KEY, null);
  const task = useTask("self-update");
  const [busy, setBusy] = useState<"install" | "skip" | null>(null);
  const [error, setError] = useState<string>();

  useEffect(() => {
    if (startupChecked) return;
    // 等首屏安顿下来再问：启动时本来就有价目、出口 IP、门禁几件事在抢网络。
    const timer = window.setTimeout(() => {
      if (startupChecked) return;
      startupChecked = true;
      // 设置里关了「启动时检查更新」时后端一个请求都不发（`manual: false`）。
      void api
        .updateCheck(false)
        .then((s) => {
          if (s.update_available && !s.skipped) openUpdateDialog(s);
        })
        .catch(() => undefined);
    }, 4000);
    return () => window.clearTimeout(timer);
  }, []);

  const latest = status?.latest ?? null;
  const installing = busy === "install" || task.running;
  const close = () => {
    if (installing) return;
    setError(undefined);
    setStatus(null);
  };

  const install = async () => {
    setError(undefined);
    setBusy("install");
    resetTask("self-update");
    try {
      // 成功的话面板马上退出，这之后的代码不会再跑。
      await api.updateInstall();
    } catch (e) {
      const why = message(e);
      setError(why);
      endTask("self-update", why);
      setBusy(null);
    }
  };

  const skip = async () => {
    if (!latest) return;
    setError(undefined);
    setBusy("skip");
    try {
      await api.updateSkip(latest.version);
      setStatus(null);
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(null);
    }
  };

  const notes = parseNotes(latest?.notes ?? "");
  const published = when(latest?.published_at ?? null);
  const pct = task.total > 0 ? (task.step / task.total) * 100 : undefined;

  return (
    <Modal
      open={!!latest}
      onClose={close}
      dismissible={!installing}
      title={
        <>
          <ArrowUpCircle size={18} />
          发现新版本 {latest?.version}
        </>
      }
      footer={
        <>
          <Button
            variant="ghost"
            disabled={installing}
            loading={busy === "skip"}
            onClick={() => void skip()}
          >
            跳过这个版本
          </Button>
          <Button disabled={installing || busy === "skip"} onClick={close}>
            以后再说
          </Button>
          <Button
            variant="primary"
            icon={<Download size={15} />}
            loading={installing}
            disabled={busy === "skip"}
            onClick={() => void install()}
          >
            一键更新
          </Button>
        </>
      }
    >
      <p>
        你现在用的是 {status?.current_version}
        {published ? `，新版本发布于 ${published}` : ""}。
      </p>
      {notes.length > 0 ? (
        <Notes blocks={notes} />
      ) : (
        <p className="qb-muted">这一版没有写更新说明。</p>
      )}
      {latest && (
        <ExternalLink href={latest.page_url}>
          在 GitHub 上查看这一版
        </ExternalLink>
      )}
      <p className="notice notice--warn mt-2">
        点「一键更新」：从 GitHub 下载安装包，对照同一个发布里的 SHA256SUMS.txt
        核对无误才安装。安装时面板会先退出 —— 由面板启动的 Claude
        桌面端、对话、酒馆与桥接会一起关掉，没保存的内容会丢；装完面板会自己重新打开。
      </p>
      {(installing || task.finished) && (
        <div className="mt-2">
          <ProgressBar
            value={pct}
            tone={task.error || error ? "danger" : "accent"}
            label="更新进度"
          />
          <p className="notice mt-1">{task.phase}</p>
        </div>
      )}
      {error && (
        <p className="notice notice--danger" role="alert">
          {error}
        </p>
      )}
    </Modal>
  );
}
