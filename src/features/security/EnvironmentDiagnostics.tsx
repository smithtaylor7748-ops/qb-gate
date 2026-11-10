import { useState } from "react";
import { api } from "../../lib/api";
import type { EnvironmentDiagnostics as Report } from "../../lib/generated/EnvironmentDiagnostics";
import { getSession, useSession } from "../../lib/store";
import { Button, Card, Collapsible, Pill, TextField } from "../../ui";

type Kind = "stun" | "proxy" | "asn" | "locale";
const PREFIX = "environment.diagnostics.";
const TITLES: Record<Kind, string> = {
  stun: "UDP 出口对照",
  proxy: "代理运行规则",
  asn: "ASN 网络类型",
  locale: "Claude 客户端语言",
};

/** Latest results in memory only. Opening this component never starts a probe. */
export default function EnvironmentDiagnostics() {
  const [busy, setBusy] = useSession<string>(`${PREFIX}busy`, "");
  const [results, setResults] = useSession<Partial<Record<Kind, Report>>>(
    `${PREFIX}results`,
    {},
  );
  const [error, setError] = useSession(`${PREFIX}error`, "");
  const [endpoint, setEndpoint] = useSession(`${PREFIX}endpoint`, "");
  const [ip, setIp] = useSession(`${PREFIX}ip`, "");
  const [expected, setExpected] = useSession(`${PREFIX}expected`, "");
  // Secret exists only in this input and the in-flight IPC request; never session/storage.
  const [secret, setSecret] = useState("");

  function clear(kind: Kind) {
    const next = {
      ...getSession<Partial<Record<Kind, Report>>>(`${PREFIX}results`, {}),
    };
    delete next[kind];
    setResults(next);
    setError("");
  }

  async function run(kind: Kind) {
    if (getSession(`${PREFIX}busy`, "")) return;
    setBusy(kind);
    clear(kind);
    const transientSecret = secret;
    if (kind === "proxy") setSecret("");
    try {
      const report = await (kind === "stun"
        ? api.environmentStun()
        : kind === "proxy"
          ? api.environmentProxy(endpoint.trim(), transientSecret || null)
          : kind === "asn"
            ? api.environmentAsn(ip.trim())
            : api.environmentLocale(expected.trim() || null));
      setResults({
        ...getSession<Partial<Record<Kind, Report>>>(`${PREFIX}results`, {}),
        [kind]: report,
      });
    } catch (e) {
      setError(
        `${TITLES[kind]}：${e instanceof Error ? e.message : String(e)}`,
      );
    } finally {
      setBusy("");
    }
  }

  const button = (kind: Kind, label: string, disabled = false) => (
    <Button
      size="sm"
      loading={busy === kind}
      disabled={!!busy || disabled}
      onClick={() => void run(kind)}
    >
      {label}
    </Button>
  );

  return (
    <Card className="mb-3">
      <h3 className="card-title">补充环境检测</h3>
      <p className="notice mt-1 mb-3">
        按需运行，结果分别展示。检测期间不修改网络设置；未确认的项目不会算作通过。
      </p>
      {busy && (
        <p role="status" className="notice mb-2">
          正在检测：{TITLES[busy as Kind]}
          。联网项目可能需要数十秒，关闭详情后仍会完成本轮。
        </p>
      )}
      {error && (
        <p role="alert" className="notice notice--danger mb-2">
          {error}
        </p>
      )}
      <Collapsible summary="UDP 出口对照" defaultOpen>
        <p className="notice mb-2">
          向 Cloudflare、Google 的 STUN 服务采样 IPv4 / IPv6，并与本轮 HTTP
          出口比较。UDP
          探针遵循本机路由，可能暴露该路由的公网出口；无响应只能表示未确认。
        </p>
        {button("stun", "检测 UDP 出口")}
        <DiagnosticResult report={results.stun} />
      </Collapsible>
      <Collapsible summary="代理运行规则">
        <p className="notice mb-2">
          读取 Mihomo / Clash
          兼容控制接口，核对规则顺序、目标和当前节点。支持这类接口的客户端可用，其他客户端仍可使用出口检测。
        </p>
        <div className="grid gap-2 sm:grid-cols-2 mb-2">
          <TextField
            label="本机控制接口"
            value={endpoint}
            placeholder="http://127.0.0.1:端口"
            disabled={!!busy}
            onChange={(value) => {
              setEndpoint(value);
              clear("proxy");
            }}
            hint="填写代理客户端显示的控制地址，仅接受回环 IP。"
          />
          <TextField
            label="控制接口密钥（可选）"
            type="password"
            value={secret}
            disabled={!!busy}
            onChange={(value) => {
              setSecret(value);
              clear("proxy");
            }}
            hint="仅用于本次读取，提交后清空。"
          />
        </div>
        {button("proxy", "核验运行规则", !endpoint.trim())}
        <DiagnosticResult report={results.proxy} />
      </Collapsible>
      <Collapsible summary="ASN 网络类型">
        <p className="notice mb-2">
          IPinfo 接收你填写的公网 IP，PeeringDB 接收对应
          ASN。网络类型只描述运营商，不证明具体 IP 是住宅或信誉良好。
        </p>
        <TextField
          label="查询公网 IP"
          value={ip}
          placeholder="IPv4 或 IPv6"
          disabled={!!busy}
          onChange={(value) => {
            setIp(value);
            clear("asn");
          }}
          className="mb-2"
        />
        {button("asn", "查询 ASN 类型", !ip.trim())}
        <DiagnosticResult report={results.asn} />
      </Collapsible>
      <Collapsible summary="Claude 客户端语言">
        <p className="notice mb-2">
          读取桌面端自己的语言配置，多份配置分别显示。配置值不等于运行中网页的实际语言。
        </p>
        <TextField
          label="期望语言（可选）"
          value={expected}
          placeholder="例如 en-US、ja-JP、zh-TW"
          disabled={!!busy}
          onChange={(value) => {
            setExpected(value);
            clear("locale");
          }}
          hint="留空只读取；填写后按完整语言标签比较，不按出口国家猜测语言。"
          className="mb-2"
        />
        {button("locale", "读取客户端语言")}
        <DiagnosticResult report={results.locale} />
      </Collapsible>
    </Card>
  );
}

function DiagnosticResult({ report }: { report?: Report }) {
  if (!report) return null;
  const labels = {
    pass: "已核实",
    warn: "需核对",
    fail: "失败",
    unknown: "未确认",
  } as const;
  const tones = {
    pass: "ok",
    warn: "warn",
    fail: "danger",
    unknown: "default",
  } as const;
  return (
    <div className="mt-2" aria-live="polite">
      <p className="notice mb-2">
        {report.checked_at} · {report.scope}
      </p>
      <div className="flex flex-col gap-1.5">
        {report.items.map((item) => (
          <div key={item.id} className="slotrow">
            <div className="slotrow-main">
              <span className="slotrow-label">{item.label}</span>
              <Pill tone={tones[item.state]}>{labels[item.state]}</Pill>
            </div>
            <p className="notice" style={{ overflowWrap: "anywhere" }}>
              {item.detail}
            </p>
          </div>
        ))}
      </div>
    </div>
  );
}
