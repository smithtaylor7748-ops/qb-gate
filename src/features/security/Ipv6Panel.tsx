import { api } from "../../lib/api";
import { R } from "../../lib/resources";
import {
  getSession,
  invalidate,
  put,
  useResource,
  useSession,
} from "../../lib/store";
import { Button, Pill } from "../../ui";

const BUSY = "purity.ipv6.busy";

export default function Ipv6Panel() {
  const state = useResource("ipv6", R.ipv6);
  const [pending, setPending] = useSession(BUSY, false);
  const [error, setError] = useSession("purity.ipv6.error", "");
  const status = state.data;
  const busy = pending || status?.busy === true;
  const enabled = status?.bindings.filter((b) => b.enabled).length ?? 0;
  const allOff = !!status?.bindings.length && enabled === 0;

  async function apply(disable: boolean) {
    if (getSession(BUSY, false) || status?.busy) return;
    setPending(true);
    setError("");
    try {
      put("ipv6", await api.ipv6Set(disable));
      // 网络配置已改变，旧的检测分数不可继续表示当前状态。
      invalidate("ip", "dns", "signals", "checkup", "egress");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setPending(false);
    }
  }

  return (
    <div className="ipv6-control">
      <label
        className="flex items-center justify-end gap-2"
        title="默认开启，启动时禁用本机网卡 IPv6；关闭时恢复原设置。需要管理员授权，可能短暂断网。"
      >
        禁用 IPv6
        <input
          type="checkbox"
          role="switch"
          className="ipv6-switch"
          aria-label="禁用本机 IPv6"
          checked={status?.disable ?? true}
          disabled={!status || busy}
          onChange={(event) => void apply(event.target.checked)}
        />
      </label>
      <details className="ipv6-details">
        <summary>
          <Pill
            tone={
              error || status?.error || state.error
                ? "danger"
                : busy
                  ? "default"
                  : allOff
                    ? "ok"
                    : "warn"
            }
          >
            {busy
              ? "正在应用，请处理权限提示…"
              : !status
                ? "读取中"
                : !status.bindings.length
                  ? "网卡状态未知"
                  : allOff
                    ? "网卡 IPv6 已禁用"
                    : `${enabled} 张网卡仍启用 IPv6`}
          </Pill>
          <span className="notice">状态与恢复</span>
        </summary>
        <div className="ipv6-popover">
          <p className="notice mt-2">
            默认开启，启动面板时应用。作用于整台电脑的网卡，包括隐藏和虚拟网卡。关闭开关会恢复各网卡原来的设置；退出面板后保留当前状态。
            修改需要管理员授权，可能短暂断网或影响依赖 IPv6 的功能。
          </p>
          {(error || status?.error || state.error) && (
            <p role="alert" className="notice notice--danger mt-2">
              {error || status?.error || String(state.error)}
            </p>
          )}
          {status && !busy && (
            <div className="flex flex-wrap items-center gap-2 mt-2">
              <Button size="sm" onClick={() => void apply(status.disable)}>
                {status.disable ? "重新应用" : "重试恢复"}
              </Button>
              <Button size="sm" onClick={() => void state.refresh()}>
                刷新状态
              </Button>
              {!status.disable && status.restore_pending > 0 && (
                <span className="notice">
                  {status.restore_pending} 张网卡的原设置待恢复
                </span>
              )}
            </div>
          )}
          {status && !status.bindings.length && (
            <p className="notice mt-2">没有读到网卡，尚不能确认是否禁用。</p>
          )}
          {!!status?.bindings.length && (
            <details className="mt-2">
              <summary className="notice">
                查看 {status.bindings.length} 张网卡的实际状态
              </summary>
              <ul className="notice mt-2">
                {status.bindings.map((binding) => (
                  <li key={binding.id}>
                    {binding.name}：
                    {binding.enabled ? "IPv6 启用" : "IPv6 禁用"}
                  </li>
                ))}
              </ul>
            </details>
          )}
        </div>
      </details>
    </div>
  );
}
