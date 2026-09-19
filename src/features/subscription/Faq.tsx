import { useState } from "react";
import {
  CheckCircle2,
  ChevronDown,
  HelpCircle,
  RefreshCw,
  Receipt,
  ShieldAlert,
} from "lucide-react";
import { Bullet, Button, Card, Pill } from "../../ui";
import { FAQ_LIST, STATE_EXAMPLES } from "./data";
import { AddressNotice, ScopeNotice } from "./Notices";
import { SourceList } from "./Sources";

interface Props {
  onReopenAck: () => void;
}

/** 答疑与合规：口径、地址、税、退订、FAQ、免责。 */
export function Faq({ onReopenAck }: Props) {
  const [open, setOpen] = useState<number | null>(null);

  return (
    <div className="qb-sub-stack">
      <Card
        tone="ok"
        title="你已在本机确认过《知情说明》"
        icon={<CheckCircle2 size={16} aria-hidden="true" />}
        actions={
          <Button variant="ghost" size="sm" onClick={onReopenAck}>
            再读一遍
          </Button>
        }
      >
        <p className="qb-sub-para">
          个人经验整理，不是官方指引；不提供任何代充、代付、账号买卖或拼车服务；填报资料请用本人真实信息。
        </p>
      </Card>

      <ScopeNotice />
      <AddressNotice />

      <section className="qb-sub-section">
        <h2 className="qb-sub-h2">
          <Receipt size={18} className="text-accent" aria-hidden="true" />
          为什么总额会多出 8%～10%
        </h2>
        <div className="qb-sub-grid qb-sub-grid--2">
          <Card as="h3" title="账单地址在收销售税的州">
            <span className="qb-sub-price text-danger">$21.80</span>
            <p className="qb-sub-para">
              标价 $20 的订阅结算时加收 8%～10% 的销售税。余额正好 $20
              就会「余额不足 / 扣款失败」，反复重试还会触发风控。
              <strong>按你真实地址所在州的税率，充值时留出余量。</strong>
            </p>
          </Card>
          <Card as="h3" title="账单地址在不收州销售税的州" tone="accent">
            <span className="qb-sub-price text-ok">$20.00</span>
            <p className="qb-sub-para">
              美国只有五个州不收州销售税：俄勒冈、特拉华、蒙大拿、新罕布什尔、阿拉斯加（阿拉斯加部分城市仍有地方税）。标价多少就付多少。
            </p>
          </Card>
        </div>

        <h3 className="qb-sub-h3">美国地址长什么样 · 格式样例</h3>
        <p className="qb-sub-alert-inline text-danger">
          下面五张卡只是让你看「街道、城市、州缩写、五位邮编」怎么写、怎么对齐。
          <strong>填任何表时，请填你本人的真实地址。</strong>
        </p>
        <div className="qb-sub-grid qb-sub-grid--3">
          {STATE_EXAMPLES.map((st) => (
            <div key={st.code} className="qb-sub-state">
              <div className="qb-sub-state-head">
                <span>
                  {st.name}（{st.code}）
                </span>
                <Pill tone="ok">无州销售税</Pill>
              </div>
              <dl className="qb-sub-state-body">
                <dt>城市</dt>
                <dd>{st.cities}</dd>
                <dt>ZIP</dt>
                <dd className="font-mono">{st.zipCodes}</dd>
                <dt>区号</dt>
                <dd>{st.areaCodes}</dd>
                <dt>写法</dt>
                <dd className="font-mono">{st.example}</dd>
              </dl>
              <span className="qb-sub-state-foot">
                格式参考 · 请填本人真实地址
              </span>
            </div>
          ))}
        </div>
      </section>

      <section className="qb-sub-section">
        <h2 className="qb-sub-h2">
          <RefreshCw size={18} className="text-accent" aria-hidden="true" />
          退订与退款
        </h2>
        <div className="qb-sub-grid qb-sub-grid--3">
          <Card as="h3" title="在哪订的就在哪退">
            <Bullet marker="iOS">
              设置 → 顶部姓名 → 订阅 → 找到条目 → 取消
            </Bullet>
            <Bullet marker="Play">
              Play 商店 → 头像 → 付款和订阅 → 订阅 → 取消
            </Bullet>
            <Bullet marker="网页">官网 Settings → Billing → Cancel</Bullet>
          </Card>
          <Card as="h3" title="取消 ≠ 退款，卸载 ≠ 退订">
            <p className="qb-sub-para">
              取消续订后当期剩余天数照常可用；已扣的款要另外申请：Apple 到
              reportaproblem.apple.com，Google
              在订单页申请，官网订阅按各家消费者条款处理（Anthropic
              默认不退，少数地区 7 天）。卸载 App 什么都不会取消。
            </p>
          </Card>
          <Card as="h3" title="别重复订">
            <p className="qb-sub-para">
              在 iOS 订的只出现在 Apple 的订阅页，网页端的 Billing
              看不到它；不要以为没订成功又在网页端订一次，那会被扣两份。先月付试一个月，再考虑年付。
            </p>
          </Card>
        </div>
      </section>

      <section className="qb-sub-section">
        <h2 className="qb-sub-h2">
          <HelpCircle size={18} className="text-accent" aria-hidden="true" />
          常见问题
        </h2>
        <div className="qb-sub-faq">
          {FAQ_LIST.map((item, i) => {
            const isOpen = open === i;
            return (
              <div key={item.q} className="qb-sub-faq-item">
                <button
                  type="button"
                  className="qb-sub-faq-q"
                  aria-expanded={isOpen}
                  onClick={() => setOpen(isOpen ? null : i)}
                >
                  <span>{item.q}</span>
                  <ChevronDown
                    size={16}
                    aria-hidden="true"
                    className={`qb-sub-faq-chevron${isOpen ? " is-open" : ""}`}
                  />
                </button>
                {isOpen && <p className="qb-sub-faq-a">{item.a}</p>}
              </div>
            );
          })}
        </div>
      </section>

      <Card
        as="h3"
        title="免责与性质说明"
        icon={<ShieldAlert size={15} aria-hidden="true" />}
        className="qb-sub-disclaimer"
      >
        <Bullet marker="1">
          <strong>性质：</strong>
          个人经验整理，不代表任何官方立场，无商业营利属性；不提供、不销售任何代充值、代付款、账号买卖、拼车合租或中介服务，所有操作由使用者在官方或正规商户平台自行完成。
        </Bullet>
        <Bullet marker="2">
          <strong>第三方：</strong>
          Apple、Google、Anthropic、OpenAI、Pockyt
          Shop、接码平台等均为独立商业主体，受各自条款管辖；在第三方平台产生的资金纠纷、手续费、卡密失效等后果自负。
        </Bullet>
        <Bullet marker="3" tone="danger">
          <strong>地址与资料：</strong>
          免税州地址与地址生成器只作格式参考；注册、绑定支付、填账单地址时，请填写本人真实、合法、有效的信息。因填报虚假资料引发的审查、拒付或法律责任，由本人承担。
        </Bullet>
        <Bullet marker="4">
          <strong>风控：</strong>
          服务商的风控多维且持续变化。本页不承诺「必定成功」或「不被封号」；因触发风控、政策收紧导致的限流、封禁与款项损失自负。
        </Bullet>
        <Bullet marker="5">
          <strong>守法：</strong>
          须遵守所在地法律法规与所用服务的全部官方条款，不得将相关技术与账号用于任何违法违规活动。
        </Bullet>
      </Card>

      <SourceList />
    </div>
  );
}
