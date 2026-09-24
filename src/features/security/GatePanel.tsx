/**
 * 门禁本体，拆成三个对象：白名单 / 执行锁 / 会话内门禁。
 *
 * 原来是一个 542 行的 `IpLock.tsx` 页面，从「当前出口」一路排到「ip-gate.log」。
 * 三件事的操作代价完全不同 —— 改白名单可能把自己关在门外，应急解锁会让门禁
 * 当场形同虚设，会话内门禁是 fail-closed 的拦截器 —— 混在一屏里滚，
 * 每一条警告都在别的警告旁边，等于都没说。
 *
 * ⚠ 三块都挂在同一个 `gate` 资源上。任何改动完之后要走 `useGateAction`，
 * 它负责 `invalidate(AFTER.gate)` + 刷新 + `recordGate()` 补记进度 ——
 * 少了最后一步，评分里的 IP 锁那 20 分就会停在动作之前的旧值。
 */
import { useEffect, useState } from "react";
import {
  AlertTriangle,
  Globe,
  Lock,
  Plus,
  RotateCw,
  Trash2,
  Unlock,
} from "lucide-react";

import { api } from "../../lib/api";
import { describeLease } from "../../lib/lease";
import { AFTER, R } from "../../lib/resources";
import { invalidate, useResource } from "../../lib/store";
import { useAction } from "../../lib/workspace";
import {
  Bullet,
  Button,
  Card,
  Collapsible,
  ConfirmDialog,
  EmptyState,
  LogView,
  Metric,
  Modal,
  Pill,
  Row,
  useToast,
  TARGET_KIND_HINT,
  TARGET_KIND_LABEL,
} from "../../ui";
import { recordGate, useChecks } from "./useChecks";
import LogBand from "../overview/LogBand";

/**
 * 一次门禁改动：跑动作 → 作废相关缓存 → 重读门禁 → 补记进度。
 *
 * 最后那一步容易漏。漏了的症状是「我明明把 IP 加进白名单了，总览还在爆红」。
 */
function useGateAction() {
  const toast = useToast();
  const gate = useResource("gate", R.gate);
  const [busy, setBusy] = useState("");

  async function act(name: string, fn: () => Promise<unknown>, ok: string) {
    setBusy(name);
    try {
      await fn();
      toast.ok(ok);
      invalidate(...AFTER.gate);
      await gate.refresh();
      await recordGate();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy("");
    }
  }

  return { gate, busy, act };
}

// ------------------------------------------------------------------ 白名单

export function AllowlistPanel() {
  const { gate, busy, act } = useGateAction();
  const check = useChecks().iplock;
  const [removing, setRemoving] = useState<string | null>(null);
  const st = gate.data;

  const emptyAllowlist = !!st && st.allowlist.length === 0;
  const locked = st?.targets.filter((t) => t.locked).length ?? 0;
  const lease = st ? describeLease(st.lease, "已租给") : null;

  return (
    <>
      {gate.error && <p className="notice notice--danger mb-3">{gate.error}</p>}

      {/* 白名单为空时最要紧的一件事：告诉用户「面板没上锁」并给出一步修复。 */}
      {emptyAllowlist && (
        <Card tone="warn" className="mb-3">
          <div className="flex items-start gap-2">
            <AlertTriangle
              size={16}
              className="mt-0.5 flex-shrink-0"
              aria-hidden="true"
            />
            <div className="min-w-0">
              <div className="text-md text-[var(--warn)]">
                白名单是空的，门禁尚未启用
              </div>
              <p className="notice notice--warn mt-1">
                空名单意味着任何 IP 都过不了门禁，所以面板
                <strong>启动时不会上锁</strong> ——
                否则你会被自己的工具关在门外。 先把当前 IP 加进来。
              </p>
              <Button
                variant="primary"
                className="mt-2"
                icon={<Plus size={13} />}
                loading={busy === "add"}
                disabled={!st?.current_ip}
                onClick={() =>
                  void act(
                    "add",
                    api.allowlistAddCurrent,
                    `已把 ${st?.current_ip} 加进白名单`,
                  )
                }
              >
                把当前 IP 加进白名单
              </Button>
            </div>
          </div>
        </Card>
      )}

      <div className="mb-3 grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
        <Metric
          label="当前出口"
          mono
          loading={gate.loading && !st}
          error={gate.error}
          onRetry={() => void check.run()}
          emptyText="查不到"
          emptyHint="公网 IP 查询接口没返回。看门狗把「查不到」和「IP 变了」当两件事处理。"
        >
          {st?.current_ip}
        </Metric>
        <Metric
          label="是否在白名单"
          loading={gate.loading && !st}
          error={gate.error}
        >
          {st ? (
            st.ip_allowed ? (
              <Pill tone="ok">是</Pill>
            ) : (
              <Pill tone="danger">否</Pill>
            )
          ) : undefined}
        </Metric>
        <Metric label="执行锁" loading={gate.loading && !st} error={gate.error}>
          {st ? (
            lease ? (
              <span title={lease.title}>{lease.text}</span>
            ) : (
              `${locked} / ${st.targets.length} 已锁`
            )
          ) : undefined}
        </Metric>
        <Metric label="看门狗" loading={gate.loading && !st} error={gate.error}>
          {st ? (st.watchdog_running ? "运行中" : "未运行") : undefined}
        </Metric>
      </div>

      <Card
        as="h3"
        title="白名单"
        className="mb-3"
        actions={
          <Button
            size="sm"
            variant="primary"
            icon={<Plus size={12} />}
            loading={busy === "add"}
            disabled={!st?.current_ip}
            onClick={() =>
              void act(
                "add",
                api.allowlistAddCurrent,
                `已把 ${st?.current_ip} 加进白名单`,
              )
            }
          >
            加入当前 IP
          </Button>
        }
      >
        {st?.allowlist.length ? (
          st.allowlist.map((ip) => (
            <Row
              key={ip}
              side={
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<Trash2 size={12} />}
                  disabled={!!busy}
                  onClick={() => setRemoving(ip)}
                >
                  删除
                </Button>
              }
            >
              <span className="font-mono">{ip}</span>
              {ip === st.current_ip && <Pill tone="ok">当前</Pill>}
            </Row>
          ))
        ) : (
          <EmptyState title="白名单为空">
            没有任何 IP 会被放行。面板因此不上锁 —— 这是刻意的，见上方提示。
          </EmptyState>
        )}
      </Card>

      <GateRange />

      <CountryGate />

      <ConfirmDialog
        open={removing !== null}
        onCancel={() => setRemoving(null)}
        onConfirm={() =>
          removing !== null &&
          void act(
            "remove",
            () =>
              api.allowlistWrite(
                (st?.allowlist ?? []).filter((x) => x !== removing),
              ),
            `已从白名单删除 ${removing}`,
          ).then(() => setRemoving(null))
        }
        title="从白名单删除？"
        confirmLabel="确认删除"
        loading={busy === "remove"}
        danger={removing === st?.current_ip}
      >
        <p>
          要删除的是 <code>{removing ?? ""}</code>。
        </p>
        {removing === st?.current_ip && (
          <p className="notice notice--danger mt-2">
            <strong>这是你当前的出口 IP。</strong>
            删掉之后门禁不会再放行你，
            <strong>你会被自己的工具关在门外</strong>—— 那时只能走「应急解锁」。
          </p>
        )}
        {st?.allowlist.length === 1 && (
          <p className="notice notice--warn mt-2">
            删完白名单就空了。空名单时面板不会上锁，门禁等于关闭。
          </p>
        )}
      </ConfirmDialog>
    </>
  );
}

/**
 * 国家白名单 —— 门禁的第二维，跟 IP 白名单是「都要过」的关系。
 *
 * 重构时整个丢了：后端 `GateStatus.country_allowlist` 与 `country_presets`
 * 一直活着、demo 数据也齐，但全树**零个渲染点** —— 配过的人升上来会发现
 * 自己那份名单还在生效却改不动了。这里从 v0.11.0 取回。
 *
 * 界面上最要紧的一句话是「空 = 这一层没启用」—— 使用者最容易犯的错就是
 * 以为配了、其实是空的。所以空的时候给一条显眼的提示，而不是安静地留白。
 */
function CountryGate() {
  const settings = useResource("settings", R.settings);
  const action = useAction();
  const [draft, setDraft] = useState("");
  const [presets, setPresets] = useState<Array<[string, string[]]>>([]);

  useEffect(() => {
    let live = true;
    api
      .countryPresets()
      .then((p) => live && setPresets(p))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);

  const list = settings.data?.country_allowlist ?? [];
  const busy = !!action.pending || settings.loading;

  async function save(next: string[]) {
    if (!settings.data) return;
    await action.run(
      "country",
      () => api.settingsSave({ ...settings.data!, country_allowlist: next }),
      "国家白名单已更新",
    );
    await settings.refresh();
  }

  function add() {
    const c = draft.trim().toUpperCase();
    if (/^[A-Z]{2}$/.test(c) && !list.includes(c))
      void save([...list, c].sort());
    setDraft("");
  }

  return (
    <Card
      as="h3"
      title="国家白名单"
      icon={<Globe size={14} />}
      className="mb-3"
    >
      <p className="notice mb-2">
        出口 IP 归属的国家不在这份名单里，门禁<strong>一律判不合格</strong>—— 跟
        IP 不在白名单一个待遇：看门狗收进程，会话内门禁拦请求， 「加入当前
        IP」也会被拒。
      </p>

      {list.length === 0 ? (
        <p className="notice notice--warn">
          <strong>名单为空 = 这一层没有启用。</strong>
          空名单不等于全拒 —— 那会在你还没来得及配置时就把你关在门外，
          跟「白名单为空时不上锁」是同一条道理。要启用就往下面加国家。
        </p>
      ) : (
        <div className="flex flex-wrap gap-1">
          {list.map((c) => (
            <Button
              key={c}
              size="sm"
              variant="ghost"
              disabled={busy}
              title={`从名单移除 ${c}`}
              onClick={() => void save(list.filter((x) => x !== c))}
            >
              {c} ✕
            </Button>
          ))}
        </div>
      )}

      <Row className="mt-2">
        <input
          className="input w-24 font-mono"
          aria-label="两位国家代码"
          placeholder="US"
          maxLength={2}
          value={draft}
          disabled={busy}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
        />
        <Button
          size="sm"
          disabled={busy || draft.trim().length !== 2}
          onClick={add}
        >
          加入
        </Button>
        {presets.map(([name, codes]) => (
          <Button
            key={name}
            size="sm"
            variant="ghost"
            disabled={busy}
            onClick={() => void save(codes)}
          >
            {name}
          </Button>
        ))}
        {list.length > 0 && (
          <Button
            size="sm"
            variant="ghost"
            disabled={busy}
            onClick={() => void save([])}
          >
            清空（关掉这一层）
          </Button>
        )}
      </Row>

      {action.error && (
        <p className="notice notice--danger mt-2">{action.error}</p>
      )}

      <Collapsible className="mt-1" summary="启用之前先知道这两件事">
        <p className="notice">
          <strong>GeoIP 不准会误杀。</strong>
          面板问三家（ippure / Cloudflare / ipinfo），三家说的不一样时
          <strong>按最严的算，判不合格</strong>，冲突原文会写进日志。
          这是「宁可错杀不可放过」的直接后果 —— 正在跑的会话可能因为 一次 GeoIP
          打架被收掉。日志里那一行会写清是哪家在胡说。
        </p>
        <p className="notice mt-2">
          <strong>预设不是权威清单。</strong>
          「常用支持地区」只是个省得手打的起手式，面板不知道也不假装知道
          Anthropic 的官方完整清单，你应该自己核对后增删。
          港澳与中国大陆刻意不在预设里。
        </p>
      </Collapsible>
    </Card>
  );
}

// ------------------------------------------------------------------ 执行锁

type LockDialog = null | "unlock" | "clean";

/**
 * 运行日志。0.20.0 从总览挪进这里 ——
 * 它是总览上最长的一块，而那一页要回答的是「现在什么状态」；
 * 日志是出了事才去翻的东西，翻它的人本来就已经在门禁这一档里了。
 */
export function LockPanel() {
  const { gate, busy, act } = useGateAction();
  const [dialog, setDialog] = useState<LockDialog>(null);
  const st = gate.data;
  const emptyAllowlist = !!st && st.allowlist.length === 0;

  return (
    <>
      <Card as="h3" title="受管可执行文件" className="mb-3">
        {/* 这段说明原来只写在项目档案里（§7.22），软件里一个字都没有。
            结果是使用者反复撞见同一个现象却完全无从联想 —— 他只是关了个窗口、
            或者点了一次一键关闭，半小时后桌面端开不了新会话。
            机制要写在看得见的地方。 */}
        <p className="notice notice--warn mb-2">
          <strong>
            「版本化副本」就是 Claude 桌面端 Code 页开新会话时要拉起的那个。
          </strong>
          上锁<strong>不会</strong>
          影响已经在跑的会话（Windows 不会卸掉已加载的映像）， 挡住的是
          <strong>下一次启动</strong>—— 症状就是桌面端弹{" "}
          <code>Claude Code couldn&apos;t start</code>。 从 v0.7.0
          起，面板自己触发的重锁（升级、装包、一键关闭、切账户、面板重启）
          会在出口 IP 仍然合格时<strong>把租约还给你</strong>
          ，不再需要手动补一次。
        </p>
        {st?.targets.length ? (
          st.targets.map((t) => (
            <Row
              key={t.path}
              side={
                <>
                  <Pill tone="default" title={TARGET_KIND_HINT[t.kind]}>
                    {TARGET_KIND_LABEL[t.kind] ?? t.kind}
                  </Pill>
                  {t.locked ? (
                    <Pill tone="ok">已锁</Pill>
                  ) : (
                    <Pill tone="warn">未锁</Pill>
                  )}
                </>
              }
            >
              <span className="w-full break-all font-mono text-xs">
                {t.path}
              </span>
            </Row>
          ))
        ) : (
          <EmptyState title="没有找到可锁的 Claude 可执行文件">
            本机可能还没装 Claude，或者装在了面板不认识的位置。
          </EmptyState>
        )}

        <div className="mt-3 flex flex-wrap gap-2">
          <Button
            icon={<Lock size={13} />}
            loading={busy === "lock"}
            disabled={!!busy || emptyAllowlist}
            title={
              emptyAllowlist
                ? "白名单为空时不许上锁 —— 会把你自己关在门外"
                : undefined
            }
            onClick={() => void act("lock", api.gateLockAll, "已重新上锁")}
          >
            立即全部上锁
          </Button>
          <Button
            variant="danger"
            icon={<Unlock size={13} />}
            disabled={!!busy}
            onClick={() => setDialog("unlock")}
          >
            应急解锁
          </Button>
        </div>

        {/* 白名单为空时**直接拦住**上锁按钮，不能只靠 Rust 那边兜底。 */}
        {emptyAllowlist && (
          <p className="notice notice--warn mt-2">
            白名单为空，上锁按钮已停用 —— 锁上之后没有任何 IP 过得了门禁，
            解锁入口也会永远打不开。先加一条白名单。
          </p>
        )}

        <Collapsible className="mt-2" summary="为什么有一个不带锁的副本？">
          <p className="notice">
            <code>AnthropicClaude\app-&lt;版本&gt;\claude.exe</code>{" "}
            不在上面的列表里。 给它加 Deny ACE 会让桌面端开新窗口就崩，
            所以那一份只能靠看门狗 taskkill 收。
          </p>
          <p className="notice mt-2">
            这是<strong>已知的残留缺口</strong>
            ：刻意进那个目录双击能绕开启动门禁， 下一次巡检会尝试关闭（15
            秒间隔，加上检测与关停耗时）—— 前提是当时有看门狗在跑。
            要彻底堵死需要 AppLocker 或 WDAC， 不在本项目范围内。
          </p>
        </Collapsible>

        <Collapsible summary="应急解锁为什么不验 IP？">
          <p className="notice">
            白名单为空、查不到公网 IP、IP 填错了 —— 这几种情况都会让正常入口
            永远过不了，那时 claude.exe 锁着而你打不开它。所以留一个不验 IP 的
            逃生口。
          </p>
          <p className="notice mt-2">
            安全上不吃亏：能点这个按钮的人本来就能改白名单文件、也能自己改 ACL。
            门禁防的是「跑起来之后出口 IP 悄悄变了」，
            <strong>不是防本机管理员</strong>。
          </p>
        </Collapsible>
      </Card>

      {!!st?.stale_copies.length && (
        <Card as="h3" title="升级残留副本" tone="danger" className="mb-3">
          <p className="notice notice--danger">
            这些副本<strong>没有 Deny ACL</strong>，是可以绕过门禁的执行副本。
            如果某一份正被运行中的会话占着会删不掉 —— 那是正常的，
            关掉对应进程后再清一次。
          </p>
          <div className="mt-2">
            {st.stale_copies.map((p) => (
              <Bullet key={p} tone="danger">
                <span className="break-all font-mono text-xs">{p}</span>
              </Bullet>
            ))}
          </div>
          <Button
            variant="danger"
            className="mt-3"
            icon={<Trash2 size={13} />}
            disabled={!!busy}
            onClick={() => setDialog("clean")}
          >
            清理残留副本
          </Button>
        </Card>
      )}

      <Card as="h3" title="看门狗" className="mb-3">
        {/* 这两条是说明，不是数据 —— 旧界面把它们做成了跟真实数据行一模一样的
            两栏行，看起来像是在报告当前配置。 */}
        <div className="rounded-[var(--radius-md)] bg-[var(--surface-2)] px-3 py-2">
          <Bullet marker="·">
            <strong>Claude Code / 桥接档</strong>：每 5 秒查一次。查不到 IP
            时立即上锁并关闭受管进程，不提供宽限。
          </Bullet>
          <Bullet marker="·">
            <strong>Claude 桌面端档</strong>：每 5 秒查一次。查不到 IP
            <strong>立即关闭，不给宽限</strong>。
          </Bullet>
        </div>

        <Collapsible className="mt-2" summary="桌面端为什么不给宽限？">
          <p className="notice">
            它冻不住 —— 真正在跑的是 <code>app-*</code> 下那个不能加 Deny
            的副本，
            而且早就把自己加载进内存了。对它来说「等等看」的实际含义就是
            「让它在无法核实的网络上继续跑」。
          </p>
          <p className="notice mt-2">
            代价：VPN 重连或 IP 查询服务限流会直接关掉正在用的桌面端，
            丢掉没保存的对话。这是刻意的取舍，不是缺陷。
          </p>
        </Collapsible>

        <div className="mt-3 flex flex-wrap gap-2">
          <Button
            variant="primary"
            loading={busy === "wd-start"}
            disabled={!!busy || st?.watchdog_running}
            onClick={() =>
              void act(
                "wd-start",
                () => api.watchdogStart("Cli"),
                "看门狗已启动（CLI 档）",
              )
            }
          >
            启动看门狗（CLI 档）
          </Button>
          <Button
            loading={busy === "wd-stop"}
            disabled={!!busy || !st?.watchdog_running}
            onClick={() =>
              void act("wd-stop", api.watchdogStop, "看门狗已停止")
            }
          >
            停止看门狗
          </Button>
        </div>
      </Card>

      <Card
        as="h3"
        title="ip-gate.log"
        actions={
          <Button
            size="sm"
            icon={<RotateCw size={12} />}
            loading={gate.loading}
            onClick={() => void gate.refresh()}
          >
            刷新
          </Button>
        }
      >
        <LogView lines={st?.recent_log ?? []} />
        <p className="notice mt-2">
          只记时间、公网 IP 与上锁解锁动作，<strong>不含任何对话内容</strong>。
        </p>
      </Card>

      <ConfirmDialog
        open={dialog === "unlock"}
        onCancel={() => setDialog(null)}
        onConfirm={() =>
          void act("unlock", api.gateUnlockAll, "已摘掉全部执行锁").then(() =>
            setDialog(null),
          )
        }
        title="应急解锁？"
        confirmLabel="确认解锁"
        confirmWord="解锁"
        loading={busy === "unlock"}
        danger
      >
        <p>
          这会<strong>摘掉全部 Deny ACE</strong>，而且不验证出口 IP。
          之后任何人都能直接双击 exe 把 Claude 跑起来， IP
          门禁在你重新上锁之前形同虚设。
        </p>
        <p className="mt-2">
          只在被自己关在门外的时候用它：白名单为空、查不到公网 IP、或者 IP
          填错了。
        </p>
        <p className="notice mt-2">
          解锁后记得处理完就回来点「立即全部上锁」。
        </p>
      </ConfirmDialog>

      <ConfirmDialog
        open={dialog === "clean"}
        onCancel={() => setDialog(null)}
        onConfirm={() =>
          void act("clean", api.gateCleanStale, "已清理残留副本").then(() =>
            setDialog(null),
          )
        }
        title="删除这些残留副本？"
        confirmLabel="确认删除"
        loading={busy === "clean"}
        danger
      >
        <p>
          下面这些文件会被<strong>永久删除</strong>：
        </p>
        <div className="mt-2">
          {st?.stale_copies.map((p) => (
            <Bullet key={p} tone="danger">
              <span className="break-all font-mono text-xs">{p}</span>
            </Bullet>
          ))}
        </div>
        <p className="notice mt-3">
          正被运行中的会话占着的那份会删不掉，那是正常的 —— Windows 允许改名
          正在运行的 exe，不允许删除它。关掉对应进程后再清一次即可。
        </p>
      </ConfirmDialog>

      {/* 运行日志。0.20.0 从总览挪到这里 —— 见本函数上面那段说明。 */}
      <LogBand />
    </>
  );
}

/**
 * 门禁范围 —— GPT（Codex）算不算。
 *
 * 0.25.0 起**默认算**（使用者定的：「默认定死被 IP 锁接管」），存的是反义的
 * `codex_outside_gate`，所以复选框显示的是它的反面。放在「IP 锁」弹窗里
 * （评分栏那张 IP 锁卡点开的就是这一个）——使用者要的位置；以前在「执行锁」
 * 弹窗和一个已经没有路由的旧页面里各一份。
 *
 * 归门禁的含义跟 Claude 一样：codex.exe 加 Deny ACE、GPT 桌面端起之前验 IP、
 * 起来之后持租约、看门狗判不过就收。
 */
function GateRange() {
  const settings = useResource("settings", R.settings);
  const action = useAction();
  // 待确认的调整：改的是哪个开关、改成什么。两个开关一个弹窗，文案按 which 分。
  const [next, setNext] = useState<{
    which: "codex" | "antigravity";
    on: boolean;
  } | null>(null);
  const underGate = !(settings.data?.codex_outside_gate ?? false);
  const agUnderGate = !(settings.data?.antigravity_outside_gate ?? false);
  return (
    <Card as="h3" title="门禁范围" className="mb-3">
      <label className="qb-setting-row">
        <span>
          <strong>GPT（Codex）也归门禁管</strong>
          <small>
            默认开启，与 Claude 共用同一套门禁：启动前验出口
            IP、看门狗巡检、codex.exe 上执行锁。关闭正在运行的 GPT
            桌面端后可调整。
          </small>
        </span>
        <input
          type="checkbox"
          checked={underGate}
          disabled={!settings.data || !!action.pending}
          onChange={(e) => setNext({ which: "codex", on: e.target.checked })}
        />
      </label>
      <label className="qb-setting-row">
        <span>
          <strong>反重力（Hub + IDE）也归门禁管</strong>
          <small>
            默认开启（0.26.0）。两个产品共用这一个开关：主程序、语言服务器、第三方汉化壳留下的
            original 一起上执行锁；起之前验出口
            IP，看门狗按桌面档盯。关闭正在运行的反重力后可调整。
          </small>
        </span>
        <input
          type="checkbox"
          checked={agUnderGate}
          disabled={!settings.data || !!action.pending}
          onChange={(e) =>
            setNext({ which: "antigravity", on: e.target.checked })
          }
        />
      </label>
      <Modal
        open={next !== null}
        onClose={() => setNext(null)}
        title="调整门禁范围"
        footer={
          <Button
            loading={!!action.pending}
            onClick={() => {
              if (settings.data && next !== null)
                void action
                  .run(
                    "gate-range",
                    () =>
                      api.settingsSave(
                        next.which === "codex"
                          ? { ...settings.data!, codex_outside_gate: !next.on }
                          : {
                              ...settings.data!,
                              antigravity_outside_gate: !next.on,
                            },
                      ),
                    "门禁范围已更新",
                  )
                  .then((s) => {
                    if (s) setNext(null);
                  });
            }}
          >
            确认调整
          </Button>
        }
      >
        <p>
          {next?.which === "antigravity"
            ? next.on
              ? "之后反重力 Hub / IDE 起之前先验出口 IP，起来之后归看门狗管，检查未通过会被关闭；它们的主程序与语言服务器一并上执行锁。"
              : "之后反重力独立启动：不验 IP、不受看门狗管，并解除它们身上已知的执行锁。Claude 与 GPT 那两侧不受影响。"
            : next?.on
              ? "之后 GPT 桌面端起之前先验出口 IP，起来之后归看门狗管，检查未通过会被关闭；codex.exe 一并上执行锁。"
              : "之后 GPT 桌面端独立启动：不验 IP、不受看门狗管，并解除已知 codex.exe 上的执行锁。Claude 那一侧不受影响。"}
        </p>
        {action.error && (
          <p className="notice notice--danger">{action.error}</p>
        )}
      </Modal>
    </Card>
  );
}

// -------------------------------------------------------------- 会话内门禁

/**
 * 会话内门禁 —— 执行锁与看门狗之间那一档。
 *
 * 锁只管启动，会话跑起来之后就管不着了；看门狗手里只有 taskkill 一招，
 * 一杀就丢上下文。这个 hook 在下一次请求前温和拦下，进程留着。
 *
 * 界面上必须说清两件事，因为它们都会让人「本来能用的东西突然不能用」：
 * 面板没跑 = 一律拦；以及怎么自救。
 */
export function SessionGatePanel() {
  const toast = useToast();
  const hook = useResource("hook", R.hook);
  const gate = useResource("gate", R.gate);
  const [busy, setBusy] = useState(false);
  const h = hook.data;
  const emptyAllowlist = !!gate.data && gate.data.allowlist.length === 0;

  async function toggle() {
    setBusy(true);
    try {
      if (h?.installed) {
        await api.hookUninstall();
        toast.ok("会话内门禁已停用");
      } else {
        await api.hookInstall();
        toast.ok("会话内门禁已启用 —— 之后每次请求都会先验一遍门禁");
      }
      await hook.refresh();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card
      as="h3"
      title="会话内门禁"
      className="mb-3"
      actions={
        <Button
          size="sm"
          variant={h?.installed ? "ghost" : "primary"}
          loading={busy}
          disabled={!h?.installed && emptyAllowlist}
          title={
            !h?.installed && emptyAllowlist
              ? "白名单为空时不许启用 —— 每一次请求都会被拦"
              : undefined
          }
          onClick={() => void toggle()}
        >
          {h?.installed ? "停用" : "启用"}
        </Button>
      }
    >
      <p className="sub">
        执行锁只管<strong>启动</strong>。会话一旦跑起来，锁就管不着它了 ——
        进程已经在内存里，换了网照样能发请求，看门狗只能整个杀掉。 这一档在
        <strong>下一次请求发出前</strong>拦下，进程留着，上下文不丢。
      </p>

      <Row className="mt-2">
        <span>状态</span>
        {h?.installed ? <Pill tone="ok">已启用</Pill> : <Pill>未启用</Pill>}
        {h?.slot ? (
          <span className="sub">装在槽位 {h.slot}</span>
        ) : (
          h?.installed && (
            <span className="sub">没有激活槽位，装在 ~\.claude</span>
          )
        )}
      </Row>

      {h?.installed && (
        <p className="notice notice--warn mt-2">
          <strong>这是严格档（fail-closed）：判不过就拦，查不到也拦。</strong>
          <br />
          包括「面板没在跑」—— 裁决超过 90 秒没刷新就算不新鲜，一律拦。
          被拦时命令行里会印出自救步骤；面板打不开时，手动删掉
          {h.settings_path ? (
            <code className="mx-1">{h.settings_path}</code>
          ) : (
            " settings.json "
          )}
          里带 <code>_qb_gate</code> 的两段即可解除。
        </p>
      )}

      {!!h?.recent_blocks.length && (
        <Collapsible
          summary={`最近拦截 ${h.recent_blocks.length} 次`}
          className="mt-2"
        >
          <LogView lines={h.recent_blocks} />
        </Collapsible>
      )}

      {/* 中转会话**不**受这一档管，这件事必须写出来 —— 否则使用者会以为
          所有会话都验过了。规矩本身在 CLAUDE.md 里：会话内 hook 不写进中转
          环境目录，而且 `set_hook` 会主动摘掉旧版本留在那里的那一份。 */}
      <p className="notice mt-2">
        这一档<strong>只管官方会话</strong>。中转会话打的是第三方端点、
        用你自己买的 Key、不带官方 OAuth 身份，Anthropic 那边看不见它，
        所以门禁判不过时也不会收掉它 —— 收它换不到任何保护，
        却会把正在写的对话弄丢。
      </p>
    </Card>
  );
}
