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
 * 使用者要的是一句话：**切账户就把相关官方 Claude 关掉，后面启动什么自己点。**
 * 清场之后没有任何进程还连着旧账户，也就没有「桌面端开着不能换资料目录」这回事，
 * 托盘也能直接切。所以这里只剩：一个桌面端复选框、一个「关闭官方会话并切换」按钮。
 * 切完不起任何进程 —— 没登录过 / 过期的槽位也一样，用户回总览点 Claude Code，
 * 登录界面由 Claude Code 自己弹出。
 */

import { useEffect, useState } from "react";

import { api, type KillReport } from "../../lib/api";
import { AFTER, R } from "../../lib/resources";
import { invalidate, useResource, useSession } from "../../lib/store";
import { slotName } from "../../lib/slotName";
import { workspaceApi } from "../../lib/workspace";
import {
  Button,
  Checkbox,
  ConfirmDialog,
  Modal,
  TextField,
  useToast,
} from "../../ui";
import AccountDetail from "./AccountDetail";
import {
  DELETE_KEY,
  NEW_KEY,
  SWITCH_KEY,
  SWITCH_THEN_LOGIN_KEY,
  labelError,
  requestSwitchAndLogin,
} from "./requests";

// 会话态的键和「喊一声」的入口都在 `requests.ts` —— 详情页要喊切换和删除，
// 而这里要挂详情页，放在同一个文件里就是一个 import 环。
// 这几个再导出一次，免得已有的页面全都要改 import 路径。
export {
  labelError,
  needsLogin,
  requestAccountDetail,
  requestDelete,
  requestNewSlot,
  requestSwitch,
  requestSwitchAndLogin,
} from "./requests";

export default function AccountDialogs() {
  return (
    <>
      <SwitchFlow />
      <NewSlotDialog />
      <DeleteSlotDialog />
      <AccountDetail />
    </>
  );
}

/**
 * 起一个官方身份的 Claude Code —— 登录界面由它自己弹出来。
 *
 * ⛔ 走 `workspaceApi.launch` 而不是 `api.launchClaude`：后者拿不到身份参数，
 * 而且无条件挂看门狗。前者才是 `if target.gated()`。
 *
 * 前提是那个槽位**已经是当前的** —— `workspace::launch` 里有一道
 * 「请先在官方账户页切换到此账户」的拦截，守着「任意时刻只有一个账户激活」。
 */
async function launchOfficialCode(label: string): Promise<void> {
  await workspaceApi.launch("claude-code", "official", label);
  invalidate(...AFTER.lease);
}

// ---------------------------------------------------------------- 切换

const ROLE_WORD: Record<string, string> = {
  desktop: "桌面端",
  code: "Claude Code 会话",
  bridge: "酒馆桥接",
  antigravity: "反重力",
};
const ROLE_ORDER = ["desktop", "code", "bridge", "antigravity"];

/**
 * 「会关掉：桌面端 13 个进程、Claude Code 会话 2 个、酒馆桥接 1 个」这种一句话。
 *
 * 扫到什么就报什么，认不出的角色照原名报（2026-09-25）：原来只数三类，而切换真去关的
 * 还包括反重力 —— 只有反重力开着时这句话是「会关掉：。」，然后 IDE 被收掉。现在切换只关
 * Claude 的（后端 `Scope::AccountSwitch`），这里扫的也是同一个范围（`officialSwitchPreview`）。
 */
export function describeTargets(r: KillReport): string {
  const count = new Map<string, number>();
  for (const t of r.targets) count.set(t.role, (count.get(t.role) ?? 0) + 1);
  const rank = (k: string) =>
    ROLE_ORDER.indexOf(k) < 0 ? ROLE_ORDER.length : ROLE_ORDER.indexOf(k);
  return [...count.entries()]
    .sort(([a], [b]) => rank(a) - rank(b))
    .map(([k, n]) => `${ROLE_WORD[k] ?? k} ${n} 个`)
    .join("、");
}

function SwitchFlow() {
  const toast = useToast();
  const accounts = useResource("accounts", R.accounts);
  const [label, setLabel] = useSession<string | null>(SWITCH_KEY, null);
  const [thenLogin, setThenLogin] = useSession(SWITCH_THEN_LOGIN_KEY, false);

  const [scan, setScan] = useState<KillReport | null>(null);
  const [scanErr, setScanErr] = useState("");
  const [desktop, setDesktop] = useState(false);
  const [busy, setBusy] = useState(false);

  const data = accounts.data;
  const slot = data?.slots.find((s) => s.label === label) ?? null;
  const current = data?.slots.find((s) => s.active)?.label;
  const currentHasDesktop = !!data?.slots.find((s) => s.active)
    ?.desktop_profile;
  // 给人看的那一串：「邮箱 - 命名」。`label` 仍然是传给后端的那个键。
  const shown = slot ? slotName(slot.email, slot.label) : (label ?? "");

  // 每次打开都重来一遍：先扫一遍会关掉哪些进程，框里要报数。
  useEffect(() => {
    if (!label) return;
    setScan(null);
    setScanErr("");
    // 这个槽位有自己的桌面端资料就默认一起切；没有的话默认不动桌面端 ——
    // 新建空白资料意味着桌面端要重新登录，得本人选。
    setDesktop(!!data?.slots.find((s) => s.label === label)?.desktop_profile);
    // 扫的是切换真去关的那个范围（只 Claude 的：中转与反重力不碰），而且不发一键关闭的
    // 进度事件 —— 原来借用 `killswitchPreview`，一打开切换框，「一键关闭」那块磁贴就跟着转（2026-09-25）。
    void api
      .officialSwitchPreview()
      .then(setScan)
      .catch((e) => setScanErr(e instanceof Error ? e.message : String(e)));
    // 只在「换了一个要切的槽位」时重来，槽位列表刷新不该把使用者勾的东西冲掉。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [label]);

  function close() {
    setLabel(null);
    setThenLogin(false);
  }

  async function doSwitch() {
    if (!label) return;
    setBusy(true);
    try {
      const r = await api.accountsSwitch(label, desktop);
      const failed = r.closed.failed.length;
      const closedWord =
        `关掉了 ${r.closed.killed.length} 个 Claude 进程` +
        `${failed ? `（另有 ${failed} 个没关掉，它们还连着原来的账户）` : ""}`;
      for (const n of r.notes) toast.info(n);
      invalidate("accounts", "gate", "plugins", ...AFTER.lease);

      if (thenLogin) {
        // ⛔ 切换成功之后的失败**必须分开报**：槽位已经换过去了，
        // 这时候说一句「切换失败」是假的 —— 使用者会以为还在原来那个账户上。
        try {
          await launchOfficialCode(label);
          toast.ok(
            `已切到 ${shown}（${closedWord}），并起了一个 Claude Code。` +
              `登录界面由它自己弹出，面板不经手你的账号密码。`,
          );
        } catch (e) {
          toast.error(
            `已切到 ${shown}，但 Claude Code 没起来：${e instanceof Error ? e.message : String(e)}。` +
              `回总览点「Claude Code」再试一次。`,
          );
        }
      } else {
        toast.ok(
          `已切到 ${shown}：${r.switched.join("，")}。${closedWord}。要用哪个，回总览自己点。`,
        );
      }
      close();
    } catch (e) {
      // 清场失败时后端根本没换指向；换指向失败时整体回滚 —— 两种都是「槽位没有变动」。
      toast.error(
        `切换失败，槽位没有变动：${e instanceof Error ? e.message : String(e)}`,
      );
    } finally {
      setBusy(false);
    }
  }

  const desktopHint = (() => {
    if (!data) return null;
    if (slot?.desktop_profile) {
      return desktop
        ? `桌面端会换成 ${shown} 的资料。`
        : `桌面端这次不换资料，继续用${data.desktop.active ? ` ${data.desktop.active} 的` : "现在的"}。`;
    }
    if (!desktop) {
      return data.desktop.managed
        ? `桌面端没有 ${shown} 的资料，这次不动它（继续用 ${data.desktop.active ?? "现在"} 的）。`
        : "桌面端现在没按账户分开，这次不动它。";
    }
    return [
      `会给桌面端新建一份 ${shown} 的空白资料并切过去，打开桌面端后要重新登录。`,
      // 后端只在 `Claude-<当前>` 还空着时把现在这份存成它；已经有一份了就存成 `Claude-backup-…`
      // （`plan_desktop` 的 adopt_to）。原来一律许诺「切回时原样回来」，那种情况下切回去用的是
      // 更早那一份（2026-09-25）。
      !data.desktop.managed && current
        ? currentHasDesktop
          ? `现在桌面端用的那份资料会另存为 Claude-backup-…（${current} 已经有一份自己的桌面端资料）；切回 ${current} 时用的是它原来那一份，不是现在这一份。`
          : `现在桌面端用的那份资料会存为 ${current} 的，切回 ${current} 时原样回来。`
        : null,
    ]
      .filter(Boolean)
      .join(" ");
  })();

  return (
    <ConfirmDialog
      open={!!label}
      onCancel={close}
      onConfirm={() => void doSwitch()}
      title={thenLogin ? `切换到 ${shown} 并登录？` : `切换到 ${shown}？`}
      confirmLabel={
        thenLogin ? "关闭官方会话、切换并登录" : "关闭官方会话并切换"
      }
      loading={busy}
      danger
    >
      <p>
        会先<strong>关闭正在跑的官方 Claude</strong>
        （桌面端、Claude Code、酒馆桥接；中转会话和反重力不动），再切过去。
        <strong>未保存的对话会丢。</strong>
      </p>

      <p className="notice mt-2">
        {scanErr
          ? `扫描正在跑的 Claude 失败：${scanErr}（切换时仍会按同一套规则关闭）`
          : scan === null
            ? "正在扫描正在跑的 Claude……"
            : scan.targets.length === 0
              ? "现在没有 Claude 在跑，直接切。"
              : `会关掉：${describeTargets(scan)}。`}
      </p>

      <div className="mt-3">
        <Checkbox checked={desktop} onChange={setDesktop} disabled={busy}>
          {slot?.desktop_profile
            ? `桌面端也切到 ${shown}`
            : `桌面端也按账户分开：给 ${shown} 建一份空白的桌面端资料`}
        </Checkbox>
        {desktopHint && <p className="notice mt-1">{desktopHint}</p>}
      </div>

      <p className="notice mt-3">
        {thenLogin
          ? "切完自动起一个 Claude Code，登录界面由官方客户端弹出。"
          : "切完不启动任何东西，要用哪个回总览自己点。"}
      </p>
    </ConfirmDialog>
  );
}

// ---------------------------------------------------------------- 新建

function NewSlotDialog() {
  const toast = useToast();
  const accounts = useResource("accounts", R.accounts);
  const [open, setOpen] = useSession(NEW_KEY, false);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  /**
   * 建好之后立刻登录。默认勾上 —— 使用者要的就是「新建时就把账户登录好」，
   * 而不是建完一个空目录再自己想办法。
   */
  const [login, setLogin] = useState(true);

  const err = labelError(name);
  const taken = !!accounts.data?.slots.some((s) => s.label === name.trim());
  const ready = !!name.trim() && !err && !taken;
  /** 已经有激活槽位 = 建完要登录就得先切，而切会关掉全部 Claude。 */
  const hasActive = !!accounts.data?.slots.some((s) => s.active);
  /** 有槽位、但一个都没激活（`claude-profile` 指向的目录没了）。那时新建的也直接成为当前的。 */
  const hasSlots = !!accounts.data?.slots.length;

  function close() {
    setOpen(false);
    setName("");
    setLogin(true);
  }

  /**
   * 建槽位，勾了就接着登录。
   *
   * # 为什么「登录」不是一步而是两条岔路
   *
   * 登录 = 在那个槽位里起一个 Claude Code，让官方客户端自己弹登录界面。
   * 而 `workspace::launch` 里有一道拦截：要起的身份必须**已经是当前槽位**
   * （那道拦截守着「任意时刻只有一个账户激活」，不许绕）。于是：
   *
   * - 原来一个激活槽位都没有 → 新槽位直接成了当前的（后端 `create_slot` 干的），
   *   **不用切、不用关任何进程**，直接起。全新使用者不该平白挨一次清场；
   * - 已经有别的槽位在用 → 得先切过去，而切 = 关掉本机全部 Claude。
   *   这个代价必须在确认框里说清楚，所以交给 `SwitchFlow` 去问 ——
   *   它本来就会先扫一遍「会关掉哪些」。
   */
  async function create() {
    setBusy(true);
    try {
      const out = await api.accountsCreate(name.trim());
      await accounts.refresh();
      invalidate(`tokens:${out.label}`);
      for (const n of out.notes) toast.info(n);
      close();

      if (!login) {
        toast.ok(
          out.activated
            ? `槽位 ${out.label} 已建好，它现在就是当前账户。回总览点「Claude Code」，在弹出的窗口里登录。`
            : `槽位 ${out.label} 已建好。切过去之后回总览点「Claude Code」登录。`,
        );
        return;
      }

      if (out.activated) {
        try {
          await launchOfficialCode(out.label);
          toast.ok(
            `槽位 ${out.label} 已建好并起了一个 Claude Code。登录界面由它自己弹出，面板不经手你的账号密码。`,
          );
        } catch (e) {
          // 槽位确实建好了，别把它说成「新建失败」。
          toast.error(
            `槽位 ${out.label} 已建好，但 Claude Code 没起来：${e instanceof Error ? e.message : String(e)}`,
          );
        }
        return;
      }

      // 要先切过去。把代价交给切换确认框去讲。
      toast.info(
        `槽位 ${out.label} 已建好。登录前要先切过去 —— 那会关掉正在跑的全部 Claude。`,
      );
      requestSwitchAndLogin(out.label);
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
          <Button
            variant="primary"
            loading={busy}
            disabled={!ready}
            onClick={() => void create()}
          >
            {login ? "建好并登录" : "建好"}
          </Button>
        </>
      }
    >
      <TextField
        label="槽位名"
        value={name}
        onChange={setName}
        placeholder="例如 work"
        error={err ?? (taken ? "已经有这个名字的槽位了" : undefined)}
        hint="字母、数字、中文和 - _ .，最长 32 个字符。"
        disabled={busy}
      />
      <div className="mt-3">
        <Checkbox checked={login} onChange={setLogin} disabled={busy}>
          建好之后立刻登录
        </Checkbox>
        <p className="notice mt-1">
          {login
            ? hasActive
              ? "会先切过去 —— 那一步会关掉正在跑的官方 Claude，下一个框里会告诉你关哪些。"
              : hasSlots
                ? // 原来这里也写「这是第一个槽位」—— 列表里明明还有别的（2026-09-25）。
                  "现在没有激活的槽位（原来那个的指向已经失效），建好就直接是当前的：不切、不关任何进程。"
                : "这是第一个槽位，直接就是当前的：不切、不关任何进程。"
            : "只建目录，不启动任何东西。"}
        </p>
      </div>
      <p className="notice mt-3">
        空目录，不复制任何凭证。<strong>所有账户必须是你本人拥有的。</strong>
      </p>
    </Modal>
  );
}

// ---------------------------------------------------------------- 删除

/**
 * 删掉一个槽位。**不可恢复。**
 *
 * 要求手打「删除」两个字（跟完全卸载那个框要求打「卸载」同一套）——
 * 这是本页最贵的操作：删完那个账户在这台机器上要重新登录一次，
 * 而重新登录要走一遍官方的登录流程，不是点一下就回来的。
 *
 * 当前槽位删不了，判定在后端（`accounts::delete_slot`）。界面上按钮就是禁用的，
 * 但**判定不能只留在界面上** —— 托盘、以后别的入口都到不了这个组件。
 */
function DeleteSlotDialog() {
  const toast = useToast();
  const accounts = useResource("accounts", R.accounts);
  const [label, setLabel] = useSession<string | null>(DELETE_KEY, null);
  const [dropDesktop, setDropDesktop] = useState(true);
  const [busy, setBusy] = useState(false);

  const slot = accounts.data?.slots.find((s) => s.label === label) ?? null;
  const shown = slot ? slotName(slot.email, slot.label) : (label ?? "");

  // 每次打开都回到默认值：留着上一次的选择，下一次就会在使用者没看清的
  // 情况下把桌面端资料也带走。
  useEffect(() => {
    if (label) setDropDesktop(true);
  }, [label]);

  function close() {
    setLabel(null);
  }

  async function doDelete() {
    if (!label) return;
    setBusy(true);
    try {
      const out = await api.accountsDelete(label, dropDesktop);
      for (const n of out.notes) toast.info(n);
      toast.ok(
        `槽位 ${out.label} 已删除（删掉了 ${out.removed.length} 个目录）。`,
      );
      // 详情页的 token 表按 `tokens:<标签>` 缓存、不会自己过期：不扔的话，删了再建一个同名的，
      // 详情页显示的还是被删那个的数（2026-09-25）。
      invalidate("accounts", `tokens:${out.label}`);
      close();
    } catch (e) {
      toast.error(
        `删除失败，槽位没有变动：${e instanceof Error ? e.message : String(e)}`,
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <ConfirmDialog
      open={!!label}
      onCancel={close}
      onConfirm={() => void doDelete()}
      title={`删除槽位 ${shown}？`}
      confirmLabel="删除"
      confirmWord="删除"
      loading={busy}
      danger
    >
      <p>
        整个目录一起删：凭证、设置、会话历史。<strong>不可恢复</strong>，
        这个账户以后要用得重新登录。
      </p>
      <p className="notice mt-2">{slot?.dir ?? "读不出槽位目录"}</p>

      {slot?.desktop_profile && (
        <div className="mt-3">
          <Checkbox
            checked={dropDesktop}
            onChange={setDropDesktop}
            disabled={busy}
          >
            连桌面端的那份资料一起删
          </Checkbox>
          <p className="notice mt-1">
            {dropDesktop
              ? `一并删掉 ${slot.desktop_dir ?? `Claude-${label}`}。`
              : "保留着 —— 以后建同名槽位会接着用这一份。"}
          </p>
        </div>
      )}
    </ConfirmDialog>
  );
}
