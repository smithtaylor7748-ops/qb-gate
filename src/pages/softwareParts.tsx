/**
 * 软件页几张卡共用的版式。原来是 `Environment.tsx` 里的局部函数；2026-09-29 软件页多了
 * SillyTavern（酒馆）那张卡（`features/tavern/SillyTavernCard.tsx`），它要用同样的栏与进度块 ——
 * 挪到这里导出，免得那张卡反过来 import `Environment.tsx`（两边互相引用就是一个环）。
 *
 * 2026-11-05：软件卡统一成一个模板 [`SoftwareCard`]（照 Claude Code 那张）：标题行、一行说明 +
 * 「当前：」、三栏「版本 / 门禁 / 卸载」、底部折叠行。各卡只往插槽里填东西。
 */
import type { ReactNode } from "react";
import { Lock, Terminal, Trash2 } from "lucide-react";

import { api, type PurgeTarget } from "../lib/api";
import type { Software } from "../lib/generated/Software";
import { R } from "../lib/resources";
import { pillLabel, pillTone } from "../lib/software";
import { useResource } from "../lib/store";
import type { TaskState } from "../lib/tasks";
import {
  Button,
  Card,
  Checkbox,
  Collapsible,
  LogView,
  Pill,
  ProgressBar,
  Row,
} from "../ui";

/**
 * 长任务的进度条 + 日志。
 *
 * ⛔ **要画在按下去的那张卡里。** 0.28.0 之前 `install` 这个任务的进度块只渲染在
 * 第一张卡（Claude Code），而 Codex CLI 与 Claude 桌面端的「安装」按钮走的是同一个
 * 任务名 —— 点的是第三张卡，进度条出现在页面最上面那张卡里，按钮那一带一动不动。
 * 所以共用任务名的几张卡要用 `show` 指明「这一次是谁在跑」。
 */
export function TaskBlock({
  task,
  show = true,
  label = "安装进度",
}: {
  task: TaskState;
  show?: boolean;
  label?: string;
}) {
  if (!show || (!task.running && task.log.length === 0)) return null;
  return (
    <div className="mt-3">
      <ProgressBar
        value={task.total > 0 ? (task.step / task.total) * 100 : undefined}
        tone={task.error ? "danger" : "accent"}
        label={label}
      />
      <p className="notice mt-1">{task.phase}</p>
      <LogView lines={task.log} follow={task.running} />
    </div>
  );
}

/** 卡内一栏。三栏之间用左边框分隔，窄屏时自然堆叠。 */
export function Col({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <section className="min-w-0 border-line px-1 py-2 lg:border-l lg:px-4 lg:first:border-l-0 lg:first:pl-0">
      <span className="notice mb-2 block font-mono uppercase tracking-wide">
        {label}
      </span>
      {children}
    </section>
  );
}

/** 三栏「版本 / 门禁 / 卸载」。卡片模板与「其他命令行工具」每一行展开后共用。 */
export function SwCols({
  version,
  relocate,
  gate,
  uninstall,
}: {
  version: ReactNode;
  /** 版本栏底部，留给以后的「迁移…」按钮。 */
  relocate?: ReactNode;
  gate: ReactNode;
  uninstall: ReactNode;
}) {
  return (
    <div className="qb-sw-cols grid gap-1 lg:grid-cols-3">
      <Col label="版本">
        {version}
        {relocate && <div className="qb-sw-relocate">{relocate}</div>}
      </Col>
      <Col label="门禁">{gate}</Col>
      <Col label="卸载">{uninstall}</Col>
    </div>
  );
}

/**
 * 软件卡模板：标题行（名称 + 右侧药丸与主按钮）、一行说明 +「当前：」、三栏、
 * 卡内进度块（`children`）、底部折叠行（`footer`）。
 */
export function SoftwareCard({
  title,
  id,
  actions,
  intro,
  current,
  version,
  relocate,
  gate,
  uninstall,
  footer,
  children,
}: {
  title: string;
  id?: string;
  actions: ReactNode;
  intro?: ReactNode;
  current?: ReactNode;
  version: ReactNode;
  relocate?: ReactNode;
  gate: ReactNode;
  uninstall: ReactNode;
  footer?: ReactNode;
  children?: ReactNode;
}) {
  const card = (
    <Card title={title} className="mb-3 qb-sw-card" actions={actions}>
      {(intro || current) && (
        <p className="notice mb-2 break-words">
          {intro}
          {intro && current ? " " : null}
          {current}
        </p>
      )}
      <SwCols
        version={version}
        relocate={relocate}
        gate={gate}
        uninstall={uninstall}
      />
      {children}
      {footer && <div className="qb-sw-footer">{footer}</div>}
    </Card>
  );
  return id ? <div id={id}>{card}</div> : card;
}

/**
 * 「当前：<路径>」。路径是**本机实际在用**的那一份（检测结果），不是托管目标路径 ——
 * 原来写的是 `managedOf(..).path`，本机那里其实没装也照样显示。
 * 不是托管那份时注明「不在托管目录」。
 */
export function CurrentPath({
  path,
  managedPath,
}: {
  path: string | null | undefined;
  /** 托管那份的路径；不传 = 这个软件不进托管目录，不做比较。 */
  managedPath?: string | null;
}) {
  if (!path) return <>当前：未检测到。</>;
  const outside =
    managedPath !== undefined &&
    !(managedPath && path.toLowerCase() === managedPath.toLowerCase());
  return (
    <>
      当前：<code className="break-all">{path}</code>
      {outside && "（不在托管目录）"}
    </>
  );
}

/**
 * 卡片右上角那颗「装没装」的 Pill。
 *
 * ⛔ **每一张卡都用它**，别再各写各的三元表达式。文案与色调都来自
 * `lib/software.ts`，跟版本行同源 —— 这是「Pill 和版本行互相矛盾」那一类 bug
 * （0.28.0 之前四张卡都有）唯一治得住的办法。`product` 跟 [`SwVersion`] 配对，
 * test:ui 按它逐个产品比（反重力一卡两个产品）。
 */
export function SwPill({
  sw,
  prefix,
  product = "main",
}: {
  sw: Software | undefined;
  prefix?: string;
  product?: string;
}) {
  return (
    <span className="contents" data-sw-pill={product}>
      <Pill tone={pillTone(sw)}>
        {prefix ? `${prefix} ` : ""}
        {pillLabel(sw)}
      </Pill>
    </span>
  );
}

/** 版本栏那行大字。`product` 跟 [`SwPill`] 配对。 */
export function SwVersion({
  product = "main",
  children,
}: {
  product?: string;
  children: ReactNode;
}) {
  return (
    <span className="metric-v mono" data-sw-version={product}>
      {children}
    </span>
  );
}

/**
 * 「卸载提示词」按钮 + 它下面那句说明。
 *
 * ⛔ 一份就够。0.28.0 之前 Chrome 那一列自己抄了一份 `UninstallCol`，
 * 于是同一个按钮有两套说明。
 */
export function PromptButton({
  onPrompt,
  disabled,
}: {
  onPrompt: () => void;
  disabled?: boolean;
}) {
  return (
    <div className="mt-3 border-t border-line pt-3">
      <Button
        size="sm"
        icon={<Terminal size={13} />}
        disabled={disabled}
        onClick={onPrompt}
      >
        卸载提示词
      </Button>
      <p className="notice mt-1">
        给别的 AI 用，兜面板够不到的地方：WSL 发行版、其它 Windows 用户账户。
      </p>
    </div>
  );
}

/**
 * 卸载栏的内容（外面那一栏由 [`SoftwareCard`] 套）。
 *
 * `compact`：反重力 / Gemini CLI 这种没有官方重装流程的，只有「完全卸载」，
 * 不给「卸载提示词」—— 走的仍是同一套「只读盘点 → 输入确认词 → 执行 → 复扫」。
 */
export function UninstallCol({
  target,
  busy,
  onPlan,
  onPrompt,
  note,
  compact = false,
}: {
  target: PurgeTarget;
  busy: string;
  onPlan: (t: PurgeTarget) => void;
  onPrompt?: (t: PurgeTarget) => void;
  note: string;
  compact?: boolean;
}) {
  const button = (
    <Button
      size="sm"
      variant="danger"
      icon={<Trash2 size={13} />}
      loading={busy === `plan-${target}`}
      disabled={!!busy}
      onClick={() => onPlan(target)}
    >
      完全卸载
    </Button>
  );
  if (compact) {
    return (
      <>
        {button}
        <p className="notice mt-1">
          先<strong>只读盘点</strong>，输入「卸载」两个字才动手。{note}
        </p>
      </>
    );
  }
  return (
    <>
      <p className="notice">
        按类清掉：程序、版本库、缓存与登记、配置与会话、认证、环境变量与 PATH、
        Shell 配置行、启动项、账户槽位。<strong>没有选项，一次清完。</strong>
      </p>
      <div className="mt-2">{button}</div>
      <p className="notice mt-1">
        先<strong>只读盘点</strong>：每一处带着绝对路径与归属依据摆给你看，
        输入「卸载」两个字才动手。{note}
      </p>
      {onPrompt && (
        <PromptButton onPrompt={() => onPrompt(target)} disabled={!!busy} />
      )}
    </>
  );
}

/** 一次「点一下 → 调后端 → 把界面对齐」，见 `Environment.tsx` 的 `act`。 */
export type Act = (
  key: string,
  fn: () => Promise<string>,
  after: readonly string[],
) => Promise<boolean>;

/**
 * 「纳入 IP 门禁」那颗复选框。
 *
 * ⛔ Codex CLI 与 Codex 桌面端写的是**同一个设置键** `codex_outside_gate`
 * （同一个 OpenAI 账户，只管一个就是给另一个留门）。
 *
 * 反义存法（`*_outside_gate` 默认 false = 归门禁）是升级路径，别改成正着存：
 * 旧配置文件里没有这个键，读出来就是 `false`，于是所有人升级后自动落到「归门禁」。
 */
export function GateToggle({
  field,
  label,
  busy,
  act,
}: {
  field: "codex_outside_gate" | "antigravity_outside_gate";
  label: string;
  busy: boolean;
  act: Act;
}) {
  const settings = useResource("settings", R.settings);
  return (
    <Checkbox
      checked={!settings.data?.[field]}
      disabled={busy || !settings.data}
      onChange={(v) =>
        void act(
          `gate:${field}`,
          () =>
            api
              .settingsSave({ ...settings.data!, [field]: !v })
              .then(() => `${label} ${v ? "已纳入" : "已移出"} IP 门禁`),
          ["settings", "gate"],
        )
      }
    >
      纳入 IP 门禁
    </Checkbox>
  );
}

/**
 * 执行锁那一行。三种形态：
 * - `always`：始终开，没有开关（Claude Code 每一份都必须锁）。
 * - `toggle`：可切换（`toggle` 里放 [`GateToggle`]）。
 * - `na`：不适用，`reason` 写清为什么。
 */
export function LockRow({
  kind,
  reason,
  toggle,
}: {
  kind: "always" | "toggle" | "na";
  reason: ReactNode;
  toggle?: ReactNode;
}) {
  return (
    <>
      {kind === "toggle" ? (
        toggle
      ) : (
        <div className="flex flex-wrap items-center gap-2">
          <Lock size={13} aria-hidden="true" />
          <strong className="text-sm">执行锁</strong>
          {kind === "always" ? (
            <Pill tone="ok">始终开</Pill>
          ) : (
            <Pill>不适用</Pill>
          )}
        </div>
      )}
      <p className="notice mt-1">{reason}</p>
    </>
  );
}

/** 「本机共 N 份副本」折叠。每一份标「锁得到 / 锁不到」。 */
export function InstallsFold({
  items,
}: {
  items: ReadonlyArray<{ path: string; lockable: boolean }> | undefined;
}) {
  if (!items?.length) return null;
  return (
    <Collapsible className="mt-3" summary={`本机共 ${items.length} 份副本`}>
      {items.map((i) => (
        <Row
          key={i.path}
          side={
            i.lockable ? (
              <Pill tone="ok">锁得到</Pill>
            ) : (
              <Pill tone="warn">锁不到</Pill>
            )
          }
        >
          <span className="notice block w-full break-all font-mono">
            {i.path}
          </span>
        </Row>
      ))}
    </Collapsible>
  );
}
