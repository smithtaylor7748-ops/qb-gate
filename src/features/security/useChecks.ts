/** Five checks share a serialized runner. A complete batch also includes local
 * environment details and retains each failure while continuing later checks.
 */
import { useCallback } from "react";

import type { DnsReport, GateStatus, Risk } from "../../lib/api";
import { markStep } from "../../lib/progress";
import { R } from "../../lib/resources";
import type { ScanResult } from "../../lib/signals";
import {
  getSession,
  peek,
  refresh,
  setSession,
  useResource,
  useSession,
} from "../../lib/store";
import { useToast } from "../../ui";

export const CHECK_IDS = [
  "purity",
  "dns",
  "signals",
  "iplock",
  "egress",
] as const;
export type CheckId = (typeof CHECK_IDS)[number];
export type CheckRun = {
  completed: CheckId[];
  failed: Partial<Record<CheckId, string>>;
};
const BATCH = "security.batch";

/** 跑哪一项。空串 = 没在跑。会话态，所以两个消费者看到的是同一个。 */
const RUNNING = "security.running";
/** 上一次失败留下的错误原文。toast 会飘走，面板上那句不能飘。 */
const FAILED = "security.failed";

export const CHECK_LABEL: Record<CheckId, string> = {
  purity: "IP 纯净度",
  dns: "DNS 泄露",
  signals: "环境体检",
  iplock: "IP 锁",
  egress: "出口一致性",
};

/** 跑这一项时进度条上那句话。 */
const PHASE: Record<CheckId, string> = {
  purity: "正在查出口 IP 与纯净度…",
  dns: "正在查 DNS 泄露（约 6 秒）…",
  signals: "正在扫描环境信号…",
  iplock: "正在读门禁状态…",
  egress: "正在比较出口与浏览器策略…",
};

export interface Check {
  id: CheckId;
  label: string;
  /** 跑一次。已经有别的检测在跑就直接返回 —— 串行由这一句保证。 */
  run: () => Promise<void>;
  /** 这一项正在跑。 */
  running: boolean;
  /** 已经有结果了 —— 决定按钮写「开始检测」还是「重新检测」。 */
  done: boolean;
  /** 上一次的错误原文，没有就是 undefined。 */
  error?: string;
}

export type Checks = Record<CheckId, Check> & {
  /** 依次跑完五项。某一项失败不影响后面的。 */
  runAll: () => Promise<CheckRun>;
  /** 有检测在跑。 */
  busy: boolean;
  /** 正在跑的那一项的进度文字。空串 = 没在跑。 */
  phase: string;
};

export function useChecks(): Checks {
  const toast = useToast();
  const [running] = useSession<CheckId | "">(RUNNING, "");
  const [batch] = useSession(BATCH, false);
  const [failed] = useSession<Partial<Record<CheckId, string>>>(FAILED, {});

  // 这五个 `useResource` 在这里只为拿 `data`（判断「检测过没有」）。
  // 刷新一律走模块级的 `refresh(key)`，免得每个 run 都要多带一个参数。
  const ip = useResource("ip", R.ip);
  const dns = useResource("dns", R.dns);
  const signals = useResource("signals", R.signals);
  const gate = useResource("gate", R.gate);
  const egress = useResource("egress", R.egress);
  useResource("checkup", R.checkup);

  const run = useCallback(
    async (id: CheckId, inBatch = false) => {
      // 读实时值，不读 render 快照 —— `runAll` 串到第二项时闭包里那份还是旧的。
      if (
        getSession<CheckId | "">(RUNNING, "") ||
        (!inBatch && getSession(BATCH, false))
      )
        return;
      setSession(RUNNING, id);
      setSession(FAILED, {
        ...getSession<Partial<Record<CheckId, string>>>(FAILED, {}),
        [id]: undefined,
      });
      try {
        await BODY[id](toast);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        setSession(FAILED, {
          ...getSession<Partial<Record<CheckId, string>>>(FAILED, {}),
          [id]: msg,
        });
        toast.error(`${CHECK_LABEL[id]}：${msg}`);
      } finally {
        setSession(RUNNING, "");
      }
    },
    [toast],
  );

  const runAll = useCallback(async (): Promise<CheckRun> => {
    if (getSession(BATCH, false) || getSession<CheckId | "">(RUNNING, "")) {
      return { completed: [], failed: {} };
    }
    setSession(BATCH, true);
    const result: CheckRun = { completed: [], failed: {} };
    try {
      for (const id of CHECK_IDS) {
        await run(id, true);
        const error = getSession<Partial<Record<CheckId, string>>>(FAILED, {})[
          id
        ];
        if (error) result.failed[id] = error;
        else result.completed.push(id);
      }
      return result;
    } finally {
      setSession(BATCH, false);
    }
  }, [run]);

  const has: Record<CheckId, boolean> = {
    // IP、系数和住宅判定来自同一份读数，避免第二次联网失败否定第一次结果。
    purity: !!ip.data && !ip.error,
    dns: !!dns.data,
    signals: !!signals.data,
    iplock: !!gate.data,
    egress: !!egress.data,
  };

  const one = (id: CheckId): Check => ({
    id,
    label: CHECK_LABEL[id],
    run: () => run(id),
    running: running === id,
    done: has[id],
    error: id === "purity" ? ip.error : failed[id],
  });

  return {
    purity: one("purity"),
    dns: one("dns"),
    signals: one("signals"),
    iplock: one("iplock"),
    egress: one("egress"),
    runAll,
    busy: running !== "" || batch,
    phase: running ? PHASE[running] : "",
  };
}

// ------------------------------------------------------------ 各项的正文

type Toast = ReturnType<typeof useToast>;

const BODY: Record<CheckId, (toast: Toast) => Promise<void>> = {
  /**
   * 面板自测：一次请求取得出口 IP、系数与住宅判定，**不改人工结论**。
   * 通过与否由使用者自己去 IPQS / ippure 核对后勾选（`judgePurity`）。
   */
  async purity(toast) {
    await refresh("ip");
    toast.info("本机 IP 自测完成");
  },

  async dns(toast) {
    await refresh("dns");
    // 注意：调用方手里的 `dns.data` 是本次渲染的快照，refresh 之后它还是旧值。
    // 要拿刚回来的结果得直接读缓存。
    const r = peek<DnsReport>("dns");
    if (!r) return;
    const risk: Risk = r.passed
      ? "low"
      : r.findings.length > 1
        ? "high"
        : "medium";
    await markStep(
      "dns",
      r.passed ? "passed" : "failed",
      risk,
      r.passed
        ? "简易检测通过，未发现配置层或解析层泄露"
        : `发现 ${r.findings.length} 项：${r.findings[0]}`,
    );
    if (r.passed) toast.ok("未发现泄露");
    else toast.error(`发现 ${r.findings.length} 项问题`);
  },

  async signals(toast) {
    await refresh("signals");
    await refresh("checkup");
    const r = peek<ScanResult>("signals");
    if (r) toast.ok(`识别度 ${r.total} / 100 · 命中 ${r.hits.length} 项`);
  },

  async egress() {
    await refresh("egress");
  },

  async iplock(toast) {
    await refresh("gate");
    const risk = await recordGate();
    if (risk === "high") toast.error("门禁不会放行当前出口");
    else if (risk) toast.ok("门禁状态已更新");
  },
};

/**
 * 把**缓存里现有的**门禁状态记进 `progress.json`，返回算出来的风险档。
 *
 * 单独拆出来是因为有两类调用方：这里的「检测」（先 refresh 再记），
 * 以及白名单/上锁/清残留那几个动作（自己已经 refresh 过了，只补记一笔）。
 * 判定规则写两遍的话，一边改了另一边没改，评分就会取决于你是怎么触发的。
 *
 * 返回 `null` = 缓存里还没有门禁状态，什么都没记。
 */
export async function recordGate(): Promise<Risk | null> {
  const s = peek<GateStatus>("gate");
  if (!s) return null;
  const risk: Risk =
    s.allowlist.length === 0 || !s.ip_allowed
      ? "high"
      : s.stale_copies.length > 0
        ? "medium"
        : "low";
  await markStep(
    "iplock",
    risk === "high" ? "failed" : "passed",
    risk,
    s.allowlist.length === 0
      ? "白名单为空，门禁不会放行任何 IP"
      : s.ip_allowed
        ? `当前 IP ${s.current_ip} 在白名单内`
        : `当前 IP ${s.current_ip ?? "未知"} 不在白名单内`,
  );
  return risk;
}

/**
 * 纯净度的人工判定 —— 这一项的真值只能人工给。
 *
 * `verdict.rs` 把 `native` 写死成 `Unknown`（公开接口就是没这个字段），
 * 所以 `PanelVerdict.passed` 结构上永远是 false。评分那 35 分完全建立在
 * 这条路径上，见 `score.ts::purityItem`。
 */
export async function judgePurity(
  pass: boolean,
  ip: string | undefined,
): Promise<void> {
  if (pass && !ip) throw new Error("先检测当前出口 IP，再记录人工复核结果");
  if (pass) {
    await markStep(
      "purity",
      "passed",
      "low",
      `已人工确认两家均通过（${ip ?? "未知 IP"}）`,
    );
  } else {
    await markStep(
      "purity",
      "failed",
      "high",
      "人工复核未通过：三项硬指标至少缺一",
    );
  }
}
