/**
 * 环境体检 —— 浏览器侧十项加权指纹 + 本机侧四项检查。
 *
 * 从旧 `Environment.tsx` 里拆出来 —— 那个文件 391 行塞了五件事
 * （软件检测 / 安装 / 升级 / 时区 / 中文环境识别），而这一项本身是完整独立的
 * 话题：10 项加权指纹，背后 475 行 `signals.ts`。
 *
 * 它**没有自己的进度步骤**，归在 `environment` 那一步里 ——
 * `progress.json` 的五个 key 一个都不能多，见 `steps.ts`。
 */
import { useEffect, useState } from "react";
import { Globe, Play, RotateCw, Stethoscope, Wrench } from "lucide-react";
import { Link } from "react-router-dom";

import { api, type CheckItem } from "../../lib/api";
import {
  browserName,
  extraChecks,
  scanFromReport,
} from "../../lib/browserProbe";
import type { BrowserReport } from "../../lib/generated/BrowserReport";
import type { ProbeStart } from "../../lib/generated/ProbeStart";
import { R } from "../../lib/resources";
import { peek, put, refresh, useResource, useSession } from "../../lib/store";
import {
  Button,
  Card,
  Collapsible,
  EmptyState,
  ExternalLink,
  Metric,
  Pill,
  ProgressBar,
  Row,
  useToast,
} from "../../ui";
import { useChecks } from "./useChecks";

const BAND_LABEL = { low: "低", medium: "中", high: "高" } as const;
const BAND_TONE = { low: "ok", medium: "warn", high: "danger" } as const;

/** 右上角那个按钮，详情标题栏和评分格子共用。 */
export function SignalsRunButton({ compact = false }: { compact?: boolean }) {
  const check = useChecks().signals;
  return (
    <Button
      variant={check.done ? "default" : "primary"}
      size={compact ? "sm" : undefined}
      icon={check.done ? <RotateCw size={13} /> : <Play size={13} />}
      loading={check.running}
      onClick={() => void check.run()}
    >
      {check.running ? "检测中…" : check.done ? "重新检测" : "开始检测"}
    </Button>
  );
}

export function SignalsScore() {
  const scan = useResource("signals", R.signals);
  const check = useChecks().signals;
  const r = scan.data;

  if (!r && !check.running) {
    return (
      <Card className="mb-3">
        {(check.error ?? scan.error) && (
          <p className="notice notice--danger mb-2">
            {check.error ?? scan.error}
          </p>
        )}
        <EmptyState
          title="还没检测过"
          action={
            <Button
              variant="primary"
              icon={<Play size={13} />}
              onClick={() => void check.run()}
            >
              开始检测
            </Button>
          }
        >
          检测浏览器语言、已装字体、WebRTC、Emoji 渲染这些浏览器能读到的信号，
          以及系统时区。其中 WebRTC 那一项有 1 秒超时，整体大约两三秒。
        </EmptyState>
      </Card>
    );
  }

  return (
    <Card className="mb-3">
      {(check.error ?? scan.error) && (
        <p className="notice notice--danger mb-2">
          {check.error ?? scan.error}
        </p>
      )}
      <div className="flex items-end gap-3">
        <span className="text-[32px] leading-none font-medium">
          {check.running && !r ? "…" : (r?.total ?? "—")}
        </span>
        <span className="notice mb-1">/ 100</span>
        {r && (
          <span className="mb-1">
            <Pill tone={BAND_TONE[r.band]}>{BAND_LABEL[r.band]}风险</Pill>
          </span>
        )}
        {r && (
          <span className="notice mb-1 ml-auto">命中 {r.hits.length} 项</span>
        )}
      </div>
      {r && (
        <ProgressBar
          className="mt-2"
          value={r.total}
          tone={BAND_TONE[r.band]}
          label={`中文环境识别得分 ${r.total} / 100`}
        />
      )}
      {r && (
        <p className="notice mt-2">
          {r.source?.kind === "browser" ? (
            <>
              来源：<strong>{r.source.browser}</strong>（默认浏览器实测 ·{" "}
              {r.source.at.slice(5, 16)}）
            </>
          ) : (
            <>
              来源：面板内置的 WebView2 —— 不是你登 claude.ai 用的那个浏览器。
              下面「用默认浏览器测」能测那一个。
            </>
          )}
        </p>
      )}
      <p className="notice mt-2">
        分档：低 0–30、中 31–60、高 61–100；单项 score ≥ 0.25 计为命中。
        全部在本地算，不上传任何数据。
      </p>
    </Card>
  );
}

/** 用哪个会话键记最近一份实测报告。面板开着期间都在；后端内存里也有一份。 */
const REPORT_KEY = "security.browserProbe";

/**
 * 用系统默认浏览器实测（2026-09-24，参照 CheckClaude 的 BrowserBridge）。
 *
 * 上面那十项默认在面板内置的 WebView2 里测 —— 那不是使用者登 claude.ai 的浏览器。
 * 点一下，后端在 127.0.0.1 上开一个一次性小服务（`qb-app::browser_probe`），用默认浏览器
 * 打开 `probe.html`；那一页跑同一份 `signals.ts`，结果交回来后在这里按同一张权重表算分，
 * 顶掉「中文环境」那一份（带着来源），出口一致性那两行（浏览器语言、WebRTC）也改用实测。
 */
export function BrowserProbeCard() {
  const toast = useToast();
  const [report, setReport] = useSession<BrowserReport | null>(
    REPORT_KEY,
    null,
  );
  const [started, setStarted] = useState<ProbeStart | null>(null);
  const [phase, setPhase] = useState<"idle" | "waiting" | "timeout">("idle");
  const [err, setErr] = useState("");

  // 面板刷新过页面但后端还开着：内存里那一份拿回来显示。
  useEffect(() => {
    if (report) return;
    void api
      .browserProbeLast()
      .then((r) => r && setReport(r))
      .catch(() => undefined);
    // 只在第一次挂上时问一次。
  }, []);

  const run = async () => {
    setErr("");
    setPhase("waiting");
    try {
      const s = await api.browserProbeStart();
      setStarted(s);
      const r = await api.browserProbeWait(s.token);
      if (!r) {
        setPhase("timeout");
        return;
      }
      setReport(r);
      setPhase("idle");
      const scan = scanFromReport(r);
      if (scan) put("signals", scan);
      toast.ok(`${browserName(r.user_agent)} 的采集结果交回来了`);
      // 出口一致性那两行要用这份实测 —— 已经测过的话顺手重测一次。
      if (peek("egress")) void refresh("egress");
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
      setPhase("idle");
    }
  };

  const copy = () => {
    if (!started) return;
    void navigator.clipboard
      ?.writeText(started.url)
      .then(() => toast.info("链接复制好了，粘到要测的那个浏览器里打开"))
      .catch(() => undefined);
  };

  const extras = report ? extraChecks(report) : [];

  return (
    <Card
      as="h3"
      title="用默认浏览器测"
      icon={<Globe size={14} />}
      className="mb-3"
      actions={
        <Button
          size="sm"
          variant={report ? "default" : "primary"}
          icon={<Play size={12} />}
          loading={phase === "waiting"}
          onClick={() => void run()}
        >
          {phase === "waiting"
            ? "等浏览器交回…"
            : report
              ? "再测一次"
              : "用默认浏览器测"}
        </Button>
      }
    >
      <p className="notice mb-2">
        上面那十项默认是在面板<strong>内置的 WebView2</strong> 里测的 ——
        那不是你登 claude.ai 用的那个浏览器，语言、WebRTC、扩展都可能不一样。
        点这里，面板在 127.0.0.1 上开一个一次性页面、用系统默认浏览器打开，
        测它真正的语言、时区、字体、WebRTC 和请求头。结果只回到本机面板；
        链接只能用一次，两分钟后作废。
      </p>

      {phase === "waiting" && started && (
        <div className="mb-2">
          <p className="notice">
            {started.opened
              ? "已经用默认浏览器打开了，等它交回结果（最多两分钟）……"
              : "页面开好了，但没能替你打开浏览器 —— 把下面的链接复制到浏览器里打开。"}{" "}
            想测别的浏览器，把链接复制过去打开就行（只认第一次交回的那一份）。
          </p>
          <div className="mt-1 flex flex-wrap items-center gap-2">
            <code className="break-all">{started.url}</code>
            <Button size="sm" onClick={copy}>
              复制链接
            </Button>
          </div>
        </div>
      )}
      {phase === "timeout" && (
        <p className="notice notice--danger mb-2">
          两分钟没等到结果，那个页面已经作废了。浏览器里如果显示「采集失败」，
          照那一句处理；没打开的话点「再测一次」，或者复制链接手动打开。
        </p>
      )}
      {err && <p className="notice notice--danger mb-2">{err}</p>}

      {report && (
        <>
          <p className="notice">
            最近一次：<strong>{browserName(report.user_agent)}</strong> ·{" "}
            {report.received_at}
            {report.accept_language && (
              <>
                {" "}
                · 请求头语言 <code>{report.accept_language}</code>
              </>
            )}
          </p>
          {extras.map((x) => (
            <Row
              key={x.id}
              side={
                <Pill tone={ITEM_TONE[x.state]}>{ITEM_LABEL[x.state]}</Pill>
              }
            >
              <span>{x.label}</span>
              <span className="notice">{x.detail}</span>
            </Row>
          ))}
          <p className="notice mt-1">
            这两条只报告、不进上面那 100
            分：它们说明的是浏览器里装了什么，跟出口无关。
          </p>
        </>
      )}
    </Card>
  );
}

export function SignalsBreakdown() {
  const r = useResource("signals", R.signals).data;
  if (!r) return null;
  return (
    <Card as="h3" title="逐项" className="mb-3">
      {r.signals.map((s) => (
        <Row
          key={s.id}
          side={
            <>
              <span className="notice max-w-[16ch] truncate" title={s.raw}>
                {s.raw}
              </span>
              <Pill tone={BAND_TONE[s.verdict]}>+{s.points}</Pill>
            </>
          }
        >
          <span title={s.hint}>{s.label}</span>
          <span className="notice">权重 {s.weight}</span>
          {s.claudeUsed && <Pill tone="accent">Claude 会读</Pill>}
          {!s.fixable && s.points > 0 && <Pill tone="warn">不可修复</Pill>}
        </Row>
      ))}
      {/* 逐项的 points 是各自四舍五入的，加起来不等于上面那个总分
          （总分是先求和再取整）。所以这里不给「小计」，免得对不上。 */}
      <p className="notice mt-2">
        逐项分数各自取整，相加与上方总分可能差 1–2 分 ——
        总分是先求和再取整的，以总分为准。
      </p>
    </Card>
  );
}

export function SignalsFixable() {
  const r = useResource("signals", R.signals).data;
  if (!r) return null;
  return (
    <Card
      as="h3"
      title="能修的与修不了的"
      icon={<Wrench size={14} />}
      className="mb-3"
    >
      <div className="grid gap-2 sm:grid-cols-2">
        <Metric label="能修" hint="改系统设置或卸载对应软件就能降下来">
          {r.signals.filter((s) => s.fixable && s.points > 0).length} 项 ·{" "}
          {r.signals.filter((s) => s.fixable).reduce((a, s) => a + s.points, 0)}{" "}
          分
        </Metric>
        <Metric label="修不了" hint="删掉会让中文显示整体崩坏，代价远大于收益">
          {r.signals.filter((s) => !s.fixable && s.points > 0).length} 项 ·{" "}
          {r.signals
            .filter((s) => !s.fixable)
            .reduce((a, s) => a + s.points, 0)}{" "}
          分
        </Metric>
      </div>
      <p className="notice mt-3">
        <strong>已装中文字体那 14 分修不掉</strong>：删掉系统中文字体会让中文
        显示整体崩坏，代价远大于收益。面板如实展示分数并标注「不可修复」，
        <strong>不提供修复按钮，也不假装能修</strong>。
      </p>
      <div className="mt-2 flex flex-wrap gap-2">
        <Link className="btn btn--md" to="/software">
          去对齐系统时区
        </Link>
      </div>
    </Card>
  );
}

export function SignalsWhy() {
  return (
    <>
      <Collapsible summary="两条不能改错的判定">
        <p className="notice">
          <strong>台湾不计分。</strong>
          <code>Asia/Taipei</code> 与 <code>zh-TW</code> 是 Anthropic
          完整支持的地区， 给它们加分属于误报。港澳属受限地区，保留部分风险分。
        </p>
        <p className="notice mt-2">
          <strong>繁体字体压在命中阈值以下。</strong>
          繁体字体在台湾很常见，而且区分不出 TW 与 HK/MO，所以给的分刻意低于
          0.25 的命中线。
        </p>
        <p className="notice mt-2">
          这两条原样继承自上游 issue，动了就是误报。
        </p>
      </Collapsible>

      <Collapsible summary="来源与那个「未证实」的说法">
        <p className="notice">
          评分逻辑改编自{" "}
          <ExternalLink href="https://github.com/LinXiaoTao/FuckClaude">
            FuckClaude
          </ExternalLink>
          （MIT），逐条说明见仓库根目录的 ATTRIBUTION.md。
        </p>
        <p className="notice mt-2">
          另有一种说法称 Claude Code 会把系统时区用 Unicode 隐写术藏进 system
          prompt。这是<strong>第三方逆向分析主张，本项目未做独立验证</strong>，
          不作为既定事实。注意其描述的触发条件是走中转端点， 官方 OAuth
          直连不在该描述范围内。
        </p>
      </Collapsible>
    </>
  );
}

const ITEM_TONE = {
  pass: "ok",
  warn: "warn",
  fail: "danger",
  unknown: "default",
} as const;
const ITEM_LABEL = {
  pass: "通过",
  warn: "注意",
  fail: "有问题",
  unknown: "查不出",
} as const;

/**
 * 本机侧体检 —— 浏览器里看不到的那几项。
 *
 * 项目划分参考 check-cc 与 claude-antiban-macos（都是 MIT），Windows 侧读法自己写。
 *
 * # 为什么没有「一键修复」按钮
 *
 * 检测本身只读。IPv6 的禁用/恢复入口在 IP 纯净度里，默认启动应用并保留逐网卡
 * 原值；代理和浏览器策略仍走各自的显式操作入口，不能混进体检自动执行。
 */
export function LocalCheckup() {
  const checkup = useResource("checkup", R.checkup);
  const c = checkup.data;

  return (
    <Card
      as="h3"
      title="本机体检"
      icon={<Stethoscope size={14} />}
      className="mb-3"
      actions={
        <Button
          size="sm"
          variant="primary"
          icon={<Play size={12} />}
          loading={checkup.loading}
          onClick={() => void checkup.refresh()}
        >
          {c ? "重新体检" : "开始体检"}
        </Button>
      }
    >
      <p className="notice mb-2">
        读注册表与本地配置文件，<strong>不改任何东西</strong>。
        查的是浏览器指纹看不到的那几项：代理形态（TUN / 系统代理 / PAC）、
        出口一致性、Anthropic 服务可达、claude.ai 的解析、IPv6 出口、浏览器 DoH
        策略、环境变量残留，以及 MCP / Codex 配置里有没有写成明文的密钥。
        联网的那几项只在你点的这一次发，<strong>都不带任何账户凭据</strong>。
      </p>

      {checkup.error && (
        <p className="notice notice--danger">{checkup.error}</p>
      )}

      {!c && !checkup.loading && (
        <EmptyState title="还没体检过">
          点右上角「开始体检」。它只读不写，随时可以再跑一次。
        </EmptyState>
      )}

      {c?.items.map((it: CheckItem) => (
        <Row
          key={it.id}
          side={<Pill tone={ITEM_TONE[it.state]}>{ITEM_LABEL[it.state]}</Pill>}
        >
          <span>{it.label}</span>
          <span className="notice">{it.detail}</span>
          {it.manual && (
            <Collapsible className="mt-1" summary="要自己动手的话">
              <pre className="logview whitespace-pre-wrap">{it.manual}</pre>
            </Collapsible>
          )}
        </Row>
      ))}

      {!!c?.env.length && (
        <div className="mt-2">
          <p className="notice">
            <strong>这些环境变量会影响 Claude Code 的行为。</strong>
            值是掩码过的：Key 类只说「已设置」，地址类<strong>
              只留 host
            </strong>{" "}
            —— 有些中转站把 token 放在路径里，显示全量等于把它印在截图上。
          </p>
          {c.env.map((e) => (
            <Row key={`${e.scope}-${e.name}`} side={<Pill>{e.scope}</Pill>}>
              <span className="font-mono">{e.name}</span>
              <span className="notice font-mono">{e.shown}</span>
            </Row>
          ))}
        </div>
      )}

      {!!c?.secrets.length && (
        <div className="mt-2">
          <p className="notice notice--danger">
            <strong>下面这些位置写着明文密钥。</strong>
            面板<strong>只报位置，不读也不显示内容</strong> ——
            报出来的东西迟早会 被打进日志、截图或者 issue
            里，所以结构上就不带值。
          </p>
          {c.secrets.map((h, i) => (
            <Row key={i} side={<Pill tone="danger">{h.field}</Pill>}>
              <span className="w-full break-all font-mono text-xs">
                {h.file}
              </span>
            </Row>
          ))}
        </div>
      )}
    </Card>
  );
}
