/**
 * 账户切换与新建槽位 —— **全局只挂一份**（在 `App.tsx` 里）。
 *
 * 原来总览（`AccountBand`）和账户页（`Accounts`）各挂一份切换对话框，
 * 两份的文案、默认值、失败处理已经各走各的了。现在谁要切都喊一声 `requestSwitch(label)`。
 *
 * # v0.9.0：切换 = 先清场，再切，不自动启动
 *
 * v0.8.0 把切换做成了「只在桌面端要跟着切时关桌面端、别的会话可选关、切完按关掉的是什么
 * 给『重新打开』」，外加「没登录的槽位切完自动起 Claude Code 登录」。条件分支一大堆，
 * 托盘还因为弹不了框要把面板叫出来。
 *
 * 使用者要的是一句话：**切账户就把全部 Claude 关掉，后面启动什么自己点。**
 * 清场之后没有任何进程还连着旧账户，也就没有「桌面端开着不能换资料目录」这回事，
 * 托盘也能直接切。所以这里只剩：一个桌面端复选框、一个「关闭全部并切换」按钮。
 * 切完不起任何进程 —— 没登录过 / 过期的槽位也一样，用户回总览点 Claude Code，
 * 登录界面由 Claude Code 自己弹出。
 */

import { useEffect, useState } from 'react';

import { api, type KillReport, type Slot } from '../../lib/api';
import { AFTER, R } from '../../lib/resources';
import { invalidate, setSession, useResource, useSession } from '../../lib/store';
import { Button, Checkbox, ConfirmDialog, Modal, TextField, useToast, fmtDaysLeft } from '../../ui';

const SWITCH_KEY = 'accounts.switchTo';
const NEW_KEY = 'accounts.newSlot';

/** 打开切换对话框。总览、账户页都走这一个。 */
export function requestSwitch(label: string) {
  setSession<string | null>(SWITCH_KEY, label);
}

/** 打开「新建槽位」对话框。 */
export function requestNewSlot() {
  setSession(NEW_KEY, true);
}

/** 凭证过期或从没登过 —— 这两种都得在那个槽位里登录一次。 */
export function needsLogin(s: Slot | null | undefined): boolean {
  if (!s) return true;
  return !s.logged_in || (s.cli_days_left ?? 0) < 0;
}

/** 槽位名规矩，与 Rust `accounts::validate_label` 一致。后端仍会再验一次。 */
export function labelError(name: string): string | undefined {
  const n = name.trim();
  if (!n) return undefined;
  if ([...n].length > 32) return '最长 32 个字符';
  if (!/^[\p{L}\p{N}_.-]+$/u.test(n)) return '只能用字母、数字、中文和 - _ .';
  if (n.startsWith('.') || n.endsWith('.')) return '不能以点开头或结尾';
  return undefined;
}

export default function AccountDialogs() {
  return (
    <>
      <SwitchFlow />
      <NewSlotDialog />
    </>
  );
}

// ---------------------------------------------------------------- 切换

const ROLE_WORD: Record<string, string> = {
  desktop: '桌面端',
  code: 'Claude Code 会话',
  bridge: '酒馆桥接',
};

/** 「会关掉：桌面端 13 个进程、Claude Code 会话 2 个、酒馆桥接 1 个」这种一句话。 */
function describeTargets(r: KillReport): string {
  const count = new Map<string, number>();
  for (const t of r.targets) count.set(t.role, (count.get(t.role) ?? 0) + 1);
  return ['desktop', 'code', 'bridge']
    .filter((k) => count.get(k))
    .map((k) => `${ROLE_WORD[k]} ${count.get(k)} 个`)
    .join('、');
}

function SwitchFlow() {
  const toast = useToast();
  const accounts = useResource('accounts', R.accounts);
  const [label, setLabel] = useSession<string | null>(SWITCH_KEY, null);

  const [scan, setScan] = useState<KillReport | null>(null);
  const [scanErr, setScanErr] = useState('');
  const [desktop, setDesktop] = useState(false);
  const [busy, setBusy] = useState(false);

  const data = accounts.data;
  const slot = data?.slots.find((s) => s.label === label) ?? null;
  const current = data?.slots.find((s) => s.active)?.label;

  // 每次打开都重来一遍：先扫一遍会关掉哪些进程，框里要报数。
  useEffect(() => {
    if (!label) return;
    setScan(null);
    setScanErr('');
    // 这个槽位有自己的桌面端资料就默认一起切；没有的话默认不动桌面端 ——
    // 新建空白资料意味着桌面端要重新登录，得本人选。
    setDesktop(!!data?.slots.find((s) => s.label === label)?.desktop_profile);
    void api
      .killswitchPreview()
      .then(setScan)
      .catch((e) => setScanErr(e instanceof Error ? e.message : String(e)));
    // 只在「换了一个要切的槽位」时重来，槽位列表刷新不该把使用者勾的东西冲掉。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [label]);

  function close() {
    setLabel(null);
  }

  async function doSwitch() {
    if (!label) return;
    setBusy(true);
    try {
      const r = await api.accountsSwitch(label, desktop);
      const failed = r.closed.failed.length;
      toast.ok(
        `已切到 ${label}：${r.switched.join('，')}。关掉了 ${r.closed.killed.length} 个 Claude 进程` +
          `${failed ? `（另有 ${failed} 个没关掉，它们还连着原来的账户）` : ''}。要用哪个，回总览自己点。`
      );
      for (const n of r.notes) toast.info(n);
      invalidate('accounts', 'gate', 'plugins', ...AFTER.lease);
      close();
    } catch (e) {
      // 清场失败时后端根本没换指向；换指向失败时整体回滚 —— 两种都是「槽位没有变动」。
      toast.error(`切换失败，槽位没有变动：${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setBusy(false);
    }
  }

  const desktopHint = (() => {
    if (!data) return null;
    if (slot?.desktop_profile) {
      return desktop
        ? `桌面端会换成 ${label} 的资料。`
        : `桌面端这次不换资料，继续用${data.desktop.active ? ` ${data.desktop.active} 的` : '现在的'}。`;
    }
    if (!desktop) {
      return data.desktop.managed
        ? `桌面端没有 ${label} 的资料，这次不动它（继续用 ${data.desktop.active ?? '现在'} 的）。`
        : '桌面端现在没按账户分开，这次不动它。';
    }
    return [
      `会给桌面端新建一份 ${label} 的空白资料并切过去，打开桌面端后要重新登录。`,
      !data.desktop.managed && current
        ? `现在桌面端用的那份资料会存为 ${current} 的，切回 ${current} 时原样回来。`
        : null,
    ]
      .filter(Boolean)
      .join(' ');
  })();

  return (
    <ConfirmDialog
      open={!!label}
      onCancel={close}
      onConfirm={() => void doSwitch()}
      title={`切换到 ${label ?? ''}？`}
      confirmLabel="关闭全部并切换"
      loading={busy}
      danger
    >
      <p>
        会先<strong>关闭全部正在跑的 Claude</strong>（桌面端、所有 Claude Code 会话
        {data?.bridgePresent ? '、酒馆桥接' : ''}），再把 Claude Code
        {data?.bridgePresent ? '、酒馆桥接' : ''} 切到这个账户
        {slot ? `（${slot.logged_in ? fmtDaysLeft(slot.cli_days_left) : '未登录'}）` : ''}。
        <strong>未保存的对话会丢。</strong>
      </p>

      <p className="notice mt-2">
        {scanErr
          ? `扫描正在跑的 Claude 失败：${scanErr}（切换时仍会按同一套规则关闭）`
          : scan === null
            ? '正在扫描正在跑的 Claude……'
            : scan.targets.length === 0
              ? '现在没有 Claude 在跑，直接切。'
              : `会关掉：${describeTargets(scan)}。`}
      </p>

      <div className="mt-3">
        <Checkbox checked={desktop} onChange={setDesktop} disabled={busy}>
          {slot?.desktop_profile
            ? `桌面端也切到 ${label}`
            : `桌面端也按账户分开：给 ${label} 建一份空白的桌面端资料`}
        </Checkbox>
        {desktopHint && <p className="notice mt-1">{desktopHint}</p>}
      </div>

      <p className="mt-3">
        切完<strong>不会自动启动任何东西</strong>，要用哪个回总览自己点。
        {needsLogin(slot) &&
          ` 这个账户${slot?.logged_in ? '的凭证已经过期' : '还没登录过'}：点「Claude Code」后登录界面会自己弹出，面板不经手你的账号密码。`}
      </p>
      <p className="notice mt-3">
        关闭走的是一键关闭那套双重证据（Anthropic 签名、npm 装的 Claude Code，或命令行同时命中
        bridge.py 与本项目数据目录），<strong>不按进程名杀</strong>，面板自己和它的祖先会放过。
        账户切换只能由你手动触发。<strong>所有账户必须是你本人拥有的。</strong>
      </p>
    </ConfirmDialog>
  );
}

// ---------------------------------------------------------------- 新建

function NewSlotDialog() {
  const toast = useToast();
  const accounts = useResource('accounts', R.accounts);
  const [open, setOpen] = useSession(NEW_KEY, false);
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);

  const err = labelError(name);
  const taken = !!accounts.data?.slots.some((s) => s.label === name.trim());
  const ready = !!name.trim() && !err && !taken;

  function close() {
    setOpen(false);
    setName('');
  }

  async function create() {
    setBusy(true);
    try {
      const out = await api.accountsCreate(name.trim());
      await accounts.refresh();
      close();
      // 不自动启动任何东西（v0.9.0）—— 告诉人下一步点哪里。
      toast.ok(
        out.activated
          ? `槽位 ${out.label} 已建好，它现在就是当前账户。回总览点「Claude Code」，在弹出的窗口里登录。`
          : `槽位 ${out.label} 已建好。切过去之后回总览点「Claude Code」登录。`
      );
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      open={open}
      onClose={busy ? () => undefined : close}
      dismissible={!busy}
      title="新建账户槽位"
      footer={
        <>
          <Button onClick={close} disabled={busy}>
            取消
          </Button>
          <Button variant="primary" loading={busy} disabled={!ready} onClick={() => void create()}>
            建好
          </Button>
        </>
      }
    >
      <TextField
        label="槽位名"
        value={name}
        onChange={setName}
        placeholder="例如 work"
        error={err ?? (taken ? '已经有这个名字的槽位了' : undefined)}
        hint="字母、数字、中文和 - _ .，最长 32 个字符。"
        disabled={busy}
      />
      <p className="notice mt-3">
        新槽位是一个空的登录目录，<strong>不会从别处复制任何凭证</strong>
        —— 复制凭证等于又造一份同样的 refresh token，两边都用起来之后一边刷新就可能让另一份作废。
        建好之后在里面登录一次；每个槽位有自己的一套设置和历史。
      </p>
      <p className="notice mt-2">
        <strong>所有账户必须是你本人拥有的。</strong>
      </p>
    </Modal>
  );
}
