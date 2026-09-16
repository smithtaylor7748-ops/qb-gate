/** A reviewable repair plan built from measured evidence, never from score alone. */
import { useState } from "react";
import { ListChecks, Wrench } from "lucide-react";
import { api } from "../../lib/api";
import { describeLease } from "../../lib/lease";
import { R } from "../../lib/resources";
import { measuredAt, useResource } from "../../lib/store";
import { Button, Modal, Pill, fmtMeasured } from "../../ui";
import { CHECK_IDS, CHECK_LABEL, useChecks } from "./useChecks";
import { openSecurity } from "./SecuritySheet";
import type { ObjectId } from "./objects";

export default function RepairCenter() {
  const [open, setOpen] = useState(false);
  const [fixing, setFixing] = useState(false);
  const [results, setResults] = useState<
    Array<{ label: string; ok: boolean; detail: string }>
  >([]);
  const checks = useChecks();
  const ip = useResource("ip", R.ip);
  const dns = useResource("dns", R.dns);
  const signals = useResource("signals", R.signals);
  const local = useResource("checkup", R.checkup);
  const gate = useResource("gate", R.gate);
  const egress = useResource("egress", R.egress);
  const fixes = (egress.data?.items ?? []).filter(
    (it) => it.fixable && ["browser_webrtc", "browser_doh"].includes(it.id),
  );
  const g = gate.data;
  const repairLocks =
    !!g &&
    !gate.error &&
    g.allowlist.length > 0 &&
    !describeLease(g.lease, "") &&
    g.targets.some((t) => !t.locked);
  const fixCount = fixes.length + Number(repairLocks);

  async function repair() {
    if (fixing || checks.busy) return;
    setFixing(true);
    setResults([]);
    const report: typeof results = [];
    const actions = fixes.map((it) => ({
      label: it.label,
      run: () => api.egressFix(it.id),
    }));
    if (repairLocks)
      actions.push({
        label: "补齐已发现的执行锁",
        run: async () => `已处理 ${await api.gateLockAll()} 个执行锁`,
      });
    try {
      for (const action of actions) {
        try {
          report.push({
            label: action.label,
            ok: true,
            detail: await action.run(),
          });
        } catch (e) {
          report.push({
            label: action.label,
            ok: false,
            detail: e instanceof Error ? e.message : String(e),
          });
        }
        setResults([...report]);
      }
      const verify = await checks.runAll();
      const failures = Object.entries(verify.failed);
      report.push({
        label: "修复后复检",
        ok: failures.length === 0 && verify.completed.length === 5,
        detail: failures.length
          ? failures.map(([id, e]) => `${id}: ${e}`).join("；")
          : `已重新检测 ${verify.completed.length}/5 项。浏览器策略仍需重启 Chrome 后核对生效。`,
      });
      setResults([...report]);
    } finally {
      setFixing(false);
    }
  }

  const details = {
    purity: ip.data
      ? [
          `出口：${ip.data.ip ?? "未知"} · ${ip.data.countryCode ?? "国家未知"}`,
          `纯净度：${ip.data.fraudScore ?? "未知"}；住宅：${ip.data.isResidential == null ? "未知" : ip.data.isResidential ? "是" : "否"}；原生属性：公开接口未提供`,
          "无法一键改变 IP 信誉，需要人工复核权威报告。",
        ]
      : ["尚未获得出口 IP 与纯净度证据。"],
    dns: dns.data
      ? [
          `真实回显 ${dns.data.resolvers.filter((r) => !r.from_adapter).length} 个；网卡 DNS ${dns.data.resolvers.filter((r) => r.from_adapter).length} 个`,
          ...dns.data.findings,
          "修复系统 DNS 需要确认实际使用的网卡和解析器；本批次不改整机网络。",
        ]
      : ["尚未获得 DNS 解析回显与网卡配置。"],
    signals: signals.data
      ? [
          `10 项本地环境信号；命中 ${signals.data.hits.length} 项`,
          ...signals.data.hits.map(
            (s) => `${s.label}：${s.raw}（${s.points} 分）`,
          ),
          `本机补充检查 ${local.data?.items.length ?? 0} 项：代理、IPv6、DoH、配置密钥、环境变量与出口。`,
          "语言、字体与时区仅报告；不会为降低分数删除字体或改机器身份。",
        ]
      : ["尚未扫描本地环境信号。"],
    iplock: g
      ? [
          `已锁 ${g.targets.filter((t) => t.locked).length}/${g.targets.length} 份；白名单 ${g.allowlist.length} 条；残留 ${g.stale_copies.length} 份`,
          describeLease(g.lease, "有效租约")?.text ?? "没有有效租约",
          repairLocks
            ? "可补齐已发现副本的执行锁。"
            : "租约中的放行会保留；空白名单和残留文件需要单独处理。",
        ]
      : ["尚未读到执行锁状态。"],
    egress: egress.data?.items.map((it) => `${it.label}：${it.detail}`) ?? [
      "尚未比较直连、系统代理与浏览器策略。",
    ],
  };
  const targets: Record<(typeof CHECK_IDS)[number], ObjectId> = {
    purity: "purity",
    dns: "dns",
    signals: "signals",
    iplock: "lock",
    egress: "egress",
  };
  return (
    <>
      <Button
        size="sm"
        icon={<Wrench size={12} />}
        onClick={() => setOpen(true)}
      >
        一键修复 / 明细
      </Button>
      <Modal
        open={open}
        onClose={() => !fixing && setOpen(false)}
        title={
          <>
            <ListChecks size={18} />
            五项检测与修复
          </>
        }
        dismissible={!fixing}
        footer={
          <>
            <Button
              disabled={fixing}
              loading={checks.busy}
              onClick={() => void checks.runAll()}
            >
              重新检测五项
            </Button>
            <Button
              variant="primary"
              loading={fixing}
              disabled={checks.busy || fixCount === 0}
              onClick={() => void repair()}
            >
              一键修复 {fixCount} 项并复检
            </Button>
          </>
        }
      >
        <p className="notice mb-3">
          可修复项：
          {[
            ...fixes.map((it) => it.label),
            ...(repairLocks ? ["补齐执行锁"] : []),
          ].join("、") || "暂无，先完成检测"}
          。浏览器策略只修改当前用户，保存原值并支持撤销；WebRTC
          收紧可能影响部分通话连接，DoH 自动模式仍可能回退到系统 DNS。
        </p>
        {CHECK_IDS.map((id) => (
          <section key={id} className="mb-3 rounded border border-line p-3">
            <div className="mb-2 flex flex-wrap items-center gap-2">
              <strong>{CHECK_LABEL[id]}</strong>
              <Pill
                tone={
                  checks[id].error
                    ? "danger"
                    : checks[id].done
                      ? "default"
                      : "warn"
                }
              >
                {checks[id].error
                  ? "检测失败"
                  : checks[id].done
                    ? "已有证据"
                    : "未检测"}
              </Pill>
              <span className="notice">
                {fmtMeasured(
                  measuredAt(
                    id === "iplock" ? "gate" : id === "purity" ? "ip" : id,
                  ),
                )}
              </span>
              <Button
                size="sm"
                className="ml-auto"
                disabled={fixing}
                onClick={() => {
                  setOpen(false);
                  openSecurity(targets[id]);
                }}
              >
                详细处理
              </Button>
            </div>
            {checks[id].error && (
              <p role="alert" className="notice notice--danger">
                {checks[id].error}
              </p>
            )}
            {details[id].map((line, i) => (
              <p key={i} className="notice mt-1 break-words">
                {line}
              </p>
            ))}
          </section>
        ))}
        {results.length > 0 && (
          <div role="status">
            <h3>本次修复结果</h3>
            {results.map((r) => (
              <p
                key={r.label}
                className={`notice mt-2 ${r.ok ? "" : "notice--danger"}`}
              >
                <strong>
                  {r.label}：{r.ok ? "已执行" : "未完成"}
                </strong>{" "}
                · {r.detail}
              </p>
            ))}
          </div>
        )}
      </Modal>
    </>
  );
}
