/** Codex desktop: official login slots, explicit switching, local rollout usage. */
import { useEffect, useState } from "react";
import {
  Gauge,
  MonitorSmartphone,
  Power,
  Plus,
  RefreshCw,
  Trash2,
  Users,
} from "lucide-react";
import { CODEX_R, codexApi } from "../../lib/codexAccounts";
import type { CodexSlot } from "../../lib/generated/CodexSlot";
import type { CodexUsage } from "../../lib/generated/CodexUsage";
import { useResource, useSession } from "../../lib/store";
import { Button, Card, ConfirmDialog, Modal, Pill, useToast } from "../../ui";

const PER_PAGE = 4;
const short = (n: number) =>
  new Intl.NumberFormat("zh-CN", {
    notation: "compact",
    maximumFractionDigits: 1,
  }).format(n);

export default function GptBand() {
  const accounts = useResource("codexAccounts", CODEX_R.accounts);
  const desktop = useResource("codexDesktop", CODEX_R.desktop);
  const toast = useToast();
  const [pageRaw, setPage] = useSession("home.codex.page", -1);
  const [adding, setAdding] = useState(false);
  const [label, setLabel] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [ask, setAsk] = useState<
    | { id: string; label: string; action: "switch" | "launch" | "archive" }
    | { action: "close" }
    | null
  >(null);
  const slots = accounts.data?.slots ?? [];
  const active = slots.find((s) => s.active);
  const ownedProcess = desktop.data?.processes.some(
    (p) =>
      p.pid === accounts.data?.launched_pid &&
      p.started === accounts.data?.launched_at,
  );
  const runningSlot = ownedProcess
    ? slots.find((s) => s.id === accounts.data?.launched_id)
    : undefined;
  const pages = Math.max(1, Math.ceil(slots.length / PER_PAGE));
  const autoPage = Math.floor(
    Math.max(
      0,
      slots.findIndex((s) => s.active),
    ) / PER_PAGE,
  );
  const page = Math.min(pageRaw < 0 ? autoPage : pageRaw, pages - 1);

  async function reload() {
    await accounts.refresh();
    await desktop.refresh();
  }
  async function create() {
    setBusy(true);
    setError("");
    try {
      const id = await codexApi.create(label);
      await accounts.refresh();
      setAdding(false);
      setPage(Math.floor(slots.length / PER_PAGE));
      setAsk({ id, label: label.trim(), action: "launch" });
      setLabel("");
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
    }
  }
  async function confirm() {
    if (!ask) return;
    setBusy(true);
    setError("");
    try {
      if (ask.action === "close") await codexApi.close();
      else await codexApi[ask.action](ask.id);
      await reload();
      toast.ok(
        ask.action === "close"
          ? "Codex 桌面端已关闭，账户与登录资料已保留"
          : ask.action === "launch"
            ? "Codex 桌面端已打开，请在官方窗口完成登录"
            : ask.action === "switch"
              ? "账户已切换，点击右侧启动"
              : "槽位已移除，登录资料保留在本机归档中",
      );
      setAsk(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }
  const open = (slot: CodexSlot, action: "switch" | "launch" | "archive") => {
    setError("");
    setAsk({ id: slot.id, label: slot.label, action });
  };

  return (
    <>
      <div className="account-workspace" data-testid="codex-accounts">
        <Card
          className="account-slots"
          title={
            <>
              <Users size={14} />
              账户槽位
            </>
          }
          actions={
            <div className="flex gap-1.5">
              <Button
                size="sm"
                icon={<RefreshCw size={12} />}
                aria-label="刷新 Codex 账户"
                loading={accounts.loading}
                onClick={() =>
                  void reload().catch((e) => toast.error(String(e)))
                }
              />
              <Button
                size="sm"
                icon={<Plus size={12} />}
                onClick={() => {
                  setError("");
                  setAdding(true);
                }}
              >
                新建
              </Button>
            </div>
          }
        >
          {accounts.error && (
            <p role="alert" className="notice notice--danger">
              {accounts.error}
            </p>
          )}
          {!slots.length && (
            <div className="py-4">
              <h3>登录你的 Codex 账户</h3>
              <p className="notice mt-2">
                新建槽位后，在 Codex 桌面端选择「使用 ChatGPT
                登录」。每个槽位单独保存登录状态。
              </p>
              <p className="notice mt-2">
                现有 Codex 的默认账户和会话保留原处。
              </p>
            </div>
          )}
          <div className="account-slot-list">
            {slots.slice(page * PER_PAGE, (page + 1) * PER_PAGE).map((s) => (
              <div
                key={s.id}
                className={"slotrow" + (s.active ? " slotrow--active" : "")}
                data-testid="codex-slot"
              >
                <div className="slotrow-main">
                  <strong className="slotrow-label">{s.label}</strong>
                  <span className="slotrow-side">
                    {s.active ? (
                      <Pill tone="accent">当前槽位</Pill>
                    ) : (
                      <Button
                        size="sm"
                        disabled={busy}
                        onClick={() => open(s, "switch")}
                      >
                        切换
                      </Button>
                    )}
                    <Button
                      size="sm"
                      disabled={busy || !desktop.data?.executable}
                      onClick={() => open(s, "launch")}
                    >
                      {s.logged_in ? "打开" : "登录"}
                    </Button>
                    <Button
                      size="sm"
                      aria-label={`移除 Codex ${s.label}`}
                      icon={<Trash2 size={12} />}
                      disabled={busy || s.active}
                      onClick={() => open(s, "archive")}
                    />
                  </span>
                </div>
                <p
                  className="notice codex-slot-identity"
                  title={`${s.email ?? "尚未读取到账户邮箱"}${s.plan ? ` · ${s.plan}` : ""}`}
                >
                  {s.email ?? "尚未读取到账户邮箱"}
                  {s.plan ? ` · ${s.plan}` : ""}
                </p>
                <span
                  className={`notice ${s.logged_in ? "text-[var(--ok)]" : ""}`}
                >
                  {s.auth_state}
                </span>
              </div>
            ))}
          </div>
          <div className="account-pagination">
            <span className="notice">{slots.length} 个槽位 · 每页 4 个</span>
            {pages > 1 && (
              <span className="pager ml-auto">
                {Array.from({ length: pages }, (_, i) => (
                  <button
                    key={i}
                    className="pager-btn"
                    type="button"
                    aria-label={`Codex 第 ${i + 1} 页`}
                    aria-current={page === i}
                    onClick={() => setPage(i)}
                  >
                    {i + 1}
                  </button>
                ))}
              </span>
            )}
          </div>
        </Card>
        <div className="account-workspace-right">
          <Card
            className="account-launch"
            title="Codex 桌面端"
            actions={
              <Pill tone={desktop.data?.executable ? "ok" : "default"}>
                {desktop.data?.running
                  ? "运行中"
                  : desktop.data?.executable
                    ? "未运行"
                    : "未检测到"}
              </Pill>
            }
          >
            {desktop.error && (
              <p role="alert" className="notice notice--danger">
                {desktop.error}
              </p>
            )}
            <div className="flex flex-wrap gap-2">
              <Button
                variant="primary"
                icon={<MonitorSmartphone size={20} />}
                className="flex-1 justify-center"
                disabled={busy || !active || !desktop.data?.executable}
                onClick={() => active && open(active, "launch")}
              >
                {active?.logged_in ? "启动 Codex 桌面端" : "打开桌面端并登录"}
              </Button>
              <Button
                variant="danger"
                icon={<Power size={16} />}
                disabled={busy || !desktop.data?.running}
                onClick={() => {
                  setError("");
                  setAsk({ action: "close" });
                }}
              >
                一键关闭
              </Button>
            </div>
            <p className="notice mt-2">
              {active ? `当前槽位：${active.label}` : "先新建一个账户槽位"}
              {desktop.data?.version ? ` · v${desktop.data.version}` : ""}
            </p>
            <p className="notice mt-2">
              切换或启动前会关闭已打开的 Codex
              桌面端，正在运行的任务会中断，请先保存。
            </p>
            <p className="notice mt-1">
              桌面端独立启动，不沿用 Codex CLI 的 IP 执行锁。
            </p>
          </Card>
          <CodexUsageCard
            slot={runningSlot ?? active}
            running={!!runningSlot}
          />
        </div>
      </div>
      <Modal
        open={adding}
        onClose={() => !busy && setAdding(false)}
        title="新建 Codex 账户槽位"
      >
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void create();
          }}
          className="flex flex-col gap-3"
        >
          <label htmlFor="codex-label">账户名称</label>
          <input
            id="codex-label"
            className="input"
            value={label}
            maxLength={40}
            autoFocus
            onChange={(e) => setLabel(e.target.value)}
            placeholder="例如：个人账户"
          />
          <p className="notice">
            下一步打开官方桌面端登录，不需要填写密码或粘贴 Token。
          </p>
          {error && (
            <p role="alert" className="notice notice--danger">
              {error}
            </p>
          )}
          <Button
            type="submit"
            variant="primary"
            loading={busy}
            disabled={!label.trim()}
          >
            新建并登录
          </Button>
        </form>
      </Modal>
      <ConfirmDialog
        open={!!ask}
        onCancel={() => !busy && setAsk(null)}
        onConfirm={() => void confirm()}
        loading={busy}
        title={
          ask?.action === "close"
            ? "关闭 Codex 桌面端？"
            : ask?.action === "archive"
              ? `移除 ${ask.label}？`
              : `${ask?.action === "switch" ? "切换到" : "打开"} ${ask?.label ?? ""}？`
        }
        confirmLabel={
          ask?.action === "close"
            ? "确认关闭"
            : ask?.action === "archive"
              ? "移除并保留归档"
              : ask?.action === "switch"
                ? "关闭桌面端并切换"
                : "关闭旧窗口并打开"
        }
        danger
      >
        <p>
          {ask?.action === "close"
            ? "将关闭所有 Codex 桌面端窗口及其正在运行的任务，请先保存工作。账户槽位、登录资料和历史会话会保留。"
            : ask?.action === "archive"
              ? "槽位从列表移除，登录资料和会话保留在本机归档中。"
              : "会先关闭所有 Codex 桌面端窗口及其正在运行的任务。请确认当前工作已经保存，再继续。"}
        </p>
        {error && (
          <p role="alert" className="notice notice--danger mt-2">
            {error}
          </p>
        )}
      </ConfirmDialog>
    </>
  );
}

function CodexUsageCard({
  slot,
  running,
}: {
  slot?: CodexSlot;
  running: boolean;
}) {
  const [details, setDetails] = useState(false);
  const [days, setDays] = useState(1);
  const [revision, setRevision] = useState(0);
  const [data, setData] = useState<CodexUsage | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let cancelled = false;
    let pending = false;
    setData(null);
    setError("");
    if (!slot) return;
    const load = async () => {
      if (pending) return;
      pending = true;
      setBusy(true);
      try {
        const next = await codexApi.usage(slot.id, days);
        if (!cancelled) {
          setData(next);
          setError("");
        }
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      } finally {
        pending = false;
        if (!cancelled) setBusy(false);
      }
    };
    void load();
    const timer = window.setInterval(() => void load(), 60_000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [slot?.id, days, revision]);
  return (
    <Card
      className="account-usage"
      title={
        <>
          <Gauge size={14} />
          {slot ? `${slot.label} 的用量` : "当前账户的用量"}
        </>
      }
    >
      <p className="notice usage-identity">
        {running ? "当前启动账户" : "所选槽位历史 · 未检测到运行"}
      </p>
      <div className="usage-ranges">
        {(
          [
            [1, "今天"],
            [7, "7 天"],
            [0, "全部"],
          ] as const
        ).map(([d, n]) => (
          <Button
            key={d}
            size="sm"
            variant={days === d ? "primary" : "default"}
            onClick={() => setDays(d)}
          >
            {n}
          </Button>
        ))}
        <Button
          size="sm"
          aria-label="刷新 Codex 用量"
          icon={<RefreshCw size={12} />}
          loading={busy}
          onClick={() => setRevision((n) => n + 1)}
        />
      </div>
      {error && (
        <p role="alert" className="notice notice--danger">
          {error}
        </p>
      )}
      <div className="ustats">
        {[
          ["输入", data?.input],
          ["输出", data?.output],
          ["缓存读取", data?.cached],
          ["合计", data?.total],
        ].map(([name, n]) => (
          <div key={String(name)} className="ustat">
            <span className="ustat-name">{name}</span>
            <span className="ustat-value">
              {typeof n === "number" ? short(n) : "—"}
            </span>
          </div>
        ))}
      </div>
      <div className="usage-footer">
        <span className="notice">
          {data && (data.files_failed > 0 || data.incomplete > 0)
            ? "统计不完整"
            : "本机记录 · 每分钟刷新"}
        </span>
        <Button size="sm" variant="ghost" onClick={() => setDetails(true)}>
          统计说明
        </Button>
      </div>
      <Modal
        open={details}
        onClose={() => setDetails(false)}
        title="Codex 用量明细"
      >
        <p className="notice mt-2">
          {data
            ? `${data.sessions} 个会话 · ${data.checked_at} 更新`
            : "登录并使用该槽位后显示用量"}{" "}
          · 每分钟刷新
        </p>
        <p className="notice mt-1">
          仅统计该槽位的本机会话；缓存已包含在输入中。订阅剩余额度以官方桌面端为准。
        </p>
        {!!data && (data.files_failed > 0 || data.incomplete > 0) && (
          <p className="notice notice--warn mt-1">
            统计不完整：{data.files_failed} 个文件未读，{data.incomplete}{" "}
            条记录无法核算。
          </p>
        )}
      </Modal>
    </Card>
  );
}
