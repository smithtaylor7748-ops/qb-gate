import { useState } from 'react';
import {
  Archive,
  Camera,
  History,
  Layers,
  Pencil,
  Play,
  Plus,
  RotateCcw,
  RotateCw,
  Trash2,
} from 'lucide-react';
import { openUrl } from '@tauri-apps/plugin-opener';

import { api, type Profile, type SnapshotEntry } from '../lib/api';
import { AFTER, R } from '../lib/resources';
import { invalidate, useResource } from '../lib/store';
import {
  Bullet,
  Button,
  Card,
  Collapsible,
  ConfirmDialog,
  EmptyState,
  Modal,
  PageHeader,
  Pill,
  Row,
  TextField,
  useToast,
} from '../ui';

const RELAY_LABEL: Record<string, string> = {
  'claude-code': 'Claude Code',
  'claude-desktop': 'Claude 桌面端',
  codex: 'Codex',
};

/**
 * 档案与快照。
 *
 * 两件事放一页，因为它们是同一个问题的两面：档案是「主动切到一套已知状态」，
 * 快照是「退回到一个曾经的状态」。应用档案本身也会先存一份快照。
 */
export default function Profiles() {
  const toast = useToast();
  const profiles = useResource('profiles', R.profiles);
  const snapshots = useResource('snapshots', R.snapshots);
  const accounts = useResource('accounts', R.accounts);
  const relays = useResource('relay', R.relay);

  const [busy, setBusy] = useState('');
  const [edit, setEdit] = useState<Profile | null>(null);
  const [askApply, setAskApply] = useState<Profile | null>(null);
  const [askRestore, setAskRestore] = useState<SnapshotEntry | null>(null);
  const [askDelete, setAskDelete] = useState<Profile | null>(null);

  const list = profiles.data?.profiles ?? [];
  const lastApplied = profiles.data?.last_applied;

  async function act(name: string, fn: () => Promise<unknown>, ok?: string) {
    setBusy(name);
    try {
      await fn();
      if (ok) toast.ok(ok);
      invalidate(...AFTER.profile);
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy('');
    }
  }

  async function capture() {
    setBusy('capture');
    try {
      const draft = await api.profileCapture('新档案');
      setEdit(draft);
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy('');
    }
  }

  async function saveProfile() {
    if (!edit) return;
    if (!edit.name.trim()) {
      toast.error('给档案起个名字');
      return;
    }
    await act(
      'save',
      async () => {
        await api.profileSave({ ...edit, name: edit.name.trim() });
        setEdit(null);
      },
      '档案已保存'
    );
  }

  async function apply(p: Profile) {
    setAskApply(null);
    setBusy(`apply-${p.id}`);
    try {
      const r = await api.profileApply(p.id);
      // 有失败项时不能只弹一个「成功」—— 逐项结果都要让用户看到。
      if (r.failed.length > 0) toast.error(r.detail);
      else toast.ok(r.detail);
      invalidate(...AFTER.profile);
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy('');
    }
  }

  function setEditField(patch: Partial<Profile>) {
    setEdit((e) => (e ? { ...e, ...patch } : e));
  }

  return (
    <>
      <PageHeader
        title="档案与快照"
        sub="档案是主动切到一套已知状态，快照是退回到一个曾经的状态。"
        actions={
          <>
            <Button
              icon={<RotateCw size={13} />}
              loading={profiles.loading || snapshots.loading}
              onClick={() => {
                void profiles.refresh();
                void snapshots.refresh();
              }}
            >
              刷新
            </Button>
            <Button
              variant="primary"
              icon={<Camera size={13} />}
              loading={busy === 'capture'}
              disabled={!!busy}
              onClick={() => void capture()}
            >
              按当前状态建档案
            </Button>
          </>
        }
      />

      {/* ------------------------------------------------------ 档案 */}
      <Card title="统一档案" icon={<Layers size={14} />} className="mb-3">
        <p className="notice mb-2">
          一个档案 = 账户槽位 + 三个中转站 + 时区。
          <strong>没填的部分保持原样，不会被清空</strong> ——
          只想换中转站的人不该被迫也把账户绑进去。
        </p>

        {list.length === 0 ? (
          <EmptyState
            title="还没有档案"
            action={
              <Button
                variant="primary"
                icon={<Camera size={13} />}
                onClick={() => void capture()}
              >
                按当前状态建一个
              </Button>
            }
          >
            先把账户、中转站、时区调成你想要的样子，再点上面那个按钮，
            当前状态就会被抓成一个档案。
          </EmptyState>
        ) : (
          <div className="grid gap-2 sm:grid-cols-2">
            {list.map((p) => (
              <div
                key={p.id}
                className={`rounded-md border p-3 ${
                  lastApplied === p.id ? 'border-accent-line bg-accent-bg' : 'border-line'
                }`}
              >
                <div className="flex items-center gap-2">
                  <span className="min-w-0 flex-1 truncate font-medium">{p.name}</span>
                  {lastApplied === p.id && <Pill tone="accent">上次应用</Pill>}
                </div>

                <div className="mt-1.5">
                  <Bullet marker="账户">{p.account ?? '不动'}</Bullet>
                  {Object.entries(RELAY_LABEL).map(([k, label]) => {
                    const pid = p.relays?.[k];
                    const found = (relays.data ?? []).find((r) => r.id === pid);
                    return (
                      <Bullet key={k} marker={label}>
                        {pid ? (found?.name ?? `已删除的记录（${pid}）`) : '不动'}
                      </Bullet>
                    );
                  })}
                  <Bullet marker="时区">{p.timezone ?? '不动'}</Bullet>
                </div>

                {p.note && <p className="notice mt-1">{p.note}</p>}

                <div className="mt-2 flex flex-wrap gap-1">
                  <Button
                    size="sm"
                    variant="primary"
                    icon={<Play size={11} />}
                    loading={busy === `apply-${p.id}`}
                    disabled={!!busy}
                    onClick={() => setAskApply(p)}
                  >
                    应用
                  </Button>
                  <Button size="sm" icon={<Pencil size={11} />} onClick={() => setEdit(p)}>
                    编辑
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    icon={<Trash2 size={11} />}
                    aria-label="删除档案"
                    disabled={!!busy}
                    onClick={() => setAskDelete(p)}
                  />
                </div>
              </div>
            ))}
          </div>
        )}

        <Collapsible className="mt-2" summary="应用档案会做什么？不会做什么？">
          <p className="notice">
            会做：重建账户联结点、把指定的中转站配置写进各自的工具、
            并在动手<strong>之前先存一份快照</strong>。
          </p>
          <p className="notice mt-2">
            <strong>不会自动改时区。</strong>改时区要管理员权限会弹 UAC ——
            在一个「切换档案」的动作里突然弹 UAC，你会以为程序出了问题。
            档案里的时区与当前不一致时只做提示，请到「环境与安装」页手动切。
          </p>
          <p className="notice mt-2">
            <strong>某一步失败不影响别的步骤</strong>，全部结果一起报回来。
            一步失败就整体回滚听着更干净，实际更糟 —— 你会得到一个
            「什么都没变、也不知道哪一步不行」的结果。
          </p>
          <p className="notice mt-2">
            应用档案会切账户，所以它<strong>只能由你在这里点</strong>：
            没有定时器、没有看门狗、没有任何自动调用点。
          </p>
        </Collapsible>
      </Card>

      {/* ------------------------------------------------------ 快照 */}
      <Card
        title="配置快照"
        icon={<History size={14} />}
        className="mb-3"
        actions={
          <Button
            size="sm"
            icon={<Plus size={12} />}
            loading={busy === 'snap'}
            disabled={!!busy}
            onClick={() => void act('snap', () => api.snapshotCreate('手动创建'), '快照已创建')}
          >
            立即快照
          </Button>
        }
      >
        <p className="notice mb-2">
          收 <code>~/.claude/settings.json</code>、<code>~/.codex/</code> 下两个文件、
          白名单、中转站目录、面板设置与进度，并记下当时的账户、时区与上锁状态。
          保留最近 20 份。
        </p>

        {(snapshots.data ?? []).length === 0 ? (
          <p className="notice">还没有快照。</p>
        ) : (
          (snapshots.data ?? []).map((s) => (
            <Row
              key={s.id}
              side={
                <>
                  <Button
                    size="sm"
                    onClick={() => void api.snapshotDir(s.id).then((d) => openUrl(d))}
                  >
                    打开目录
                  </Button>
                  <Button
                    size="sm"
                    icon={<RotateCcw size={11} />}
                    disabled={!!busy}
                    onClick={() => setAskRestore(s)}
                  >
                    回滚
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    icon={<Trash2 size={11} />}
                    aria-label="删除快照"
                    disabled={!!busy}
                    onClick={() => void act('rm', () => api.snapshotRemove(s.id), '快照已删除')}
                  />
                </>
              }
            >
              <span>
                <span className="font-mono">{s.id}</span>
                {s.manifest.all_locked && <Pill tone="ok">当时已上锁</Pill>}
              </span>
              <span className="notice">
                {s.manifest.note || '—'} · {s.manifest.files.length} 个文件
                {s.manifest.active_account ? ` · 账户 ${s.manifest.active_account}` : ''}
              </span>
            </Row>
          ))
        )}

        <Collapsible className="mt-2" summary="快照里没有什么？">
          <p className="notice">
            <strong>没有凭证文件。</strong>
            快照会被拷来拷去，把 OAuth 凭证复制到第二个地方是在扩大暴露面，
            而它本来就能重新登录拿回来。
          </p>
          <p className="notice mt-2">
            <strong>没有账户槽位目录本身</strong>（它们已经在数据目录里了，
            再复制一份等于把整个账户树翻倍），也没有{' '}
            <code>~/.claude.json</code>（那是会话历史，不是这个面板改的东西）。
            快照只记住当时激活的是哪个槽位。
          </p>
          <p className="notice mt-2">
            快照是<strong>目录复制不是打包</strong>：出问题时可以直接进文件夹
            把文件拷回去，不需要本程序也能恢复。
          </p>
        </Collapsible>
      </Card>

      {/* ---------------------------------------------------- 编辑框 */}
      <Modal
        open={!!edit}
        onClose={() => setEdit(null)}
        title={edit?.id ? `编辑 ${edit.name}` : '新建档案'}
        icon={<Archive size={15} />}
        footer={
          <>
            <Button onClick={() => setEdit(null)}>取消</Button>
            <Button variant="primary" loading={busy === 'save'} onClick={() => void saveProfile()}>
              保存
            </Button>
          </>
        }
      >
        {edit && (
          <>
            <div className="grid gap-3 sm:grid-cols-2">
              <TextField label="名称" value={edit.name} onChange={(v) => setEditField({ name: v })} />
              <TextField
                label="备注"
                value={edit.note ?? ''}
                onChange={(v) => setEditField({ note: v || null })}
              />
            </div>

            <div className="mt-3">
              <span className="field-label">账户槽位</span>
              <div className="mt-1 flex flex-wrap gap-2">
                <Button
                  size="sm"
                  variant={edit.account ? 'default' : 'primary'}
                  onClick={() => setEditField({ account: null })}
                >
                  不动
                </Button>
                {(accounts.data?.slots ?? []).map((s) => (
                  <Button
                    key={s.label}
                    size="sm"
                    variant={edit.account === s.label ? 'primary' : 'default'}
                    onClick={() => setEditField({ account: s.label })}
                  >
                    {s.label}
                  </Button>
                ))}
              </div>
            </div>

            {Object.entries(RELAY_LABEL).map(([target, label]) => {
              const rows = (relays.data ?? []).filter((r) => r.target === target);
              return (
                <div className="mt-3" key={target}>
                  <span className="field-label">{label} 中转站</span>
                  <div className="mt-1 flex flex-wrap gap-2">
                    <Button
                      size="sm"
                      variant={edit.relays?.[target] ? 'default' : 'primary'}
                      onClick={() => {
                        const next = { ...(edit.relays ?? {}) };
                        delete next[target];
                        setEditField({ relays: next });
                      }}
                    >
                      不动
                    </Button>
                    {rows.length === 0 ? (
                      <span className="notice self-center">这个工具还没配过中转站</span>
                    ) : (
                      rows.map((r) => (
                        <Button
                          key={r.id}
                          size="sm"
                          variant={edit.relays?.[target] === r.id ? 'primary' : 'default'}
                          onClick={() =>
                            setEditField({ relays: { ...(edit.relays ?? {}), [target]: r.id } })
                          }
                        >
                          {r.name}
                        </Button>
                      ))
                    )}
                  </div>
                </div>
              );
            })}

            <div className="mt-3">
              <TextField
                label="系统时区"
                hint="Windows 时区名。留空表示不动。应用档案时只做提示，不自动改"
                value={edit.timezone ?? ''}
                onChange={(v) => setEditField({ timezone: v || null })}
              />
            </div>
          </>
        )}
      </Modal>

      {/* ---------------------------------------------------- 确认框 */}
      <ConfirmDialog
        open={!!askApply}
        onCancel={() => setAskApply(null)}
        onConfirm={() => askApply && void apply(askApply)}
        title={`应用档案「${askApply?.name ?? ''}」？`}
        confirmLabel="确认应用"
      >
        <p>
          会重建账户联结点、把指定的中转站写进各自的工具配置。
          <strong>动手之前会先存一份快照</strong>，出问题能整体退回去。
        </p>
        <p className="notice mt-2">
          正在跑的会话不会自动跟着换，要重启才生效。时区不会自动改。
        </p>
      </ConfirmDialog>

      <ConfirmDialog
        open={!!askRestore}
        onCancel={() => setAskRestore(null)}
        onConfirm={() => {
          const s = askRestore;
          setAskRestore(null);
          if (s) void act(`restore-${s.id}`, async () => toast.ok(await api.snapshotRestore(s.id)));
        }}
        title={`回滚到 ${askRestore?.id ?? ''}？`}
        confirmLabel="确认回滚"
        danger
      >
        <p>
          快照里的文件会覆盖现在的，账户也会切回当时那个槽位。
          <strong>回滚之前会先把现状再存一份</strong> —— 回滚错了还能再退回来。
        </p>
        <p className="notice mt-2">
          时区不会自动改（要管理员权限）。当时与现在不一致的话会在结果里提示。
        </p>
      </ConfirmDialog>

      <ConfirmDialog
        open={!!askDelete}
        onCancel={() => setAskDelete(null)}
        onConfirm={() => {
          const p = askDelete;
          setAskDelete(null);
          if (p) void act('del', () => api.profileRemove(p.id), `已删除档案「${p.name}」`);
        }}
        title={`删除档案「${askDelete?.name ?? ''}」？`}
        confirmLabel="确认删除"
        danger
      >
        <p>
          只删这条档案记录。<strong>账户、中转站配置、时区都不会被改动</strong> ——
          档案只是一份「怎么切」的说明，不是配置本身。
        </p>
      </ConfirmDialog>
    </>
  );
}
