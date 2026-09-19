/**
 * 订阅页：Claude 与 ChatGPT 的官方订阅怎么买、为什么要自己买。
 *
 * 五个分页，默认落在「首页」（为什么自己订阅 + 价目 + 倍率换算 + 计算器）。
 * 三条购买路线与答疑页要先读过一次知情说明才解锁；确认记在本机 localStorage。
 *
 * 这一页不联网、不读账号、不写文件、没有 Rust 命令 —— 全部是资料与本地算术。
 * 资料与数字都在 `data.ts`，改文案别碰组件。
 */
import { useState } from "react";
import {
  Apple,
  CreditCard,
  HelpCircle,
  Home,
  Lock,
  Smartphone,
  type LucideIcon,
} from "lucide-react";
import { Button, Pill } from "../../ui";
import { AckModal, readAck, writeAck } from "./AckModal";
import { REVIEWED_ON } from "./data";
import { Faq } from "./Faq";
import { RouteAndroid } from "./RouteAndroid";
import { RouteApple } from "./RouteApple";
import { RouteCard } from "./RouteCard";
import { WhyOfficial } from "./WhyOfficial";

type Tab = "why" | "apple" | "android" | "card" | "faq";

const TABS: { id: Tab; name: string; hint: string; icon: LucideIcon }[] = [
  { id: "why", name: "首页", hint: "为什么自己订阅", icon: Home },
  { id: "apple", name: "苹果内购", hint: "首选", icon: Apple },
  { id: "android", name: "安卓", hint: "Google Play", icon: Smartphone },
  { id: "card", name: "网页绑卡", hint: "本人银行卡", icon: CreditCard },
  {
    id: "faq",
    name: "答疑与合规",
    hint: "地址 · 退订 · FAQ",
    icon: HelpCircle,
  },
];

export default function Subscription() {
  const [acknowledged, setAcknowledged] = useState<boolean>(readAck);
  const [showAck, setShowAck] = useState<boolean>(() => !readAck());
  const [tab, setTab] = useState<Tab>("why");

  const confirm = () => {
    writeAck();
    setAcknowledged(true);
    setShowAck(false);
  };

  const locked = !acknowledged && tab !== "why";

  return (
    <div className="qb-subscription">
      <AckModal
        open={showAck}
        acknowledged={acknowledged}
        onConfirm={confirm}
        onClose={() => setShowAck(false)}
      />

      <header className="qb-page-heading">
        <div>
          <h1>官方订阅指南</h1>
          <p>
            Claude 与 ChatGPT
            的各档订阅怎么买、要多少钱，以及为什么不要买中转站的「会员」。
          </p>
        </div>
        <div className="qb-sub-heading-side">
          <Pill tone="ok">资料复核 {REVIEWED_ON}</Pill>
          <Pill>个人经验整理 · 非官方</Pill>
        </div>
      </header>

      <div className="qb-tabs" role="tablist" aria-label="订阅指南分页">
        {TABS.map((t) => {
          const Icon = t.icon;
          return (
            <button
              key={t.id}
              type="button"
              role="tab"
              aria-selected={tab === t.id}
              onClick={() => setTab(t.id)}
            >
              <Icon size={14} aria-hidden="true" />
              {t.name}
              <span>{t.hint}</span>
            </button>
          );
        })}
      </div>

      {locked ? (
        <div className="qb-sub-locked">
          <Lock size={28} className="text-accent" aria-hidden="true" />
          <p className="qb-sub-locked-title">先读一遍知情说明</p>
          <p className="sub">
            购买步骤里有地址、付款与条款相关的提醒，读过一次再看步骤。首页不需要确认，可以先看。
          </p>
          <Button variant="primary" onClick={() => setShowAck(true)}>
            打开知情说明
          </Button>
        </div>
      ) : (
        <>
          {tab === "why" && <WhyOfficial />}
          {tab === "apple" && <RouteApple />}
          {tab === "android" && <RouteAndroid />}
          {tab === "card" && <RouteCard />}
          {tab === "faq" && <Faq onReopenAck={() => setShowAck(true)} />}
        </>
      )}
    </div>
  );
}
