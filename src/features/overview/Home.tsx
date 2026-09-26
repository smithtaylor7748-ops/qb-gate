/**
 * 总览。
 *
 * 只做组装：评分、账户与启动，以及真实风险的处理入口。
 *
 * # 布局为什么是这样（v0.7.0）
 *
 *   1. `ScoreBand` —— 综合评分，**通栏，不进页签**。
 *      IP 纯净度 / DNS / 中文环境 / IP 锁讲的是「这台机器安不安全」，
 *      两侧共用同一套指标，塞进任一页签都是错的。
 *   2. `Claude` / `GPT` 两边 —— 各自的账户与启动。
 *      0.20.0 起**由侧栏切**（`SIDE_KEY`），页面上不再有那条页签。
 *      分开的理由见 `GptBand.tsx` 的文件头：原来四个磁贴挤在一个 2×2 宫格里，
 *      而左边的账户槽位只对 Claude 有效。
 *   3. Claude 的 `KillBar` 与 Codex 桌面端的关闭按钮各自位于启动卡内，
 *      分别核验各自客户端的进程证据。
 *   4. ~~`LogBand`~~ —— 0.20.0 从这一页撤了，挪进「执行锁」那个小窗。
 *      它是一屏里最长的一块，而总览上真正要看的是「现在什么状态」；
 *      日志是出了事才去翻的东西，翻它的人本来就在门禁那一档里。
 *
 * # 风险提示
 *
 * 只在出事时出现，任何一处都不许缩进上面那行 meta 里：
 *   * **门禁已关闭**（v0.7.0 新增）—— 见下面那段注释，这是最要紧的一条；
 *   * IP 不合格爆红卡（档案 §4.2 的硬要求）。
 */

import { useEffect, useState } from "react";
import { DoorOpen, ShieldAlert } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";

import { api } from "../../lib/api";
import { useProgress } from "../../lib/progress";
import { AFTER, R } from "../../lib/resources";
import { invalidate, useResource, useSession } from "../../lib/store";
import { SIDE_KEY, type Side } from "../../lib/side";
import { Button, Card, Modal, useToast } from "../../ui";
import { openSecurity } from "../security/SecuritySheet";
import { useChecks } from "../security/useChecks";

import ScoreBand from "./ScoreBand";
import AccountBand from "./AccountBand";
import GptBand from "./GptBand";
import AntigravityBand from "./AntigravityBand";

export default function Home() {
  const progress = useProgress();
  const toast = useToast();
  const checks = useChecks();

  const ip = useResource("ip", R.ip);
  const gate = useResource("gate", R.gate);
  const criteria = useResource("criteria", R.criteria);

  const [reopening, setReopening] = useState(false);
  const [showAlerts, setShowAlerts] = useState(false);
  /** 记在会话里 —— 切到别的页再回来，还停在原来那一边。 */
  // 哪一边由侧栏切（`SIDE_KEY`），这里只读。
  const [side] = useSession<Side>(SIDE_KEY, "claude");

  const purityRec = progress.steps["purity"];
  const purityFailed =
    purityRec?.state === "failed" || purityRec?.risk === "high";

  /** 门被面板自己关上了，而且还没能自己开回来。 */
  const needsReopen = gate.data?.needs_reopen ?? null;

  // 待办都处理完了（重新放行成功、复核通过）就把弹窗收起来 —— 原来它留着一个只有标题的空框（2026-09-25）。
  useEffect(() => {
    if (showAlerts && !needsReopen && !purityFailed) setShowAlerts(false);
  }, [showAlerts, needsReopen, purityFailed]);

  /**
   * 一键全面体检。跑什么、怎么记进度全在 `useChecks` 里，这里只负责按一下。
   *
   * 那边是**串行**的：DNS 那项本身就要六秒、中文环境识别里还有个 1 秒的
   * WebRTC 超时 —— 并发跑省不了多少时间，却会让进度文字没法看。
   *
   * **跑不了 IP 纯净度的结论**：那一项只能由用户自己去 IPQS / ippure 看，
   * 面板自测与人工复核分别计数，已拿到本机读数就显示「已自测」。
   */
  async function runCheckup() {
    const result = await checks.runAll();
    const failures = Object.keys(result.failed).length;
    if (failures)
      toast.error(
        `已完成 ${result.completed.length}/5 项检测，${failures} 项失败，详见检测明细。`,
      );
    else if (result.completed.length)
      toast.info("五项检测已完成，结果见明细。IP 纯净度仍需人工复核。");
  }

  /** 重新放行：验一次出口 IP，过了就把门开回来并接回看门狗。 */
  async function reopen() {
    setReopening(true);
    try {
      toast.ok(await api.gateReopen());
      invalidate(...AFTER.lease);
    } catch (e) {
      // 验不过就该开不了 —— 原文已经说清是「不在白名单」还是「查不到 IP」，
      // 两者必须分开，不能合并成「重新放行失败」。
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setReopening(false);
    }
  }

  return (
    <div className="official-page">
      {/* 0.20.0 删掉了可见的页头：「官方账户」这四个字侧栏里已经高亮着，
          副标题是一句废话，而那两个按钮讲的就是评分栏里的事 ——
          留一行只放标题的横条，等于白占一屏的高度。
          两个按钮并进了评分栏的信息行。

          ⛔ 但 `h1` 要留着，只是读屏器可见。删掉可见的标题不等于这一页
          没有标题：整页的其余标题都是 `h2` / `h3`，没有 `h1` 的话标题层级
          从二级开始，读屏器的「按标题跳转」直接少一层。
          `test:ui` 那圈等的就是 `.qb-page h1`。 */}
      <h1 className="sr-only">官方账户</h1>

      {/* 五格里点任意一格开小窗，就地做完，不跳页。小窗本体挂在 Shell 上。 */}
      <ScoreBand
        onRecheck={() => {
          void ip.refresh();
          void gate.refresh();
        }}
        rechecking={ip.loading}
        onCheckup={() => void runCheckup()}
        checkupBusy={checks.busy}
        checkupPhase={checks.phase}
      />

      {/* 门禁被面板自己关上了。

          这一条必须常驻、必须显眼，因为它是唯一一个**使用者必须知道、
          却完全看不见**的状态：面板多半收在托盘里，ip-gate.log 等于没写，
          而症状要等他下次在 Claude 桌面端开新会话时才出现
          （`Claude Code couldn't start`）—— 那时候他不会把两件事联系起来。
          不给「不再提醒」，因为它不是提醒，是一个待办。 */}
      {(needsReopen || purityFailed) && (
        <div className="official-alert banner banner--danger">
          <ShieldAlert size={14} aria-hidden="true" />
          <strong>{needsReopen ? "门禁已关闭" : "IP 不合格"}</strong>
          <Button
            size="sm"
            className="ml-auto"
            onClick={() => setShowAlerts(true)}
          >
            查看并处理
          </Button>
        </div>
      )}
      <Modal
        open={showAlerts}
        onClose={() => setShowAlerts(false)}
        title="账户环境待办"
      >
        {needsReopen && (
          <div className="banner banner--danger mb-3">
            <DoorOpen size={14} className="banner-icon" aria-hidden="true" />
            <div className="banner-body">
              <strong>门禁已关闭</strong>
              <span className="notice block">{needsReopen}</span>
            </div>
            <div className="flex flex-shrink-0 gap-1.5">
              <Button
                size="sm"
                variant="primary"
                loading={reopening}
                onClick={() => void reopen()}
              >
                重新放行
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => openSecurity("lock")}
              >
                去执行锁
              </Button>
            </div>
          </div>
        )}

        {purityFailed && (
          <Card tone="danger" className="mb-3">
            <div className="flex items-start gap-2">
              <ShieldAlert
                size={16}
                className="mt-0.5 flex-shrink-0"
                aria-hidden="true"
              />
              <div className="min-w-0">
                <div className="text-md text-[var(--danger)]">IP 不合格</div>
                <p className="notice notice--danger mt-1">
                  {purityRec?.detail ||
                    "三项硬指标至少缺一：纯净度、原生 IP、住宅 IP。"}
                  继续用这条 IP 登录，风险由你自己承担。
                </p>
                <div className="mt-2 flex flex-wrap gap-2">
                  <Button
                    variant="danger"
                    onClick={() =>
                      criteria.data && openUrl(criteria.data.iproyal)
                    }
                    disabled={!criteria.data}
                  >
                    前往 IPRoyal 购买住宅 IP
                  </Button>
                  <Button onClick={() => openSecurity("purity")}>
                    重新复核
                  </Button>
                </div>
              </div>
            </div>
          </Card>
        )}
      </Modal>
      {/* ---------------------------------------------- Claude / Codex 页签 */}
      {/* 不认识的值落回默认那一边（Claude），不落进最后一个分支 —— 原来「不是 Claude
          也不是 GPT」就显示反重力，一个写错的键值就把人带到另一个产品的页面上。 */}
      {side === "gpt" ? (
        <GptBand />
      ) : side === "antigravity" ? (
        <AntigravityBand />
      ) : (
        <AccountBand />
      )}
    </div>
  );
}
