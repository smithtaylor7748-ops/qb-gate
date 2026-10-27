import { useEffect, useState } from "react";
import { NavLink, useParams } from "react-router-dom";
import {
  Archive,
  Plus,
  RefreshCw,
  RotateCcw,
  Save,
  Trash2,
} from "lucide-react";
import {
  api,
  type Settings as Preferences,
  type SnapshotEntry,
  type UpdateStatus,
} from "../lib/api";
import { openUpdateDialog } from "./UpdateDialog";
import { R } from "../lib/resources";
import { useResource, useSession } from "../lib/store";
import { useAction, useWorkspace } from "../lib/workspace";
import { Button, ExternalLink, Modal } from "../ui";
import { ManagedDirControl } from "../pages/managed/ManagedPanel";
import packageInfo from "../../package.json";

export default function SettingsCenter() {
  const { section = "general" } = useParams();
  return (
    <>
      <header className="qb-page-heading">
        <div>
          <h1>设置</h1>
          <p>调整工作空间偏好，管理备份与程序信息。</p>
        </div>
      </header>
      <nav className="qb-tabs" aria-label="设置分类">
        {[
          ["general", "常规"],
          ["backups", "备份与恢复"],
          ["help", "帮助与来源"],
          ["advanced", "高级维护"],
        ].map(([id, name]) => (
          <NavLink
            key={id}
            className={section === id ? "active" : ""}
            to={"/settings/" + id}
          >
            {name}
          </NavLink>
        ))}
      </nav>
      {section === "backups" ? (
        <Backups />
      ) : section === "help" ? (
        <Help />
      ) : section === "advanced" ? (
        // 原来这里还有一张「恢复系统时区」：它只认旧时区卡记下的原值，而启动对齐从来不记，
        // 点了永远提示「已恢复」、其实什么都没动（2026-09-27 删了）。时区的原值在审计日志里。
        <section className="qb-legacy-detail">
          <ManagedDirControl />
        </section>
      ) : (
        <General />
      )}
    </>
  );
}
/** 一行开关：设置里的那个键、标题、底下那行小字。 */
type Toggle = [keyof Preferences, string, string];
function General() {
  const settings = useResource("settings", R.settings);
  const action = useAction();
  const [theme, setTheme] = useSession("theme", "system");
  if (!settings.data) return <p>{settings.error ?? "正在读取设置…"}</p>;
  // 两组开关长得一模一样，只有文案不同 —— 渲染只写一份。
  const row = ([key, title, description]: Toggle) => (
    <label className="qb-setting-row" key={key}>
      <span>
        <strong>{title}</strong>
        <small>{description}</small>
      </span>
      <input
        type="checkbox"
        checked={Boolean(settings.data?.[key])}
        disabled={!!action.pending}
        onChange={(e) =>
          void action.run(
            key,
            () =>
              api.settingsSave({
                ...settings.data!,
                [key]: e.target.checked,
              }),
            "偏好已保存",
          )
        }
      />
    </label>
  );
  return (
    <section className="qb-detail">
      <h2>外观</h2>
      <label className="qb-setting-row">
        <span>
          <strong>颜色主题</strong>
          <small>选择适合当前工作环境的外观</small>
        </span>
        <select value={theme} onChange={(e) => setTheme(e.target.value)}>
          <option value="system">跟随系统</option>
          <option value="light">浅色</option>
          <option value="dark">深色</option>
        </select>
      </label>
      <h2 className="qb-subheading">程序行为</h2>
      {(
        [
          [
            "gate_auto_rearm",
            "网络恢复后重新放行",
            "通过门禁检查后恢复放行，不自动重新启动已停止的会话。",
          ],
          [
            "disable_telemetry",
            "关闭 Claude Code 的非必要遥测",
            "只影响之后启动的 Claude Code 会话。",
          ],
        ] as Toggle[]
      ).map(row)}
      <h2 className="qb-subheading">后台运行</h2>
      <p className="qb-muted">
        门禁、看门狗、本机路由和智能调度都在面板进程里跑：关窗口只是收进托盘，托盘菜单的「退出
        QB Gate」才真的停。
      </p>
      <AutostartRow />
      {(
        [
          [
            "lightweight_on_close",
            "收进托盘时释放界面（轻量模式）",
            "关窗口时把界面整个关掉、只留托盘，省下界面占的内存；再打开要重新加载一两秒，没保存的输入会丢。",
          ],
          [
            "start_router_on_launch",
            "面板启动时拉起本机路由",
            "开机自启或重开面板后，走中转的桌面端和终端不用先去中转站页点「启动」；面板开着就一直占着 15721 端口（cc-switch 的本机代理默认也用它）。下次启动生效。",
          ],
        ] as Toggle[]
      ).map(row)}
      <h2 className="qb-subheading">启动时对齐</h2>
      <p className="qb-muted">
        面板启动时把系统对齐到出口 IP 的归属地；已经一致就不做任何动作。
      </p>
      {(
        [
          [
            "align_timezone_on_start",
            "系统时区",
            "只在与出口 IP 不一致时切换，那一次需要管理员确认。",
          ],
          [
            "align_locale_on_start",
            "区域格式",
            "日期、数字与货币的显示方式，之后启动的程序才读到新值。",
          ],
          [
            "align_display_language_on_start",
            "系统显示语言",
            "需要先装好对应语言包，而且要注销后才生效，因此默认关闭。",
          ],
        ] as Toggle[]
      ).map(row)}
      <h2 className="qb-subheading">软件更新</h2>
      {(
        [
          [
            "update_check_on_start",
            "启动时检查更新",
            "打开面板时问一次 GitHub 有没有新版，有就弹窗提醒；只提醒，不会自己安装。",
          ],
        ] as Toggle[]
      ).map(row)}
      <UpdateLine />
      {/* 原来这句指向「环境与门禁」那一页 —— 那页早就没有了（地址会被送回首页）。 */}
      <p className="qb-help-note">
        IP 白名单、国家规则与会话内门禁在「官方账户」页：点评分栏里的「IP
        锁」或旁边那排门禁读数。安装目录迁移在「高级维护」。
      </p>
      <div className="qb-about">
        <strong>QB Gate {packageInfo.version}</strong>
        <p>本地 AI 客户端工作空间 · AGPL-3.0-only + 第 7 节附加条款</p>
        {/* AGPL 第 5(d) 节：交互界面要显示版权与免责。别只留许可证名字。
            两行分开写：合成一行会被 Prettier 折在中文逗号后面，界面上多出一个空格。
            下面那行署名是 LICENSE-ADDITIONAL-TERMS.md 第 1 条要求修改版保留的那一句，
            原版自己先照做 —— 下游只要不删就合规，措辞别改。 */}
        <p>Copyright (C) 2026 smithtaylor7748-ops</p>
        <p>
          基于 QB Gate，版权所有 (C) 2026 smithtaylor7748-ops。源码：
          https://github.com/smithtaylor7748-ops/qb-gate
        </p>
        <p>
          本软件不提供任何担保；可按 AGPL-3.0-only
          及附加条款再分发：保留上面这行署名、改版须标明、不得使用「QB
          Gate」名称。
        </p>
        {/* 图标由 ExternalLink 自己补，这里不要再加一个 —— 加了就是两个箭头。 */}
        <ExternalLink href="https://github.com/smithtaylor7748-ops/qb-gate/releases">
          查看 GitHub 发布版本
        </ExternalLink>
      </div>
    </section>
  );
}
/**
 * 开机自启那一行（中转站重构阶段 7 · X7）。
 *
 * 勾不勾以注册表为准（`autostart_status` 的 `effective`：登记了、没被任务管理器禁用、指向这一份），
 * 不在设置文件里另存 —— 使用者在任务管理器里关掉它，这里要跟着变成没勾。
 */
function AutostartRow() {
  const status = useResource("autostart", R.autostart);
  const action = useAction();
  const s = status.data;
  const note = !s
    ? (status.error ?? "正在读取…")
    : s.dev_build
      ? "这是开发版，不登记开机自启（登记下去开机起的会是一份编译中间产物）。用装好的面板来开。"
      : s.disabled_by_user
        ? "已登记，但在任务管理器 / 设置 › 启动应用里被禁用了；在这里重新勾上会解除禁用。"
        : s.registered && !s.points_here
          ? `登记的是另一份 QB Gate：${s.command ?? "（读不出命令）"}。在这里勾上会改成这一份。`
          : "登录 Windows 后自动打开面板并收在托盘里。写的是当前用户的启动项「QB Gate」，卸载面板时一起删。";
  const warn = !!s && (s.disabled_by_user || (s.registered && !s.points_here));
  return (
    <label className="qb-setting-row">
      <span>
        <strong>开机自启</strong>
        <small className={warn || status.error ? "notice--warn" : ""}>
          {note}
        </small>
      </span>
      <input
        type="checkbox"
        checked={!!s?.effective}
        disabled={!s || s.dev_build || !!action.pending}
        onChange={(e) => {
          const on = e.target.checked;
          void action.run(
            "autostart",
            () => api.autostartSet(on),
            on ? "已登记开机自启" : "已取消开机自启",
          );
        }}
      />
    </label>
  );
}
/**
 * 「软件更新」那一行：上次查到什么、什么时候查的、手动「检查更新」。
 *
 * 读状态不联网（`update_status` 只读内存）；只有点「检查更新」才问 GitHub。
 * 查到新版就喊全局那一份更新弹窗（`openUpdateDialog`），这里不另挂一个。
 */
function UpdateLine() {
  const [status, setStatus] = useState<UpdateStatus>();
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<string>();
  useEffect(() => {
    let alive = true;
    void api
      .updateStatus()
      .then((s) => {
        if (alive) setStatus(s);
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, []);
  const check = async () => {
    setChecking(true);
    setError(undefined);
    try {
      const s = await api.updateCheck(true);
      setStatus(s);
      if (s.update_available) openUpdateDialog(s);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setChecking(false);
    }
  };
  const latest = status?.latest;
  const at = status?.checked_at
    ? new Date(status.checked_at).toLocaleTimeString("zh-CN", {
        hour: "2-digit",
        minute: "2-digit",
      })
    : null;
  const line = !status
    ? "正在读取…"
    : status.update_available && latest
      ? `GitHub 上有新版本 ${latest.version}${status.skipped ? "（这一版你点过跳过，启动时不再提醒）" : ""}`
      : status.error
        ? `上次检查没成功：${status.error}`
        : latest
          ? `已经是最新版本（GitHub 上最新是 ${latest.version}）`
          : "这次打开面板后还没检查过。";
  return (
    <div className="qb-setting-row">
      <span>
        <strong>
          当前版本 {status?.current_version ?? packageInfo.version}
        </strong>
        <small className={status?.error || error ? "notice--danger" : ""}>
          {error ?? line}
          {at && !error ? ` · ${at} 查过` : ""}
        </small>
      </span>
      <span className="qb-inline-actions">
        {status?.update_available && (
          <Button size="sm" onClick={() => openUpdateDialog(status)}>
            查看并更新
          </Button>
        )}
        <Button
          size="sm"
          icon={<RefreshCw size={14} />}
          loading={checking}
          onClick={() => void check()}
        >
          检查更新
        </Button>
      </span>
    </div>
  );
}
function Backups() {
  const snapshots = useResource("snapshots", R.snapshots);
  const workspace = useWorkspace();
  const action = useAction();
  const [note, setNote] = useSession("snapshot.note", "");
  const [confirm, setConfirm] = useState<{
    kind: "restore" | "delete";
    snapshot: SnapshotEntry;
  } | null>(null);
  return (
    <>
      <section className="qb-detail">
        <div className="qb-section-heading">
          <div>
            <h2>配置快照</h2>
            <p className="qb-muted">
              恢复具体环境的配置；不复制 OAuth 凭证，也不改变安装位置。
            </p>
          </div>
          <Archive size={23} />
        </div>
        <div className="qb-inline-actions">
          <input
            className="qb-input qb-grow"
            aria-label="快照备注"
            placeholder="给这次备份留一句备注…"
            value={note}
            onChange={(e) => setNote(e.target.value)}
          />
          <Button
            variant="primary"
            icon={<Plus size={15} />}
            loading={action.pending === "create"}
            onClick={() =>
              void action
                .run(
                  "create",
                  () => api.snapshotCreate(note || "手动备份"),
                  "快照已创建",
                )
                .then((s) => {
                  if (s) setNote("");
                })
            }
          >
            创建快照
          </Button>
        </div>
        {(snapshots.data ?? []).map((s) => (
          <article className="qb-snapshot-row" key={s.id}>
            <div className="qb-grow">
              <strong>{s.manifest.note || "配置快照"}</strong>
              <p>
                {s.manifest.created} · {s.manifest.files.length} 个文件 ·
                官方账户 {s.manifest.active_account ?? "默认"}
              </p>
            </div>
            <Button
              size="sm"
              icon={<RotateCcw size={14} />}
              onClick={() => setConfirm({ kind: "restore", snapshot: s })}
            >
              恢复
            </Button>
            <Button
              size="sm"
              variant="ghost"
              aria-label="删除快照"
              icon={<Trash2 size={14} />}
              onClick={() => setConfirm({ kind: "delete", snapshot: s })}
            />
          </article>
        ))}
        {!snapshots.data?.length && (
          <div className="qb-empty">
            <Save size={26} />
            <h3>还没有配置快照</h3>
            <p>在调整配置前保存一份，便于之后恢复。</p>
          </div>
        )}
        {(snapshots.error || action.error) && (
          <p className="notice notice--danger">
            {snapshots.error || action.error}
          </p>
        )}
      </section>
      {!!workspace.data?.migration_notes.length && (
        <details className="qb-help-details">
          <summary>旧版数据迁移记录</summary>
          {workspace.data.migration_notes.map((n) => (
            <p className="qb-break" key={n}>
              {n}
            </p>
          ))}
        </details>
      )}
      <Modal
        open={!!confirm}
        onClose={() => setConfirm(null)}
        title={confirm?.kind === "restore" ? "恢复配置快照" : "删除配置快照"}
        footer={
          <Button
            variant={confirm?.kind === "delete" ? "danger" : "primary"}
            loading={!!action.pending}
            onClick={() => {
              if (confirm)
                void action
                  .run(
                    confirm.kind,
                    () =>
                      confirm.kind === "restore"
                        ? api.snapshotRestore(confirm.snapshot.id)
                        : api
                            .snapshotRemove(confirm.snapshot.id)
                            .then(() => "快照已删除"),
                    "操作已完成",
                  )
                  .then((r) => {
                    if (r) setConfirm(null);
                  });
            }}
          >
            确认{confirm?.kind === "restore" ? "恢复" : "删除"}
          </Button>
        }
      >
        <p>
          {confirm?.kind === "restore"
            ? "先校验恢复源并备份当前配置，再恢复文件。快照所属官方账户需要与当前账户一致；配置写入失败将恢复原状。"
            : "删除此份快照，不会删除当前客户端配置。"}
        </p>
      </Modal>
    </>
  );
}
function Help() {
  return (
    <div className="qb-detail">
      <h2>关于 QB Gate</h2>
      <p className="qb-muted">
        官方账户、中转站与扩展的本地管理工具。门禁根据定期检测结果控制受管进程；执行锁不会冻结已加载进程的网络请求。
      </p>
      <h3 className="qb-subheading">项目与许可</h3>
      <div className="qb-link-list">
        <ExternalLink href="https://github.com/smithtaylor7748-ops/qb-gate">
          GitHub 项目与使用说明
        </ExternalLink>
        <ExternalLink href="https://github.com/smithtaylor7748-ops/qb-gate/blob/main/ATTRIBUTION.md">
          第三方来源与许可证
        </ExternalLink>
        <ExternalLink href="https://github.com/smithtaylor7748-ops/qb-gate/issues">
          报告问题
        </ExternalLink>
        <ExternalLink href="https://github.com/smithtaylor7748-ops/qb-gate/releases">
          发行版本与安装包
        </ExternalLink>
      </div>
      <p className="qb-help-note">
        QB Gate 使用 AGPL-3.0-only 及第 7
        节附加条款（署名、改版标明、不授予名称）。外部应用与扩展遵循各自的许可证；详情页提供来源和固定版本。
      </p>
    </div>
  );
}
