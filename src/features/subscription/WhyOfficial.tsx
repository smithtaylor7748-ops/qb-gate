import {
  AlertTriangle,
  BadgeDollarSign,
  CheckCircle2,
  Scale,
} from "lucide-react";
import { Card, Metric, Pill } from "../../ui";
import { OFFICIAL_BENEFITS, RELAY_BANDS, RELAY_DOWNSIDES } from "./data";
import { fmtMultiplier } from "./multiplier";
import { MultiplierCalculator } from "./MultiplierCalculator";
import { MultiplierMeter, PlanTable } from "./PlanTable";
import { SourceList, SourceRefs } from "./Sources";

/** 首页：为什么推荐自己订阅，而不是买中转站的「会员」。 */
export function WhyOfficial() {
  const reverse = RELAY_BANDS[0];
  const official = RELAY_BANDS[1];
  return (
    <div className="qb-sub-stack">
      <section className="qb-sub-hero">
        <div className="qb-sub-hero-text">
          <span className="qb-eyebrow">先看结论</span>
          <h2>自己订阅官方，比任何中转站都便宜，而且没有那些坑</h2>
          <p>
            中转站卖的「会员」，本质是几十个人共用别人名下的一个订阅账号。把官方订阅按中转站的「倍率」口径算一遍：$200
            的 Claude Max 20x 实测能跑到约 $8,000 的 API 用量，倍率 0.025×；$200
            的 ChatGPT Pro 20x 约 $14,000，倍率 0.014× ——
            比最便宜的逆向中转还低，还是你自己的账号。
          </p>
        </div>
        <div className="qb-sub-hero-metrics">
          <Metric label="Claude Max 20x 等效倍率" hint="$200 ÷ 实测上限 $8,000">
            <strong className="text-ok">0.025×</strong>
          </Metric>
          <Metric
            label="ChatGPT Pro 20x 等效倍率"
            hint="$200 ÷ 实测上限 $14,000"
          >
            <strong className="text-ok">0.014×</strong>
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
