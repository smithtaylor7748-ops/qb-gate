import { useState } from 'react';
import { RotateCw, Save } from 'lucide-react';

import { api, type AuthStyle, type Provider, type WireApi } from '../lib/api';
import { R } from '../lib/resources';
import { invalidate, useResource } from '../lib/store';
import { QUICKSTART_DOC } from '../prompts';
import {
  Button,
  Card,
  Collapsible,
  EmptyState,
  ExternalLink,
  PageHeader,
  Pill,
  Row,
  TextField,
  useToast,
} from '../ui';

/** 常见预设。用户可以改成任何中转站。 */
const PRESETS: Array<Pick<Provider, 'id' | 'name' | 'base_url'>> = [
  { id: 'sulianyan', name: '速联言', base_url: 'https://api.sulianyan.com/v1' },
  { id: 'openai', name: 'OpenAI 官方', base_url: 'https://api.openai.com/v1' },
];

interface Form {
  target: 'codex' | 'claude';
  id: string;
  name: string;
  base_url: string;
  model: string;
  api_key: string;
  wire_api: WireApi;
  auth_style: AuthStyle;
}

const BLANK: Form = {
  target: 'codex',
  id: 'sulianyan',
  name: '速联言',
  base_url: 'https://api.sulianyan.com/v1',
  model: '',
  api_key: '',
  wire_api: 'responses',
  auth_style: 'env_key',
};

const TARGET_LABEL: Record<'codex' | 'claude', string> = {
  codex: 'Codex',
  claude: 'Claude Code',
};

function toForm(p: Provider): Form {
  return {
    target: p.target ?? 'codex',
    id: p.id,
    name: p.name,
    base_url: p.base_url,
    model: p.model ?? '',
    api_key: '',
    wire_api: p.wire_api ?? 'responses',
    auth_style: p.auth_style ?? 'env_key',
  };
}

export default function Relay() {
  const toast = useToast();
  const cur = useResource('relay', R.relay);
  const [form, setForm] = useState<Form | null>(null);
  const [busy, setBusy] = useState(false);

  // 每个 target 各一条。后端不再「读到 Claude 就提前返回」，
  // 所以 Codex 那条也看得见了。
  const list = cur.data ?? [];

  // 首次拿到当前配置时用它填表单，之后以用户的编辑为准。
  const f = form ?? (list[0] ? toForm(list[0]) : BLANK);

  function set(patch: Partial<Form>) {
    setForm({ ...f, ...patch });
  }

  async function save() {
    setBusy(true);
    try {
      await api.relayApply({
        target: f.target,
        id: f.id.trim(),
        name: f.name.trim(),
        base_url: f.base_url.trim(),
        model: f.model.trim() || undefined,
        wire_api: f.wire_api,
        auth_style: f.auth_style,
        api_key: f.api_key.trim() || undefined,
      });
      toast.ok(f.target === 'claude' ? '已写入 Claude 配置，原有配置项保留，并已自动备份' : '已写入 Codex 配置，原有配置项保留，并已自动备份');
      setForm({ ...f, api_key: '' });
      invalidate('relay');
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <PageHeader
        title="中转站"
        sub="给 Codex 配置模型供应商，供「DNS 泄露 · 高级通过」使用。"
        actions={
          <Button icon={<RotateCw size={13} />} loading={cur.loading} onClick={() => void cur.refresh()}>
            刷新
          </Button>
        }
      />

      <Card title="当前配置" className="mb-3">
        {list.length > 0 ? (
          list.map((p) => {
            const target = p.target ?? 'codex';
            return (
              <Row
                key={target}
                side={
                  <>
                    <Pill tone="default">{p.wire_api ?? 'responses'}</Pill>
                    <Button size="sm" onClick={() => setForm(toForm(p))}>
                      编辑
                    </Button>
                  </>
                }
              >
                <span>
                  <Pill tone="accent">{TARGET_LABEL[target]}</Pill>{' '}
                  <span className="text-md">{p.name}</span>
                </span>
                <span className="break-all font-mono">{p.base_url}</span>
                <span className="notice">模型 {p.model ?? '未指定'}</span>
              </Row>
            );
          })
        ) : (
          <EmptyState title="还没有配置过供应商">
            在下面填好 Base URL 与 API Key，保存之后对应的工具就能用了。
          </EmptyState>
        )}
      </Card>

      <Card title="预设" className="mb-3">
        {PRESETS.map((p) => (
          <Row
            key={p.id}
            side={
              <Button size="sm" onClick={() => set({ id: p.id, name: p.name, base_url: p.base_url })}>
                填入
              </Button>
            }
          >
            <span>{p.name}</span>
            <span className="notice break-all font-mono">{p.base_url}</span>
          </Row>
        ))}
      </Card>

      <Card title="编辑" className="mb-3">
        <div className="mb-3 flex gap-2">
          <Button size="sm" variant={f.target === 'codex' ? 'primary' : 'default'} onClick={() => set({ target: 'codex' })}>Codex</Button>
          <Button size="sm" variant={f.target === 'claude' ? 'primary' : 'default'} onClick={() => set({ target: 'claude' })}>Claude</Button>
        </div>
        {f.target === 'claude' && <p className="notice notice--warn mb-3">Claude 目标只写入官方支持的 <code>~/.claude/settings.json</code> 环境变量。请填写你已获授权使用的中转端点；面板不会提供虚构地址或绕过官方登录。</p>}
        <div className="grid gap-3 sm:grid-cols-2">
          <TextField label="标识 id" value={f.id} onChange={(v) => set({ id: v })} placeholder="sulianyan" />
          <TextField label="显示名" value={f.name} onChange={(v) => set({ name: v })} />
        </div>
        <div className="mt-3">
          <TextField
            label="Base URL"
            value={f.base_url}
            onChange={(v) => set({ base_url: v })}
            placeholder="https://api.example.com/v1"
          />
        </div>
        <div className="mt-3 grid gap-3 sm:grid-cols-2">
          <TextField
            label="默认模型"
            hint="可以留空，由目标工具自己决定"
            value={f.model}
            onChange={(v) => set({ model: v })}
          />
          <TextField
            label="API Key"
            type="password"
            hint="留空则不改动现有 Key"
            value={f.api_key}
            onChange={(v) => set({ api_key: v })}
          />
        </div>

        {/* 这两个选项以前是写死的（chat + OPENAI_API_KEY），会把实机在用的
            responses 中转站改坏 —— 所以现在必须由用户选。 */}
        <div className="mt-3 grid gap-3 sm:grid-cols-2">
          {f.target === 'codex' && (
            <div>
              <span className="field-label">上游协议</span>
              <div className="mt-1 flex gap-2">
                {(['responses', 'chat'] as const).map((w) => (
                  <Button
                    key={w}
                    size="sm"
                    variant={f.wire_api === w ? 'primary' : 'default'}
                    onClick={() => set({ wire_api: w })}
                  >
                    {w}
                  </Button>
                ))}
              </div>
              <span className="field-hint">
                大多数中转站是 responses。填错了连不上，但不会弄坏别的配置。
              </span>
            </div>
          )}
          <div>
            <span className="field-label">凭证写在哪</span>
            <div className="mt-1 flex flex-wrap gap-2">
              {(
                [
                  ['env_key', f.target === 'codex' ? 'auth.json' : 'API_KEY'],
                  ['bearer_token', f.target === 'codex' ? 'config.toml' : 'AUTH_TOKEN'],
                  ['none', '不写'],
                ] as const
              ).map(([style, label]) => (
                <Button
                  key={style}
                  size="sm"
                  variant={f.auth_style === style ? 'primary' : 'default'}
                  onClick={() => set({ auth_style: style })}
                >
                  {label}
                </Button>
              ))}
            </div>
            <span className="field-hint">
              {f.target === 'codex'
                ? 'auth.json 会并入写，不会动你的 Codex 登录凭证。'
                : '两个环境变量只会留一个，避免「改了没生效」。'}
            </span>
          </div>
        </div>

        <Button
          className="mt-3"
          variant="primary"
          icon={<Save size={13} />}
          loading={busy}
          disabled={!f.base_url.trim()}
          onClick={save}
        >
          保存并切换
        </Button>

        <Collapsible className="mt-2" summary="Key 会存到哪里？">
          <p className="notice">
            按上面「凭证写在哪」决定：Codex 是并入 <code>~/.codex/auth.json</code>{' '}
            或写进 <code>config.toml</code> 的 provider 段；Claude Code 是{' '}
            <code>~/.claude/settings.json</code> 的 <code>env</code> 段。
            <strong>面板本身不保存明文</strong>，读回来时也不会把完整 Key 传回界面 ——
            后端那个字段带 <code>skip_serializing</code>，有单测钉着。
          </p>
          <p className="notice mt-2">
            <strong>auth.json 是并入写，不是整份覆盖。</strong>
            覆盖会把 Codex 官方登录留下的 <code>tokens</code> 和 <code>type</code>{' '}
            一起抹掉，等于把你从 Codex 登出 —— 这是修掉的三个真 bug 之一。
          </p>
          <p className="notice mt-2">
            配置文件用增量方式改写，你自己加的其它配置项（
            <code>[projects.*]</code>、<code>[mcp_servers.*]</code> 这些）会保留；
            写入前自动备份，原子写。
          </p>
        </Collapsible>
      </Card>

      <Card tone="warn">
        <p className="notice notice--warn">
          走中转端点时请留意：据第三方逆向分析主张（<strong>未经证实</strong>），
          Claude Code 在 <code>ANTHROPIC_BASE_URL</code> 指向中转端点时会读取系统
          时区与中转 hostname。官方 OAuth 直连路径不在该描述范围内。
        </p>
      </Card>

      <div className="mt-3">
        <ExternalLink href={QUICKSTART_DOC} asButton>
          新手指引文档
        </ExternalLink>
      </div>
    </>
  );
}
