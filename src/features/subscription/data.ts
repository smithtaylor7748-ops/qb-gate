/**
 * 订阅页的全部资料：套餐价、周额度、中转站的问题、路线里要用的数字、FAQ、来源。
 *
 * 文案改这里，组件只负责排版。每一条带数字的都有出处（`SOURCES` 里按 id 引用），
 * 复核日期是 `REVIEWED_ON` —— 价格与额度会过时，改数字的同时把日期一起改掉。
 *
 * 额度口径（2026-09-24 使用者定的）：取 linux.do 那个帖子里网友估算区间的**中间值**，
 * 以周为单位；原来那组 SemiAnalysis 的「月上限」（Max 20x $8,000 等）作废。
 *
 * 口径（使用者定的，别改）：
 *   - 不提任何特定国家、货币或发卡行。「国内」的定义见 `ScopeNotice`。
 *     唯一的例外是倍率（2026-09-24 使用者定的）：官方月价按 1 美元 = 7 元折成元再比，
 *     界面上标明「1:7」、往贵了取、实际一般 6.8 左右（`multiplier.ts` 文件头）。
 *     页面上写「元」，不写那几个被测试盯着的词。
 *   - 免税州地址与地址生成器只作格式参考，填表用本人真实地址，这类提醒一律红字。
 *   - 中转站的每一条坏处都标明是「报道 / 研究 / 使用者反馈 / 官方条款」，
 *     未经官方证实的就写「未证实」，不把社区反馈写成事实。
 */

export const REVIEWED_ON = "2026-09-24";

export const ADDRESS_GENERATOR_URL =
  "https://usaddressgen.com/tax-free-address/";
/** 地址生成器网站自己写在页面上的话，原样引用。 */
export const ADDRESS_GENERATOR_OWN_WORDS =
  "测试地址不能作为免税证明，真实交易仍应使用真实所在地。";

export const CLAUDE_REGIONS_URL =
  "https://www.anthropic.com/supported-countries";
export const OPENAI_REGIONS_URL =
  "https://help.openai.com/en/articles/7947663-chatgpt-supported-countries";

// ---------------------------------------------------------------- 套餐

export type Vendor = "claude" | "chatgpt";

export interface Plan {
  id: string;
  vendor: Vendor;
  name: string;
  /** 网页端月付，美元。 */
  webMonthly: number;
  /** iOS 内购月付（App Store 页面实读），美元。 */
  iosMonthly: number;
  /**
   * 周额度：按官方 API 牌价折算的美元。取 linux.do「国内外 AI 订阅性价比」帖
   * （`SOURCES` 的 `linuxdo-quota`，2026-08/09）里网友估算区间的**中间值**
   * （2026-09-24 使用者定的口径：「额度取这个帖子里的中间值」）。
   * `null` = 帖子里没有这一档。
   */
  weeklyValue: number | null;
  /** 帖子里的原区间（只给了一个数时两头相同），界面上标注用。 */
  weeklyRange: [number, number] | null;
  /** 帖子里单独给的 5 小时额度区间（目前只有 Plus 有）。 */
  fiveHourRange?: [number, number];
  note?: string;
}

/**
 * 一个月按几个周限算。帖子作者的说法是「官方经常重置，一个月可以用远超 4 个周限」——
 * 这里保守地按 4 个算，界面上写明。
 */
export const WEEKS_PER_MONTH = 4;

/** 一个月能用到的 API 等值 = 周额度中间值 × 4。帖子里没有这一档就是 `null`。 */
export function monthlyValue(plan: Plan): number | null {
  return plan.weeklyValue === null ? null : plan.weeklyValue * WEEKS_PER_MONTH;
}

export const PLANS: Plan[] = [
  {
    id: "claude-pro",
    vendor: "claude",
    name: "Claude Pro",
    webMonthly: 20,
    iosMonthly: 20,
    // 帖子写「周 200-500 刀？不确定」；回帖另有 $300～400（用 Opus 5）、约 $200 两种说法。
    weeklyValue: 350,
    weeklyRange: [200, 500],
    note: "年付 $17/月（$200 一次付清）；iOS 年付 $214.99",
  },
  {
    id: "claude-max-5",
    vendor: "claude",
    name: "Claude Max 5x",
    webMonthly: 100,
    iosMonthly: 124.99,
    // 帖子只给了一个数：「周能到 2500 刀，ccusage 统计口径」。
    weeklyValue: 2500,
    weeklyRange: [2500, 2500],
    note: "只有月付；iOS 内购比网页贵 25%",
  },
  {
    id: "claude-max-20",
    vendor: "claude",
    name: "Claude Max 20x",
    webMonthly: 200,
    iosMonthly: 249.99,
    weeklyValue: 4000,
    weeklyRange: [3000, 5000],
    note: "只有月付；iOS 内购比网页贵 25%",
  },
  {
    id: "chatgpt-go",
    vendor: "chatgpt",
    name: "ChatGPT Go",
    webMonthly: 8,
    iosMonthly: 8,
    weeklyValue: null,
    weeklyRange: null,
    note: "2025-08 起在 170 多个国家和地区上线的入门档，没有额度数据",
  },
  {
    id: "chatgpt-plus",
    vendor: "chatgpt",
    name: "ChatGPT Plus",
    webMonthly: 20,
    iosMonthly: 19.99,
    weeklyValue: 130,
    weeklyRange: [100, 160],
    fiveHourRange: [20, 30],
  },
  {
    id: "chatgpt-pro-5",
    vendor: "chatgpt",
    name: "ChatGPT Pro 5x",
    webMonthly: 100,
    iosMonthly: 100,
    weeklyValue: 600,
    weeklyRange: [500, 700],
    note: "2026-04-09 新增，Codex 用量是 Plus 的 5 倍",
  },
  {
    id: "chatgpt-pro-20",
    vendor: "chatgpt",
    name: "ChatGPT Pro 20x",
    webMonthly: 200,
    iosMonthly: 200,
    weeklyValue: 2500,
    weeklyRange: [2000, 3000],
    note: "Codex 用量是 Plus 的 20 倍",
  },
];

/**
 * 帖子里跟额度一起说的那几句前提。**界面上跟数字放在一起**，不放悬停：
 * 这些数离了前提就会被当成承诺。
 */
export const QUOTA_CAVEATS: string[] = [
  "全部是网友估算，不是官方数字；帖子作者自己写着「不一定准」。",
  "Claude 那几档是 2026 年 8 月官方临时给 1.5 倍额度时测的，帖子说之后会降到 1.25 倍，可能更少。",
  "账号被风控后额度会大幅缩水（帖子里说 ChatGPT Pro 有被压到周限只剩 $200～300 的），同一档不同账号也有多有少。",
];

export const VENDOR_LABEL: Record<Vendor, string> = {
  claude: "Claude（Anthropic）",
  chatgpt: "ChatGPT（OpenAI）",
};

/**
 * 中转站常见的倍率区间。倍率是站内标价相对官方牌价的倍数，按站内额度算 ——
 * 常见的充值口径是 1 元 = 1 美元额度，所以 1× 就是每 $1 官方牌价的用量付 1 元。
 */
export interface RelayBand {
  id: string;
  label: string;
  min: number;
  max: number;
  note: string;
}

export const RELAY_BANDS: RelayBand[] = [
  {
    id: "reverse",
    label: "逆向流量中转",
    min: 0.05,
    max: 0.3,
    note: "号池、拼车、逆向网页版。便宜的来源就是别人的订阅账号，随时封、随时换模型",
  },
  {
    id: "official-relay",
    label: "官转（转卖官方 API Key）",
    min: 0.8,
    max: 1.5,
    note: "拿的是官方 API，站内标价是牌价的 0.8～1.5 倍；按常见的 1 元 = 1 美元额度充值，仍比自己订阅贵好几倍",
  },
];

// ---------------------------------------------------------------- 中转站的问题

export type EvidenceKind = "报道" | "研究" | "使用者反馈" | "官方条款";

export interface RelayDownside {
  id: string;
  title: string;
  kind: EvidenceKind;
  /** 一段一段写；组件按段落渲染。 */
  body: string[];
  /** `SOURCES` 里的 id。 */
  sources: string[];
}

export const RELAY_DOWNSIDES: RelayDownside[] = [
  {
    id: "unstable",
    title: "不稳定，首字延迟高",
    kind: "报道",
    body: [
      "你的请求先到中转站，再由它挑一个账号或 Key 转给官方，多绕至少一跳。节点拥堵、号池被封、上游一变，表现就是流式中断、半天不出第一个字、充完钱账号不能用。",
      "「包月不限量」的套餐有报道称不到一个月就断服，商家失联、售后群解散；也有商家以「封杀严重、成本上涨」为由把计费直接涨到原来的 3～4 倍。",
    ],
    sources: ["sina-relay", "tencent-relay"],
  },
  {
    id: "downgrade",
    title: "降智、偷换模型",
    kind: "报道",
    body: [
      "付高端模型的钱，后台实际调用便宜得多的模型；截短上下文窗口；注入限制性的系统提示。界面上显示的模型名不会变，你只会觉得「AI 变笨了」。",
      "行业里的经验口径：Opus 这档的倍率低于 0.5 倍，「很可能掺入逆向流量或存在阉割、偷梁换柱」。",
    ],
    sources: ["tencent-relay", "sina-relay"],
  },
  {
    id: "enforcement",
    title: "官方的反制：封号池、限并发",
    kind: "官方条款",
    body: [
      "Anthropic 从 2026 年 1 月起在服务端限制订阅账号的 OAuth 只能用于官方 Claude Code，2 月 19 日把这条明写进消费者条款，4 月 4 日全面执行：把 Pro / Max 订阅拿去给第三方工具或中转用，属于违反条款，账号会被封。号池型中转随之大面积封号，有运营方称九成账号被封。",
      "OpenAI 这边是使用者反馈（在官方 Codex 仓库提的 issue，未经官方证实）：高并发下请求 GPT-6 Astra、GPT-5.6 Sol，疑似被路由成 GPT-4o 或 GPT-5.6 Luna。号池共享恰恰就是高并发场景 —— 真被降级，分到你头上的就是那一份。",
    ],
    sources: [
      "register-anthropic",
      "zhihu-relay-ban",
      "codex-45199",
      "codex-46632",
    ],
  },
  {
    id: "ratelimit",
    title: "限流",
    kind: "报道",
    body: [
      "中转站要在有限的账号上养几十个人，只能给每个用户加请求数、token 数的限制；高峰期「单个请求可占用的 GPU 时间、显存窗口、工具次数都会被限制」，或者把一部分请求路由到更快更便宜的模型。",
      "一个 $200/月 的账号「可被拆分，以不同定价的套餐卖给数十人」—— 你买到的「会员」，就是这几十分之一。",
    ],
    sources: ["sina-relay"],
  },
  {
    id: "data",
    title: "卖数据、投毒、偷币",
    kind: "研究",
    body: [
      "2026 年 4 月的一项测量研究测了 428 个中转：9 个在回复里主动注入恶意代码，17 个拿走了研究者故意放进请求里的云平台凭证，1 个直接把私钥里的资产转走了。注入可以只在第 50 次调用之后、只对某几种编程语言、只在检测到自动执行模式时触发 —— 平时看起来完全正常。",
      "从业者的说法：一些中转站「通过买卖用户数据来实现盈利，低价只是吸引用户的手段」；「数据在中转站手里基本是裸奔状态……中间是 100% 的黑盒，很难验证」。",
    ],
    sources: ["arxiv-2604-08407", "sina-relay"],
  },
  {
    id: "runaway",
    title: "跑路、涨价、连坐",
    kind: "报道",
    body: [
      "中转站的成本是别人的账号。官方一次封号，它的库存就没了；它会用你的余额去赌下一批账号，赌输了就是断服或跑路。",
      "几千个 Key 挤在少数几个出口上追求高并发，一个 Key 的调用模式被判定异常，共用同一出口的账号可能一起被封 —— 你没做错任何事，也会被连坐。",
    ],
    sources: ["zhihu-relay-ip", "sina-relay"],
  },
];

export interface Benefit {
  title: string;
  body: string;
}

export const OFFICIAL_BENEFITS: Benefit[] = [
  {
    title: "直连官方，没有中间人",
    body: "请求直接到 Anthropic / OpenAI 的接口，没有多绕的那一跳，也没有一个能看你对话的中间层。",
  },
  {
    title: "点什么模型就是什么模型",
    body: "订阅里选的 Opus、GPT-6 就是你拿到的模型；用量在账户设置里看得见，不存在「偷梁换柱」的空间。",
  },
  {
    title: "按时间窗口算，不按 token 扣",
    body: "订阅额度按 5 小时与每周两个窗口滚动，跑长任务不用盯着余额；网友估算 Max 20x 一周能跑到约 $3,000～5,000 的 API 用量（中间值 $4,000）。",
  },
  {
    title: "有条款、有收银台、有申诉",
    body: "Apple / Google 内购能在订阅页随时退订，退款有正式渠道；官网订阅受消费者条款保护。中转站的「售后群」不是任何一种保障。",
  },
  {
    title: "账号只属于你，不被连坐",
    body: "不跟几十个陌生人共用一个账号或一个出口。别人的滥用不会算到你头上。",
  },
  {
    title: "数据按官方隐私政策处理",
    body: "对话是否用于训练可以自己设置；不会被拿去转卖或喂给别人的模型。",
  },
  {
    title: "官方客户端直接登录，再加一层门禁",
    body: "Claude Code、Claude 桌面端、Codex 用同一个账号登录即可。回到本面板一键受控启动，IP 门禁与看门狗替你守着出口不漂移。",
  },
];

// ---------------------------------------------------------------- 免税州（格式样例）

export interface StateExample {
  name: string;
  code: string;
  cities: string;
  zipCodes: string;
  areaCodes: string;
  example: string;
}

/**
 * 五个不收州销售税的州。**这里的每一行都是格式样例**：看街道、城市、州缩写、
 * 五位邮编怎么对齐，不是让你填的地址。
 */
export const STATE_EXAMPLES: StateExample[] = [
  {
    name: "俄勒冈",
    code: "OR",
    cities: "Portland · Eugene",
    zipCodes: "97201 · 97204 · 97401",
    areaCodes: "503 · 541",
    example: "1211 SW 5th Ave, Portland, OR 97201",
  },
  {
    name: "特拉华",
    code: "DE",
    cities: "Wilmington · Newark",
    zipCodes: "19801 · 19711",
    areaCodes: "302",
    example: "1000 N West St, Wilmington, DE 19801",
  },
  {
    name: "蒙大拿",
    code: "MT",
    cities: "Helena · Billings",
    zipCodes: "59601 · 59101",
    areaCodes: "406",
    example: "1301 E 6th Ave, Helena, MT 59601",
  },
  {
    name: "新罕布什尔",
    code: "NH",
    cities: "Manchester · Concord",
    zipCodes: "03101 · 03301",
    areaCodes: "603",
    example: "1000 Elm St, Manchester, NH 03101",
  },
  {
    name: "阿拉斯加",
    code: "AK",
    cities: "Anchorage · Fairbanks",
    zipCodes: "99501 · 99701",
    areaCodes: "907",
    example: "632 W 6th Ave, Anchorage, AK 99501",
  },
];

// ---------------------------------------------------------------- FAQ

export interface FaqItem {
  q: string;
  a: string;
}

export const FAQ_LIST: FaqItem[] = [
  {
    q: "在服务区之外发行的银行卡，能在网页端直接付款吗？",
    a: "基本不能。官网收银台由 Stripe 处理，它按卡号前 6 位（BIN）识别发卡机构所在地区，不在两家服务区内发行的卡会在提交卡号的一瞬间被拒，根本不会向发卡行发起扣款。手头只有这类卡的话，不要在网页端反复尝试（连续被拒会触发风控甚至锁号），直接走「苹果内购」：扣款由 Apple 处理，服务商只收到开通回执。",
  },
  {
    q: "为什么最推荐「苹果内购 · Apple 账户余额」这条路线？",
    a: "因为 Anthropic 和 OpenAI 根本接触不到你的卡：在 iPhone / iPad 的官方 App 里订阅，收银台是 Apple 的，扣的是 Apple 账户余额。余额用礼品卡充，礼品卡用支付宝买，全程不需要一张能过 Stripe 的外币卡。",
  },
  {
    q: "为什么 Claude Max 在 iOS 上比网页贵 25%？要不要在 iOS 上买？",
    a: "App Store 内购要给 Apple 交佣金，Anthropic 把这部分加进了 iOS 价格：Max 5x 网页 $100、iOS $124.99；Max 20x 网页 $200、iOS $249.99；Pro 两边都是 $20。ChatGPT 的各档在两边价格一样。如果你手上有能过 Stripe 的本人银行卡，Max 走网页绑卡一年能省下 $300～$600；没有的话，iOS 内购多付 25% 换一个「肯定能开通」，也值。",
  },
  {
    q: "为什么会多出 8%～10% 的税？礼品卡该充多少？",
    a: "美国部分州对数字订阅征销售税，税额按你账单地址所在州算：标价 $20 的订阅在有税的州结算时可能是 $21.80。Apple 余额不够这 $1.80 就会扣款失败，反复重试还会触发风控。所以按你真实地址所在州的税率，充值时留出余量，不要卡着 $20 充。不收州销售税的州只有五个（见「答疑与合规」里的格式样例）—— 但那是格式样例，账单地址请填你本人的真实地址。",
  },
  {
    q: "网页端绑卡老是提示「Your card was declined」怎么办？",
    a: "常见三个原因：发卡行拒绝了境外无卡交易（去发卡行 App 里开通境外线上支付），当前出口 IP 被判定为机房或高欺诈分（换纯净住宅节点，可在本面板「IP 纯净度」页自测），余额没覆盖税费。依次排查；连续两次被拒就停，不要再试，改走苹果内购。",
  },
  {
    q: "注册时要手机验证，怎么办？",
    a: "用本人的手机号完成验证，这是最稳妥的。社区里也有人用海外短信接码平台（如 SMS-Activate、5SIM）接一次注册验证码，国家选服务区内的地区，之后日常登录用邮箱验证码或快捷授权即可。接码平台是独立的第三方，资金与交易成败自负；账号资料仍应使用本人真实、有效的信息，因不合规登记引发的审查后果由使用者自负。",
  },
  {
    q: "在手机上开通之后，电脑端要再交一次钱吗？",
    a: "不用。订阅绑在你的账号上，网页、桌面端、手机、Claude Code / Codex 全平台通用。回到电脑端打开本面板，在总览页受控启动，登录同一个账号即可。注意：在 iOS 订的只能在 Apple 的订阅页管理和取消，不要在网页端再订一次，否则会重复扣费。",
  },
  {
    q: "ChatGPT Go 是什么？值得选吗？",
    a: "Go 是 2025 年 8 月起在 170 多个国家和地区上线的 $8/月 入门档，比 Plus 便宜但用量和功能都少，也没有额度数据。只是偶尔聊聊天可以先从 Go 开始；要用 Codex 跑代码任务，直接看 Plus 或 Pro。",
  },
  {
    q: "支付宝里搜不到礼品卡怎么办？",
    a: "先把支付宝首页左上角的定位城市手动切到「旧金山」或「纽约」，再搜「大牌礼卡」或「Pockyt Shop」。小程序维护时，可在电脑浏览器打开 Pockyt 官方商店（shop.pockyt.io）购买，同样支持支付宝扫码。只买「App Store & iTunes US」这一种，别的区的卡兑换不了、也退不了。",
  },
  {
    q: "下个月不想用了怎么取消？取消会影响本月吗？",
    a: "不影响，取消后当期剩余天数照常可用。在哪订的就在哪取消：iOS 在「设置 → 姓名 → 订阅」；Android 在 Play 商店「付款和订阅 → 订阅」；网页在官网 Settings → Billing。卸载 App 不等于退订。",
  },
  {
    q: "已经扣了款，能退吗？",
    a: "看在哪订的。Apple 内购到 reportaproblem.apple.com 申请，是否受理由 Apple 决定；Google Play 在订单页申请退款；官网订阅按 Anthropic / OpenAI 的消费者条款处理 —— Anthropic 的条款默认不退款，只有少数地区有 7 天窗口。所以先用月付试一个月，别一上来就年付。",
  },
  {
    q: "我自己的订阅，能拿去给第三方工具或中转站用吗？",
    a: "不能。Anthropic 2026 年 2 月起在条款里明写：Pro / Max 订阅的 OAuth 只能用于官方 Claude Code 与 Claude 应用，用在任何第三方工具、服务或中转上都属违规，会封号，而且 4 月起已经在服务端强制执行。要给别的工具用，走官方 API Key 按量付费。",
  },
  {
    q: "在中转站买的「会员」、跟人拼车合租，算不算自己订阅了？",
    a: "不算。那是几十个人共用别人名下的一个账号，你既不是账号的所有者，也享受不到任何条款保护；账号被封、商家跑路、模型被换，你都只能认。首页那六条问题说的就是这种「会员」。",
  },
  {
    q: "照着这一页做，能保证成功、保证不封号吗？",
    a: "不能，也没有任何资料能做到。服务商的风控是多维度、持续变化的，会综合网络环境、设备、支付方式和使用行为判断。这一页只是把公开的经验和已核实的数字整理出来，帮你避开已知的坑，不构成任何承诺。",
  },
];

// ---------------------------------------------------------------- 来源

export interface Source {
  id: string;
  label: string;
  url: string;
  note?: string;
}

export const SOURCES: Source[] = [
  {
    id: "claude-pricing",
    label: "Claude 官方价目",
    url: "https://claude.com/pricing",
    note: "Pro $20（年付 $17）· Max 5x $100 · Max 20x $200；价格不含税",
  },
  {
    id: "claude-appstore",
    label: "App Store · Claude by Anthropic（内购价）",
    url: "https://apps.apple.com/us/app/claude-by-anthropic/id6473753684",
    note: "Pro $20.00 / 年付 $214.99 · Max 5x $124.99 · Max 20x $249.99",
  },
  {
    id: "chatgpt-appstore",
    label: "App Store · ChatGPT（内购价）",
    url: "https://apps.apple.com/us/app/chatgpt/id6448311069",
    note: "Go $8.00 · Plus $19.99 · Pro 5x $100.00 · Pro 20x $200.00",
  },
  {
    id: "openai-pro-tiers",
    label: "OpenAI 帮助中心 · ChatGPT Pro 各档",
    url: "https://help.openai.com/en/articles/9793128-about-chatgpt-pro-tiers",
    note: "2026-04-09 新增 $100 档：Codex 用量 5× Plus；$200 档 20× Plus",
  },
  {
    id: "linuxdo-quota",
    label:
      "linux.do ·【持续更新】国内外 AI 订阅性价比（网友估算，2026-08 / 09）",
    url: "https://linux.do/t/topic/2831355",
    note: "周额度：Claude Pro $200～500、Max 5x 约 $2,500、Max 20x $3,000～5,000；ChatGPT Plus $100～160（5 小时 $20～30）、Pro 5x $500～700、Pro 20x $2,000～3,000。本页取中间值",
  },
  {
    id: "register-anthropic",
    label: "The Register · Anthropic 明确禁止第三方工具使用订阅 OAuth",
    url: "https://www.theregister.com/2026/02/20/anthropic_clarifies_ban_third_party_claude_access/",
    note: "2026-01 服务端限制，02-19 写进条款，违规封号；合规做法是官方 API Key",
  },
  {
    id: "codex-45199",
    label: "openai/codex issue #45199（使用者反馈，未证实）",
    url: "https://github.com/openai/codex/issues/45199",
    note: "高并发下请求 GPT-6 Astra / 5.6 Sol，疑似收到 GPT-4o 的回答",
  },
  {
    id: "codex-46632",
    label: "openai/codex issue #46632（使用者反馈，未证实）",
    url: "https://github.com/openai/codex/issues/46632",
    note: "Plus 账号请求 gpt-6-astra 被以 gpt-5.6-luna 响应",
  },
  {
    id: "arxiv-2604-08407",
    label: "arXiv 2604.08407 · Your Agent Is Mine（2026-04）",
    url: "https://arxiv.org/abs/2604.08407",
    note: "测 428 个中转：9 个注入恶意代码、17 个窃取凭证、1 个转走资产",
  },
  {
    id: "sina-relay",
    label:
      "新浪财经 · 起底「AI 中转站」：封号跑路、模型降智、倒卖用户数据（2026-05-12）",
    url: "https://finance.sina.com.cn/roll/2026-05-12/doc-inhxrfsp8402044.shtml",
    note: "拆号卖给数十人、包月断服、涨价 3～4 倍、从业者谈卖数据",
  },
  {
    id: "tencent-relay",
    label: "腾讯云开发者社区 · AI API 中转站完全解析",
    url: "https://cloud.tencent.com/developer/article/2657436",
    note: "官转 0.8～1.5×、逆向 0.05～0.3×；Opus 低于 0.5× 多半掺逆向或偷换",
  },
  {
    id: "zhihu-relay-ban",
    label: "知乎 · Claude Code 海量封号，中转站如何稳坐钓鱼台",
    url: "https://zhuanlan.zhihu.com/p/1980278520749002862",
    note: "号池型中转的运作方式；有运营方称九成账号被封",
  },
  {
    id: "zhihu-relay-ip",
    label: "知乎 · OpenAI API 中转安全：出口集中导致的关联风控",
    url: "https://zhuanlan.zhihu.com/p/2025640763321499891",
    note: "几千个 Key 挤在少数出口，一个被判异常，同出口账号可能一起被封",
  },
  {
    id: "claude-regions",
    label: "Anthropic · 支持的国家与地区",
    url: CLAUDE_REGIONS_URL,
    note: "分 API 与 Claude.ai 两张名单",
  },
  {
    id: "openai-regions",
    label: "OpenAI · ChatGPT 支持的国家与地区",
    url: OPENAI_REGIONS_URL,
  },
  {
    id: "claude-ios-signup",
    label: "Claude 帮助中心 · 在 iOS App 里订阅 Pro",
    url: "https://support.claude.com/en/articles/9266495-how-do-i-sign-up-for-claude-pro-on-the-claude-app-for-ios",
    note: "右上角头像 → 设置 → Upgrade to Pro → 完成内购",
  },
  {
    id: "apple-redeem",
    label: "Apple · 兑换礼品卡；礼品卡只能在购买国家或地区兑换",
    url: "https://support.apple.com/118285",
  },
  {
    id: "apple-balance",
    label: "Apple · 账户余额能买什么",
    url: "https://support.apple.com/118245",
  },
  {
    id: "apple-refund",
    label: "Apple · 申请退款",
    url: "https://reportaproblem.apple.com/",
  },
  {
    id: "google-cancel",
    label: "Google Play · 取消订阅（卸载不等于退订）",
    url: "https://support.google.com/googleplay/answer/7018481",
  },
  {
    id: "google-region",
    label: "Google Play · 付款资料所在国家与地区",
    url: "https://support.google.com/googleplay/answer/7431675",
  },
  {
    id: "anthropic-terms",
    label: "Anthropic · 消费者条款（退款与账号共享）",
    url: "https://www.anthropic.com/legal/consumer-terms",
    note: "款项默认不退，少数地区 7 天窗口；禁止共享账号登录信息",
  },
  {
    id: "address-generator",
    label: "免税州地址生成器（只看格式）",
    url: ADDRESS_GENERATOR_URL,
    note: "网站自己写着：「" + ADDRESS_GENERATOR_OWN_WORDS + "」",
  },
];

export function sourceById(id: string): Source | undefined {
  return SOURCES.find((s) => s.id === id);
}
