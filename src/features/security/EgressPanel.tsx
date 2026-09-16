/**
 * 第五个安全对象：**出口一致性**。
 *
 * # 它只问一个问题
 *
 * 不是「浏览器隐私面怎么样」（那是软件页的 Chrome 卡片），也不是
 * 「这台机器像不像中文环境」（那是「中文环境」那十项）。这一项只收
 * **与出口 IP 对不上**的信号：绕过代理与跟随代理量到的是不是同一个国家、
 * IPv6 会不会从另一条路出去、浏览器报的语言跟 IP 地区合不合得上、
 * WebRTC 会不会把真实地址捅出去、DNS 是不是走了另一条路。
 *
 * 浏览器里的 claude.ai 痕迹、扩展权限、凭据管理器、环境变量残留都**不在这里**
 * —— 它们跟出口 IP 没关系，各自已经有地方（软件页的 Chrome 卡片、
 * 「中文环境」详情里的本机体检）。
 *
 * # ⛔ 「查不出来」不是「通过」
 *
 * `unknown` 单独一档语气色，而且**从评分的分子分母里一起去掉**
 * （判定在 Rust 的 `EgressChecks::ratio`）—— 算 0 分是在冤枉使用者，
 * 算满分是在替一个没做过的检查打包票。跟 `score.ts` 顶上那条同一个道理。
 *
 * # ⛔ 只检测、只如实报告
 *
 * 「浏览器语言与出口对不上」只报告，没有「修」。改机器身份去对上出口，
 * 是 CLAUDE.md「不许加的功能」第一行点名的设备指纹伪装。
 */

import { useState } from "react";
import { useChecks } from "./useChecks";
import { Play, RotateCw } from "lucide-react";

import { api, type CheckItem, type CheckState } from "../../lib/api";
import { R } from "../../lib/resources";
import { useResource } from "../../lib/store";
import { Button, Card, EmptyState, Pill, useToast } from "../../ui";

/** 语气色。`unknown` 自己一档 —— 见文件头。 */
const TONE: Record<CheckState, "ok" | "warn" | "danger" | "default"> = {
  pass: "ok",
  warn: "warn",
  fail: "danger",
  unknown: "default",
};

const WORD: Record<CheckState, string> = {
  pass: "对得上",
  warn: "对不上",
  fail: "不一致",
  unknown: "查不了",
};

/** 改过之后能撤销回去的那几项 —— 只有面板自己写过注册表的。 */
const UNDOABLE = new Set(["browser_webrtc", "browser_doh"]);

export function EgressRunButton({ compact = false }: { compact?: boolean }) {
  const egress = useResource("egress", R.egress);
  const check = useChecks().egress;
  const done = !!egress.data;
  return (
    <Button
      variant={done ? "default" : "primary"}
      size={compact ? "sm" : undefined}
      icon={done ? <RotateCw size={13} /> : <Play size={13} />}
      loading={egress.loading}
      onClick={() => void check.run()}
    >
      {egress.loading ? "检测中…" : done ? "重新检测" : "开始检测"}
    </Button>
  );
}

export function EgressResult() {
  const toast = useToast();
  const egress = useResource("egress", R.egress);
  const ip = useResource("ip", R.ip);
  const [busy, setBusy] = useState("");

  const data = egress.data;

  async function run(id: string, undo: boolean) {
    setBusy(id + (undo ? ":undo" : ""));
    try {
      const msg = undo ? await api.egressUndo(id) : await api.egressFix(id);
      toast.ok(msg);
      // 改完立刻重测 —— 「改了」和「生效了」是两件事，界面上摆着的
      // 必须是重新量过的那一份。
      await egress.refresh();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy("");
    }
  }

  if (!data && !egress.loading) {
    return (
      <Card className="mb-3">
        {egress.error && (
          <p className="notice notice--danger mb-2">{egress.error}</p>
        )}
        <EmptyState
          title="还没检测过"
          action={
            <Button
              variant="primary"
              icon={<Play size={13} />}
              onClick={() => void egress.refresh()}
            >
              开始检测
            </Button>
          }
        >
          查<strong>跟出口 IP 对不上的东西</strong>：代理前后是不是同一个国家、
          IPv6、浏览器语言、WebRTC、DNS。要真发两轮请求，几秒钟。
        </EmptyState>
      </Card>
    );
  }

  return (
    <Card className="mb-3">
      <div className="mb-2 flex flex-wrap items-center gap-2">
        <h3 className="card-title">出口一致性</h3>
        <span className="notice">
          {data ? `${data.checked_at} 测的` : "检测中…"}
          {!ip.data && " · 出口 IP 还没测，有一项对不上号"}
        </span>
        <span className="ml-auto">
          <EgressRunButton compact />
        </span>
      </div>

      {egress.error && (
        <p className="notice notice--danger mb-2">{egress.error}</p>
      )}

      <div className="flex flex-col gap-1.5">
        {(data?.items ?? []).map((it) => (
          <Row
            key={it.id}
            item={it}
            busy={busy}
            undoable={data?.undoable?.includes(it.id) ?? false}
            onFix={(undo) => void run(it.id, undo)}
          />
        ))}
      </div>

      <p className="notice mt-2">
        浏览器那三项只看 Chrome。写进注册表
        <strong>不等于 Chrome 已经用上</strong>：改完重启 Chrome，在{" "}
        <code>chrome://policy</code> 核对。「查不了」的项不参与评分。
      </p>
    </Card>
  );
}

function Row({
  item: it,
  busy,
  onFix,
  undoable,
}: {
  item: CheckItem;
  busy: string;
  undoable: boolean;
  onFix: (undo: boolean) => void;
}) {
  return (
    <div className="slotrow">
      <div className="slotrow-main">
        <span className="slotrow-label">{it.label}</span>
        <span className="slotrow-side">
          <Pill tone={TONE[it.state]}>{WORD[it.state]}</Pill>
          {it.fixable && (
            <Button
              size="sm"
              variant="primary"
              loading={busy === it.id}
              disabled={!!busy}
              onClick={() => onFix(false)}
            >
              修
            </Button>
          )}
          {undoable && UNDOABLE.has(it.id) && (
            <Button
              size="sm"
              loading={busy === `${it.id}:undo`}
              disabled={!!busy}
              onClick={() => onFix(true)}
            >
              撤销
            </Button>
          )}
        </span>
      </div>
      <p className="notice">{it.detail}</p>
      {it.manual && <p className="notice">{it.manual}</p>}
    </div>
  );
}

export function EgressWhy() {
  return (
    <Card>
      <h3 className="card-title">为什么单列这一项</h3>
      <p className="notice mt-1">
        面板显示的出口 IP 是它<strong>绕过系统代理</strong>量出来的。 浏览器或
        Claude 走另一条路（系统代理、IPv6、WebRTC 的非代理 UDP、 系统 DNS）时，
        <strong>面板显示的出口就不是请求实际走的那个</strong>
        —— 而门禁、白名单、纯净度全建立在它上面。
      </p>
      <p className="notice mt-2">
        面板<strong>不替你改浏览器或系统的身份</strong>，只告诉你对不上。
      </p>
    </Card>
  );
}
