/**
 * 七个安全对象的唯一定义处：叫什么、详情由哪些块组成、标题栏上放什么按钮。
 *
 * **没有「安全」页了。** 七项全在总览上：五项是评分卡里的明细格，
 * 执行锁与会话内门禁挂在评分卡右上角那排门禁读数上，点下去都开
 * `SecuritySheet` 那个小窗。留一个安全页等于同一份内容两个入口，
 * 正是这次重排要消灭的那类重复。
 *
 * 所以这张表现在只有一个消费者（`SecuritySheet`），但仍然独立成文件：
 * 「叫什么、详情由哪些块组成」是会各自变的事，混进弹窗组件里，
 * 下次加一项就得在弹窗里翻。（原来还有一个给旧安全页左栏用的 `useReadouts`，
 * 那一页没了之后它一直没人调，2026-09-27 删了。）
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

import { AllowlistPanel, LockPanel, SessionGatePanel } from "./GatePanel";
import {
  PurityFailure,
  PurityPill,
  PurityProbe,
  PurityVerdict,
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
