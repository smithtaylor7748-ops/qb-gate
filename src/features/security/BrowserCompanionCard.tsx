import { useState } from "react";
import { Globe, Copy, Link2, Unplug } from "lucide-react";
import { api } from "../../lib/api";
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

/** Only the extension enables protection; opening this card never starts monitoring. */
export default function BrowserCompanionCard() {
  const status = useResource("browserCompanion", linkResource);
  const [pairing, setPairing] = useSession<BrowserLinkStart | null>(
    "browserCompanion.pairing",
    null,
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const toast = useToast();
  const s = status.data;
  const start = async () => {
    setBusy(true);
    setError("");
    try {
      setPairing(await api.browserCompanionStart());
      await refresh("browserCompanion");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const stop = async () => {
    setBusy(true);
    setError("");
    try {
      await api.browserCompanionStop();
      setPairing(null);
      await refresh("browserCompanion");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const copy = async () => {
    if (!pairing) return;
    try {
      await navigator.clipboard.writeText(pairing.pairing);
      toast.ok("配对码已复制，在 Chrome 插件中粘贴并连接");
    } catch {
      setError("复制失败，请从下方配对码手动复制。");
    }
  };
  return (
    <Card title="Chrome 插件联动" icon={<Globe size={15} />} className="mb-3">
      <div className="flex flex-wrap items-center gap-2 mb-2">
        <Pill tone={s?.connected ? "ok" : "default"}>
          {s?.connected ? "插件已连接" : s?.running ? "等待插件连接" : "未连接"}
        </Pill>
        <Pill tone={s?.armed ? (s.safe ? "ok" : "danger") : "default"}>
          {s?.armed
            ? s.safe
              ? "IP 保护已开启"
              : "保护已触发"
            : "IP 保护未开启"}
        </Pill>
      </div>
      <p className="notice mb-2">
        在 Chrome 的扩展程序页开启开发者模式，加载 QB Gate 插件文件夹。
        在这里生成配对码，再到插件里连接。配对后可复用站点登录、读取公开模型单价，并检测
        Chrome 实际环境。
      </p>
      <p className="notice mb-2">
        在插件中单独开启 IP 保护后，出口 IP 变化、IP 锁拒绝或连接中断会关闭当前
        Chrome 资料的全部窗口。
        其他资料、未授权的隐身窗口及后台进程不在关闭范围内。QB Gate
        需要保持运行。
      </p>
      {s?.reason && (
        <p className="notice mb-2" role="status">
          {s.reason}
        </p>
      )}
      {s?.ip && <p className="notice mb-2">当前受检出口：{s.ip}</p>}
      {enhancementRows(s?.enhancement_json).map(([label, value]) => (
        <p className="notice mb-1" key={label}>
          {label}：{value}
        </p>
      ))}
      <div className="flex flex-wrap gap-2">
        <Button
          icon={<Link2 size={13} />}
          disabled={!!s?.running || status.loading || !!status.error}
          loading={busy}
          onClick={() => void start()}
        >
          生成配对码
        </Button>
        {s?.running && pairing && (
          <Button icon={<Copy size={13} />} onClick={() => void copy()}>
            复制配对码
          </Button>
        )}
        {s?.running && (
          <Button
            icon={<Unplug size={13} />}
            disabled={!!s.armed && s.connected}
            loading={busy}
            onClick={() => void stop()}
          >
            {s.armed && !s.connected ? "重置失效连接" : "停止联动"}
          </Button>
        )}
      </div>
      {s?.running && pairing && (
        <details className="mt-2">
          <summary className="notice">手动复制配对码</summary>
          <textarea
            className="w-full mt-2"
            rows={3}
            readOnly
            aria-label="Chrome 插件配对码"
            value={pairing.pairing}
            spellCheck={false}
          />
        </details>
      )}
      {s?.running && !pairing && (
        <p className="notice mt-2">
          配对码只保留在本次界面会话中；需要重新配对时，先停止联动，再生成配对码。
        </p>
      )}
      {s?.armed && s.connected && (
        <p className="notice mt-2">先在插件里停止 IP 保护，再停止联动。</p>
      )}
      {s?.armed && !s.connected && (
        <p className="notice mt-2">
          插件保护连接已失效，可重置后重新配对；重新连接不会自动开启保护。
        </p>
      )}
      {(error || status.error) && (
        <p role="alert" className="notice notice--danger mt-2">
          {error || status.error}
        </p>
      )}
    </Card>
  );
}
