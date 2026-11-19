/**
 * 「扩展 → 浏览器助手」（2026-11-19，P3）：插件在面板里唯一的家。
 *
 * 原来只有一张卡，藏在「中文环境」「执行锁」两个小窗和查套路里，侧栏、扩展页、Ctrl+K 都找不到；
 * 插件文件夹也不随安装包走。现在三步安装（打开插件文件夹 → 在 Chrome 里加载 → 连接浏览器）、
 * 每个浏览器的连接与保护状态、受保护网站、三个附带功能各去哪看，都在这一页。那三个旧入口只剩一句说明 + 跳到这里。
 *
 * 打开这一页不开任何端口：只有点「连接浏览器」或已经配对过，面板才在本机监听（协议 2）。
 * 保护开不开、受保护网站有哪些，都在插件里设；面板只给「放 / 不放 + 原因」。
 */
import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import {
  ArrowLeft,
  ArrowRight,
  Copy,
  FolderOpen,
  Globe,
  Link2,
  RefreshCw,
  Unplug,
} from "lucide-react";
import { api } from "../../lib/api";
import type { BrowserExtensionFolder } from "../../lib/generated/BrowserExtensionFolder";
import type { BrowserLinkPairing } from "../../lib/generated/BrowserLinkPairing";
import type { BrowserLinkStart } from "../../lib/generated/BrowserLinkStart";
import { refresh, res, useResource, useSession } from "../../lib/store";
import { Button, Pill, useToast } from "../../ui";
import { ConfirmButton } from "../station/InlineConfirm";
import { openSecurity } from "../security/sheetState";
import { DEFAULT_PROTECTED_SITES } from "./where";

/** 三个旧入口里那一小段（`BrowserCompanionLink`）用同一个缓存键。 */
const linkResource = res(api.browserCompanionStatus, { pollMs: 2000 });
const folderResource = res(api.browserCompanionFolder);
const EXTENSION_ID = "dffmajnemniojfblgfhenndeeelbjgoh";

function enhancementRows(raw: string | null | undefined): [string, string][] {
  if (!raw) return [];
  try {
    const value = JSON.parse(raw);
    const labels: Record<string, string> = {
      default: "默认策略",
      default_public_and_private_interfaces: "公共和私有接口",
      default_public_interface_only: "仅公共接口",
      disable_non_proxied_udp: "禁用不经代理的 UDP",
      direct: "直连",
      auto_detect: "自动检测",
      pac_script: "PAC 脚本",
      fixed_servers: "固定代理服务器",
      system: "跟随系统",
      not_controllable: "受浏览器策略管理",
      controlled_by_other_extensions: "由其他扩展控制",
      controllable_by_this_extension: "未被其他扩展接管",
      controlled_by_this_extension: "由此扩展控制",
    };
    return [
      ["WebRTC 策略", "webrtc_policy"],
      ["代理模式", "proxy_mode"],
      ["代理控制方", "proxy_control"],
    ].map(([label, key]) => [
      label,
      typeof value?.[key] === "string"
        ? (labels[value[key]] ?? "未知")
        : "未知",
    ]);
  } catch {
    return [];
  }
}

/** 连没连上，一个词。 */
export function linkWord(p: BrowserLinkPairing): string {
  return p.connected ? "已连接" : p.needs_relink ? "要一键重连" : "没连上";
}

/** 保护此刻是什么样子，一个词。 */
function protectionWord(p: BrowserLinkPairing): [string, "ok" | "danger" | ""] {
  const s = p.protection;
  if (s.allow) return ["保护开着 · 放行中", "ok"];
  if (s.armed) return ["保护开着 · 拦着", "danger"];
  return ["保护没开", ""];
}

function when(seconds: number): string {
  if (!seconds) return "";
  const d = new Date(seconds * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getMonth() + 1}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function PairingRow({
  pairing,
  busy,
  onUnpair,
}: {
  pairing: BrowserLinkPairing;
  busy: boolean;
  onUnpair: (id: string) => void;
}) {
  const [word, tone] = protectionWord(pairing);
  const s = pairing.protection;
  return (
    <li className="qb-companion-row" data-testid="companion-pairing">
      <div className="qb-companion-row-head">
        <strong>{pairing.label}</strong>
        <small>插件 {pairing.extension_version}</small>
        <Pill
          tone={
            pairing.connected ? "ok" : pairing.needs_relink ? "warn" : "default"
          }
        >
          {linkWord(pairing)}
        </Pill>
        <Pill tone={tone || "default"}>{word}</Pill>
        <span className="qb-companion-row-action">
          <ConfirmButton
            label={
              <>
                <Unplug size={13} /> 解除配对
              </>
            }
            ariaLabel={`解除 ${pairing.label} 的配对`}
            message={`解除之后 ${pairing.label} 里的插件立刻断开，要再用得重新点「连接浏览器」。`}
            confirmLabel="解除配对"
            disabled={busy}
            onConfirm={() => onUnpair(pairing.pairing_id)}
          />
        </span>
      </div>
      <p className="qb-companion-reason">
        最近一次判定：{s.reason}
        {s.checked_at ? `（${when(s.checked_at)}）` : ""}
      </p>
      {enhancementRows(pairing.enhancement_json).length > 0 && (
        <p className="qb-muted qb-companion-enh">
          {enhancementRows(pairing.enhancement_json)
            .map(([label, value]) => `${label}：${value}`)
            .join(" · ")}
        </p>
      )}
    </li>
  );
}

function FolderStep({ folder }: { folder?: BrowserExtensionFolder }) {
  const toast = useToast();
  const [error, setError] = useState("");
  const [opening, setOpening] = useState(false);
  const open = async () => {
    setError("");
    setOpening(true);
    try {
      await api.browserCompanionOpenFolder();
    } catch (e) {
      setError(String(e));
    } finally {
      setOpening(false);
    }
  };
  const copy = async () => {
    if (!folder?.path) return;
    try {
      await navigator.clipboard.writeText(folder.path);
      toast.ok("插件文件夹的路径已复制");
    } catch {
      setError("复制失败，请手动选中上面的路径。");
    }
  };
  return (
    <>
      <div className="qb-inline-actions">
        <Button
          variant="primary"
          icon={<FolderOpen size={14} />}
          loading={opening}
          onClick={() => void open()}
        >
          打开插件文件夹
        </Button>
        <Button
          icon={<Copy size={14} />}
          disabled={!folder?.path}
          onClick={() => void copy()}
        >
          复制路径
        </Button>
      </div>
      {folder?.path ? (
        <p className="qb-muted qb-break">
          {folder.path}
          {folder.version ? ` · 插件 ${folder.version}` : ""}
          {folder.source === "development"
            ? " · 开发构建（仓库里 npm run build:browser-extension 的产物）"
            : " · 随安装包放在面板安装目录里"}
        </p>
      ) : folder?.problem ? (
        <p className="notice notice--danger" role="alert">
          {folder.problem}
        </p>
      ) : null}
      {error && (
        <p className="notice notice--danger" role="alert">
          {error}
        </p>
      )}
    </>
  );
}

export default function BrowserCompanionPage() {
  const status = useResource("browserCompanion", linkResource);
  const folder = useResource("browserCompanion.folder", folderResource);
  const [last, setLast] = useSession<BrowserLinkStart | null>(
    "browserCompanion.link",
    null,
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const toast = useToast();
  const navigate = useNavigate();
  const s = status.data;
  const run = async (action: () => Promise<unknown>) => {
    setBusy(true);
    setError("");
    try {
      await action();
      await refresh("browserCompanion");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const connect = () =>
    run(async () => setLast(await api.browserCompanionConnect()));
  const reconnect = () =>
    run(async () => setLast(await api.browserCompanionReconnect()));
  const unpair = (id: string) => run(() => api.browserCompanionUnpair(id));
  const copyText = async (text: string, done: string) => {
    try {
      await navigator.clipboard.writeText(text);
      toast.ok(done);
    } catch {
      setError("复制失败，请手动选中那一段文字。");
    }
  };
  const claim = s?.claim;
  const connected = s?.pairings.filter((p) => p.connected).length ?? 0;
  return (
    <>
      <Link className="qb-text-link qb-back-link" to="/extensions">
        <ArrowLeft size={15} />
        返回扩展中心
      </Link>
      <header className="qb-extension-detail-hero">
        <div className="qb-extension-icon qb-extension-icon-browser">
          <Globe size={34} />
        </div>
        <div>
          <span className="qb-eyebrow">浏览器插件</span>
          <h1>浏览器助手</h1>
          <p>
            装在 Chrome 或 Edge 里的 QB Gate 插件：面板没确认出口 IP
            时，拦下受保护的网站；其他网站照常打开。另带三个小功能：检查当前浏览器、查套路用浏览器里已登录的账户、读取中转站公开价格。
          </p>
        </div>
      </header>
      <div className="qb-extension-layout">
        <section className="qb-detail qb-companion">
          <h2>三步装好</h2>
          <ol className="qb-companion-steps">
            <li>
              <h3>打开插件文件夹</h3>
              <p className="qb-muted">
                插件随 QB Gate 一起装好了，就在面板安装目录里。
              </p>
              <FolderStep folder={folder.data} />
            </li>
            <li>
              <h3>在 Chrome 里加载它</h3>
              <p className="qb-muted">
                在 Chrome 地址栏打开 chrome://extensions（Edge 是
                edge://extensions），打开右上角的「开发者模式」，点「加载已解压的扩展程序」，选上一步那个文件夹。这类地址点链接打不开，复制下来粘到地址栏里。
              </p>
              <div className="qb-inline-actions">
                <Button
                  size="sm"
                  icon={<Copy size={13} />}
                  onClick={() =>
                    void copyText(
                      "chrome://extensions",
                      "已复制 chrome://extensions，粘到 Chrome 地址栏里打开",
                    )
                  }
                >
                  复制 chrome://extensions
                </Button>
                <Button
                  size="sm"
                  icon={<Copy size={13} />}
                  onClick={() =>
                    void copyText(
                      "edge://extensions",
                      "已复制 edge://extensions，粘到 Edge 地址栏里打开",
                    )
                  }
                >
                  复制 edge://extensions
                </Button>
              </div>
              <p className="qb-muted">
                装好会自动打开一页引导，插件 ID 应当是 {EXTENSION_ID}。Chrome
                启动时提示「停用开发者模式下的扩展程序」，点「保留」。
              </p>
            </li>
            <li>
              <h3>回到这里点「连接浏览器」</h3>
              <p className="qb-muted">
                浏览器里会打开一个确认页，插件自己完成连接，不用复制粘贴。连一次长期有效，面板重启、浏览器重启后插件都会自己连回来。
              </p>
              <div className="qb-inline-actions">
                <Button
                  variant="primary"
                  icon={<Link2 size={14} />}
                  disabled={status.loading || !!status.error}
                  loading={busy}
                  onClick={() => void connect()}
                >
                  连接浏览器
                </Button>
              </div>
              {claim?.state === "pending" && (
                <p className="notice" role="status">
                  {claim.problem ?? "等插件在浏览器里认领这次连接…"}
                </p>
              )}
              {claim?.state === "claimed" && (
                <p className="notice" role="status">
                  已连接：{claim.browser}
                </p>
              )}
              {claim?.state === "expired" && (
                <p className="notice" role="status">
                  这次连接已超时，需要的话再点一次「连接浏览器」。
                </p>
              )}
              {last?.url && !last.opened && (
                <div className="qb-companion-url">
                  <p className="notice">
                    没能替你打开浏览器。在装了插件的浏览器里打开这个地址（几分钟内有效）：
                  </p>
                  <code className="qb-break">{last.url}</code>
                  <Button
                    size="sm"
                    icon={<Copy size={13} />}
                    onClick={() =>
                      void copyText(
                        last.url ?? "",
                        "连接地址已复制，在装了插件的浏览器里打开",
                      )
                    }
                  >
                    复制地址
                  </Button>
                </div>
              )}
            </li>
          </ol>

          <section className="qb-section">
            <div className="qb-companion-status-head">
              <h2>连接状态</h2>
              <Pill tone={s?.listening ? "ok" : "default"}>
                {s?.listening ? `本机端口 ${s.port}` : "没在监听"}
              </Pill>
              <Pill tone={connected > 0 ? "ok" : "default"}>
                已配对 {s?.pairings.length ?? 0} 个浏览器 · {connected} 个连着
              </Pill>
            </div>
            {s?.relink_needed && (
              <div className="notice notice--warn qb-companion-relink">
                <p role="status">
                  原来的端口被别的程序占用，面板换了一个；已配对的浏览器要点「一键重连」，再在装了插件的浏览器里打开弹出的确认页。
                </p>
                <Button
                  size="sm"
                  icon={<RefreshCw size={13} />}
                  disabled={busy}
                  onClick={() => void reconnect()}
                >
                  一键重连
                </Button>
              </div>
            )}
            {s?.problem && (
              <p className="notice notice--danger" role="alert">
                {s.problem}
              </p>
            )}
            {s && s.pairings.length === 0 && (
              <p className="qb-muted">
                还没有配对的浏览器。装好插件之后点上面的「连接浏览器」。没配对过的时候面板不监听任何端口。
              </p>
            )}
            {s && s.pairings.length > 0 && (
              <ul className="qb-companion-list">
                {s.pairings.map((pairing) => (
                  <PairingRow
                    key={pairing.pairing_id}
                    pairing={pairing}
                    busy={busy}
                    onUnpair={(id) => void unpair(id)}
                  />
                ))}
              </ul>
            )}
            {s?.last_check && (
              <p className="qb-muted">
                面板最近一次核对出口：{s.last_check.reason}
              </p>
            )}
            {(error || status.error) && (
              <p role="alert" className="notice notice--danger">
                {error || status.error}
              </p>
            )}
          </section>

          <section className="qb-section">
            <h2>受保护的网站</h2>
            <ul className="qb-companion-sites">
              {DEFAULT_PROTECTED_SITES.map((site) => (
                <li key={site}>{site}</li>
              ))}
            </ul>
            <p className="qb-muted">
              这是插件默认的名单，各连同子域名。名单只存在插件里：在插件弹窗点「设置」增删，面板这边看不到改过之后的样子。其他网站、其他标签页一律不动。
            </p>
            <p className="qb-muted">
              保护默认是关的：在插件弹窗或引导页点「开启保护」才生效，之后只有面板确认出口没问题，受保护的网站才打得开；出口变了、查不到出口、IP
              锁被手动关闭、面板退出或连不上，一律拦着。随时可以在插件里「暂停保护」，暂停就撤掉全部拦截规则。
            </p>
          </section>

          <section className="qb-section">
            <h2>附带的三个功能，结果在哪看</h2>
            <ul className="qb-companion-extras">
              <li>
                <div>
                  <strong>检查当前浏览器</strong>
                  <p className="qb-muted">
                    在插件弹窗里点。语言、时区、字体、WebRTC
                    等结果回到官方账户页评分卡的「中文环境」里；WebRTC
                    策略与代理模式写在上面每个浏览器那一行。
                  </p>
                </div>
                <Button
                  size="sm"
                  icon={<ArrowRight size={13} />}
                  onClick={() => {
                    navigate("/");
                    openSecurity("signals");
                  }}
                >
                  看「中文环境」
                </Button>
              </li>
              <li>
                <div>
                  <strong>查套路用浏览器里已登录的账户</strong>
                  <p className="qb-muted">
                    在中转站页面登录后点插件里的「连接已登录账户」，再回中转站页「查套路」选「使用
                    Chrome 已登录账户」。
                  </p>
                </div>
                <Button
                  size="sm"
                  icon={<ArrowRight size={13} />}
                  onClick={() => navigate("/relays")}
                >
                  去中转站
                </Button>
              </li>
              <li>
                <div>
                  <strong>读取中转站公开价格</strong>
                  <p className="qb-muted">
                    不带登录地读站点公布的价格，只作报价对照；真实收费倍率仍由「查套路」核对实扣账单。
                  </p>
                </div>
              </li>
            </ul>
          </section>
        </section>
        <aside className="qb-extension-meta">
          <h3>插件信息</h3>
          <dl>
            <dt>版本</dt>
            <dd>{folder.data?.version ?? "—"}</dd>
            <dt>适用浏览器</dt>
            <dd>Chrome 或 Edge 116 及以上</dd>
            <dt>扩展 ID</dt>
            <dd className="qb-break">{EXTENSION_ID}</dd>
            <dt>每个浏览器资料</dt>
            <dd>各装各的、各连各的，最多 8 个</dd>
            <dt>不是什么</dt>
            <dd>
              不是代理或 VPN，不改网络、不转发流量；只管装了它的那个浏览器资料。
            </dd>
          </dl>
          <h3>面板更新之后</h3>
          <p className="qb-muted">
            插件文件会跟着换成新版：在 chrome://extensions
            里点这个插件的刷新按钮，或者重启浏览器，才用上新版。卸载面板后插件会显示文件丢失。
          </p>
        </aside>
      </div>
    </>
  );
}
