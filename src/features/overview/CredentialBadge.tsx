import { useCallback, useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { credentialApi, type CredentialProvider } from "../../lib/credentials";
import type { CredentialStatus } from "../../lib/generated/CredentialStatus";
import { Button, Pill } from "../../ui";
import type { Tone } from "../../ui/labels";

/** 每个槽位的凭证状态（只读本机，跟着账户列表一起刷新）。 */
export function useCredentials(provider: CredentialProvider, version: unknown) {
  const [map, setMap] = useState<Record<string, CredentialStatus>>({});
  const reload = useCallback(() => {
    credentialApi
      .status(provider)
      .then((list) =>
        setMap(Object.fromEntries(list.map((c) => [c.id, c.status]))),
      )
      .catch(() => setMap({}));
  }, [provider]);
  useEffect(() => reload(), [reload, version]);
  return { map, reload };
}

const TONE: Record<string, Tone> = {
  ok: "ok",
  access_expired: "warn",
  access_rejected: "warn",
  transient: "default",
  relogin: "danger",
  logged_out: "default",
  unreadable: "danger",
  disabled: "danger",
  needs_web_verify: "warn",
  needs_subscription: "warn",
  forbidden: "danger",
};

const ACTION_LABEL: Record<string, string> = {
  official_refresh: "让官方换新",
  open_desktop: "打开桌面端",
  switch_open: "切换并打开",
  relogin: "重新登录",
  web_verify: "去网页验证",
};

/**
 * 账户行上的常驻凭证徽标：悬停是原因，旁边最多一个动作按钮。
 * 「正常」什么都不画（行上本来就挤，跟 Cockpit 一样只在有事时标）；邮箱对不上时多一个「是我」确认。
 */
export default function CredentialBadge({
  status,
  busy,
  onAction,
  onEmailAccept,
}: {
  status: CredentialStatus | undefined;
  busy?: boolean;
  /** `web_verify` 由这里自己用默认浏览器打开，不交给调用方。 */
  onAction?: (action: string) => void;
  onEmailAccept?: () => void;
}) {
  if (!status || (status.kind === "ok" && !status.email_mismatch)) return null;
  const action = status.action;
  return (
    <span className="inline-flex items-center gap-1">
      <Pill tone={TONE[status.kind] ?? "default"} title={status.reason}>
        {status.label}
      </Pill>
      {status.email_mismatch && (
        <Pill tone="warn" title={status.email_mismatch}>
          邮箱变了
        </Pill>
      )}
      {status.email_mismatch && onEmailAccept && (
        <Button
          size="sm"
          variant="ghost"
          className="credential-inline-btn"
          title={status.email_mismatch}
          onClick={onEmailAccept}
        >
          是我
        </Button>
      )}
      {action &&
        action !== "relogin" &&
        (action === "web_verify" ? status.url : onAction) && (
          <Button
            size="sm"
            className="credential-inline-btn"
            loading={busy}
            disabled={busy}
            title={status.reason}
            onClick={() =>
              action === "web_verify"
                ? status.url && void openUrl(status.url)
                : onAction?.(action)
            }
          >
            {ACTION_LABEL[action] ?? action}
          </Button>
        )}
    </span>
  );
}
