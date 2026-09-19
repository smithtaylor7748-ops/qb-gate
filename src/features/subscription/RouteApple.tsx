import { Apple, Sparkles } from "lucide-react";
import { Card, Pill } from "../../ui";
import { AddressNotice } from "./Notices";
import { Step, StepList } from "./Steps";

/** 路线一：iPhone / iPad 上用 Apple 账户余额内购。首选。 */
export function RouteApple() {
  return (
    <div className="qb-sub-stack">
      <Card
        tone="accent"
        title="苹果内购：Apple 收银台代扣，服务商碰不到你的卡"
        icon={<Apple size={16} aria-hidden="true" />}
        actions={<Pill tone="ok">首选 · 成功率最高</Pill>}
      >
        <p className="qb-sub-para">
          在 iPhone / iPad 的官方 App 里订阅，收银台是 Apple 的：Anthropic 和
          OpenAI 只收到一张开通回执，接触不到你的银行卡，也就不存在 Stripe
          按发卡地区拒付这回事。余额用美区礼品卡充，礼品卡用支付宝买。六步走完。
        </p>
        <div className="qb-sub-facts">
          <div>
            <span className="qb-sub-fact-k">Claude</span>
            <span>Pro $20.00 · Max 5x $124.99 · Max 20x $249.99</span>
          </div>
          <div>
            <span className="qb-sub-fact-k">ChatGPT</span>
            <span>
              Go $8.00 · Plus $19.99 · Pro 5x $100.00 · Pro 20x $200.00
            </span>
          </div>
          <div>
            <span className="qb-sub-fact-k">注意</span>
            <span>
              Claude Max 在 iOS 上比网页贵
              25%；有本人可用的银行卡想省这笔，看「网页绑卡」。
            </span>
          </div>
        </div>
      </Card>

      <AddressNotice compact />

      <StepList>
        <Step
          title="准备一个美区 Apple 账户（不需要银行卡）"
          tag={{ tone: "ok", text: "基石" }}
          tip={
            <>
              <strong>安全关键：</strong>在 iPhone 上登录新账户时，
              <strong>只在 App Store（媒体与购买项目）里退出再登录</strong>
              ，绝不要注销「设置」顶部的 iCloud
              主账户，否则通讯录和相册会跟着乱。
            </>
          }
          warn={
            <>
              账单地址请填你本人的真实地址。免税州地址只是给你看格式的，见上面那条红色提醒。
            </>
          }
        >
          <p>
            在浏览器打开 Apple 账户管理页 account.apple.com（原
            appleid.apple.com， 「Apple ID」已改名「Apple
            账户」），创建新账户；国家或地区选「美国（United
            States）」，出生日期填满 18
            岁，邮箱用国际通用邮箱。付款方式选「无（None）」。
          </p>
        </Step>

        <Step
          title="用支付宝买美区 App Store 礼品卡"
          tag={{ tone: "accent", text: "支付宝扫码" }}
          tip={
            <>
              <strong>面额按套餐算，留出税费余量：</strong>
              Claude Pro / ChatGPT Plus 充 $25；Claude Max 5x 至少 $125，Max 20x
              至少 $250；ChatGPT Pro 按 $100 / $200
              加一点余量。你真实地址所在的州 收销售税的话，总额会比标价高
              8%～10%，别卡着标价充。
            </>
          }
        >
          <p>
            打开支付宝，首页左上角把定位城市手动切到「旧金山」或「纽约」，搜「大牌礼卡」或「Pockyt
            Shop」进入小程序；选「App Store &amp; iTunes US」或「Apple Gift Card
            US」，填接收卡密的常用邮箱并付款，通常 1～5 分钟收到 16
            位卡密。小程序维护时，可在电脑浏览器打开 shop.pockyt.io
            购买，同样支持支付宝扫码。
          </p>
          <p>
            只买美区这一种：礼品卡只能在购买国家或地区兑换，别的区的卡兑不了、也退不了。
          </p>
        </Step>

        <Step
          title="把卡密兑换到 Apple 账户余额"
          tag={{ tone: "ok", text: "秒到账" }}
          tip="余额不会过期；内购时系统默认优先从余额扣。"
        >
          <p>
            打开 iPhone / iPad 的 App Store，点右上角头像
            →「兑换礼品卡或代码（Redeem Gift Card or
            Code）」→「手动输入代码」，粘贴 16 位卡密确认。页面显示当前余额（如
            Balance: $25.00），核对余额不少于你要订的那档。
          </p>
        </Step>

        <Step
          title="在纯净网络下装官方 App"
          tag={{ tone: "warn", text: "认准开发者" }}
          tip="不要从任何第三方渠道装「改版」客户端，只认 App Store 里这两个开发者。"
        >
          <p>
            手机连接到服务区内、干净的住宅网络；在 App Store
            搜「Claude」或「ChatGPT」， 开发者分别是 Anthropic PBC 与 OpenAI
            OpCo, LLC。装好后登录你准备升级的账号（快捷授权或邮箱验证码都行）。
          </p>
        </Step>

        <Step
          title="App 里一键开通"
          tag={{ tone: "ok", text: "风控隔离的核心" }}
          tip={
            <>
              <strong>没点亮会员标：</strong>在 App 设置里点「恢复购买（Restore
              Purchases）」即可同步。收据由 Apple 发到邮箱。
              <strong>在 iOS 订的只能在 Apple 的订阅页管理和取消</strong>
              ，不要再去网页端订一次，否则重复扣费。
            </>
          }
        >
          <p>
            Claude：右上角头像 → 设置 →「Upgrade to Pro」（或选
            Max）。ChatGPT：设置 →「Upgrade to Plus /
            Pro」。选月付，核对价格与上面那张表一致，点「Subscribe」，弹出 Apple
            原生内购确认框，付款方式是「Apple 账户余额」，侧边键双击 + Face ID /
            触控 ID 确认，立即生效。
          </p>
        </Step>

        <Step
          title="回到电脑端，权益全端通用"
          tag={{ tone: "accent", text: "零二次消费" }}
          tip="本面板只管本机环境与门禁，不读取、不中转、不上传你的付款信息与凭据。"
        >
          <p>
            订阅绑在账号上：网页、桌面端、手机、Claude Code / Codex
            全部同步生效。回到本面板，在总览页一键受控启动，登录同一个账号即可，IP
            门禁与看门狗会替你守着出口。
          </p>
        </Step>
      </StepList>

      <Card
        tone="ok"
        as="h3"
        title="安卓用户也可以走这条路"
        icon={<Sparkles size={15} aria-hidden="true" />}
      >
        <p className="qb-sub-para">
          订阅绑的是账号，不是设备。借一台 iPhone /
          iPad，登录你的账号按上面六步开通一次，登出后回到自己的安卓手机登录同一账号，权益照样在。这比在
          Google Play 上折腾付款方式省心得多。
        </p>
      </Card>
    </div>
  );
}
