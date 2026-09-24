import {
  AlertTriangle,
  BadgeDollarSign,
  CheckCircle2,
  Scale,
} from "lucide-react";
import { Card, Metric, Pill } from "../../ui";
import {
  OFFICIAL_BENEFITS,
  PLANS,
  RELAY_BANDS,
  RELAY_DOWNSIDES,
  WEEKS_PER_MONTH,
  monthlyValue,
  type Plan,
} from "./data";
import {
  YUAN_PER_USD,
  fmtMultiplier,
  fmtUsd,
  planMultiplier,
} from "./multiplier";
import { MultiplierCalculator } from "./MultiplierCalculator";
import { MultiplierMeter, PlanTable } from "./PlanTable";
import { SourceList, SourceRefs } from "./Sources";

/**
 * 首页那两格的数字**从 `PLANS` 算**，不写死 —— 原来这里写死了 SemiAnalysis 的
 * $8,000 / $14,000，表格里的数一改，首页就跟着说错话（2026-09-24 换口径时撞上的）。
 * 倍率跟表格走同一个函数（`planMultiplier`，按 1:7 折成元）：同一档在页面上只许有一个数。
 */
function heroNumbers(id: string) {
  const plan = PLANS.find((p) => p.id === id) as Plan;
  const monthly = monthlyValue(plan) ?? 0;
  const m = planMultiplier(plan) ?? 0;
  return {
    price: fmtUsd(plan.webMonthly),
    weekly: fmtUsd(plan.weeklyValue ?? 0),
    monthly: fmtUsd(monthly),
    multiplier: fmtMultiplier(m),
  };
}

const RATE_LABEL = `1:${YUAN_PER_USD}`;

/** 首页：为什么推荐自己订阅，而不是买中转站的「会员」。 */
export function WhyOfficial() {
  const reverse = RELAY_BANDS[0];
  const official = RELAY_BANDS[1];
  const claude = heroNumbers("claude-max-20");
  const gpt = heroNumbers("chatgpt-pro-20");
  return (
    <div className="qb-sub-stack">
      <section className="qb-sub-hero">
        <div className="qb-sub-hero-text">
          <span className="qb-eyebrow">先看结论</span>
          <h2>自己订阅官方，跟最便宜的中转一个价，还没有那些坑</h2>
          <p>
            中转站卖的「会员」，本质是几十个人共用别人名下的一个订阅账号。把官方订阅按中转站的「倍率」口径算一遍（美元月价按{" "}
            {RATE_LABEL} 折成元，往贵了取）：{claude.price} 的 Claude Max 20x
            网友估算一周能跑约 {claude.weekly} 的 API 用量（一个月按{" "}
            {WEEKS_PER_MONTH} 周约 {claude.monthly}），倍率 {claude.multiplier}
            ；{gpt.price} 的 ChatGPT Pro 20x 一周约 {gpt.weekly}，倍率{" "}
            {gpt.multiplier} —— 跟逆向中转（{fmtMultiplier(reverse.min)}～
            {fmtMultiplier(reverse.max)}）一个价位，官转（
            {fmtMultiplier(official.min)}～{fmtMultiplier(official.max)}
            ）要贵好几倍；而且是你自己的账号。
          </p>
        </div>
        <div className="qb-sub-hero-metrics">
          <Metric
            label={`Claude Max 20x 等效倍率（${RATE_LABEL}）`}
            hint={`${claude.price} × ${YUAN_PER_USD} ÷ 一个月约 ${claude.monthly}（美元按 ${RATE_LABEL} 折成元；周额度中间值 × ${WEEKS_PER_MONTH}）`}
          >
            <strong className="text-ok">{claude.multiplier}</strong>
          </Metric>
          <Metric
            label={`ChatGPT Pro 20x 等效倍率（${RATE_LABEL}）`}
            hint={`${gpt.price} × ${YUAN_PER_USD} ÷ 一个月约 ${gpt.monthly}（美元按 ${RATE_LABEL} 折成元；周额度中间值 × ${WEEKS_PER_MONTH}）`}
          >
            <strong className="text-ok">{gpt.multiplier}</strong>
          </Metric>
          <Metric label="逆向流量中转常见倍率" hint={reverse.note}>
            <strong className="text-warn">
              {fmtMultiplier(reverse.min)}～{fmtMultiplier(reverse.max)}
            </strong>
          </Metric>
          <Metric label="官转常见倍率" hint={official.note}>
            <strong className="text-danger">
              {fmtMultiplier(official.min)}～{fmtMultiplier(official.max)}
            </strong>
          </Metric>
        </div>
      </section>

      <section className="qb-sub-section">
        <h2 className="qb-sub-h2">
          <AlertTriangle size={18} className="text-danger" aria-hidden="true" />
          不正规中转站会遇到什么
        </h2>
        <p className="sub">
          每一条都标了是报道、研究、官方条款还是使用者反馈；未经官方证实的就写「未证实」。
        </p>
        <div className="qb-sub-grid qb-sub-grid--3">
          {RELAY_DOWNSIDES.map((d) => (
            <Card key={d.id} tone="danger" as="h3" title={d.title}>
              <Pill tone={d.kind === "使用者反馈" ? "warn" : "danger"}>
                {d.kind}
              </Pill>
              {d.body.map((para, i) => (
                <p key={i} className="qb-sub-para">
                  {para}
                </p>
              ))}
              <SourceRefs ids={d.sources} />
            </Card>
          ))}
        </div>
      </section>

      <section className="qb-sub-section">
        <h2 className="qb-sub-h2">
          <CheckCircle2 size={18} className="text-ok" aria-hidden="true" />
          自己订阅拿到的是什么
        </h2>
        <div className="qb-sub-grid qb-sub-grid--3">
          {OFFICIAL_BENEFITS.map((b) => (
            <Card key={b.title} tone="ok" as="h3" title={b.title}>
              <p className="qb-sub-para">{b.body}</p>
            </Card>
          ))}
        </div>
      </section>

      <section className="qb-sub-section">
        <h2 className="qb-sub-h2">
          <BadgeDollarSign
            size={18}
            className="text-accent"
            aria-hidden="true"
          />
          官方各档要多少钱，换算成中转站倍率是多少
        </h2>
        <PlanTable />
      </section>

      <section className="qb-sub-section">
        <h2 className="qb-sub-h2">
          <Scale size={18} className="text-accent" aria-hidden="true" />
          放在同一把尺子上
        </h2>
        <MultiplierMeter />
      </section>

      <MultiplierCalculator />

      <SourceList />
    </div>
  );
}
