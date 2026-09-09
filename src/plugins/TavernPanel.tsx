import { useState } from 'react';
import { openUrl } from '@tauri-apps/plugin-opener';
import {
  Archive,
  ExternalLink as LinkIcon,
  FolderTree,
  History,
  Play,
  RotateCw,
  Settings2,
  Square,
} from 'lucide-react';

import { api, type TavernConfig } from '../lib/api';
import { AFTER, R } from '../lib/resources';
import { invalidate, useResource } from '../lib/store';
import { endTask, resetTask, useTask } from '../lib/tasks';
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
} from '../ui';

export default function TavernPanel() {
  const toast = useToast();
  const cfgRes = useResource('tavernConfig', R.tavernConfig);
  const assets = useResource('tavernAssets', R.tavernAssets);
  const backups = useResource('tavernBackups', R.tavernBackups);
  const plugins = useResource('plugins', R.plugins);
  const task = useTask('tavern-start');

  const [busy, setBusy] = useState('');
  const [draft, setDraft] = useState<TavernConfig | null>(null);
  const [openCat, setOpenCat] = useState<string | null>('worlds');
  const [restoreId, setRestoreId] = useState<string | null>(null);

  const cfg = draft ?? cfgRes.data ?? null;
  const running = plugins.data?.[0]?.state === 'running';

  async function start() {
    setBusy('start');
    resetTask('tavern-start');
    try {
      const url = await api.pluginStart();
      endTask('tavern-start');
      // 无论新起还是复用现有服务都要打开页面 —— 少了这步，
      // 成功的启动和崩溃看起来一模一样。
      if (url.startsWith('http')) await openUrl(url);
      toast.ok('酒馆已就绪，已打开页面');
      invalidate(...AFTER.tavern);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      endTask('tavern-start', msg);
      toast.error(msg);
    } finally {
      setBusy('');
    }
  }

  async function act(name: string, fn: () => Promise<unknown>, ok: string) {
    setBusy(name);
    try {
      await fn();
      toast.ok(ok);
      invalidate(...AFTER.tavern, 'tavernAssets', 'tavernBackups', 'tavernConfig');
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy('');
      setRestoreId(null);
    }
  }

  return (
    <>
      <Card tone="accent" className="mb-3">
        <p className="notice mb-3">
          启动会先校验出口 IP，通过后才拉起桥接与酒馆，并挂上看门狗。
          <strong>门禁不过就一个进程都不会起。</strong>
        </p>
        <div className="flex flex-wrap gap-2">
          <Button
            variant="primary"
            icon={<Play size={13} />}
            loading={busy === 'start'}
            disabled={!!busy}
            onClick={start}
          >
            {running ? '打开酒馆页面' : '启动酒馆'}
          </Button>
          <Button
            variant="danger"
            icon={<Square size={13} />}
            loading={busy === 'stop'}
            disabled={!!busy || !running}
            onClick={() => act('stop', api.pluginStop, '酒馆与桥接已停止，租约已收回')}
          >
            停止
          </Button>
        </div>

        {(task.running || task.error) && (
          <div className="mt-3">
            <div className="mb-1.5 flex items-center gap-2">
              <span className="text-sm">{task.phase || '启动中…'}</span>
              {task.total > 0 && (
                <span className="notice ml-auto">
                  {task.step} / {task.total}
                </span>
              )}
            </div>
            <ProgressBar
              value={task.total > 0 ? (task.step / task.total) * 100 : undefined}
              tone={task.error ? 'danger' : 'accent'}
              label="酒馆启动进度"
            />
            {task.error && <p className="notice notice--danger mt-1.5">{task.error}</p>}
          </div>
        )}

        <Collapsible className="mt-3" summary="启动过程中都做了什么？">
          <ol className="notice ml-4 list-decimal">
            <li>校验出口 IP，不过就直接退出，一个进程都不起</li>
            <li>起桥接（排除 claude-code-cli 下的 vendored 副本，并修数据目录 ACL）</li>
            <li>
              轮询 token 文件与 <code>/health</code>，上限 20 秒 ——
              <strong>不能只看进程活着</strong>
            </li>
            <li>起酒馆，轮询端口，上限 60 秒</li>
            <li>打开浏览器页面</li>
          </ol>
          <p className="notice mt-2">
            端口被别人占住会<strong>报错退出</strong>，绝不去停无关进程。
            PID 文件也会先比对命令行，指向别的进程就报错，不覆盖也不杀。
          </p>
        </Collapsible>
      </Card>

      {/* ------------------------------------------------------ 路径 */}
      <Collapsible summary={<span className="flex items-center gap-1.5"><Settings2 size={13} />路径与端口设置</span>}>
        {cfg ? (
          <>
            <p className="notice mb-3">默认值指向本机现有部署。换机器或换目录时在这里改。</p>
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
                loading={busy === 'cfg'}
                disabled={!draft || !!busy}
                onClick={() =>
                  act('cfg', () => api.tavernConfigSave(cfg), '路径已保存').then(() =>
                    setDraft(null)
                  )
                }
              >
                保存
              </Button>
              <Button disabled={!draft} onClick={() => setDraft(null)}>
                放弃改动
              </Button>
            </div>
          </>
        ) : (
          <p className="notice">读取配置中…</p>
        )}
      </Collapsible>

      {/* ------------------------------------------------------ 资产 */}
      <Card
        title="资产"
        icon={<FolderTree size={14} />}
        as="h3"
        className="mt-3"
        actions={
          <Button size="sm" icon={<RotateCw size={12} />} onClick={() => void assets.refresh()}>
            刷新
          </Button>
        }
      >
        <p className="notice mb-2">
          面板只做盘点与搬运。世界书、角色卡的<strong>编辑仍在酒馆自己界面里</strong>做 ——
          在面板里重写一套编辑器，只会永远落后于上游。
        </p>

        {assets.data?.length ? (
          assets.data.map((c) => (
            <div key={c.id}>
              <Row
                side={
                  <Button
                    size="sm"
                    variant="ghost"
                    aria-expanded={openCat === c.id}
                    onClick={() => setOpenCat(openCat === c.id ? null : c.id)}
                  >
                    {openCat === c.id ? '收起' : '展开'}
                  </Button>
                }
              >
                <span>{c.label}</span>
                <Pill tone={c.exists ? 'default' : 'warn'}>
                  {c.exists ? `${c.items.length} 项` : '目录不存在'}
                </Pill>
              </Row>
              {openCat === c.id &&
                (c.items.length ? (
                  <div className="pl-4">
                    {c.items.map((it) => (
                      <Row key={it.path} side={<span className="notice">{fmtSize(it.size)}</span>}>
                        <span className="notice">
                          {it.is_dir ? '目录 · ' : ''}
                          {it.name}
                        </span>
                        <span className="notice">{it.modified ?? ''}</span>
                      </Row>
                    ))}
                  </div>
                ) : (
                  <p className="notice pl-4 py-2">这一类还是空的。</p>
                ))}
            </div>
          ))
        ) : (
          <EmptyState title="还没盘点到资产">
            确认上面的 SillyTavern 目录填对了，再点一次刷新。
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
            loading={busy === 'backup'}
            disabled={!!busy}
            onClick={() => act('backup', api.tavernBackup, '已备份全部资产')}
          >
            立即备份
          </Button>
        }
      >
        <p className="notice mb-2">
          备份是<strong>目录复制</strong>不是打包 —— 出问题时你可以直接进文件夹翻，
          不需要本程序也能恢复。
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
          <EmptyState
            icon={<Archive size={22} />}
            title="还没有备份"
            action={
              <Button
                variant="primary"
                loading={busy === 'backup'}
                onClick={() => act('backup', api.tavernBackup, '已备份全部资产')}
              >
                立即备份全部资产
              </Button>
            }
          >
            备份世界书、角色卡、预设与扩展。恢复之前面板会自动把现状再存一份。
          </EmptyState>
        )}
      </Card>

      <div className="mt-3">
        <Button
          size="sm"
          variant="ghost"
          icon={<LinkIcon size={12} />}
          onClick={() => openUrl(`http://127.0.0.1:${cfg?.st_port ?? 8000}`)}
        >
          直接打开酒馆页面
        </Button>
      </div>

      <ConfirmDialog
        open={restoreId !== null}
        onCancel={() => setRestoreId(null)}
        onConfirm={() =>
          restoreId && act('restore', () => api.tavernRestore(restoreId), `已恢复到 ${restoreId}`)
        }
        title="恢复这份备份？"
        confirmLabel="确认恢复"
        confirmWord="恢复"
        loading={busy === 'restore'}
        danger
      >
        <p>
          恢复 <code>{restoreId}</code> 会
          <strong>覆盖现有的世界书、角色卡、预设与扩展</strong>。
          现在文件夹里的内容会被备份里的版本替换掉。
        </p>
        <p className="notice mt-2">
          面板在恢复<strong>之前会自动把现状再存一份</strong>，点错了还能退回去 ——
          但正在跑的酒馆不会自动重载，建议先停掉再恢复。
        </p>
      </ConfirmDialog>
    </>
  );
}
