import { createElement, useEffect, useState } from 'react';
import { Blocks, RotateCw } from 'lucide-react';

import { R } from '../lib/resources';
import { useResource } from '../lib/store';
import { api } from '../lib/api';
import { PLUGINS } from '../plugins/registry';
import {
  Button,
  Card,
  Collapsible,
  EmptyState,
  ExternalLink,
  PageHeader,
  Pill,
  Row,
  PLUGIN_STATE_LABEL,
  PLUGIN_STATE_TONE,
} from '../ui';

export default function Plugins() {
  const status = useResource('plugins', R.plugins);
  const [catalog, setCatalog] = useState<{ configured: boolean; signed: boolean; detail: string } | null>(null);
  const [open, setOpen] = useState<string | null>('sillytavern');

  useEffect(() => {
    let live = true;
    void api.pluginCatalogStatus().then((s) => live && setCatalog(s)).catch(() => undefined);
    return () => { live = false; };
  }, []);

  return (
    <>
      <PageHeader
        title="插件商店"
        sub="把外部工具接进 QB Gate，共用同一套 IP 门禁与看门狗。"
        actions={
          <Button
            icon={<RotateCw size={13} />}
            loading={status.loading}
            onClick={() => void status.refresh()}
          >
            刷新
          </Button>
        }
      />

      {status.error && <p className="notice notice--danger mb-3">{status.error}</p>}

      <Card title="官方清单" className="mb-3">
        <p className="notice">{catalog?.detail ?? '正在读取清单状态…'}</p>
        {!catalog?.configured && <p className="notice mt-1">清单仓库创建并完成签名配置后，这里会提供刷新、安装、升级、启停和卸载入口。</p>}
      </Card>

      {PLUGINS.map((meta) => {
        const st = status.data?.find((s) => s.id === meta.id);
        const isOpen = open === meta.id;
        return (
          <Card
            key={meta.id}
            title={meta.name}
            icon={<Blocks size={14} />}
            className="mb-3"
            actions={
              <>
                {st && <Pill tone={PLUGIN_STATE_TONE[st.state]}>{PLUGIN_STATE_LABEL[st.state]}</Pill>}
                <Button
                  size="sm"
                  aria-expanded={isOpen}
                  onClick={() => setOpen(isOpen ? null : meta.id)}
                >
                  {isOpen ? '收起' : '展开'}
                </Button>
              </>
            }
          >
            <p className="notice">{meta.blurb}</p>
            {st && <p className="notice mt-1">{st.detail}</p>}

            {meta.upstream && (
              <p className="notice mt-2">
                上游：
                <ExternalLink href={meta.upstream.url}>{meta.upstream.label}</ExternalLink>
                {' · '}
                {meta.upstream.license}
                {meta.upstream.license.startsWith('AGPL') && <Pill tone="warn">AGPL</Pill>}
              </p>
            )}

            {meta.upstream?.license.startsWith('AGPL') && (
              <Collapsible className="mt-1" summary="AGPL 对我有什么要求？">
                <p className="notice">
                  本面板<strong>只是启动与管理它，没有修改其源码</strong>，
                  也没有链接进来，所以 QB Gate 自己仍然是 MIT。
                </p>
                <p className="notice mt-2">
                  但如果<strong>你</strong>改了酒馆的源码并拿它对外提供网络服务，
                  AGPL 要求你公开修改后的源码。默认部署是本机回环
                  （<code>127.0.0.1</code>）只有自己用，不触发这一条；
                  一旦暴露到公网就要重新评估。
                </p>
              </Collapsible>
            )}

            {isOpen && (
              <>
                {st && !!st.checks.length && (
                  <div className="mt-3">
                    {st.checks.map((c) => (
                      <Row
                        key={c.label}
                        side={c.ok ? <Pill tone="ok">就绪</Pill> : <Pill tone="danger">缺</Pill>}
                      >
                        <span>{c.label}</span>
                        <span className="notice break-all font-mono text-xs">{c.detail}</span>
                      </Row>
                    ))}
                  </div>
                )}
                <div className="mt-3">{createElement(meta.panel)}</div>
              </>
            )}
          </Card>
        );
      })}

      {PLUGINS.length === 0 && (
        <Card>
          <EmptyState icon={<Blocks size={22} />} title="还没有可用的插件" />
        </Card>
      )}

      <Collapsible summary="以后还会有别的插件吗？">
        <p className="notice">
          插件清单的格式已经按「日后从远程 index 拉」设计好了，
          调用方只认那个形状、不认具体插件。
          远程插件市场这一轮没做，接口留着。
        </p>
      </Collapsible>
    </>
  );
}
