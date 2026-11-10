import { usePlatform } from "../lib/platform";
/**
 * 导入链接的确认框（中转站重构阶段 5，cc-switch 的 X4 Deep Link 的 `DeepLinkImportDialog`）。
 *
 * **全局只挂一份**（`Shell` 里，跟「发现新版本」一样）。链接从三处来，都落到这一个框：
 * - 点了 `qbgate://…`（面板没开着 = 冷启动，开着 = 第二个实例转过来）：后端收下、发 `workspace://deeplink`，这里去取；
 * - 中转站页的「从链接导入」（[`DeepLinkPasteButton`]）：粘贴 `qbgate://` 或中转站给 CC Switch 的 `ccswitch://`。
 *
 * 规矩（CLAUDE.md「导入链接」那一节）：
 * - ⛔ **导入前一律先在这里确认，导入后什么都不自动启用**：站点导入成一条线路但不切过去；MCP 进列表、一个目标都不勾；
 *   提示词进库、不启用；Skill 只是带着来源去 Skills 分页，扫描、挑、装都由使用者来。
 * - ⛔ 站点的 Key 只看得到打码的那一份；链接没带 Key 时在这里填（跟线路弹窗一样，只进后端、不回界面）。
 * - 链接要求「导入后直接用」、带着用量脚本的，照实列成「没照做的」。
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useNavigate } from "react-router-dom";
import { Link2 } from "lucide-react";

import { CHANNELS } from "../lib/channels";
import { DEMO_ENABLED } from "../lib/demo";
import { deeplinkApi, type DeepLinkEntry } from "../lib/deeplink";
import type { Client } from "../lib/generated/Client";
import type { RouteModelMap } from "../lib/generated/RouteModelMap";
import { UPSTREAM_FORMAT_LABEL } from "../lib/station";
import { getSession, setSession, useSession } from "../lib/store";
import { Button, CodeBlock, Modal, useToast } from "../ui";

const KEY = "deeplink.entry";
// A peek can finish after a paste, dismissal or import. None of those actions
// may be undone by that old read, even when the visible inbox is now empty.
let inboxRevision = 0;

const CLIENT_NAME: Record<string, string> = {
  "claude-code": "Claude Code",
  "claude-desktop": "Claude 桌面端",
  codex: "Codex 桌面端",
  "gemini-cli": "Gemini CLI",
  "grok-build": "Grok Build",
};

const TIER_NAME: [keyof RouteModelMap, string][] = [
  ["default", "兜底"],
  ["opus", "Opus 档"],
  ["sonnet", "Sonnet 档"],
  ["haiku", "Haiku 档"],
];

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** 让确认框显示这一条（粘贴之后、收到通知之后都走这里）。 */
export function showDeepLink(entry: DeepLinkEntry | null): void {
  const current = getSession<DeepLinkEntry | null>(KEY, null);
  if (entry && current && entry.id < current.id) return;
  inboxRevision++;
  setSession<DeepLinkEntry | null>(KEY, entry);
}

function dismissDeepLink(id: number): void {
  if (getSession<DeepLinkEntry | null>(KEY, null)?.id === id)
    showDeepLink(null);
}

/** 没照做的那几样，照实列出来。 */
function Ignored({ items }: { items: string[] }) {
  if (!items.length) return null;
  return (
    <div className="qb-st-note" role="note">
      下面这些没照做：
      <ul style={{ margin: "4px 0 0", paddingLeft: 18 }}>
        {items.map((t) => (
          <li key={t}>{t}</li>
        ))}
      </ul>
    </div>
  );
}

function Row({ k, children }: { k: string; children: React.ReactNode }) {
  return (
    <tr>
      <th style={{ textAlign: "left", fontWeight: 500, whiteSpace: "nowrap" }}>
        {k}
      </th>
      <td style={{ wordBreak: "break-all" }}>{children}</td>
    </tr>
  );
}

export default function DeepLinkHost() {
  const [entry] = useSession<DeepLinkEntry | null>(KEY, null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [group, setGroup] = useState("");
  const [key, setKey] = useState("");
  const toast = useToast();
  const navigate = useNavigate();
  const peekSequence = useRef(0);

  const fetchPending = useCallback(async () => {
    const sequence = ++peekSequence.current;
    const revision = inboxRevision;
    try {
      const e = await deeplinkApi.peek();
      if (e && sequence === peekSequence.current && revision === inboxRevision)
        showDeepLink(e);
    } catch {
      // 读不到就算了：链接还在后端，下次通知或重开面板再取。
    }
  }, []);

  // 挂上来先取一次（冷启动时链接比界面先到），之后每次通知再取。
  useEffect(() => {
    void fetchPending();
    if (DEMO_ENABLED) return;
    let disposed = false;
    const p = listen(CHANNELS.deeplink, () => void fetchPending());
    void p.then((stop) => {
      if (disposed) stop();
      else void fetchPending(); // Cover links received while listener registration was pending.
    });
    return () => {
      disposed = true;
      peekSequence.current++;
      void p.then((stop) => stop());
    };
  }, [fetchPending]);

  // 换了一条链接：清掉上一条的输入。
  const id = entry?.id;
  useEffect(() => {
    setGroup("");
    setKey("");
    setError(null);
  }, [id]);

  if (!entry) return null;
  const v = entry.view;
  const from =
    entry.scheme === "ccswitch"
      ? "这是给 CC Switch 的链接（格式相同，QB Gate 照样认）。"
      : "";

  const dismiss = () => {
    if (busy) return;
    void deeplinkApi.ack(entry.id).catch(() => undefined);
    dismissDeepLink(entry.id);
  };

  const doImport = async () => {
    setBusy(true);
    setError(null);
    try {
      const out = await deeplinkApi.import(entry.id, {
        group,
        key: key.trim() ? key.trim() : null,
      });
      dismissDeepLink(entry.id);
      toast.ok(out.message);
      if (out.client) {
        setSession<Client>("station.client", out.client);
        // 已经停在中转站页时也要重读线路池（跳过去那一下不会重挂页面）。
        setSession("station.routes.bump", Date.now());
        navigate("/relays");
      }
    } catch (e) {
      if (getSession<DeepLinkEntry | null>(KEY, null)?.id === entry.id)
        setError(message(e));
    } finally {
      setBusy(false);
    }
  };

  const footer = (primary: string | null, onPrimary?: () => void) => (
    <>
      <Button disabled={busy} onClick={dismiss}>
        {primary ? "不导入" : "关闭"}
      </Button>
      {primary && (
        <Button
          variant="primary"
          loading={busy}
          onClick={onPrimary ?? (() => void doImport())}
        >
          {primary}
        </Button>
      )}
    </>
  );

  let title = "导入链接";
  let body: React.ReactNode = null;
  let actions = footer(null);

  if (v.kind === "error") {
    title = "这条导入链接用不了";
    body = (
      <p className="qb-st-note" role="alert">
        {v.message}
      </p>
    );
  } else if (v.kind === "provider") {
    const name = CLIENT_NAME[v.client] ?? v.client;
    const tiers = TIER_NAME.filter(([k]) => v.model_map[k]);
    title = `导入站点 · ${name}`;
    body = (
      <>
        <p className="qb-st-note">
          导入成 {name} 的一条线路：<b>不会切过去</b>
          ，要用它在中转站页点这一行。Key 加密存在面板里，不写进任何软件的配置。
          {v.existing_station &&
            ` 这个地址已经有站点「${v.existing_station}」，接着用它、不改它。`}
        </p>
        <div className="qb-st-tw">
          <table>
            <tbody>
              <Row k="站点">{v.name}</Row>
              <Row k="地址">
                {v.endpoints[0]}
                {v.endpoints.length > 1 && (
                  <span className="qb-st-dim">
                    {" "}
                    · 另有 {v.endpoints.length - 1} 个备用地址：
                    {v.endpoints.slice(1).join("、")}
                  </span>
                )}
              </Row>
              <Row k="API Key">
                {v.key_masked ?? (
                  <input
                    className="input"
                    type="password"
                    autoComplete="off"
                    aria-label="API Key（链接里没带）"
                    placeholder="链接里没带，填这条线的 Key"
                    value={key}
                    disabled={busy}
                    onChange={(e) => setKey(e.target.value)}
                  />
                )}
              </Row>
              {tiers.length > 0 && (
                <Row k="模型映射">
                  {tiers
                    .map(([k, label]) => `${label} → ${v.model_map[k]}`)
                    .join("；")}
                </Row>
              )}
              {v.upstream_format && (
                <Row k="上游协议">
                  {UPSTREAM_FORMAT_LABEL[v.upstream_format]}
                  （链接里写的；本机路由两头换协议）
                </Row>
              )}
              {v.homepage && <Row k="主页">{v.homepage}</Row>}
              {v.notes && <Row k="备注">{v.notes}</Row>}
              <Row k="分组">
                <input
                  className="input"
                  aria-label="分组"
                  placeholder="留空 = 默认分组"
                  value={group}
                  disabled={busy}
                  onChange={(e) => setGroup(e.target.value)}
                />
              </Row>
            </tbody>
          </table>
        </div>
        <Ignored items={v.ignored} />
      </>
    );
    actions = footer("导入这条线路");
  } else if (v.kind === "mcp") {
    const fresh = v.servers.filter((s) => !s.exists);
    title = "导入 MCP";
    body = (
      <>
        <p className="qb-st-note">
          加进 MCP 列表，<b>一个目标都不勾、不写进任何软件</b>：要用去扩展中心的
          MCP 分页勾目标。勾上之后，下面的命令会由那个软件执行 —— 先看清楚。
          {v.apps.length > 0 && ` 链接说给：${v.apps.join("、")}。`}
        </p>
        {v.servers.map((s) => (
          <details key={s.id} open={v.servers.length === 1}>
            <summary>
              <b>{s.id}</b> · {s.summary}
              {s.exists && (
                <span className="qb-st-dim"> · 已经有同名的，会跳过</span>
              )}
            </summary>
            <CodeBlock text={s.spec} maxHeight={220} showLineNumbers={false} />
          </details>
        ))}
        <Ignored items={v.ignored} />
      </>
    );
    actions = footer(fresh.length ? "加进 MCP 列表" : null);
  } else if (v.kind === "prompt") {
    title = "导入提示词";
    body = (
      <>
        <p className="qb-st-note">
          放进提示词库，<b>不用在任何地方</b>
          ：要用去扩展中心的提示词分页挑它。链接说给：{v.app}。
        </p>
        <div className="qb-st-tw">
          <table>
            <tbody>
              <Row k="名字">{v.name}</Row>
              {v.description && <Row k="说明">{v.description}</Row>}
            </tbody>
          </table>
        </div>
        <CodeBlock
          text={v.content}
          caption="内容"
          maxHeight={260}
          showLineNumbers={false}
        />
        <Ignored items={v.ignored} />
      </>
    );
    actions = footer("放进提示词库");
  } else {
    title = "导入 Skill";
    body = (
      <p className="qb-st-note">
        来源：<b>{v.repo}</b>（{v.source}）。Skill 不从链接直接装：去 Skills
        分页扫描这个来源，挑好、选好装到哪再装 ——
        跟手动装一样，只读文件、不执行仓库里的脚本。
      </p>
    );
    actions = footer("去 Skills 分页扫描", () => {
      void deeplinkApi.ack(entry.id).catch(() => undefined);
      setSession("extensions.tab", "skills");
      setSession("skills.add.source", v.source);
      showDeepLink(null);
      navigate("/extensions");
    });
  }

  return (
    <Modal
      open
      onClose={dismiss}
      dismissible={!busy}
      title={title}
      icon={<Link2 size={16} aria-hidden="true" />}
      footer={actions}
    >
      <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        {from && <p className="qb-st-dim">{from}</p>}
        {body}
        {error && (
          <p className="qb-st-note" role="alert">
            没导入：{error}
          </p>
        )}
      </div>
    </Modal>
  );
}

/** 中转站页的「从链接导入」：粘贴 `qbgate://` / `ccswitch://` 链接，交给上面那个确认框。 */
export function DeepLinkPasteButton() {
  const [open, setOpen] = useState(false);
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const go = async () => {
    setBusy(true);
    setError(null);
    try {
      const e = await deeplinkApi.paste(url);
      setOpen(false);
      setUrl("");
      showDeepLink(e);
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <Button
        icon={<Link2 size={14} aria-hidden="true" />}
        onClick={() => setOpen(true)}
        title="粘贴中转站给的导入链接（qbgate:// 或给 CC Switch 的 ccswitch://）"
      >
        从链接导入
      </Button>
      <Modal
        open={open}
        onClose={() => !busy && setOpen(false)}
        title="从链接导入"
        icon={<Link2 size={16} aria-hidden="true" />}
        footer={
          <>
            <Button disabled={busy} onClick={() => setOpen(false)}>
              取消
            </Button>
            <Button
              variant="primary"
              loading={busy}
              disabled={!url.trim()}
              onClick={() => void go()}
            >
              看看里面是什么
            </Button>
          </>
        }
      >
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          <DeepLinkWebImport />
          <p className="qb-st-note">
            已经复制了导入链接也可以粘贴在下面。支持 <code>ccswitch://…</code>和{" "}
            <code>qbgate://…</code>；普通站点首页地址不包含账号或
            Key，不能直接导入。 先预览、确认后才添加线路。
          </p>
          <textarea
            className="input"
            aria-label="导入链接"
            rows={4}
            value={url}
            disabled={busy}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="ccswitch://v1/import?resource=provider&…"
          />
          {error && (
            <p className="qb-st-note" role="alert">
              {error}
            </p>
          )}
        </div>
      </Modal>
    </>
  );
}

function DeepLinkWebImport() {
  const platform = usePlatform();
  const [status, setStatus] = useState<Awaited<
    ReturnType<typeof deeplinkApi.handlerStatus>
  > | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let dropped = false;
    void deeplinkApi
      .handlerStatus()
      .then((next) => {
        if (!dropped) setStatus(next);
      })
      .catch((e) => {
        if (!dropped) setError(message(e));
      });
    return () => {
      dropped = true;
    };
  }, []);
  if (platform?.os === "macos")
    return (
      <p className="qb-st-note">
        macOS 请复制站点的导入链接，在下方粘贴后预览并确认导入。
      </p>
    );
  const change = async (enabled: boolean) => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await deeplinkApi.setHandler(enabled));
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="qb-st-note">
      <strong>从 New API / Sub2API 网页导入</strong>
      <p>
        在站点的 API Key / 令牌列表点「导入到 CC
        Switch」，选择软件。开启下面的接收方式后，网页按钮会把链接送到 QB
        Gate，仍需确认才导入。
      </p>
      {status && (
        <>
          <p>
            {status.enabled
              ? "当前由 QB Gate 接收网页导入。"
              : status.registered
                ? "QB Gate 已登记，但 Windows 当前仍由其它默认应用接收。"
                : status.handler
                  ? `当前接收程序：${status.handler}`
                  : "当前没有可用的接收程序。"}
          </p>
          {!status.registered && (
            <p>
              开启后将把 CC Switch 导入交给 QB
              Gate；原接收方式会保存，关闭时可以还原。
            </p>
          )}
          {status.dev_build && (
            <p>开发版不改系统接收方式；请在安装版开启，或在下方粘贴链接。</p>
          )}
          {status.user_choice && (
            <p>
              Windows 已指定其它默认应用。请在 Windows「默认应用」中把 CCSWITCH
              协议改为 QB Gate 后，网页链接才能由 QB Gate 接收。
            </p>
          )}
          <Button
            disabled={busy || status.dev_build}
            loading={busy}
            onClick={() => void change(!status.registered)}
          >
            {status.registered
              ? "还原原来的接收方式"
              : "由 QB Gate 接收 CC Switch 导入"}
          </Button>
        </>
      )}
      {error && <p role="alert">{error}</p>}
    </div>
  );
}
