import { useState } from "react";
import { Link, NavLink, useParams } from "react-router-dom";
import { Archive, Plus, RotateCcw, Save, Trash2 } from "lucide-react";
import {
  api,
  type Settings as Preferences,
  type SnapshotEntry,
} from "../lib/api";
import { R } from "../lib/resources";
import { useResource, useSession } from "../lib/store";
import { useAction, useWorkspace } from "../lib/workspace";
import { Button, ExternalLink, Modal } from "../ui";
import LegacySettings from "../pages/Settings";
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
        <section className="qb-legacy-detail">
          <LegacySettings />
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
      <p className="qb-help-note">
        IP
        白名单、国家规则与会话门禁集中在“环境与门禁”。安装目录迁移位于“高级维护”。
      </p>
      <div className="qb-about">
        <strong>QB Gate {packageInfo.version}</strong>
        <p>本地 AI 客户端工作空间 · GPL-3.0-or-later</p>
        {/* 图标由 ExternalLink 自己补，这里不要再加一个 —— 加了就是两个箭头。 */}
        <ExternalLink href="https://github.com/smithtaylor7748-ops/qb-gate/releases">
          查看 GitHub 发布版本
        </ExternalLink>
      </div>
    </section>
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
      <h3 className="qb-subheading">重新走一遍新手引导</h3>
      <p className="qb-muted">
        加白名单 → 实测出口 IP → 清理或安装 → 建槽位 →
        启动。已经做过的步骤可以直接跳过。
      </p>
      <div className="qb-link-list">
        <Link to="/onboarding">打开新手引导</Link>
      </div>
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
        QB Gate 使用
        GPL-3.0-or-later。外部应用与扩展遵循各自的许可证；详情页提供来源和固定版本。
      </p>
    </div>
  );
}
