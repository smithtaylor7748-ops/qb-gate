import { useState } from "react";
import { Copy, Globe, Link2, RefreshCw, Unplug } from "lucide-react";
import { api } from "../../lib/api";
import type { BrowserLinkPairing } from "../../lib/generated/BrowserLinkPairing";
import type { BrowserLinkStart } from "../../lib/generated/BrowserLinkStart";
import { refresh, res, useResource, useSession } from "../../lib/store";
import { Button, Card, Pill, useToast } from "../../ui";

const linkResource = res(api.browserCompanionStatus, { pollMs: 2000 });

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

function PairingRow({
  pairing,
  busy,
  onUnpair,
}: {
  pairing: BrowserLinkPairing;
  busy: boolean;
  onUnpair: (id: string) => void;
}) {
  const p = pairing.protection;
  return (
    <li className="mb-2">
      <div className="flex flex-wrap items-center gap-2">
        <strong>{pairing.label}</strong>
        <Pill tone={pairing.connected ? "ok" : "default"}>
          {pairing.connected
            ? "已连接"
            : pairing.needs_relink
              ? "需要重连"
              : "未连接"}
        </Pill>
        <Pill tone={p.allow ? "ok" : p.armed ? "danger" : "default"}>
          {p.allow ? "放行中" : p.armed ? "已拦截" : "保护未开启"}
        </Pill>
        <Button
          size="sm"
          icon={<Unplug size={13} />}
          disabled={busy}
          onClick={() => onUnpair(pairing.pairing_id)}
        >
          解除配对
        </Button>
      </div>
      <p className="notice mt-1">{p.reason}</p>
      {enhancementRows(pairing.enhancement_json).map(([label, value]) => (
        <p className="notice mb-1" key={label}>
          {label}：{value}
        </p>
      ))}
    </li>
  );
}

/**
 * 浏览器助手的连接与配对。界面重做在后面一轮；这里只把协议 2 的四条命令接好。
 * 打开这张卡不开任何端口：只有点「连接浏览器」或已经配对过，面板才在本机监听。
 */
export default function BrowserCompanionCard() {
  const status = useResource("browserCompanion", linkResource);
  const [last, setLast] = useSession<BrowserLinkStart | null>(
    "browserCompanion.link",
    null,
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const toast = useToast();
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
  const copy = async () => {
    if (!last?.url) return;
    try {
      await navigator.clipboard.writeText(last.url);
      toast.ok("连接地址已复制，在装了插件的浏览器里打开");
    } catch {
      setError("复制失败，请手动选中上面的地址。");
    }
  };
  const claim = s?.claim;
  return (
    <Card title="Chrome 浏览器助手" icon={<Globe size={15} />} className="mb-3">
      <div className="flex flex-wrap items-center gap-2 mb-2">
        <Pill tone={s?.listening ? "ok" : "default"}>
          {s?.listening ? `本机端口 ${s.port}` : "未监听"}
        </Pill>
        <Pill tone={s?.pairings.some((p) => p.connected) ? "ok" : "default"}>
          已配对 {s?.pairings.length ?? 0} 个浏览器
        </Pill>
      </div>
      <p className="notice mb-2">
        在 Chrome 的扩展程序页开启开发者模式，加载 QB Gate
        插件文件夹，再点「连接浏览器」：浏览器里会打开一个本机页面，插件自动完成配对，不用复制粘贴。配对一次长期有效，面板重启后插件自己连回来。
      </p>
      <p className="notice mb-2">
        插件开启保护后，只有面板确认出口 IP
        正常时才放行受保护的网站；出口变化、IP
        锁拒绝、面板退出或连接中断时一律拦截。保护只对装了插件的那个浏览器资料有效。
      </p>
      {s?.relink_needed && (
        <p className="notice notice--danger mb-2" role="status">
          原来的端口被别的程序占用，面板换了一个；已配对的浏览器要点「一键重连」才找得到。
        </p>
      )}
      {claim?.state === "pending" && (
        <p className="notice mb-2" role="status">
          {claim.problem ?? "等插件在浏览器里认领这次连接…"}
        </p>
      )}
      {claim?.state === "claimed" && (
        <p className="notice mb-2" role="status">
          已连接：{claim.browser}
        </p>
      )}
      {claim?.state === "expired" && (
        <p className="notice mb-2" role="status">
          这次连接已超时，需要的话再点一次「连接浏览器」。
        </p>
      )}
      {s?.last_check && (
        <p className="notice mb-2">最近一次判定：{s.last_check.reason}</p>
      )}
      {s?.problem && (
        <p className="notice notice--danger mb-2" role="alert">
          {s.problem}
        </p>
      )}
      {s && s.pairings.length > 0 && (
        <ul className="mb-2">
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
      <div className="flex flex-wrap gap-2">
        <Button
          icon={<Link2 size={13} />}
          disabled={status.loading || !!status.error}
          loading={busy}
          onClick={() => void connect()}
        >
          连接浏览器
        </Button>
        {s && s.pairings.length > 0 && (
          <Button
            icon={<RefreshCw size={13} />}
            disabled={busy}
            onClick={() => void reconnect()}
          >
            一键重连
          </Button>
        )}
      </div>
      {last?.url && !last.opened && (
        <div className="mt-2">
          <p className="notice mb-1">
            没能替你打开浏览器。在装了插件的浏览器里打开这个地址（几分钟内有效）：
          </p>
          <code className="notice">{last.url}</code>
          <Button
            size="sm"
            icon={<Copy size={13} />}
            onClick={() => void copy()}
          >
            复制地址
          </Button>
        </div>
      )}
      {(error || status.error) && (
        <p role="alert" className="notice notice--danger mt-2">
          {error || status.error}
        </p>
      )}
    </Card>
  );
}
