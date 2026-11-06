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
  open_desktop: "打开桌面端",
  relogin: "重新登录",
  web_verify: "去网页验证",
};

/**
 * 行上不给的两个动作（2026-11-06）：
 * - 「切换并打开」：使用者没要过；
 * - 「让官方换新」：GPT 那一行点刷新时自己先让官方 `codex app-server` 换新再问（`GptBand` 的 `refreshRow`），
 *   不用再单独摆一颗按钮。
 */
const HIDDEN_ACTIONS = new Set(["switch_open", "official_refresh"]);

/**
 * 不画徽标的几种（2026-11-06 使用者：「为什么都令牌到点，这个不科学」）：
 * - 访问令牌到点：只管几个小时，官方客户端下次运行自己就换新，是常态，不是毛病；
 *   Claude 那份是槽位里 Claude Code 命令行的，平时用桌面端时它本来就一直「到点」；
 * - 网络 / 代理之类的瞬时问题：不是登录的事；
 * - 没登录：行上已经有「登录」。
 */
const QUIET = new Set(["ok", "access_expired", "transient", "logged_out"]);

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
  if (!status) return null;
  const showKind = !QUIET.has(status.kind);
  if (!showKind && !status.email_mismatch) return null;
  // 常态那几种不带动作按钮；「切换并打开」「让官方换新」也不给（见 HIDDEN_ACTIONS）。
  const action =
    showKind && status.action && !HIDDEN_ACTIONS.has(status.action)
      ? status.action
      : null;
  return (
    <span className="inline-flex items-center gap-1">
      {showKind && (
        <Pill tone={TONE[status.kind] ?? "default"} title={status.reason}>
          {status.label}
        </Pill>
      )}
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
