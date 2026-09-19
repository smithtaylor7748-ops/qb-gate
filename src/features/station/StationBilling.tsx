import { useEffect, useState } from "react";
import {
  CheckCircle2,
  Globe2,
  KeyRound,
  LockKeyhole,
  RefreshCw,
} from "lucide-react";
import { call } from "../../lib/ipc";
import type { StationBillingSettings } from "../../lib/generated/StationBillingSettings";
import { Button } from "../../ui";

export default function StationBilling({
  stationId,
  disabled,
  onChange,
}: {
  stationId: string;
  disabled: boolean;
  onChange: (value: StationBillingSettings | null) => void;
}) {
  const [settings, setSettings] = useState<StationBillingSettings | null>(null);
  const [mode, setMode] = useState("password");
  const [backend, setBackend] = useState("auto");
  const [account, setAccount] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  useEffect(() => {
    let live = true;
    setPassword("");
    setMessage("");
    setError("");
    setLoading(true);
    call<StationBillingSettings>("station_billing_settings", { stationId })
      .then((v) => {
        if (live) {
          setSettings(v);
          onChange(v);
        }
      })
      .catch((e) => {
        if (live) setError(String(e));
      })
      .finally(() => {
        if (live) setLoading(false);
      });
    return () => {
      live = false;
    };
  }, [stationId, onChange]);
  const connect = async (command: string) => {
    setBusy(true);
    setError("");
    setMessage(
      command === "station_billing_browser"
        ? "请在独立窗口登录当前站点；可选择 Linux DO，完成后会自动返回。"
        : "正在验证登录并读取账单…",
    );
    const args =
      command === "station_billing_connect"
        ? { stationId, backend, account, password }
        : { stationId };
    setPassword("");
    try {
      const value = await call<StationBillingSettings>(command, args);
      setSettings(value);
      onChange(value);
      setEditing(false);
      setMessage("后台已连接，账单读取已验证。");
    } catch (e) {
      setMessage("");
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };
  const disconnect = async () => {
    setBusy(true);
    setError("");
    try {
      const value = await call<StationBillingSettings>("station_billing_save", {
        stationId,
        backend: "none",
        userId: "",
        token: null,
      });
      setSettings(value);
      onChange(value);
      setMessage("本机账单登录已移除。");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const locked = disabled || busy || loading;
  return (
    <section className="audit-login">
      <div className="audit-section-heading">
        <span className="audit-step">01</span>
        <div>
          <h3>连接后台账单</h3>
          <p>使用中转站网站的登录账户</p>
        </div>
        <LockKeyhole size={19} />
      </div>
      {settings?.configured && !editing ? (
        <div className="audit-connected">
          <CheckCircle2 size={24} />
          <div>
            <strong>{settings.account || "后台会话已保存"}</strong>
            <p>
              {settings.backend === "newapi" ? "New API" : "Sub2API"} ·{" "}
              {settings.user_id ? "用户 " + settings.user_id : "已连接"}
            </p>
          </div>
          <div className="audit-balance">
            <small>账户余额 · 上次读取</small>
            <b>
              {settings.balance == null
                ? "—"
                : settings.balance.toLocaleString(undefined, {
                    maximumFractionDigits: 4,
                  })}{" "}
              <small>{settings.currency}</small>
            </b>
          </div>
          <div className="audit-inline-actions">
            <Button
              disabled={locked}
              onClick={() => void connect("station_billing_refresh")}
            >
              <RefreshCw size={14} />
              刷新账单连接
            </Button>
            <Button
              variant="ghost"
              disabled={locked}
              onClick={() => setEditing(true)}
            >
              重新登录
            </Button>
            <Button
              variant="ghost"
              disabled={locked}
              onClick={() => void disconnect()}
            >
              断开
            </Button>
          </div>
        </div>
      ) : (
        <>
          <div className="audit-segments" aria-label="后台登录方式">
            <button
              aria-pressed={mode === "password"}
              disabled={locked}
              onClick={() => setMode("password")}
            >
              <KeyRound size={15} />
              账号密码
            </button>
            <button
              aria-pressed={mode === "browser"}
              disabled={locked}
              onClick={() => setMode("browser")}
            >
              <Globe2 size={15} />
              浏览器 / Linux DO
            </button>
          </div>
          {mode === "password" ? (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                void connect("station_billing_connect");
              }}
            >
              <div className="audit-fields">
                <label>
                  站点账号 / 邮箱
                  <input
                    autoComplete="username"
                    value={account}
                    disabled={locked}
                    onChange={(e) => setAccount(e.target.value)}
                    placeholder="和站点网页登录一致"
                  />
                </label>
                <label>
                  站点密码
                  <input
                    type="password"
                    autoComplete="off"
                    value={password}
                    disabled={locked}
                    onChange={(e) => setPassword(e.target.value)}
                    placeholder="登录后加密保存在本机"
                  />
                </label>
              </div>
              <div className="audit-login-bottom">
                <label>
                  账单后端
                  <select
                    value={backend}
                    disabled={locked}
                    onChange={(e) => setBackend(e.target.value)}
                  >
                    <option value="auto">自动识别</option>
                    <option value="newapi">New API</option>
                    <option value="sub2">Sub2API</option>
                  </select>
                </label>
                <Button
                  type="submit"
                  variant="primary"
                  disabled={locked || !account.trim() || !password}
                >
                  {busy ? "正在连接…" : "登录并读取账单"}
                </Button>
              </div>
            </form>
          ) : (
            <div className="audit-browser-login">
              <p>
                在独立窗口中打开本站登录页，选择 Linux DO
                或完成二次验证。登录成功后自动取得本站会话和用户 ID。
              </p>
              <Button
                variant="primary"
                disabled={locked}
                onClick={() => void connect("station_billing_browser")}
              >
                <Globe2 size={16} />
                {busy ? "等待站点授权…" : "打开独立登录窗口"}
              </Button>
            </div>
          )}
          <p className="audit-caption">
            <LockKeyhole size={13} /> 自动保存 Cookie /
            会话，无需手动查找后台令牌。只用于当前站点。
          </p>
        </>
      )}
      {message && (
        <p role="status" className="audit-feedback">
          {message}
        </p>
      )}
      {error && (
        <p role="alert" className="audit-error">
          {error}
        </p>
      )}
    </section>
  );
}
