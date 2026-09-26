/**
 * DNS 泄露 —— 简易通过（面板自己查）与高级通过（交给 Codex）两块。
 *
 * 检测动作本身不在这里，在 `useChecks.ts`：同一个动作总览那边也要按，
 * 写两遍迟早有一边忘了记进度。
 */
import { useState } from "react";
import { Play } from "lucide-react";
import { Link } from "react-router-dom";

import { R } from "../../lib/resources";
import { isLegacyDnsReport } from "../../lib/score";
import { useResource } from "../../lib/store";
import { useTask } from "../../lib/tasks";
import { DNS_LEAK_PROMPT, QUICKSTART_DOC } from "../../prompts";
import {
  Bullet,
  Button,
  Card,
  CodeBlock,
  Collapsible,
  EmptyState,
  ExternalLink,
  Metric,
  Pill,
  ProgressBar,
  Row,
} from "../../ui";
import { useChecks } from "./useChecks";

/** 一次只列这么多解析器，其余折起来 —— 但要给真的展开按钮，不是死截断。 */
const HEAD = 12;

/** 右上角那个按钮，详情标题栏和评分格子共用。 */
export function DnsRunButton({ compact = false }: { compact?: boolean }) {
  const check = useChecks().dns;
  return (
    <Button
      variant={check.done ? "default" : "primary"}
      size={compact ? "sm" : undefined}
      icon={<Play size={13} />}
      loading={check.running}
      onClick={() => void check.run()}
    >
      {check.running ? "检测中…" : check.done ? "重新检测" : "开始检测"}
    </Button>
  );
}

/** 后端逐个域名报进度，不再是六秒钟一动不动。 */
export function DnsProgress() {
  const check = useChecks().dns;
  const task = useTask("dns-probe");
  if (!check.running) return null;
  return (
    <div className="mb-3">
      <div className="mb-1.5 flex items-center gap-2">
        <span className="text-sm">
          {task.phase || "正在向 bash.ws 取测试 id…"}
        </span>
        {task.total > 0 && (
          <span className="notice ml-auto">
            {task.step} / {task.total}
          </span>
        )}
      </div>
      <ProgressBar
        value={task.total > 0 ? (task.step / task.total) * 100 : undefined}
        label="DNS 检测进度"
      />
    </div>
  );
}

export function DnsResult() {
  const dns = useResource("dns", R.dns);
  const check = useChecks().dns;
  const [showAll, setShowAll] = useState(false);

  const report = dns.data;
  // 存盘的旧报告（2026-09-24 之前的评分规则）：分数不显示，请使用者重测。
  const legacy = report ? isLegacyDnsReport(report) : false;
  const resolvers = report?.resolvers ?? [];
  const shown = showAll ? resolvers : resolvers.slice(0, HEAD);

  return (
    <Card as="h3" title="简易通过" className="mb-3">
      {(check.error ?? dns.error) && (
        <p className="notice notice--danger mb-2">{check.error ?? dns.error}</p>
      )}

      {!report && !check.running ? (
        <EmptyState
          title="还没检测过"
          action={
            <Button
              variant="primary"
              icon={<Play size={13} />}
              onClick={() => void check.run()}
            >
              开始检测（约 6 秒）
            </Button>
          }
        >
          向 bash.ws 取一个测试 id，依次解析 10 个探针域名，由它的权威域名服务器
          回报「是谁来查的」，再叠加本机网卡 DNS 配置一起判定。
        </EmptyState>
      ) : report ? (
        <>
          <div className="grid gap-2 sm:grid-cols-3">
            <Metric label="判定">
              {report.passed ? (
                <Pill tone="ok">通过</Pill>
              ) : (
                <Pill tone="danger">发现 {report.findings.length} 项问题</Pill>
              )}
            </Metric>
            <Metric label="解析器数量">{report.resolvers.length}</Metric>
            <Metric label="出口 ASN" emptyHint="回显里没带 ASN">
              {report.egress_asn}
            </Metric>
            <Metric label="DNS 评分">
              {report.score === null || legacy ? (
                <Pill>—</Pill>
              ) : (
                <Pill
                  tone={
                    report.score === 100
                      ? "ok"
                      : report.score >= 60
                        ? "warn"
                        : "danger"
                  }
                >
                  {report.score} / 100
                </Pill>
              )}
            </Metric>
            <Metric label="网卡 DNS">
              {report.adapters_safe === true ? (
                <Pill tone="ok">走隧道</Pill>
              ) : report.adapters_safe === false ? (
                <Pill tone="danger">绕过隧道</Pill>
              ) : (
                <Pill>不适用</Pill>
              )}
            </Metric>
          </div>
          <p className="notice mt-2">
            {legacy
              ? "这份结果是按旧的评分规则测的（按名字找「以太网」、断开的网卡也扣分），请重测一次。"
              : report.adapters_note}{" "}
            评分只算适用的项：真实解析里没有国内解析器 70 分；开着 TUN
            时，连着的网卡（有线、Wi-Fi 都算）上配的 DNS 都走隧道 30 分。
          </p>

          {report.findings.length > 0 && (
            <div className="mt-3 rounded-[var(--radius-md)] border border-[var(--danger-border)] bg-[var(--danger-bg)] px-3 py-2">
              {report.findings.map((f, i) => (
                <Bullet key={f} marker={i + 1} tone="danger">
                  {f}
                </Bullet>
              ))}
            </div>
          )}

          {resolvers.length > 0 && (
            <div className="mt-3">
              {shown.map((r) => (
                <Row
                  key={`${r.address}-${r.interface ?? ""}`}
                  side={
                    <>
                      {r.from_adapter && (
                        <Pill tone="default">网卡 {r.interface}</Pill>
                      )}
                      {r.from_adapter && r.connected === false && (
                        <Pill tone="default">已断开 · 不看</Pill>
                      )}
                      {r.from_adapter &&
                        !r.tunnel &&
                        r.connected !== false &&
                        r.via_tunnel === true && <Pill tone="ok">走隧道</Pill>}
                      {r.is_domestic && <Pill tone="danger">国内</Pill>}
                      {r.is_private && <Pill tone="default">内网</Pill>}
                    </>
                  }
                >
                  <span className="font-mono">{r.address}</span>
                  <span className="notice">
                    {r.country_name ??
                      r.country_code ??
                      (r.is_private ? "内网地址" : "归属未知")}
                    {r.asn ? ` · ${r.asn}` : ""}
                  </span>
                </Row>
              ))}
              {resolvers.length > HEAD && (
                <Button
                  variant="ghost"
                  size="sm"
                  className="mt-1.5"
                  onClick={() => setShowAll((v) => !v)}
                >
                  {showAll ? "收起" : `展开另外 ${resolvers.length - HEAD} 条`}
                </Button>
              )}
            </div>
          )}

          {report.upstream_conclusion && (
            <p className="notice mt-3">
              bash.ws 结论：{report.upstream_conclusion}
            </p>
          )}
          <p className="notice mt-1">{report.note}</p>
        </>
      ) : null}
    </Card>
  );
}

export function DnsAdvanced() {
  return (
    <Card as="h3" title="高级通过" tone="accent">
      <p className="notice mb-3">
        让 Codex 读网卡配置、路由表、系统代理、浏览器 DoH，必要时用 PktMon
        抓包核实， 发现问题给出修复方案。
        <strong>{DNS_LEAK_PROMPT.modelHint}</strong>
      </p>

      <div className="mb-3 flex flex-wrap gap-2">
        {/* 这两件事在两个页面上。原来合成一个按钮只跳中转站，
            「安装 Codex」那半是个没兑现的承诺。 */}
        <Link className="btn btn--md" to="/relays">
          配置中转站
        </Link>
        <Link className="btn btn--md" to="/software">
          安装 Codex
        </Link>
        <ExternalLink href={QUICKSTART_DOC} asButton>
          新手指引文档
        </ExternalLink>
      </div>

      <Collapsible summary="查看提示词全文">
        <CodeBlock
          text={DNS_LEAK_PROMPT.body}
          caption={DNS_LEAK_PROMPT.title}
        />
        <p className="notice mt-2">
          提示词可以在「设置」页里逐份查看与复制。三份都刻意写成
          「先诊断、再报告、要确认才动手」—— 让模型直接对网络配置动手，
          出错代价比多问一句大得多。
        </p>
      </Collapsible>
    </Card>
  );
}

export function DnsWhy() {
  return (
    <Collapsible className="mt-3" summary="简易通过是怎么判的？">
      <p className="notice">
        两条腿。<strong>真实解析回显</strong>：向 bash.ws 取测试 id，依次解析 10
        个探针域名（间隔 200 毫秒，各等 3 秒），由它的权威域名服务器回报
        「是谁来查的」。测试 id <strong>必须由服务端签发</strong>——
        自己随机生成的 id 查回来永远是空清单，「没查到解析器」会被误读成
        「没有泄露」。
      </p>
      <p className="notice mt-2">
        <strong>网卡配置检查</strong>：只在 TUN 开着时算。看
        <strong>连着的</strong>物理网卡（有线、Wi-Fi 都算）上配的
        DNS，这条查询从哪张网卡出去：进隧道就没事，从物理网卡直接出去才是缺口 ——
        Windows 会同时向每张网卡的 DNS 发查询。断开的网卡、隧道网卡自己的 DNS
        不看；没开 TUN 时没有隧道可绕，这一项不计分。
      </p>
      <p className="notice mt-2">
        用 fake-ip 模式的代理（Clash / mihomo 这类）时，探针域名会被解析成{" "}
        <code>198.18.x.x</code>
        ，查询根本没从本机发出去，bash.ws 收不到回显 ——
        这时判不了，报告里会单独说明，不是检测坏了。
      </p>
    </Collapsible>
  );
}
