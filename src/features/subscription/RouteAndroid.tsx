import { AlertTriangle, Smartphone } from "lucide-react";
import { Card, Pill } from "../../ui";
import { AddressNotice } from "./Notices";
import { Step, StepList } from "./Steps";

/** 路线二：Google Play 原生订阅。先看礼品卡的预警。 */
export function RouteAndroid() {
  return (
    <div className="qb-sub-stack">
      <Card
        tone="danger"
        title="先看这条：千万不要买 Google Play 礼品卡"
        icon={<AlertTriangle size={16} aria-hidden="true" />}
        actions={<Pill tone="danger">极易锁卡 · 钱卡两空</Pill>}
      >
        <p className="qb-sub-para">
          Google Play
          礼品卡只能在购买国家或地区兑换，而且兑换时会核对账号地区与网络。不在当地买、不在当地网络兑，常见结果是提示「需要提供更多信息」或错误代码（如
          PRS-PGCSEFC-01），礼品卡被锁，要求提供当地购物小票和身份证明，钱退不回来。
        </p>
        <p className="qb-sub-para">
          安卓端可行的只有一条：在美区 Google 账号的付款资料里绑定
          <strong>本人在服务区发行、开通了境外线上支付的银行卡</strong>
          ，或该地区的 PayPal。没有这样的卡，就借一台 iPhone / iPad
          走「苹果内购」，开通后回到安卓机登录同一账号即可 ——
          订阅绑的是账号，不是设备。
        </p>
      </Card>

      <AddressNotice compact />

      <StepList>
        <Step
          title="准备美区 Google 账号与完整的 Play 环境"
          tag={{ tone: "ok", text: "环境" }}
          tip="手机若自带应用商店，不要用它装的改版 Play 商店，装 Google 官方套件。"
        >
          <p>
            确认手机装有完整的 Google 移动服务框架（GMS）与官方 Play
            商店；准备一个付款国家为「美国（US）」的 Google 账号（在
            payments.google.com
            看付款资料）；系统时间与时区和你所连网络所在地一致（如
            America/Los_Angeles）。
          </p>
        </Step>

        <Step
          title="绑定本人的银行卡或 PayPal"
          tag={{ tone: "warn", text: "不买礼品卡" }}
          tip="绑卡时 Google 可能发起一笔约 $1 的临时预授权，验证后会撤销。"
          warn="账单地址填你本人的真实地址。"
        >
          <p>
            Play 商店 → 右上角头像
            →「付款和订阅」→「付款方式」→「添加信用卡或借记卡」， 输入本人 Visa
            / Mastercard 的卡号、到期日与 CVC；或绑定该地区的
            PayPal。提示地区不符或发卡行拒绝时，不要反复尝试，直接改走苹果内购。
          </p>
        </Step>

        <Step
          title="装官方 App，在 App 内订阅"
          tag={{ tone: "ok", text: "Google 收银台" }}
          tip="弹出「无法在您所在国家/地区完成交易」，说明账号地区或网络触发了风控，换节点或改走苹果内购。"
        >
          <p>
            网络稳定在服务区内的住宅节点，在 Play 商店核实开发者（Anthropic PBC
            / OpenAI）后安装官方 App；进「Upgrade to Pro / Plus」，弹出 Google
            Play 原生结算窗，核对扣款卡与金额，点「Subscribe」，指纹或密码确认。
          </p>
        </Step>

        <Step
          title="核对收据，记住怎么退订"
          tag={{ tone: "accent", text: "全端通用" }}
          tip="卸载 App 不会取消订阅，必须在 Play 商店里主动退订。"
        >
          <p>
            Google 会把带 GPA 订单号的电子收据发到 Gmail。不想续订时，Play 商店
            → 头像
            →「付款和订阅」→「订阅」里取消，当期剩余天数照常可用。回到电脑端打开本面板，一键受控启动，登录同一账号。
          </p>
        </Step>
      </StepList>

      <Card
        as="h3"
        title="更省事的办法"
        icon={<Smartphone size={15} aria-hidden="true" />}
      >
        <p className="qb-sub-para">
          没有合适的卡就别在 Google Play 上耗：借身边的 iPhone / iPad
          走「苹果内购」，六步开通，登出后在自己的安卓机登录同一账号，Pro / Plus
          权益照样在。
        </p>
      </Card>
    </div>
  );
}
