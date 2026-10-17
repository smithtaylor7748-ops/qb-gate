/**
 * 酒馆的资产盘点与备份恢复（2026-09-29 从扩展中心搬到软件页；同一天第二轮分页重构时收进「备份与回收站」，
 * 原来叫 `TavernData.tsx`；第三轮那几页整体搬进了账户页的「酒馆」弹窗）。
 *
 * **这里不重写酒馆的编辑界面。** 编辑仍然在酒馆自己那边做，面板只负责「有哪些、多大、什么时候改的」
 * 以及备份 / 恢复。备份是目录复制不是打包：出问题时可以直接进文件夹翻，不需要本程序也能恢复；
 * 恢复之前会自动把现状再存一份。
 */
import { useState } from "react";
import { Archive, FolderTree, History, RotateCw } from "lucide-react";

import { api } from "../../../lib/api";
import { AFTER, R } from "../../../lib/resources";
import { invalidate, useResource } from "../../../lib/store";
import {
  Button,
  ConfirmDialog,
  EmptyState,
  Row,
  fmtSize,
  useToast,
} from "../../../ui";
import { errorText } from "./common";

export default function Assets() {
  const toast = useToast();
  const assets = useResource("tavernAssets", R.tavernAssets);
  const backups = useResource("tavernBackups", R.tavernBackups);
  const [openCat, setOpenCat] = useState<string | null>(null);
  const [restoreId, setRestoreId] = useState<string | null>(null);
  const [busy, setBusy] = useState("");

  async function act(name: string, fn: () => Promise<unknown>, ok: string) {
    setBusy(name);
    try {
      await fn();
      toast.ok(ok);
      invalidate(...AFTER.tavern, ...AFTER.tavernLibrary, "tavernBackups");
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setBusy("");
      setRestoreId(null);
    }
  }

  return (
    <>
      <section className="tv-block" aria-labelledby="tv-assets">
        <div className="tv-block-head">
          <h3 id="tv-assets" className="tv-block-title">
            <FolderTree size={13} aria-hidden="true" /> 资产盘点
          </h3>
          <Button
            size="sm"
            variant="ghost"
            icon={<RotateCw size={12} />}
            onClick={() => void assets.refresh()}
          >
            刷新
          </Button>
        </div>
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
            酒馆装好（或指到你自己那份）之后再点刷新。
          </EmptyState>
        )}
      </section>

      <section className="tv-block" aria-labelledby="tv-backups">
        <div className="tv-block-head">
          <h3 id="tv-backups" className="tv-block-title">
            <Archive size={13} aria-hidden="true" /> 备份与恢复
          </h3>
          <Button
            size="sm"
            variant="primary"
            loading={busy === "backup"}
            disabled={!!busy}
            onClick={() => act("backup", api.tavernBackup, "已备份全部资产")}
          >
            立即备份
          </Button>
        </div>
        <p className="notice">
          世界书、角色卡、预设、群组、聊天、扩展与酒馆设置整份复制，不打包 ——
          不开面板也能直接进文件夹翻。
        </p>
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
            点「立即备份」存一份。
          </EmptyState>
        )}
      </section>

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
