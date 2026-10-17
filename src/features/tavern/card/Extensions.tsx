/**
 * 「酒馆」弹窗的「扩展」分页（2026-09-29；第三轮从软件页的卡搬进弹窗）。
 *
 * 列出酒馆的第三方扩展（`data\default-user\extensions\` 与全局的 `public\scripts\extensions\third-party\`），
 * 每个的 manifest（名字、版本、作者、主页）与 git 事实（分支、提交、远端 —— 远端里的账号密码抹掉再显示）。
 *
 * - **检查更新 / 更新 / 从地址安装都跑 git、都联网，只在点了之后**；本机没有 git 就照实说、按钮不给；
 * - 更新是 `git pull --ff-only`：你本地改过扩展的文件就不动，照实说（面板不替你丢改动）；
 * - 删除 = 挪进回收站；「停用」只读出来显示（在酒馆里切 —— 面板不改酒馆的 settings.json）。
 */
import { useState } from "react";
import {
  ArrowDownToLine,
  GitBranch,
  PackagePlus,
  RefreshCw,
  Search,
  Trash2,
} from "lucide-react";

import {
  api,
  type ExtensionCheck,
  type TavernExtension,
} from "../../../lib/api";
import { AFTER, R } from "../../../lib/resources";
import { invalidate, useResource } from "../../../lib/store";
import {
  Button,
  ConfirmDialog,
  EmptyState,
  ExternalLink,
  Pill,
  TextField,
  useToast,
} from "../../../ui";
import { errorText } from "./common";

const key = (e: { name: string; scope: string }) => `${e.scope}/${e.name}`;

export default function Extensions() {
  const toast = useToast();
  const list = useResource("tavernExtensions", R.tavernExtensions);
  const [checks, setChecks] = useState<Record<string, ExtensionCheck | string>>(
    {},
  );
  const [busy, setBusy] = useState("");
  const [url, setUrl] = useState("");
  const [removing, setRemoving] = useState<TavernExtension | null>(null);

  const items = list.data?.items ?? [];
  const git = list.data?.git ?? null;
  const gitItems = items.filter((e) => e.git);

  async function check(e: TavernExtension) {
    setBusy("check:" + key(e));
    try {
      const r = await api.tavernExtensionCheck(e.name, e.scope);
      setChecks((c) => ({ ...c, [key(e)]: r }));
    } catch (err) {
      setChecks((c) => ({ ...c, [key(e)]: errorText(err) }));
    } finally {
      setBusy("");
    }
  }

  async function checkAll() {
    setBusy("check-all");
    for (const e of gitItems) {
      try {
        const r = await api.tavernExtensionCheck(e.name, e.scope);
        setChecks((c) => ({ ...c, [key(e)]: r }));
      } catch (err) {
        setChecks((c) => ({ ...c, [key(e)]: errorText(err) }));
      }
    }
    setBusy("");
  }

  async function update(e: TavernExtension) {
    setBusy("update:" + key(e));
    try {
      const msg = await api.tavernExtensionUpdate(e.name, e.scope);
      toast.ok(`${e.display_name}：${msg}。刷新一下酒馆的页面才会用上新的`);
      setChecks((c) => {
        const next = { ...c };
        delete next[key(e)];
        return next;
      });
      invalidate(...AFTER.tavernLibrary);
    } catch (err) {
      toast.error(errorText(err));
    } finally {
      setBusy("");
    }
  }

  async function install() {
    setBusy("install");
    try {
      toast.ok(await api.tavernExtensionInstall(url.trim()));
      setUrl("");
      invalidate(...AFTER.tavernLibrary);
    } catch (err) {
      toast.error(errorText(err));
    } finally {
      setBusy("");
    }
  }

  async function remove() {
    if (!removing) return;
    setBusy("remove");
    try {
      const t = await api.tavernExtensionDelete(removing.name, removing.scope);
      toast.ok(`「${t.name}」挪进了回收站，在「备份与回收站」里能恢复`);
      invalidate(...AFTER.tavernLibrary);
      setRemoving(null);
    } catch (err) {
      toast.error(errorText(err));
    } finally {
      setBusy("");
    }
  }

  return (
    <div data-testid="tavern-extensions">
      {list.data &&
        (git ? (
          <p className="notice">
            git：<span className="mono break-all">{git}</span>
          </p>
        ) : (
          <p className="notice notice--warn">
            本机没找到 git：检查更新、更新、从地址安装都要它（装 Git for
            Windows：winget install Git.Git，或到 git-scm.com）。
            也可以在酒馆自己的扩展管理里装 —— 1.19 起它没有 git 也能装。
          </p>
        ))}

      <div className="tv-install mt-2">
        <TextField
          label="从仓库地址安装"
          value={url}
          placeholder="https://github.com/作者/扩展名"
          onChange={setUrl}
        />
        <Button
          size="sm"
          variant="primary"
          icon={<PackagePlus size={12} />}
          loading={busy === "install"}
          disabled={!git || !url.trim().startsWith("https://") || !!busy}
          onClick={() => void install()}
        >
          安装
        </Button>
      </div>
      <p className="notice mt-1">
        git clone 到 <code>data\default-user\extensions\</code>
        （酒馆默认装的那一层）；联网，只在你点的时候。 只装你信得过的仓库 ——
        扩展能在酒馆页面里跑任意脚本。
      </p>

      <div className="tv-toolbar mt-3">
        <strong className="text-sm">
          {items.length ? `${items.length} 个扩展` : "扩展"}
        </strong>
        <span className="tv-toolbar-end">
          <Button
            size="sm"
            icon={<Search size={12} />}
            loading={busy === "check-all"}
            disabled={!git || !gitItems.length || !!busy}
            onClick={() => void checkAll()}
          >
            全部检查更新
          </Button>
          <Button
            size="sm"
            variant="ghost"
            icon={<RefreshCw size={12} />}
            loading={list.loading}
            onClick={() => void list.refresh()}
          >
            刷新
          </Button>
        </span>
      </div>

      {list.error ? (
        <p className="notice notice--danger mt-2">{list.error}</p>
      ) : list.neverLoaded ? (
        <p className="notice mt-2">读取中…</p>
      ) : items.length === 0 ? (
        <EmptyState title="还没有第三方扩展">
          上面填一个仓库地址装，或者在酒馆的扩展管理里装。酒馆自带的扩展不在这里列。
        </EmptyState>
      ) : (
        <ul className="tv-ext-list mt-2">
          {items.map((e) => {
            const r = checks[key(e)];
            const behind = typeof r === "object" ? r.behind : null;
            return (
              <li key={key(e)} className="tv-ext">
                <div className="tv-ext-main">
                  <div className="tv-ext-head">
                    <strong>{e.display_name}</strong>
                    {e.version && (
                      <span className="notice mono">v{e.version}</span>
                    )}
                    {e.scope === "global" && <Pill tone="accent">全局</Pill>}
                    {e.disabled && <Pill>在酒馆里停用了</Pill>}
                    {!e.has_manifest && (
                      <Pill tone="warn">没有 manifest.json</Pill>
                    )}
                  </div>
                  <p className="notice break-all">
                    {[
                      e.author && `作者 ${e.author}`,
                      e.name !== e.display_name && e.name,
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                  </p>
                  {e.git ? (
                    <p className="notice break-all">
                      <GitBranch size={11} aria-hidden="true" />{" "}
                      {e.branch || "（游离提交）"}
                      {e.commit ? ` @ ${e.commit}` : ""}
                      {e.remote ? ` · ${e.remote}` : ""}
                    </p>
                  ) : (
                    <p className="notice">
                      不是 git 克隆来的（多半是解压进来的）：面板没法检查更新。
                    </p>
                  )}
                  {!e.has_manifest && (
                    <p className="notice notice--warn">
                      酒馆不会加载没有 manifest.json 的目录 ——
                      多半是删到一半留下的，挪进回收站就干净了。
                    </p>
                  )}
                  {r !== undefined && (
                    <p
                      className={`notice ${typeof r === "string" ? "notice--danger" : behind ? "notice--warn" : ""}`}
                      data-testid="tavern-extension-check"
                    >
                      {typeof r === "string" ? r : r.detail}
                    </p>
                  )}
                </div>
                <div className="tv-ext-actions">
                  {e.home_page && (
                    <ExternalLink href={e.home_page}>主页</ExternalLink>
                  )}
                  {e.git && (
                    <Button
                      size="sm"
                      loading={busy === "check:" + key(e)}
                      disabled={!git || !!busy}
                      onClick={() => void check(e)}
                    >
                      检查更新
                    </Button>
                  )}
                  {e.git && behind !== null && behind > 0 && (
                    <Button
                      size="sm"
                      variant="primary"
                      icon={<ArrowDownToLine size={12} />}
                      loading={busy === "update:" + key(e)}
                      disabled={!!busy}
                      onClick={() => void update(e)}
                    >
                      更新
                    </Button>
                  )}
                  <Button
                    size="sm"
                    variant="ghost"
                    icon={<Trash2 size={12} />}
                    aria-label={`把扩展「${e.display_name}」挪进回收站`}
                    disabled={!!busy}
                    onClick={() => setRemoving(e)}
                  />
                </div>
              </li>
            );
          })}
        </ul>
      )}

      <ConfirmDialog
        open={removing !== null}
        onCancel={() => setRemoving(null)}
        onConfirm={() => void remove()}
        title={`把扩展「${removing?.display_name ?? ""}」挪进回收站？`}
        confirmLabel="挪进回收站"
        loading={busy === "remove"}
        danger
      >
        <p>
          整个目录挪进面板的回收站，在「备份与回收站」里能原样放回去。它存在酒馆设置里的选项不动；
          酒馆开着的话，刷新一下页面它就不再加载了。
        </p>
      </ConfirmDialog>
    </div>
  );
}
