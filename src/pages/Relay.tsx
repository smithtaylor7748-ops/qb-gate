import { useEffect, useMemo, useState } from 'react';
import {
  Check,
  ChevronDown,
  ChevronUp,
  Copy,
  Download,
  ExternalLink as ExternalIcon,
  Gauge,
  MoreHorizontal,
  Pencil,
  Plus,
  RotateCw,
  Save,
  Search,
  Trash2,
} from 'lucide-react';

import {
  api,
  type AuthStyle,
  type BackendReport,
  type LatencyResult,
  type Preset,
  type ProviderView,
  type RelayTarget,
  type WireApi,
} from '../lib/api';
import { R } from '../lib/resources';
import { invalidate, useResource, useSession } from '../lib/store';
import { QUICKSTART_DOC } from '../prompts';
import {
  Button,
  Card,
  Collapsible,
  ConfirmDialog,
  EmptyState,
  ExternalLink,
  Modal,
  PageHeader,
  Pill,
  Row,
  TextField,
  useToast,
} from '../ui';

/**
 * 中转站。
 *
 * 三个 target 各一份独立列表，卡片式排布。布局思路参考 Cockpit Tools
 * （标签 + 筛选压密度），但**代码是自己写的** —— 那个项目是
 * CC BY-NC-SA 4.0，抄源码会把本项目从 MIT 拖成同一个协议。
 *
 * Key 从来不会到这个文件里来：后端 `ProviderView` 结构上装不下 Key，
 * 界面上只有掩码和「有没有配」。
 */

const TARGETS: Array<{ id: RelayTarget; label: string; hint: string }> = [
  { id: 'claude-code', label: 'Claude Code', hint: '写 ~/.claude/settings.json 的 env 段' },
  { id: 'claude-desktop', label: 'Claude 桌面端', hint: '与 Claude Code 共用 settings.json' },
  { id: 'codex', label: 'Codex', hint: '写 ~/.codex/config.toml 与 auth.json' },
];

interface Form {
  id: string;
  target: RelayTarget;
  slug: string;
  name: string;
  base_url: string;
  model: string;
  small_fast_model: string;
  wire_api: WireApi;
  auth_style: AuthStyle;
  note: string;
  website: string;
  api_key: string;
}

function blank(target: RelayTarget): Form {
  return {
    id: '',
    target,
    slug: '',
    name: '',
    base_url: '',
    model: '',
    small_fast_model: '',
    wire_api: 'responses',
    auth_style: target === 'codex' ? 'env_key' : 'bearer_token',
    note: '',
    website: '',
    api_key: '',
  };
}

function toForm(p: ProviderView): Form {
  return {
    id: p.id,
    target: p.target,
    slug: p.slug,
    name: p.name,
    base_url: p.base_url,
    model: p.model ?? '',
    small_fast_model: p.small_fast_model ?? '',
    wire_api: p.wire_api,
    auth_style: p.auth_style,
    note: p.note ?? '',
    website: p.website ?? '',
    api_key: '',
  };
}

export default function Relay() {
  const toast = useToast();
  const list = useResource('relay', R.relay);
  const [tab, setTab] = useSession<RelayTarget>('relay.tab', 'claude-code');

  const [form, setForm] = useState<Form | null>(null);
  const [busy, setBusy] = useState('');
  const [askDelete, setAskDelete] = useState<ProviderView | null>(null);
  const [presets, setPresets] = useState<Preset[]>([]);
  const [models, setModels] = useState<string[] | null>(null);
  const [latency, setLatency] = useState<Record<string, LatencyResult>>({});
  const [backends, setBackends] = useState<Record<string, BackendReport>>({});
  /** 哪张卡展开了「更多」。低频操作收起来，常用的三个才留在外面。 */
  const [expanded, setExpanded] = useState<string | null>(null);

  const rows = useMemo(
    () => (list.data ?? []).filter((p) => p.target === tab),
    [list.data, tab]
  );

  useEffect(() => {
    let live = true;
    void api
      .relayPresets(tab)
      .then((p) => live && setPresets(p))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [tab]);

  function set(patch: Partial<Form>) {
    setForm((f) => (f ? { ...f, ...patch } : f));
  }

  async function act(name: string, fn: () => Promise<unknown>, ok?: string) {
    setBusy(name);
    try {
      await fn();
      if (ok) toast.ok(ok);
      invalidate('relay');
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy('');
    }
  }

  async function save() {
    if (!form) return;
    if (!form.name.trim() || !form.base_url.trim()) {
      toast.error('显示名和 Base URL 都要填');
      return;
    }
    await act(
      'save',
      async () => {
        await api.relaySave({
          id: form.id,
          target: form.target,
          slug: form.slug.trim(),
          name: form.name.trim(),
          base_url: form.base_url.trim(),
          model: form.model.trim() || null,
          small_fast_model: form.small_fast_model.trim() || null,
          wire_api: form.wire_api,
          auth_style: form.auth_style,
          note: form.note.trim() || null,
          website: form.website.trim() || null,
          icon: null,
          sort: 0,
          created_at: '',
          api_key: form.api_key.trim() || undefined,
        });
        setForm(null);
        setModels(null);
      },
      '已保存'
    );
  }

  async function fetchModels() {
    if (!form) return;
    setBusy('models');
    try {
      const r = await api.relayFetchModels(form.base_url.trim(), form.id || undefined);
      setModels(r.models);
      toast.ok(r.detail);
    } catch (e) {
      setModels(null);
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy('');
    }
  }

  async function testOne(p: ProviderView) {
    setBusy(`ping-${p.id}`);
    try {
      const r = await api.relayTestLatency(p.base_url, p.id);
      setLatency((m) => ({ ...m, [p.id]: r }));
    } finally {
      setBusy('');
    }
  }

  /**
   * 查真实后端。**会真发一次请求、消耗一点点额度**，所以只有点了才跑，
   * 也没有「全部检测」那种批量入口 —— 批量意味着一次点击烧掉 N 次调用。
   */
  async function detectBackend(p: ProviderView) {
    setBusy(`be-${p.id}`);
    try {
      const r = await api.relayDetectBackend(p.base_url, p.id, p.model ?? undefined);
      setBackends((m) => ({ ...m, [p.id]: r }));
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy('');
    }
  }

  async function testAll() {
    setBusy('ping-all');
    try {
      // 串行跑。并发打同一批端点容易触发对方的限流，
      // 测出来的数字反而不准。
      for (const p of rows) {
        const r = await api.relayTestLatency(p.base_url, p.id);
        setLatency((m) => ({ ...m, [p.id]: r }));
      }
    } finally {
      setBusy('');
    }
  }

  async function move(p: ProviderView, dir: -1 | 1) {
    const ids = rows.map((r) => r.id);
    const i = ids.indexOf(p.id);
    const j = i + dir;
    if (i < 0 || j < 0 || j >= ids.length) return;
    [ids[i], ids[j]] = [ids[j], ids[i]];
    await act('reorder', () => api.relayReorder(tab, ids));
  }

  async function importLive() {
    setBusy('import');
    try {
      const m = await api.relayImportLive(tab);
      if (!m) {
        toast.info('这个工具的配置里还没有中转端点');
        return;
      }
      setForm({
        ...blank(tab),
        slug: m.slug,
        name: m.name,
        base_url: m.base_url,
        model: m.model ?? '',
        wire_api: m.wire_api,
        auth_style: m.auth_style,
      });
      toast.ok('已读入当前配置，确认后保存');
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy('');
    }
  }

  const tabMeta = TARGETS.find((t) => t.id === tab)!;

  return (
    <>
      <PageHeader
        title="中转站"
        sub="三个工具各一份供应商列表，一键切换。写入前自动备份，原有配置项保留。"
        actions={
          <>
            <Button
              icon={<RotateCw size={13} />}
              loading={list.loading}
              onClick={() => void list.refresh()}
            >
              刷新
            </Button>
            <Button
              variant="primary"
              icon={<Plus size={13} />}
              onClick={() => {
                setForm(blank(tab));
                setModels(null);
              }}
            >
              添加
            </Button>
          </>
        }
      />

      {/* --------------------------------------------------- 三个应用分栏 */}
      <div className="mb-3 flex flex-wrap gap-2">
        {TARGETS.map((t) => {
          const n = (list.data ?? []).filter((p) => p.target === t.id).length;
          return (
            <Button
              key={t.id}
              size="sm"
              variant={tab === t.id ? 'primary' : 'default'}
              onClick={() => setTab(t.id)}
            >
              {t.label}
              {n > 0 && ` · ${n}`}
            </Button>
          );
        })}
      </div>

      {list.error && <p className="notice notice--danger mb-3">{list.error}</p>}

      <Card
        title={tabMeta.label}
        className="mb-3"
        actions={
          <>
            <Button size="sm" disabled={!!busy} onClick={() => void importLive()}>
              从当前配置导入
            </Button>
            <Button
              size="sm"
              icon={<Gauge size={12} />}
              loading={busy === 'ping-all'}
              disabled={!!busy || rows.length === 0}
              onClick={() => void testAll()}
            >
              全部测速
            </Button>
          </>
        }
      >
        <p className="notice mb-2">{tabMeta.hint}</p>

        {rows.length === 0 ? (
          <EmptyState
            title={`${tabMeta.label} 还没有配过供应商`}
            action={
              <Button
                variant="primary"
                icon={<Plus size={13} />}
                onClick={() => setForm(blank(tab))}
              >
                添加一个
              </Button>
            }
          >
            也可以点上面的「从当前配置导入」，把这个工具现在用的端点读进来。
          </EmptyState>
        ) : (
          <div className="space-y-2">
            {rows.map((p, i) => {
              const ping = latency[p.id];
              const be = backends[p.id];
              const open = expanded === p.id;
              return (
                <div
                  key={p.id}
                  className={`relative overflow-hidden rounded-md border pl-3 ${
                    p.active ? 'border-accent-line bg-accent-bg' : 'border-line'
                  }`}
                >
                  {/* 当前启用的那条左边挂一条高亮竖条 —— 一眼就能扫到，
                      不用去读每张卡上的小 pill。 */}
                  {p.active && (
                    <span
                      aria-hidden="true"
                      className="absolute left-0 top-0 h-full w-1 bg-[var(--accent)]"
                    />
                  )}

                  <div className="p-3">
                    {/* ---- 第一行：名字 + 主操作。其余一概往下放 ---- */}
                    <div className="flex items-center gap-2">
                      <span className="min-w-0 flex-1 truncate text-md font-medium">{p.name}</span>
                      {p.active ? (
                        <Pill tone="accent" icon={<Check size={11} />}>
                          当前启用
                        </Pill>
                      ) : (
                        <Button
                          size="sm"
                          variant="primary"
                          loading={busy === `on-${p.id}`}
                          disabled={!!busy}
                          onClick={() =>
                            void act(`on-${p.id}`, () => api.relayActivate(tab, p.id), `已切换到 ${p.name}`)
                          }
                        >
                          启用
                        </Button>
                      )}
                    </div>

                    {/* ---- 第二行：地址占左边，凭证靠右 —— 用上宽度，别让右半边空着 ---- */}
                    <div className="mt-1.5 flex items-start gap-3">
                      <span className="min-w-0 flex-1 break-all font-mono text-xs text-[var(--text-2)]">
                        {p.base_url}
                      </span>
                      <span className="flex flex-shrink-0 items-center gap-1">
                        {p.target === 'codex' && <Pill tone="default">{p.wire_api}</Pill>}
                        {p.has_key ? (
                          <Pill tone={p.key_encrypted ? 'ok' : 'warn'}>
                            {p.key_masked ?? '已配 Key'}
                          </Pill>
                        ) : (
                          <Pill tone="default">未配 Key</Pill>
                        )}
                      </span>
                    </div>

                    {p.has_key && !p.key_encrypted && (
                      <p className="notice notice--warn mt-1">
                        这条的 Key 是明文存的（早期版本或手改留下的）。保存一次就会转成加密。
                      </p>
                    )}
                    {p.note && <p className="notice mt-1">{p.note}</p>}

                    {/* ---- 实测结果单独一栏：这些是**跑出来的**，不是你填的 ---- */}
                    {(ping || be) && (
                      <div className="mt-2 flex flex-wrap items-center gap-1.5 border-t border-[var(--border)] pt-2">
                        <span className="text-xs text-[var(--text-3)]">实测</span>
                        {ping && (
                          <Pill tone={ping.ok ? 'ok' : 'danger'} title={ping.detail}>
                            {ping.ms != null ? `${ping.ms} ms` : '连不上'}
                          </Pill>
                        )}
                        {be && <BackendPill r={be} />}
                      </div>
                    )}
                    {be && <BackendEvidence r={be} />}

                    {/* ---- 第三行：模型配置在左，操作在右。常用的三个留在外面，
                         低频的（复制 / 排序 / 删除）收进「更多」—— 原来七个按钮
                         挤一排，而每天真正会点的只有「启用」和「编辑」。 ---- */}
                    <div className="mt-2 flex flex-wrap items-center justify-between gap-2">
                      <span className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
                        <span className="text-[var(--text-3)]">
                          主模型{' '}
                          <span className="font-mono text-[var(--text)]">{p.model || '未设'}</span>
                        </span>
                        <span className="text-[var(--text-3)]">
                          小模型{' '}
                          <span className="font-mono text-[var(--text)]">
                            {p.small_fast_model || '未设'}
                          </span>
                        </span>
                      </span>
                      <span className="flex flex-wrap items-center gap-1">
                      <Button size="sm" icon={<Pencil size={11} />} onClick={() => setForm(toForm(p))}>
                        编辑
                      </Button>
                      <Button
                        size="sm"
                        icon={<Gauge size={11} />}
                        loading={busy === `ping-${p.id}`}
                        disabled={!!busy}
                        onClick={() => void testOne(p)}
                      >
                        测速
                      </Button>
                      <Button
                        size="sm"
                        icon={<Search size={11} />}
                        loading={busy === `be-${p.id}`}
                        disabled={!!busy}
                        title="真发一次请求看响应头，会消耗一点点额度"
                        onClick={() => void detectBackend(p)}
                      >
                        验后端
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        icon={<MoreHorizontal size={11} />}
                        aria-expanded={open}
                        onClick={() => setExpanded(open ? null : p.id)}
                      >
                        更多
                      </Button>
                      </span>
                    </div>

                    {open && (
                      <div className="mt-1 flex flex-wrap items-center gap-1">
                        <Button
                          size="sm"
                          icon={<Copy size={11} />}
                          disabled={!!busy}
                          onClick={() =>
                            void act('dup', () => api.relayDuplicate(p.id), `已复制 ${p.name}`)
                          }
                        >
                          复制
                        </Button>
                        <Button
                          size="sm"
                          icon={<ChevronUp size={11} />}
                          aria-label="上移"
                          disabled={!!busy || i === 0}
                          onClick={() => void move(p, -1)}
                        />
                        <Button
                          size="sm"
                          icon={<ChevronDown size={11} />}
                          aria-label="下移"
                          disabled={!!busy || i === rows.length - 1}
                          onClick={() => void move(p, 1)}
                        />
                        <Button
                          size="sm"
                          variant="danger"
                          icon={<Trash2 size={11} />}
                          disabled={!!busy}
                          onClick={() => setAskDelete(p)}
                        >
                          删除
                        </Button>
                      </div>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </Card>

      {/* ------------------------------------------------------- 预设 */}
      <Card title="预设" className="mb-3">
        <p className="notice mb-2">
          这里<strong>只放厂商自己文档里公开的官方端点</strong>。
          第三方中转站的地址各家自己在变，预置一个记错的地址会让你先怀疑 Key
          而不是怀疑地址 —— 所以那些请自己填。
        </p>
        {presets.length === 0 ? (
          <p className="notice">这个工具暂时没有内置预设。</p>
        ) : (
          presets.map((p) => (
            <Row
              key={p.id}
              side={
                <Button
                  size="sm"
                  onClick={() =>
                    setForm({
                      ...blank(tab),
                      name: p.name,
                      base_url: p.base_url,
                      wire_api: p.wire_api,
                      auth_style: p.auth_style,
                      model: p.model ?? '',
                      website: p.website ?? '',
                      note: p.note,
                    })
                  }
                >
                  填入
                </Button>
              }
            >
              <span>
                {p.name}
                {p.target === 'codex' && <Pill tone="default">{p.wire_api}</Pill>}
              </span>
              <span className="notice break-all font-mono">{p.base_url}</span>
              <span className="notice">{p.note}</span>
            </Row>
          ))
        )}
      </Card>

      <Card tone="warn" className="mb-3">
        <p className="notice notice--warn">
          走中转端点时请留意：据第三方逆向分析主张（<strong>未经证实</strong>），
          Claude Code 在 <code>ANTHROPIC_BASE_URL</code> 指向中转端点时会读取系统
          时区与中转 hostname。官方 OAuth 直连路径不在该描述范围内。
        </p>
      </Card>

      <Collapsible summary="Key 存在哪里？备份呢？">
        <p className="notice">
          Key 用 <strong>Windows DPAPI 加密</strong>后存在{' '}
          <code>%LOCALAPPDATA%\ClaudeIpGate\relay.json</code>。密文只有
          <strong>同一个 Windows 用户在同一台机器上</strong>解得开 ——
          文件被拷走就是一串没用的十六进制。
        </p>
        <p className="notice mt-2">
          界面上永远只有掩码。后端回给前端的结构<strong>装不下 Key</strong>，
          不是靠「记得加 skip_serializing」，是结构上就没有那个字段。
        </p>
        <p className="notice mt-2">
          写目标工具的配置前会先存一份<strong>带时间戳的备份</strong>到{' '}
          <code>relay-backups\</code>，保留最近 20 份。旧版只有一个{' '}
          <code>.bak</code>，写第二次就把第一次的备份盖掉了。
        </p>
        <p className="notice mt-2">
          Codex 的 <code>auth.json</code> 是<strong>并入写</strong>，不会动你的
          官方登录凭证；<code>config.toml</code> 里你自己加的{' '}
          <code>[projects.*]</code>、<code>[mcp_servers.*]</code> 也都保留。
        </p>
      </Collapsible>

      <div className="mt-3">
        <ExternalLink href={QUICKSTART_DOC} asButton>
          新手指引文档
        </ExternalLink>
      </div>

      {/* ------------------------------------------------------- 编辑框 */}
      <Modal
        open={!!form}
        onClose={() => setForm(null)}
        title={form?.id ? `编辑 ${form.name}` : `添加到 ${tabMeta.label}`}
        footer={
          <>
            <Button onClick={() => setForm(null)}>取消</Button>
            <Button
              variant="primary"
              icon={<Save size={13} />}
              loading={busy === 'save'}
              onClick={() => void save()}
            >
              保存
            </Button>
          </>
        }
      >
        {form && (
          <>
            <div className="grid gap-3 sm:grid-cols-2">
              <TextField label="显示名" value={form.name} onChange={(v) => set({ name: v })} />
              <TextField
                label="标识 slug"
                hint="留空自动生成。Codex 用它做 model_providers 的键"
                value={form.slug}
                onChange={(v) => set({ slug: v })}
              />
            </div>

            <div className="mt-3">
              <TextField
                label="Base URL"
                value={form.base_url}
                onChange={(v) => set({ base_url: v })}
                placeholder={form.target === 'codex' ? 'https://api.example.com/v1' : 'https://api.example.com'}
                hint={
                  form.target === 'codex'
                    ? 'Codex 侧通常要带 /v1'
                    : 'Claude 侧通常不带 /v1'
                }
              />
            </div>

            <div className="mt-3 grid gap-3 sm:grid-cols-2">
              <div>
                <TextField
                  label="默认模型"
                  hint="可以留空，由目标工具自己决定"
                  value={form.model}
                  onChange={(v) => set({ model: v })}
                />
                <Button
                  className="mt-1"
                  size="sm"
                  icon={<Download size={11} />}
                  loading={busy === 'models'}
                  disabled={!!busy || !form.base_url.trim()}
                  onClick={() => void fetchModels()}
                >
                  从端点获取模型
                </Button>
              </div>
              <TextField
                label="小模型 / 快模型"
                hint="落到 ANTHROPIC_SMALL_FAST_MODEL。留空 = 不设，用目标工具的默认值"
                value={form.small_fast_model}
                onChange={(v) => set({ small_fast_model: v })}
              />
            </div>

            <div className="mt-3 grid gap-3 sm:grid-cols-2">
              <TextField
                label="API Key"
                type="password"
                hint={form.id ? '留空则不改动现有 Key' : '会用 DPAPI 加密后存本地'}
                value={form.api_key}
                onChange={(v) => set({ api_key: v })}
              />
            </div>

            {models && (
              <div className="mt-2">
                <span className="field-label">端点返回的模型</span>
                {models.length === 0 ? (
                  <p className="notice">端点通了，但没返回任何模型。模型 id 需要手填。</p>
                ) : (
                  <div className="mt-1 flex max-h-32 flex-wrap gap-1 overflow-y-auto">
                    {models.map((m) => (
                      <Button key={m} size="sm" onClick={() => set({ model: m })}>
                        {m}
                      </Button>
                    ))}
                  </div>
                )}
              </div>
            )}

            {/* 这两个以前是写死的，会把实机在用的配置改坏 —— 必须由用户选。 */}
            <div className="mt-3 grid gap-3 sm:grid-cols-2">
              {form.target === 'codex' && (
                <div>
                  <span className="field-label">上游协议</span>
                  <div className="mt-1 flex gap-2">
                    {(['responses', 'chat'] as const).map((w) => (
                      <Button
                        key={w}
                        size="sm"
                        variant={form.wire_api === w ? 'primary' : 'default'}
                        onClick={() => set({ wire_api: w })}
                      >
                        {w}
                      </Button>
                    ))}
                  </div>
                  <span className="field-hint">
                    大多数中转站是 responses。选错了对方解析不了请求体。
                  </span>
                </div>
              )}
              <div>
                <span className="field-label">凭证写在哪</span>
                <div className="mt-1 flex flex-wrap gap-2">
                  {(
                    [
                      ['env_key', form.target === 'codex' ? 'auth.json' : 'API_KEY'],
                      ['bearer_token', form.target === 'codex' ? 'config.toml' : 'AUTH_TOKEN'],
                      ['none', '不写'],
                    ] as const
                  ).map(([style, label]) => (
                    <Button
                      key={style}
                      size="sm"
                      variant={form.auth_style === style ? 'primary' : 'default'}
                      onClick={() => set({ auth_style: style })}
                    >
                      {label}
                    </Button>
                  ))}
                </div>
                <span className="field-hint">
                  {form.target === 'codex'
                    ? 'auth.json 是并入写，不会动你的 Codex 官方登录。'
                    : '两个环境变量只会留一个，避免「改了没生效」。'}
                </span>
              </div>
            </div>

            <div className="mt-3 grid gap-3 sm:grid-cols-2">
              <TextField label="备注" value={form.note} onChange={(v) => set({ note: v })} />
              <TextField
                label="网站"
                hint="供应商控制台地址"
                value={form.website}
                onChange={(v) => set({ website: v })}
              />
            </div>

            {form.website.trim() && (
              <div className="mt-2">
                <ExternalLink href={form.website.trim()} asButton>
                  <ExternalIcon size={12} /> 打开控制台
                </ExternalLink>
              </div>
            )}
          </>
        )}
      </Modal>

      <ConfirmDialog
        open={!!askDelete}
        onCancel={() => setAskDelete(null)}
        onConfirm={() => {
          const p = askDelete;
          setAskDelete(null);
          if (p) void act('del', () => api.relayDelete(p.id), `已删除 ${p.name}`);
        }}
        title={`删除 ${askDelete?.name ?? ''}？`}
        confirmLabel="确认删除"
        danger
      >
        <p>这条记录连同它保存的 Key 一起删掉，删了恢复不了。</p>
        {askDelete?.active && (
          <p className="notice notice--warn mt-2">
            这是<strong>当前启用</strong>的那条。删掉不会改动{' '}
            {TARGETS.find((t) => t.id === askDelete.target)?.label}{' '}
            已经写好的配置文件，那个工具会继续用现在的端点 ——
            只是面板这边不再有这条记录。
          </p>
        )}
      </ConfirmDialog>
    </>
  );
}

const BACKEND_LABEL: Record<BackendReport['backend'], string> = {
  anthropic: 'Anthropic 官方',
  bedrock: 'AWS Bedrock',
  vertex: 'Google Vertex',
  unsure: '说不准',
};

/**
 * 后端徽章。
 *
 * 「说不准」是一等结论，不是加载失败 —— 证据不够就得这么显示，
 * 跟纯净度那套「拿不到的字段如实报未知，不猜成通过」是同一条。
 */
function BackendPill({ r }: { r: BackendReport }) {
  const tone =
    r.backend === 'anthropic' ? (r.confidence === 'strong' ? 'ok' : 'warn') :
    r.backend === 'unsure' ? 'default' : 'danger';
  return (
    <Pill tone={tone} title={r.detail}>
      {BACKEND_LABEL[r.backend]}
      {r.source ? ` · ${r.source}` : ''}
      {r.confidence === 'weak' && r.backend !== 'unsure' ? '（存疑）' : ''}
    </Pill>
  );
}

/** 证据逐条列出来 —— 只给结论的话，使用者没法自己复核，也就没法反驳。 */
function BackendEvidence({ r }: { r: BackendReport }) {
  return (
    <Collapsible className="mt-1" summary={`后端判定依据（${r.evidence.length} 条）`}>
      <p className="notice">{r.detail}</p>
      <ul className="mt-1">
        {r.evidence.map((e, i) => (
          <li key={i} className="notice">
            · {e}
          </li>
        ))}
      </ul>
      {r.ratelimit_real === false && (
        <p className="notice notice--warn mt-1">
          限流头两次请求之间一动不动，<strong>疑似写死的假头</strong>。
          真的官方直连不会这样。
        </p>
      )}
      <p className="notice mt-1">
        判定看的是响应头指纹与「该有却没有」的负证据。
        中转站换一层实现就可能变，<strong>这不是权威结论</strong>，
        只是给你一个自己去问对方的由头。
      </p>
    </Collapsible>
  );
}
