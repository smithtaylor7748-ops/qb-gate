/**
 * 托管安装的两块界面（v0.9.0）：托管目录的「更改 / 一键迁移」，和外部副本的「彻底清除」。
 *
 * 环境页与设置页都挂「托管目录」这一块，所以抽出来只写一份。
 *
 * # 目录为什么要当场实测
 *
 * 「改了目录之后锁不上」有两种根因：盘不是 NTFS（exFAT/FAT32 的 U 盘、移动硬盘没有权限系统），
 * 或者当前账户没资格改那个目录的权限（Program Files、网络盘）。这里在选的时候就让后端
 * 放一个探针文件试着上锁再解锁 —— 不行就当场拒绝并说清原因，**选得进来的目录一定锁得住**。
 */

import { useEffect, useState } from 'react';
import { FolderCog, Trash2, Undo2 } from 'lucide-react';

import {
  api,
  type ManagedApp,
  type ManagedExternal,
  type ManagedProbe,
  type VersionEntry,
} from '../../lib/api';
import { AFTER, R } from '../../lib/resources';
import { invalidate, useResource, useSession } from '../../lib/store';
import {
  Bullet,
  Button,
  ConfirmDialog,
  EmptyState,
  Modal,
  PathField,
  Pill,
  Row,
  useToast,
  EXTERNAL_METHOD_LABEL,
  MANAGED_APP_LABEL,
} from '../../ui';

// ---------------------------------------------------------------- 托管目录

export function ManagedDirControl({ compact = false, disabled = false }: { compact?: boolean; disabled?: boolean }) {
  const toast = useToast();
  const managed = useResource('managed', R.managed);
  const [open, setOpen] = useState(false);
  const [path, setPath] = useState('');
  const [probe, setProbe] = useState<ManagedProbe | null>(null);
  const [probing, setProbing] = useState(false);
  const [busy, setBusy] = useState(false);

  const st = managed.data;
  const anyInstalled = !!st?.apps.some((a) => a.installed);

  // 每换一次路径就实测一次。停手 400 毫秒再测，免得每敲一个字就去建一次目录。
  useEffect(() => {
    if (!open) return;
    const p = path.trim();
    setProbe(null);
    if (!p || (st && p.toLowerCase() === st.root.toLowerCase())) return;
    setProbing(true);
    const t = setTimeout(() => {
      void api
        .managedProbeDir(p)
        .then(setProbe)
        .catch((e) =>
          setProbe({ ok: false, path: p, reason: e instanceof Error ? e.message : String(e) })
        )
        .finally(() => setProbing(false));
    }, 400);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [path, open]);

  function start() {
    setPath(st?.root ?? '');
    setProbe(null);
    setOpen(true);
  }

  async function apply() {
    setBusy(true);
    try {
      const r = await api.managedSetDir(path.trim());
      toast.ok(
        r.moved.length
          ? `托管目录已迁到 ${r.to}：${r.moved.join('；')}${r.closed ? `（先关掉了 ${r.closed} 个 Claude 进程）` : ''}`
          : `托管目录改为 ${r.to}。`
      );
      invalidate(...AFTER.install);
      setOpen(false);
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <div className={compact ? 'flex flex-wrap items-center gap-2' : 'flex flex-wrap items-center gap-2 mb-3'}>
        <FolderCog size={14} aria-hidden="true" />
        <span className="text-sm">托管目录</span>
        <code className="break-all">{st?.root ?? '…'}</code>
        {st?.is_default && <Pill tone="default">默认</Pill>}
        <Button size="sm" onClick={start} disabled={!st || disabled}>
          更改目录
        </Button>
      </div>

      <Modal
        open={open}
        onClose={busy ? () => undefined : () => setOpen(false)}
        dismissible={!busy}
        title="更改托管目录"
        footer={
          <>
            <Button onClick={() => setOpen(false)} disabled={busy}>
              取消
            </Button>
            <Button variant="primary" loading={busy} disabled={!probe?.ok || probing} onClick={() => void apply()}>
              {anyInstalled ? '确定并迁移' : '确定'}
            </Button>
          </>
        }
      >
        <PathField label="新目录" value={path} onChange={setPath} kind="directory" disabled={busy} />
        <p className="notice mt-2">
          {probing
            ? '正在实测这个目录能不能上锁……'
            : probe === null
              ? '选一个目录，面板会当场放一个探针文件试着上锁再解锁 —— 锁不住的目录选不进来。'
              : probe.ok
                ? `可以用${probe.filesystem ? `（${probe.filesystem}）` : ''}：锁得上，也解得开。`
                : ''}
        </p>
        {probe && !probe.ok && <p className="notice notice--danger mt-1">{probe.reason}</p>}
        {anyInstalled && (
          <p className="notice mt-2">
            已经装在旧目录里的 Claude Code / Codex 会<strong>一起搬过去</strong>，并在新位置重新上锁。
            如果托管的 Claude Code 正在运行，会先关闭全部 Claude（未保存的对话会丢）；
            托管的 Codex 正在运行就请先自己关掉 —— 面板不按进程名杀进程，这时会拒绝迁移、一个文件都不动。
            中途哪一步没成，已经搬过去的会搬回来，不会两边各留一半。
          </p>
        )}
      </Modal>
    </>
  );
}

// ---------------------------------------------------------------- 外部副本

/**
 * 面板没装的多余副本。只有托管那份装好了才出现 —— 不然清完这台机器上一份都没有了。
 *
 * 使用者的两种选择：**彻底清除**（能卸载的用它自己的卸载命令，其余删文件，一点痕迹不留），
 * 或者**保留** —— 保留什么都不用做，它们照样被执行锁锁住。
 */
export function ExternalsBlock() {
  const toast = useToast();
  const managed = useResource('managed', R.managed);
  const externals = useResource('externals', R.externals);
  const [kept, setKept] = useSession('env.externals.kept', false);
  const [ask, setAsk] = useState<ManagedApp | null>(null);
  const [busy, setBusy] = useState(false);

  const installed = (a: ManagedApp) => !!managed.data?.apps.find((x) => x.app === a)?.installed;
  const anyManaged = installed('claude-code') || installed('codex');
  if (!anyManaged) return null;

  const list = externals.data ?? [];
  const byApp = (a: ManagedApp) => list.filter((e) => e.app === a);

  async function cleanup(which: ManagedApp) {
    setBusy(true);
    try {
      const r = await api.managedCleanup(which);
      if (r.failed.length) {
        toast.error(`清除 ${MANAGED_APP_LABEL[which]} 外部副本：${r.done.length} 项做完，${r.failed.length} 项没成 —— ${r.failed.join('；')}`);
      } else {
        toast.ok(`已彻底清除 ${MANAGED_APP_LABEL[which]} 的外部副本（${r.done.length} 项）。`);
      }
      for (const n of r.notes) toast.info(n);
      invalidate(...AFTER.install);
      await externals.refresh();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
      setAsk(null);
    }
  }

  if (kept) {
    return (
      <p className="notice mt-3">
        外部副本已选择保留，它们照样被执行锁锁住。
        <Button size="sm" variant="ghost" className="ml-1" onClick={() => setKept(false)}>
          重新考虑
        </Button>
      </p>
    );
  }

  return (
    <div className="mt-4 border-t border-line pt-3">
      <div className="mb-2 flex flex-wrap items-center gap-2">
        <Trash2 size={14} aria-hidden="true" />
        <span className="text-sm">面板没装的多余副本</span>
        <Button size="sm" className="ml-auto" loading={externals.loading} onClick={() => void externals.refresh()}>
          {externals.data ? '重新扫描' : '扫描'}
        </Button>
      </div>

      {externals.error && <p className="notice notice--danger">{externals.error}</p>}
      {!externals.data ? (
        <p className="notice">
          托管那份装好之后，本机其它 Claude Code / Codex 副本就是多余的了。点「扫描」看看有哪些 ——
          只看不动。不清的话它们照样被执行锁锁住。
        </p>
      ) : list.length === 0 ? (
        <p className="notice">没有多余的副本，本机只剩面板托管的那份。</p>
      ) : (
        (['claude-code', 'codex'] as const).map((a) => {
          const items = byApp(a);
          if (!items.length) return null;
          return (
            <div key={a} className="mb-3">
              {items.map((e: ManagedExternal) => (
                <Row key={`${e.method}:${e.target}`} side={<Pill tone="warn">{EXTERNAL_METHOD_LABEL[e.method]}</Pill>}>
                  <span>{MANAGED_APP_LABEL[e.app]}</span>
                  <span className="notice block w-full break-all font-mono">{e.action}</span>
                </Row>
              ))}
              <div className="mt-2 flex flex-wrap gap-2">
                <Button
                  variant="danger"
                  size="sm"
                  disabled={busy || !installed(a)}
                  onClick={() => setAsk(a)}
                >
                  彻底清除 {MANAGED_APP_LABEL[a]} 的这些副本
                </Button>
                {!installed(a) && (
                  <span className="notice">先把托管的 {MANAGED_APP_LABEL[a]} 装好才能清。</span>
                )}
              </div>
            </div>
          );
        })
      )}

      {!!list.length && (
        <Button size="sm" variant="ghost" onClick={() => setKept(true)}>
          保留，照样上锁
        </Button>
      )}

      <ConfirmDialog
        open={ask !== null}
        onCancel={() => setAsk(null)}
        onConfirm={() => ask && void cleanup(ask)}
        title={`彻底清除 ${ask ? MANAGED_APP_LABEL[ask] : ''} 的外部副本？`}
        confirmLabel="彻底清除"
        confirmWord="清除"
        loading={busy}
        danger
      >
        <p>下面这些会被卸载或删除，<strong>不可恢复</strong>：</p>
        <div className="mt-2">
          {(ask ? byApp(ask) : []).map((e) => (
            <Bullet key={`${e.method}:${e.target}`} tone="danger">
              {e.action}
            </Bullet>
          ))}
        </div>
        <p className="notice mt-2">
          npm / winget / scoop 装的用它们自己的卸载命令（连启动器与登记一起走）；官方安装器那份没有卸载命令，直接删文件。
          每个要删的文件都会先核对数字签名，不是 {ask === 'codex' ? 'OpenAI' : 'Anthropic'} 的一律不删。
        </p>
        <p className="notice mt-2">
          <strong>不碰</strong>：账户凭证与配置（<code>~\.claude</code>、账户槽位、<code>~\.codex</code>）、
          桌面端与它自带的副本、编辑器扩展里的副本、面板托管的那份。
          {ask === 'claude-code' && ' 清之前会先关闭全部 Claude（正在运行的删不掉）。'}
          {ask === 'codex' &&
            ' 请先自己关掉正在跑的 Codex —— 面板不按进程名杀进程，正在运行的那份删不掉，会在结果里如实列出。'}
        </p>
      </ConfirmDialog>
    </div>
  );
}

/**
 * 版本库与一键回滚。
 *
 * 在这之前，升级只留一个 `claude.exe.old.<时间戳>`，下次安装就删 ——
 * 名字里没有版本号，装第二次上一版就永远没了。能往前走却退不回来，
 * 等于每次升级都是单程票。
 *
 * 界面上要说清两件容易误解的事：回滚**不影响正在跑的会话**（Windows 不卸
 * 已加载的映像，下次启动才生效），以及版本库里的每一份**照样是锁着的**。
 */
export function VersionHistoryBlock() {
  const toast = useToast();
  const [app] = useState<ManagedApp>('claude-code');
  const [rows, setRows] = useState<VersionEntry[] | null>(null);
  const [busy, setBusy] = useState('');
  const [ask, setAsk] = useState<VersionEntry | null>(null);

  async function load() {
    try {
      setRows(await api.managedHistory(app));
    } catch {
      setRows([]);
    }
  }

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [app]);

  async function rollback(v: VersionEntry) {
    setBusy(v.version);
    try {
      toast.ok(await api.managedRollback(app, v.version));
      invalidate(...AFTER.install);
      await load();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy('');
      setAsk(null);
    }
  }

  return (
    <>
      <p className="notice mb-2">
        每次升级都会把旧版本收进版本库，最多留 3 份。新版本有问题时可以退回去 ——
        <strong>回滚不影响正在跑的会话</strong>，Windows 不会卸掉已加载的映像，
        下次启动才生效。
      </p>

      {rows === null ? (
        <p className="notice">读取中…</p>
      ) : rows.length === 0 ? (
        <EmptyState title="版本库是空的">
          面板托管安装升级过一次之后，这里才会有东西。
        </EmptyState>
      ) : (
        rows.map((v) => (
          <Row
            key={v.version}
            side={
              v.is_current ? (
                <Pill tone="ok">在用</Pill>
              ) : (
                <Button
                  size="sm"
                  icon={<Undo2 size={12} />}
                  loading={busy === v.version}
                  disabled={!!busy}
                  onClick={() => setAsk(v)}
                >
                  回滚
                </Button>
              )
            }
          >
            <span className="font-mono">{v.version}</span>
            <span className="notice">{v.archived_at || '归档时间未知'}</span>
            {/* 版本库里的每一份都是完整可执行的 claude.exe。没锁上就是
                现成的绕过入口，必须显眼 —— 不能只在日志里提一句。 */}
            {!v.locked && <Pill tone="danger">未上锁</Pill>}
          </Row>
        ))
      )}

      <ConfirmDialog
        open={!!ask}
        onCancel={() => setAsk(null)}
        onConfirm={() => ask && void rollback(ask)}
        title={`回滚到 ${ask?.version ?? ''}？`}
        confirmLabel="确认回滚"
        loading={!!busy}
      >
        <p>
          当前这份会<strong>先收进版本库</strong>再把 {ask?.version} 换上来，
          所以随时可以再换回去。
        </p>
        <p className="notice notice--warn mt-2">
          正在跑的 Claude Code 会话<strong>不受影响</strong>，
          换的是下一次启动用的那个文件。换完面板会重新上锁。
        </p>
      </ConfirmDialog>
    </>
  );
}
