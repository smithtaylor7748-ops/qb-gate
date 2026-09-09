/**
 * 所有共享数据的集中声明。
 *
 * 页面里**不要**自己写 fetcher —— 同一个 key 必须对应同一个请求和同一套
 * 轮询策略，否则又会退回到「三个页面各打一遍 probeIp」的老样子。
 *
 * 策略的取舍：
 *   - 打第三方接口的（ip / purity）给 stale，不轮询：省额度，切页回来才刷。
 *   - 本机状态（gate / plugins）可以轮询，反正是本地 ACL 和进程查询。
 *   - 慢的（dns 约 6 秒、signals 含 1 秒 WebRTC 超时）一律手动。
 */

import { api } from './api';
import { res } from './store';
import { runScan } from './signals';

export const R = {
  /** 出口 IP。全项目就这一处探测，Home / Purity / IpLock / Environment 共用。 */
  ip: res(() => api.probeIp(), { staleMs: 60_000 }),
  purity: res(() => api.probePurity(), { staleMs: 60_000 }),
  criteria: res(() => api.purityCriteria()),

  /** 门禁状态。本地 ACL 查询，轮询无妨。 */
  gate: res(() => api.gateStatus(), { pollMs: 15_000 }),

  software: res(() => api.detectSoftware()),
  accounts: res(() => api.accountsList()),
  progress: res(() => api.progressLoad()),
  /** 中转站目录（三个 target 的全部记录）。Key 不在里面，只有掩码。 */
  relay: res(() => api.relayList()),
  tz: res(() => api.tzCurrent()),
  settings: res(() => api.settingsLoad()),
  snapshots: res(() => api.snapshotList()),
  profiles: res(() => api.profileList()),

  plugins: res(() => api.pluginList(), { pollMs: 20_000 }),

  /** DNS 探测约 6 秒（10 个探针域名各等 3 秒上限），只在用户点了才跑。 */
  dns: res(() => api.probeDns(), { auto: false }),

  /**
   * 中文环境识别。纯本地计算，但 `runScan()` 是串行 `for await`，
   * WebRTC 那项自带 1 秒超时，所以也别自动跑。
   */
  signals: res(() => runScan(), { auto: false }),

  /** 升级计划走 latest 渠道 —— 写死 stable 会降级，见档案 §4.5。 */
  upgrade: res(() => api.upgradePlan('latest'), { auto: false }),

  /** winget 安装能力探测。 */
  install: res(() => api.installProbe()),

  tavernConfig: res(() => api.tavernConfig()),
  tavernAssets: res(() => api.tavernAssets()),
  tavernBackups: res(() => api.tavernBackups()),
} as const;

/** 破坏性操作之后要作废哪些资源，集中列在这里，免得每个调用点各记一份。 */
export const AFTER = {
  /** 上锁 / 解锁 / 清残留 / 改白名单 —— 门禁状态变了。 */
  gate: ['gate'] as const,
  /** 放行或收回租约：门禁变了，账户的登录态也可能跟着变。 */
  lease: ['gate', 'accounts'] as const,
  /** 装完 / 升完：软件版本、门禁目标、升级计划全都要重算。 */
  install: ['software', 'gate', 'upgrade', 'install'] as const,
  /** 酒馆启停。 */
  tavern: ['plugins', 'gate'] as const,
  /** 应用档案 / 回滚快照：账户、中转站、门禁、快照列表全都可能变了。 */
  profile: ['accounts', 'relay', 'gate', 'tz', 'snapshots', 'profiles'] as const,
};
