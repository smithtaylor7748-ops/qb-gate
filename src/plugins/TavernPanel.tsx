/**
 * 酒馆详情页。
 *
 * # 这一页要回答的只有一个问题：现在能不能起，不能的话缺什么
 *
 * 改版之前它回答不了。三个路径默认留空（对的，见
 * `TavernConfig::default`），而空路径撞上的是 `路径不存在: bridge.py` ——
 * 空目录拼出来的相对文件名。页面上同时还有一句「依赖不齐，展开看缺哪一项」，
 * 指着一个**根本没有渲染出来的** `checks` 列表。于是使用者能看到的全部信息是
 * 一句看不懂的话和一个只会失败的按钮。
 *
 * 现在：状态条一句话说清在哪一档 → 没配就直接给「自动定位」→
 * 候选按证据排好让人点「采用」→ 保存 → 启动。
 *
 * # 文案克制
 *
 * 这一页不写功能介绍。原来每张卡顶上都有一段解释「面板为什么只做盘点不做编辑」
 * 「备份为什么是目录复制不是打包」—— 那些是设计说明，属于 DESIGN-NOTES，
 * 不属于一个要用来干活的界面。留下的只有**当下能改变使用者下一步动作**的句子。
 */

import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Archive,
  ExternalLink as LinkIcon,
  FolderTree,
  History,
  Play,
  Radar,
  RotateCw,
  Settings2,
  Square,
} from "lucide-react";

import {
  api,
  type TavernCandidate,
  type TavernConfig,
  type TavernEvidence,
  type TavernSurvey,
} from "../lib/api";
import { AFTER, R } from "../lib/resources";
import { invalidate, useResource } from "../lib/store";
import { endTask, resetTask, useTask } from "../lib/tasks";
import {
  Button,
  Card,
  Collapsible,
  ConfirmDialog,
  EmptyState,
  PathField,
  Pill,
  PortField,
  ProgressBar,
  Row,
  useToast,
  fmtSize,
} from "../ui";

/** 证据档位 → 一个词。顺序即可信度，界面上不许把它们合成「找到了」。 */
const EVIDENCE: Record<
  TavernEvidence,
  { label: string; tone: "ok" | "accent" | "default" }
> = {
  running: { label: "正在运行", tone: "ok" },
  pidfile: { label: "上次运行", tone: "accent" },
  configured: { label: "当前配置", tone: "accent" },
  scan: { label: "扫描命中", tone: "default" },
};

type Slot = "bridge_root" | "sillytavern_root" | "st_launcher";

const GROUPS: {
  slot: Slot;
  key: keyof TavernSurvey & string;
  label: string;
}[] = [
  { slot: "bridge_root", key: "bridge", label: "桥接项目目录" },
  { slot: "sillytavern_root", key: "sillytavern", label: "SillyTavern 目录" },
  { slot: "st_launcher", key: "launcher", label: "酒馆启动脚本" },
];

export default function TavernPanel() {
  const toast = useToast();
  const cfgRes = useResource("tavernConfig", R.tavernConfig);
  const assets = useResource("tavernAssets", R.tavernAssets);
  const backups = useResource("tavernBackups", R.tavernBackups);
  const plugins = useResource("plugins", R.plugins);
  const task = useTask("tavern-start");

  const [busy, setBusy] = useState("");
  const [draft, setDraft] = useState<TavernConfig | null>(null);
  const [openCat, setOpenCat] = useState<string | null>(null);
  const [restoreId, setRestoreId] = useState<string | null>(null);
  const [survey, setSurvey] = useState<TavernSurvey | null>(null);

  const cfg = draft ?? cfgRes.data ?? null;
  const status = plugins.data?.[0];
  const running = status?.state === "running";
  const broken = status?.state === "broken";
  /**
   * 三个路径还空着。判据取**已落盘的那份**（`cfgRes.data`），不看 `draft` ——
   * 用草稿的话，人刚在框里敲下第一个字符，「还没配」的提示就没了。
   */
  const saved = cfgRes.data;
  const unconfigured =
    !!saved &&
    [saved.bridge_root, saved.sillytavern_root, saved.st_launcher].some(
      (p) => !p,
    );
  /** 填了但依赖检查没过 = 路径指错了或东西被挪走了，跟「还没配」是两回事。 */
  const misconfigured = !!saved && !unconfigured && status?.state === "missing";
  const needsSetup = unconfigured || misconfigured;
  const dirty = !!draft;

  // ------------------------------------------------------------ 动作

  async function start() {
    setBusy("start");
    resetTask("tavern-start");
    try {
      const url = await api.pluginStart();
      endTask("tavern-start");
      invalidate(...AFTER.tavern);
      // 无论新起还是复用现有服务都要打开页面 —— 少了这步，
      // 成功的启动和崩溃看起来一模一样。反过来也一样：走到这里酒馆已经在跑、
      // 租约也拿着，页面没打开不许报成启动失败。
      try {
        if (url.startsWith("http")) await openUrl(url);
        toast.ok("酒馆已就绪，已打开页面");
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        toast.error(
          `酒馆已就绪，但页面没打开：${msg}。可以在浏览器里手动打开 ${url}`,
        );
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      endTask("tavern-start", msg);
      toast.error(msg);
    } finally {
      setBusy("");
    }
  }

  async function locate(deep: boolean) {
    setBusy(deep ? "deep" : "locate");
    try {
      const s = await api.tavernLocate(deep);
      setSurvey(s);
      const hits = s.bridge.length + s.sillytavern.length + s.launcher.length;
      if (hits === 0) {
        toast.error(
          s.truncated
            ? `扫了 ${s.scanned_dirs} 个目录还没扫完，也没找到。可以试深扫，或自己填路径。`
            : "本机没找到酒馆和桥接。装在别处的话请自己填路径。",
        );
      } else {
        toast.ok(`找到 ${hits} 条候选`);
      }
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy("");
    }
  }

  /** 采用一条候选：只写进草稿，**保存仍然要人自己点**。 */
  function adopt(slot: Slot, path: string) {
    if (!cfg) return;
    setDraft({ ...cfg, [slot]: path });
  }

  function adoptBest() {
    if (!cfg || !survey) return;
    const next = { ...cfg };
    for (const g of GROUPS) {
      const top = (survey[g.key] as TavernCandidate[])[0];
      if (top) next[g.slot] = top.path;
    }
    setDraft(next);
  }

  async function act(name: string, fn: () => Promise<unknown>, ok: string) {
    setBusy(name);
    try {
      await fn();
      toast.ok(ok);
      invalidate(
        ...AFTER.tavern,
        "tavernAssets",
        "tavernBackups",
        "tavernConfig",
      );
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy("");
      setRestoreId(null);
    }
  }

  // ------------------------------------------------------------ 状态条

  const tone = running ? "ok" : needsSetup || broken ? "warn" : "accent";
  const headline = running
    ? "运行中"
    : unconfigured
      ? "还没配路径"
      : misconfigured
        ? "路径或依赖有问题"
        : broken
          ? "只起来了一半"
          : "就绪";

  return (
    <>
      <div className={`tv-status tv-status--${tone}`}>
        <span
          className={`tv-dot tv-dot--${running ? "ok" : needsSetup || broken ? "warn" : "ok"}`}
        />
        <span className="tv-status-text">
          <strong>{headline}</strong>
          <span>{status?.detail ?? "读取中…"}</span>
        </span>
        <span className="tv-status-actions">
          {needsSetup ? (
            <Button
              variant="primary"
              icon={<Radar size={13} />}
              loading={busy === "locate"}
              disabled={!!busy}
              onClick={() => locate(false)}
            >
              自动定位
            </Button>
          ) : (
            <Button
              variant="primary"
              icon={<Play size={13} />}
              loading={busy === "start"}
              disabled={!!busy}
              onClick={start}
            >
              {running ? "打开页面" : "启动酒馆"}
            </Button>
          )}
          <Button
            variant="danger"
            icon={<Square size={13} />}
            loading={busy === "stop"}
            disabled={!!busy || !running}
            onClick={() =>
              act("stop", api.pluginStop, "酒馆与桥接已停止，租约已收回")
            }
          >
            停止
          </Button>
        </span>
      </div>

      {(task.running || task.error) && (
        <div className="mt-2">
          <div className="mb-1.5 flex items-center gap-2">
            <span className="text-sm">{task.phase || "启动中…"}</span>
            {task.total > 0 && (
              <span className="notice ml-auto">
                {task.step} / {task.total}
              </span>
            )}
          </div>
          <ProgressBar
            value={task.total > 0 ? (task.step / task.total) * 100 : undefined}
            tone={task.error ? "danger" : "accent"}
            label="酒馆启动进度"
          />
          {task.error && (
            <p className="notice notice--danger mt-1.5">{task.error}</p>
          )}
        </div>
      )}

      {/* ------------------------------------------------------ 定位 */}
      {(needsSetup || survey) && (
        <Card
          title="定位"
          icon={<Radar size={14} />}
          as="h3"
          className="mt-3"
          actions={
            <div className="flex gap-2">
              <Button
                size="sm"
                loading={busy === "locate"}
                disabled={!!busy}
                onClick={() => locate(false)}
              >
                自动定位
              </Button>
              <Button
                size="sm"
                variant="ghost"
                loading={busy === "deep"}
                disabled={!!busy}
                onClick={() => locate(true)}
              >
                深扫
              </Button>
            </div>
          }
        >
          {survey ? (
            <>
              {GROUPS.map((g) => {
                const list = survey[g.key] as TavernCandidate[];
                return (
                  <div className="tv-group" key={g.slot}>
                    <div className="tv-group-head">{g.label}</div>
                    {list.length ? (
                      list.map((c) => (
                        <div
                          key={c.path}
                          className={`tv-cand${cfg?.[g.slot] === c.path ? " tv-cand--picked" : ""}`}
                        >
                          <Pill tone={EVIDENCE[c.evidence].tone}>
                            {EVIDENCE[c.evidence].label}
                          </Pill>
                          <span className="tv-cand-main">
                            <span className="tv-cand-path">{c.path}</span>
                            <span className="tv-cand-note">{c.note}</span>
                          </span>
                          <Button
                            size="sm"
                            disabled={cfg?.[g.slot] === c.path}
                            onClick={() => adopt(g.slot, c.path)}
                          >
                            {cfg?.[g.slot] === c.path ? "已选" : "采用"}
                          </Button>
                        </div>
                      ))
                    ) : (
                      <p className="notice">没找到。</p>
                    )}
                  </div>
                );
              })}
              <div className="mt-3 flex flex-wrap items-center gap-2">
                <Button
                  variant="primary"
                  disabled={!!busy || !survey.bridge.length}
                  onClick={adoptBest}
                >
                  采用每项第一条
                </Button>
                {dirty && (
                  <Button
                    variant="primary"
                    loading={busy === "cfg"}
                    disabled={!!busy}
                    onClick={() =>
                      act(
                        "cfg",
                        () => api.tavernConfigSave(cfg!),
                        "已保存",
                      ).then(() => setDraft(null))
                    }
                  >
                    保存
                  </Button>
                )}
                <span className="notice ml-auto">
                  扫了 {survey.scanned_dirs} 个目录
                  {survey.truncated ? " · 没扫完，结果可能不全" : ""}
                </span>
              </div>
            </>
          ) : (
            <p className="notice">
              面板不分发 SillyTavern 和 bridge.py，用的是你机器上那一份。
              点「自动定位」让它找，或在下面自己填。
            </p>
          )}
        </Card>
      )}

      {/* ------------------------------------------------------ 路径 */}
      <Collapsible
        className="mt-3"
        key={needsSetup ? "setup" : "configured"}
        defaultOpen={needsSetup}
        summary={
          <span className="flex items-center gap-1.5">
            <Settings2 size={13} />
            路径与端口
            {dirty && <Pill tone="warn">未保存</Pill>}
          </span>
        }
      >
        {cfg ? (
          <>
            <div className="flex flex-col gap-3">
              <PathField
                label="桥接项目目录"
                value={cfg.bridge_root}
                onChange={(v) => setDraft({ ...cfg, bridge_root: v })}
              />
              <PathField
                label="SillyTavern 目录"
                value={cfg.sillytavern_root}
                onChange={(v) => setDraft({ ...cfg, sillytavern_root: v })}
              />
              <PathField
                label="酒馆启动脚本"
                kind="file"
                value={cfg.st_launcher}
                onChange={(v) => setDraft({ ...cfg, st_launcher: v })}
              />
              <div className="grid gap-3 sm:grid-cols-2">
                <PortField
                  label="桥接端口"
                  value={cfg.bridge_port}
                  onChange={(v) => setDraft({ ...cfg, bridge_port: v })}
                />
                <PortField
                  label="酒馆端口"
                  value={cfg.st_port}
                  onChange={(v) => setDraft({ ...cfg, st_port: v })}
                />
              </div>
            </div>
            <div className="mt-3 flex gap-2">
              <Button
                variant="primary"
                loading={busy === "cfg"}
                disabled={!dirty || !!busy}
                onClick={() =>
                  act("cfg", () => api.tavernConfigSave(cfg), "已保存").then(
                    () => setDraft(null),
                  )
                }
              >
                保存
              </Button>
              <Button disabled={!dirty} onClick={() => setDraft(null)}>
                放弃改动
              </Button>
              {!needsSetup && (
                <Button
                  variant="ghost"
                  icon={<Radar size={12} />}
                  loading={busy === "locate"}
                  disabled={!!busy}
                  onClick={() => locate(false)}
                >
                  重新定位
                </Button>
              )}
            </div>
          </>
        ) : (
          <p className="notice">读取配置中…</p>
        )}
      </Collapsible>

      {/* ------------------------------------------------------ 依赖 */}
      {/* Rust 侧 `status()` 逐项算好了 `checks`，改版之前**没有任何一处
          把它渲染出来** —— detail 里那句「展开看缺哪一项」指着一个不存在的
          地方。算了不显示等于没算。 */}
      {!!status?.checks.length && (
        <Collapsible
          className="mt-3"
          key={status.checks.some((c) => !c.ok) ? "bad" : "ok"}
          defaultOpen={status.checks.some((c) => !c.ok)}
          summary={
            <span className="flex items-center gap-1.5">
              依赖
              <Pill tone={status.checks.some((c) => !c.ok) ? "warn" : "ok"}>
                {status.checks.filter((c) => !c.ok).length || "全部就绪"}
                {status.checks.some((c) => !c.ok) ? " 项缺失" : ""}
              </Pill>
            </span>
          }
        >
          {status.checks.map((c) => (
            <Row
              key={c.label}
              side={
                <Pill tone={c.ok ? "ok" : "warn"}>{c.ok ? "有" : "缺"}</Pill>
              }
            >
              <span className="min-w-0">
                <strong>{c.label}</strong>
                {/* 路径完整显示，不截断 —— 这一行的用处就是让人看出
                    自己填的到底是哪个目录。 */}
                <p className="notice break-all">{c.detail}</p>
              </span>
            </Row>
          ))}
        </Collapsible>
      )}

      {/* ------------------------------------------------------ 资产 */}
      <Card
        title="资产"
        icon={<FolderTree size={14} />}
        as="h3"
        className="mt-3"
        actions={
          <Button
            size="sm"
            icon={<RotateCw size={12} />}
            onClick={() => void assets.refresh()}
          >
            刷新
          </Button>
        }
      >
        {assets.data?.length ? (
          <>
            <div className="tv-cats">
              {assets.data.map((c) => (
                <button
                  type="button"
                  key={c.id}
                  className={`tv-cat${c.items.length ? "" : " tv-cat--empty"}`}
                  aria-expanded={openCat === c.id}
                  onClick={() => setOpenCat(openCat === c.id ? null : c.id)}
                >
                  <span>{c.label}</span>
                  <span className="tv-cat-n">
                    {c.exists ? c.items.length : "—"}
                  </span>
                </button>
              ))}
            </div>
            {assets.data
              .filter((c) => c.id === openCat)
              .map((c) =>
                c.items.length ? (
                  <div className="tv-files" key={c.id}>
                    {c.items.map((it) => (
                      <Row
                        key={it.path}
                        side={
                          <span className="notice">{fmtSize(it.size)}</span>
                        }
                      >
                        <span className="break-all">
                          {it.is_dir ? "📁 " : ""}
                          {it.name}
                        </span>
                        <span className="notice">{it.modified ?? ""}</span>
                      </Row>
                    ))}
                  </div>
                ) : (
                  <p className="notice mt-2" key={c.id}>
                    {c.exists ? "这一类还是空的。" : `目录不存在：${c.dir}`}
                  </p>
                ),
              )}
          </>
        ) : (
          <EmptyState title="还没盘点到资产">
            SillyTavern 目录填对了再点刷新。
          </EmptyState>
        )}
      </Card>

      {/* ------------------------------------------------------ 备份 */}
      <Card
        title="备份"
        icon={<Archive size={14} />}
        as="h3"
        className="mt-3"
        actions={
          <Button
            size="sm"
            variant="primary"
            loading={busy === "backup"}
            disabled={!!busy}
            onClick={() => act("backup", api.tavernBackup, "已备份全部资产")}
          >
            立即备份
          </Button>
        }
      >
        {backups.data?.length ? (
          backups.data.map((b) => (
            <Row
              key={b.id}
              side={
                <Button
                  size="sm"
                  icon={<History size={12} />}
                  disabled={!!busy}
                  onClick={() => setRestoreId(b.id)}
                >
                  恢复
                </Button>
              }
            >
              <span className="font-mono">{b.id}</span>
              <span className="notice">
                {b.created} · {fmtSize(b.size)}
              </span>
            </Row>
          ))
        ) : (
          <EmptyState icon={<Archive size={22} />} title="还没有备份">
            备份是目录复制，不打包。
          </EmptyState>
        )}
      </Card>

      <div className="mt-3 flex items-center gap-3">
        <Button
          size="sm"
          variant="ghost"
          icon={<LinkIcon size={12} />}
          onClick={() => openUrl(`http://127.0.0.1:${cfg?.st_port ?? 8000}`)}
        >
          直接打开酒馆页面
        </Button>
        <span className="tv-ports">
          桥接 {cfg?.bridge_port ?? 5001} · 酒馆 {cfg?.st_port ?? 8000}
        </span>
      </div>

      <ConfirmDialog
        open={restoreId !== null}
        onCancel={() => setRestoreId(null)}
        onConfirm={() =>
          restoreId &&
          act(
            "restore",
            () => api.tavernRestore(restoreId),
            `已恢复到 ${restoreId}`,
          )
        }
        title="恢复这份备份？"
        confirmLabel="确认恢复"
        confirmWord="恢复"
        loading={busy === "restore"}
        danger
      >
        <p>
          恢复 <code>{restoreId}</code> 会
          <strong>覆盖现有的世界书、角色卡、预设与扩展</strong>。
        </p>
        <p className="notice mt-2">
          恢复前会自动把现状再存一份。正在跑的酒馆不会自动重载，建议先停。
        </p>
      </ConfirmDialog>
    </>
  );
}
