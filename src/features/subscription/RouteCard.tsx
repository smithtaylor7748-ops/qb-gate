import { AlertTriangle, CreditCard } from "lucide-react";
import { Bullet, Card, Pill } from "../../ui";
import { AddressNotice } from "./Notices";
import { Step, StepList } from "./Steps";

/** 路线三：电脑浏览器里在官网用本人银行卡订阅。 */
export function RouteCard() {
  return (
    <div className="qb-sub-stack">
      <Card
        tone="accent"
        title="网页绑卡：不需要苹果设备，电脑上全程办完"
        icon={<CreditCard size={16} aria-hidden="true" />}
        actions={<Pill tone="warn">看卡与 IP · 风控最严</Pill>}
      >
        <p className="qb-sub-para">
          官网的收银台由 Stripe 托管。它按卡号前 6
          位（BIN）识别发卡机构所在地区，
          <strong>不在两家服务区内发行的卡会在提交卡号的一瞬间被拒</strong>
          ，根本不会向发卡行发起扣款；出口 IP
          被判定为机房或高欺诈分也会直接拒付。 所以这条路线只有两个前提：
        </p>
        <Bullet marker="1">
          <strong>卡：</strong>本人在服务区发行的 Visa /
          Mastercard，已在发卡行开通境外线上支付与无卡交易，卡内余额比标价多留
          $5～$25（税费与预授权）。
        </Bullet>
        <Bullet marker="2">
          <strong>IP：</strong>干净的住宅网络，欺诈分低；可在本面板「IP
          纯净度」页自测。
        </Bullet>
        <p className="qb-sub-para">
          好处是 Claude Max 按网页价付（$100 / $200），比 iOS 内购省 25%。
        </p>
      </Card>

      <AddressNotice />

      <StepList>
        <Step
          title="准备本人的银行卡"
          tag={{ tone: "ok", text: "前提一" }}
          tip="连续两次被拒就停，不要再试 —— 频繁失败会触发发卡行与 Stripe 的风控锁卡。"
        >
          <p>
            用本人在服务区发行的 Visa / Mastercard；在发卡行 App
            里确认已开通「境外线上支付」与「无卡交易（CNP）」；卡内余额比你要订的那档多留一些。
          </p>
        </Step>

        <Step
          title="账单地址：填你本人的真实地址"
          tag={{ tone: "danger", text: "红字提醒" }}
          warn={
            <>
              免税州地址与地址生成器只是让你看格式（街道 / 城市 / 州 /
              五位邮编各填哪一栏）。
              <strong>收银台里填的必须是你本人的真实账单地址。</strong>
              总额按这个地址所在州计税，有税就是标价再加 8%～10%，余额留够。
            </>
          }
        >
          <p>
            Stripe
            会校验城市、州和邮编是否属于同一地区，填错会提示不匹配；把你真实地址的这几项对齐填好即可。
          </p>
        </Step>

        <Step
          title="无痕窗口 + 纯净节点"
          tag={{ tone: "warn", text: "前提二" }}
          tip="别在开着一堆去广告插件、或多人共用的高延迟节点下发起付款，极易触发 Stripe 的风险识别。"
        >
          <p>
            新建浏览器的隐私 / 无痕窗口，避免旧 Cookie
            与指纹交叉污染；连接低欺诈分的住宅节点，时区与节点所在地对齐；先到本面板「IP
            纯净度」页确认当前出口欺诈分够低、没有黑名单标记。
          </p>
        </Step>

        <Step
          title="登录官网，打开升级页"
          tag={{ tone: "ok", text: "认准域名" }}
          tip="核对地址栏是 claude.ai 或 chatgpt.com，不在任何来路不明的页面里输卡号。"
        >
          <p>
            在无痕窗口访问 claude.ai 或
            chatgpt.com，登录账号；在设置或个人资料区点「Upgrade to Pro /
            Max」或「Upgrade to Plus / Pro」，选月付方案，页面跳到 Stripe
            托管的收银台。
          </p>
        </Step>

        <Step
          title="填卡号与账单地址，提交"
          tag={{ tone: "ok", text: "核对总额" }}
          tip="弹出「Your card was declined」：换无痕窗口 → 换更干净的住宅 IP → 核对余额是否覆盖税费；两次仍不行，改走苹果内购。"
        >
          <p>
            输入 16 位卡号、到期年月（MM/YY）、卡背 3 位
            CVC；持卡人姓名按卡面；账单地址国家与街道、城市、州、邮编填你本人的真实资料。核对总额（标价，或标价加你所在州的销售税），点「Subscribe」。
          </p>
        </Step>

        <Step
          title="订阅成功，纳入门禁保护"
          tag={{ tone: "accent", text: "收工" }}
          tip="妥善保管卡号与安全码，不向任何第三方透露。"
        >
          <p>
            付款成功后页面跳回控制台，头像旁点亮「Pro」「Max」或「Plus」「Pro」徽标。回到本面板，在总览页一键受控启动，在原生客户端或
            Claude Code / Codex
            里直接用；到发卡行后台看一眼扣款账单，下次续订日前决定要不要续。
          </p>
        </Step>
      </StepList>

      <Card
        tone="warn"
        as="h3"
        title="被拒付的三种常见原因"
        icon={<AlertTriangle size={15} aria-hidden="true" />}
      >
        <Bullet marker="1">
          发卡行拒绝了非 3D 认证的境外线上扣款 —— 去发卡行 App
          开通境外线上支付。
        </Bullet>
        <Bullet marker="2">
          当前出口 IP 被 Stripe 判定为机房或高欺诈分 —— 换干净的住宅节点。
        </Bullet>
        <Bullet marker="3">余额没覆盖税费或预授权 —— 多留 $5～$25。</Bullet>
        <p className="notice">按顺序排查；连续两次被拒就停，改走苹果内购。</p>
      </Card>
    </div>
  );
}
