/**
 * 七个安全对象的唯一定义处：叫什么、状态一行怎么写、详情由哪些块组成。
 *
 * **没有「安全」页了。** 七项全在总览上：五项是评分卡里的明细格，
 * 执行锁与会话内门禁挂在评分卡右上角那排门禁读数上，点下去都开
 * `SecuritySheet` 那个小窗。留一个安全页等于同一份内容两个入口，
 * 正是这次重排要消灭的那类重复。
 *
 * 所以这张表现在只有一个消费者（`SecuritySheet`），但仍然独立成文件：
 * 「叫什么、状态怎么写、详情由哪些块组成」是三件会各自变的事，
 * 混进弹窗组件里，下次加一项就得在弹窗里翻。
 */
import {
  Fingerprint,
  Globe,
  ListChecks,
  Lock,
  Route,
  ShieldAlert,
  Stethoscope,
  type LucideIcon,
} from "lucide-react";

import { describeLease } from "../../lib/lease";
import { R } from "../../lib/resources";
import { useResource } from "../../lib/store";
import { AllowlistPanel, LockPanel, SessionGatePanel } from "./GatePanel";
import {
  PurityFailure,
  PurityPill,
  PurityProbe,
  PurityVerdict,
  usePurityVerdict,
} from "./PurityPanel";
import {
  DnsAdvanced,
  DnsProgress,
  DnsResult,
  DnsRunButton,
  DnsWhy,
} from "./DnsPanel";
import { EgressResult, EgressRunButton, EgressWhy } from "./EgressPanel";
import {
  BrowserProbeCard,
  LocalCheckup,
  SignalsBreakdown,
  SignalsFixable,
  SignalsRunButton,
  SignalsScore,
  SignalsWhy,
} from "./SignalsPanel";

export type ObjectId =
  "purity" | "dns" | "signals" | "egress" | "allowlist" | "lock" | "session";

export type Tone = "" | "ok" | "warn" | "danger";

export interface SecurityObject {
  id: ObjectId;
  name: string;
  /** 一行说清这个对象管什么。没有读数时顶上，不是营销语。 */
  sub: string;
  icon: LucideIcon;
}

export const OBJECTS: SecurityObject[] = [
  {
    id: "purity",
    name: "IP 纯净度",
    sub: "三项硬指标，人工复核",
    icon: Fingerprint,
  },
  { id: "dns", name: "DNS 泄露", sub: "解析回显 + 网卡配置", icon: Globe },
  {
    id: "signals",
    name: "环境体检",
    sub: "浏览器指纹 + 本机检查",
    icon: Stethoscope,
  },
  {
    id: "egress",
    name: "出口一致性",
    sub: "跟出口 IP 对不上的东西",
    icon: Route,
  },
  { id: "allowlist", name: "IP 白名单", sub: "谁能过这道门", icon: ListChecks },
  {
    id: "lock",
    name: "执行锁",
    sub: "Deny ACE、残留副本、看门狗",
    icon: Lock,
  },
  {
    id: "session",
    name: "会话内门禁",
    sub: "请求发出前再验一次",
    icon: ShieldAlert,
  },
];

/** 详情正文。小窗和主从双栏用的是同一份。 */
export function ObjectDetail({ id }: { id: ObjectId }) {
  switch (id) {
    case "purity":
      return (
        <>
          <PurityFailure />
          <PurityVerdict />
          <PurityProbe />
        </>
      );
    case "dns":
      return (
        <>
          <DnsProgress />
          <DnsResult />
          <DnsAdvanced />
          <DnsWhy />
        </>
      );
    case "signals":
      return (
        <>
          <SignalsScore />
          <BrowserProbeCard />
          <SignalsBreakdown />
          <SignalsFixable />
          <LocalCheckup />
          <SignalsWhy />
        </>
      );
    case "egress":
      return (
        <>
          <EgressResult />
          <EgressWhy />
        </>
      );
    case "allowlist":
      return <AllowlistPanel />;
    case "lock":
      return <LockPanel />;
    case "session":
      return <SessionGatePanel />;
  }
}

/**
 * 标题栏右边那个东西 —— 三项检测是「开始 / 重新检测」，纯净度是结论 pill。
 *
 * 门禁那三项没有：它们的动作都带不可逆代价（上锁会把人关在门外、解锁会让
 * 门禁当场失效、清残留是永久删除），一个都不该放在标题栏上一点就走。
 */
export function ObjectAction({ id }: { id: ObjectId }) {
  if (id === "purity") return <PurityPill />;
  if (id === "dns") return <DnsRunButton />;
  if (id === "signals") return <SignalsRunButton />;
  if (id === "egress") return <EgressRunButton />;
  return null;
}

/**
 * 每个对象的状态读数。
 *
 * 没有结果时返回 `null`，调用方顶上那句 `sub` —— 六行全是「未检测」
 * 既不好看也没信息量，而说明至少告诉你点进去能干什么。
 */
export function useReadouts(): Partial<Record<ObjectId, [string, Tone]>> {
  const purity = usePurityVerdict();
  const dns = useResource("dns", R.dns);
  const signals = useResource("signals", R.signals);
  const gate = useResource("gate", R.gate);
  const hook = useResource("hook", R.hook);
  const egress = useResource("egress", R.egress);

  const out: Partial<Record<ObjectId, [string, Tone]>> = {};

  if (purity) {
    out.purity =
      purity === "passed"
        ? ["已复核通过", "ok"]
        : purity === "failed"
          ? ["复核不合格", "danger"]
          : ["已跳过", "warn"];
  }

  if (dns.data) {
    out.dns = dns.data.passed
      ? [`通过 · ${dns.data.score} / 100`, "ok"]
      : [
          `${dns.data.findings.length} 项问题 · ${dns.data.score} / 100`,
          "danger",
        ];
  }

  if (signals.data) {
    const t = signals.data.total;
    out.signals = [
      `识别度 ${t} / 100 · 命中 ${signals.data.hits.length} 项`,
      t <= 30 ? "ok" : t <= 60 ? "warn" : "danger",
    ];
  }

  if (egress.data) {
    const rows = egress.data.items.filter((i) => i.state !== "unknown");
    const bad = rows.filter((i) => i.state !== "pass").length;
    out.egress =
      rows.length === 0
        ? ["都查不了", ""]
        : bad === 0
          ? [`${rows.length} 项都对得上`, "ok"]
          : [`${bad} / ${rows.length} 项对不上`, bad > 1 ? "danger" : "warn"];
  }

  const st = gate.data;
  if (st) {
    out.allowlist =
      st.allowlist.length === 0
        ? ["空 —— 门禁未启用", "warn"]
        : [
            `${st.allowlist.length} 条 · 当前${st.ip_allowed ? "在名单内" : "不在名单内"}`,
            st.ip_allowed ? "ok" : "danger",
          ];

    const locked = st.targets.filter((t) => t.locked).length;
    const lease = describeLease(st.lease, "已放行给");
    out.lock = lease
      ? [lease.text, "ok"]
      : st.stale_copies.length > 0
        ? [
            `${locked} / ${st.targets.length} 已锁 · ${st.stale_copies.length} 个残留`,
            "danger",
          ]
        : [
            `${locked} / ${st.targets.length} 已锁`,
            locked === st.targets.length && locked > 0 ? "ok" : "warn",
          ];
  }

  if (hook.data)
    out.session = hook.data.installed ? ["已启用", "ok"] : ["未启用", ""];

  return out;
}
