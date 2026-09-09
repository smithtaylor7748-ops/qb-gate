import { useState } from 'react';
import { Play, SkipForward } from 'lucide-react';

import type { DnsReport } from '../lib/api';
import { useNav } from '../lib/nav';
import { R } from '../lib/resources';
import { peek, useResource } from '../lib/store';
import { useTask } from '../lib/tasks';
import { DNS_LEAK_PROMPT, QUICKSTART_DOC } from '../prompts';
import {
  Bullet,
  Button,
  Card,
  CodeBlock,
  Collapsible,
  EmptyState,
  ExternalLink,
  Metric,
  PageHeader,
  Pill,
  ProgressBar,
  Row,
  useToast,
} from '../ui';

/** 一次只列这么多解析器，其余折起来 —— 但要给真的展开按钮，不是死截断。 */
const HEAD = 12;

export default function DnsLeak() {
  const { mark, go } = useNav();
  const toast = useToast();
  const dns = useResource('dns', R.dns);
  const task = useTask('dns-probe');
  const [showAll, setShowAll] = useState(false);

  const report = dns.data;

  async function run() {
    await dns.refresh();
    // 注意：`dns.data` 是本次渲染的快照，refresh 之后它还是旧值。
    // 要拿刚回来的结果得直接读缓存。
    const r = peek<DnsReport>('dns');
    if (!r) return;
    const risk = r.passed ? 'low' : r.findings.length > 1 ? 'high' : 'medium';
    await mark(
      'dns',
      r.passed ? 'passed' : 'failed',
      risk,
      r.passed
        ? '简易检测通过，未发现配置层或解析层泄露'
        : `发现 ${r.findings.length} 项：${r.findings[0]}`
    );
    if (r.passed) toast.ok('未发现泄露');
    else toast.error(`发现 ${r.findings.length} 项问题`);
  }

  async function skip() {
    await mark('dns', 'skipped', 'unknown', '用户强制跳过，未做 DNS 泄露检测');
    toast.info('已标记为跳过');
  }

  const resolvers = report?.resolvers ?? [];
  const shown = showAll ? resolvers : resolvers.slice(0, HEAD);

  return (
    <>
      <PageHeader
        title="DNS 泄露"
        sub="简易通过走真实解析回显加网卡配置两层；高级通过交给 Codex 深查并修复。"
        actions={
          <Button
            variant="primary"
            icon={<Play size={13} />}
            loading={dns.loading}
            onClick={run}
          >
            {dns.loading ? '检测中…' : report ? '重新检测' : '开始检测'}
          </Button>
        }
      />

      {/* 后端逐个域名报进度，不再是六秒钟一动不动。 */}
      {dns.loading && (
        <div className="mb-3">
          <div className="mb-1.5 flex items-center gap-2">
            <span className="text-sm">{task.phase || '正在向 bash.ws 取测试 id…'}</span>
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
      )}

      <Card title="简易通过" className="mb-3">
        {dns.error && <p className="notice notice--danger mb-2">{dns.error}</p>}

        {!report && !dns.loading ? (
          <EmptyState
            title="还没检测过"
            action={
              <Button variant="primary" icon={<Play size={13} />} onClick={run}>
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
                <Pill tone={report.score === 100 ? 'ok' : report.score >= 60 ? 'warn' : 'danger'}>{report.score} / 100</Pill>
              </Metric>
              <Metric label="以太网状态">
                {report.ethernet_safe === true ? <Pill tone="ok">未发现泄露</Pill> : report.ethernet_safe === false ? <Pill tone="danger">发现风险</Pill> : <Pill>未检测到以太网</Pill>}
              </Metric>
            </div>
            <p className="notice mt-2">建议优先选择以太网连接；评分只反映当前检测到的配置，不读取或记录任何账户额度信息。</p>

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
                    key={`${r.address}-${r.interface ?? ''}`}
                    side={
                      <>
                        {r.from_adapter && <Pill tone="default">网卡 {r.interface}</Pill>}
                        {r.is_domestic && <Pill tone="danger">国内</Pill>}
                        {r.is_private && <Pill tone="default">内网</Pill>}
                      </>
                    }
                  >
                    <span className="font-mono">{r.address}</span>
                    <span className="notice">
                      {r.country_name ?? r.country_code ?? (r.is_private ? '内网地址' : '归属未知')}
                      {r.asn ? ` · ${r.asn}` : ''}
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
                    {showAll
                      ? '收起'
                      : `展开另外 ${resolvers.length - HEAD} 条`}
                  </Button>
                )}
              </div>
            )}

            {report.upstream_conclusion && (
              <p className="notice mt-3">bash.ws 结论：{report.upstream_conclusion}</p>
            )}
            <p className="notice mt-1">{report.note}</p>
          </>
        ) : null}
      </Card>

      <Card title="高级通过" tone="accent">
        <p className="notice mb-3">
          让 Codex 读网卡配置、路由表、系统代理、浏览器 DoH，必要时用 PktMon 抓包核实，
          发现问题给出修复方案。<strong>{DNS_LEAK_PROMPT.modelHint}</strong>
        </p>

        <div className="mb-3 flex flex-wrap gap-2">
          {/* 这两件事在两个页面上。原来合成一个按钮只跳中转站，
              「安装 Codex」那半是个没兑现的承诺。 */}
          <Button onClick={() => go('relay')}>配置中转站</Button>
          <Button onClick={() => go('environment')}>安装 Codex</Button>
          <ExternalLink href={QUICKSTART_DOC} asButton>
            新手指引文档
          </ExternalLink>
        </div>

        <Collapsible summary="查看提示词全文">
          <CodeBlock text={DNS_LEAK_PROMPT.body} caption={DNS_LEAK_PROMPT.title} />
          <p className="notice mt-2">
            提示词可以在「设置」页里逐份查看与复制。三份都刻意写成
            「先诊断、再报告、要确认才动手」—— 让模型直接对网络配置动手，
            出错代价比多问一句大得多。
          </p>
        </Collapsible>
      </Card>

      <Collapsible className="mt-3" summary="简易通过是怎么判的？">
        <p className="notice">
          两条腿。<strong>真实解析回显</strong>：向 bash.ws 取测试 id，依次解析
          10 个探针域名（间隔 200 毫秒，各等 3 秒），由它的权威域名服务器回报
          「是谁来查的」。测试 id <strong>必须由服务端签发</strong>——
          自己随机生成的 id 查回来永远是空清单，「没查到解析器」会被误读成
          「没有泄露」。
        </p>
        <p className="notice mt-2">
          <strong>网卡配置检查</strong>：看有没有物理网卡的 DNS 指向内网路由器。
          TUN 网卡上的 <code>172.18.0.2</code> 是合成地址，属于正常，不报。
        </p>
      </Collapsible>

      <div className="mt-4 flex justify-end">
        <Button variant="ghost" icon={<SkipForward size={13} />} onClick={skip}>
          标记为已跳过
        </Button>
      </div>
    </>
  );
}
