/**
 * SillyTavern 卡的四个确认框：下载安装 · 换版本（更新 / 指定版本 / 降级）· 换回留着的那一版 · 卸载。
 *
 * 卡片右上角的主按钮和卡上「版本」那一块都要开它们，所以做成一个控制器（`useSetupDialogs`），
 * 由卡片外壳挂一份，「版本」拿 `open(...)` 去开。
 *
 * # ⛔ 三条（2026-09-29 那一版定的，没变）
 *
 * 1. **代价常驻在确认框里**：要下载、要跑几分钟 npm、会先停正在跑的酒馆、卸载前先备份到哪 —— 不放悬停。
 * 2. **联网只在点了之后**：打开安装 / 更新框时才问 GitHub（`tavernLatest` / 指定版本时 `tavernRelease`），没有定时器。
 * 3. **Node 不够又没勾「用 winget 装」就不让点**：点了只会在第二步失败。
 */
import { useCallback, useState, type ReactNode } from "react";
import { Download, History, Trash2 } from "lucide-react";

import {
  api,
  type TavernRelease,
  type TavernSetupStatus,
} from "../../../lib/api";
import { AFTER } from "../../../lib/resources";
import { invalidate } from "../../../lib/store";
import { endTask, resetTask } from "../../../lib/tasks";
import { compareVersions, relation } from "../../../lib/tavernVersions";
import {
  Button,
  Checkbox,
  ConfirmDialog,
  ExternalLink,
  Modal,
  PathField,
  Row,
  useToast,
} from "../../../ui";
import { errorText } from "./common";

const NODE_PAGE = "https://nodejs.org/";

export type SetupKind = "install" | "update" | "swap" | "uninstall";

export interface SetupController {
  /** `tag` 给了就是「指定版本」（可以比现在旧）；不给就是最新的正式版。 */
  open: (kind: SetupKind, tag?: string) => void;
  /** 装 / 换 / 卸正在进行（主按钮与「版本」里的按钮据此变灰）。 */
  busy: string;
  dialogs: ReactNode;
}

export function useSetupDialogs(
  s: TavernSetupStatus | undefined,
): SetupController {
  const toast = useToast();
  const [kind, setKind] = useState<SetupKind | null>(null);
  const [tag, setTag] = useState<string | null>(null);
  const [target, setTarget] = useState<TavernRelease | null>(null);
  const [targetErr, setTargetErr] = useState("");
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState("");
  const [importOn, setImportOn] = useState(true);
  const [importFrom, setImportFrom] = useState("");
  const [installNode, setInstallNode] = useState(false);
  const [carryData, setCarryData] = useState(true);

  const managed = s?.managed ?? null;

  const ask = useCallback(async (which: string | null) => {
    setAsking(true);
    setTargetErr("");
    setTarget(null);
    try {
      setTarget(
        which ? await api.tavernRelease(which) : await api.tavernLatest(),
      );
    } catch (e) {
      setTargetErr(errorText(e));
    } finally {
      setAsking(false);
    }
  }, []);

  const open = useCallback(
    (which: SetupKind, wanted?: string) => {
      setKind(which);
      setTag(wanted ?? null);
      setInstallNode(false);
      setCarryData(true);
      if (which === "install") {
        // 预填「从哪份导入」：现在配置里那份不是面板装的，就是它（使用者自己的酒馆）。
        const own = s?.present && !s.managed ? s.root : "";
        setImportFrom(own);
        setImportOn(!!own);
      }
      if (which === "install" || which === "update") void ask(wanted ?? null);
    },
    [ask, s],
  );

  async function run(which: "install" | "update" | "swap") {
    setKind(null);
    setBusy(which);
    resetTask("install-sillytavern");
    try {
      const msg =
        which === "install"
          ? await api.tavernInstall(
              importOn && importFrom.trim() ? importFrom.trim() : null,
              installNode,
              tag,
            )
          : which === "update"
            ? await api.tavernUpdate(installNode, tag)
            : await api.tavernSwapPrevious(carryData);
      endTask("install-sillytavern");
      toast.ok(msg);
    } catch (e) {
      const msg = errorText(e);
      endTask("install-sillytavern", msg);
      toast.error(msg);
    } finally {
      setBusy("");
      invalidate(...AFTER.tavernSetup);
    }
  }

  async function uninstall() {
    setBusy("uninstall");
    try {
      const r = await api.tavernUninstall();
      toast.ok(
        `卸载了，数据（${r.backed_up} 个文件）备份在 ${r.backup}` +
          (r.left.length ? `；没删掉：${r.left.join("、")}` : ""),
      );
      setKind(null);
    } catch (e) {
      toast.error(errorText(e));
    } finally {
      setBusy("");
      invalidate(...AFTER.tavernSetup);
    }
  }

  // 缺 Node、或者本机那份不够新（对着要装的那一版的要求；没问到就对着现在这份）。
  const nodeShort =
    !!s && (!s.node.path || target?.node_ok === false || s.node_ok === false);
  const rel = target ? relation(target.version, s?.version) : "unknown";
  const explicit = tag !== null;
  const updateReady =
    !!target &&
    (explicit
      ? rel !== "same"
      : !!s?.version && compareVersions(target.version, s.version) > 0);
  const downgrade = explicit && rel === "older";
  const nodeBlock = (
    <NodeChoice
      nodeShort={nodeShort}
      winget={!!s?.winget}
      installNode={installNode}
      setInstallNode={setInstallNode}
      have={s?.node.version ?? null}
      need={target?.node_engine ?? s?.node_engine ?? null}
    />
  );

  const dialogs = (
    <>
      {/* ------------------------------------------------ 下载安装 */}
      <Modal
        open={kind === "install"}
        onClose={() => setKind(null)}
        title={
          explicit ? `安装 SillyTavern ${tag}？` : "下载安装 SillyTavern？"
        }
        size="wide"
        footer={
          <div className="flex flex-wrap justify-end gap-2">
            <Button onClick={() => setKind(null)}>取消</Button>
            <Button
              variant="primary"
              icon={<Download size={13} />}
              disabled={
                (importOn && !importFrom.trim()) ||
                (nodeShort && !installNode) ||
                (explicit && !target)
              }
              onClick={() => void run("install")}
              data-testid="tavern-install-confirm"
            >
              下载安装
            </Button>
          </div>
        }
      >
        <div data-testid="tavern-install-dialog">
          <Release latest={target} asking={asking} error={targetErr} />
          <Row>
            <span>装到</span>
            <span className="break-all font-mono text-xs">
              {s?.managed_dir}
            </span>
          </Row>
          {nodeBlock}
          <div className="mt-3">
            <Checkbox checked={importOn} onChange={setImportOn}>
              从已有的酒馆复制数据过来（源目录不动）
            </Checkbox>
            {importOn && (
              <div className="mt-2">
                <PathField
                  label="从哪一份复制"
                  value={importFrom}
                  onChange={setImportFrom}
                />
                <p className="notice mt-1">
                  复制 <code>data\</code>（角色卡、聊天、世界书、设置、扩展）、
                  <code>config.yaml</code>、<code>plugins\</code>
                  与全局扩展里你自己加的；不拷 <code>public\</code> 和
                  <code>node_modules</code>（SillyTavern
                  自己的更新说明里就这么写）。那份比要装的这一版还新时不导入。
                </p>
              </div>
            )}
          </div>
          {/* 代价常驻。 */}
          <p className="notice notice--warn mt-3">
            要从 GitHub 下载几十 MB、再跑几分钟 npm install；
            {s?.running ? "正在跑的酒馆（连同三条桥）会先停掉；" : ""}
            装好之后酒馆配置改指向面板装的这份
            {s?.present && !managed
              ? "，你原来那份原样留着，想换回去在「概览 › 位置与端口」里改"
              : ""}
            。
          </p>
        </div>
      </Modal>

      {/* ------------------------------------------------ 换版本（更新 / 指定版本） */}
      <Modal
        open={kind === "update"}
        onClose={() => setKind(null)}
        title={
          explicit
            ? `把面板装的 SillyTavern 换成 ${tag}？`
            : "更新面板装的 SillyTavern？"
        }
        size="wide"
        footer={
          <div className="flex flex-wrap justify-end gap-2">
            <Button onClick={() => setKind(null)}>取消</Button>
            <Button
              variant={downgrade ? "danger" : "primary"}
              disabled={!updateReady || (nodeShort && !installNode)}
              onClick={() => void run("update")}
              data-testid="tavern-update-confirm"
            >
              {target
                ? `${downgrade ? "降级到" : explicit ? "换成" : "更新到"} ${target.version}`
                : "更新"}
            </Button>
          </div>
        }
      >
        <div data-testid="tavern-update-dialog">
          <Release latest={target} asking={asking} error={targetErr} />
          <p className="notice mt-2">
            现在是 {s?.version ?? "?"}。
            {target && explicit && rel === "same"
              ? " 已经是这一版，不用换。"
              : ""}
            {target && !explicit && !updateReady
              ? " 已经是最新的正式版，不用更新。"
              : ""}
          </p>
          {nodeBlock}
          {downgrade && (
            <p
              className="notice notice--danger mt-3"
              data-testid="tavern-downgrade-note"
            >
              这是<strong>降级</strong>
              ：新版可能已经把数据改成旧版不认识的样子（聊天文件头、设置项）。
              现在这份整份留在 <code>sillytavern.previous</code>
              ，出了问题到「版本」里点「换回」就回来。
            </p>
          )}
          <p className="notice notice--warn mt-3">
            你的 <code>data\</code> 与 <code>config.yaml</code>{" "}
            复制到要换上的那一版里； 现在这份整份留在同级的{" "}
            <code>sillytavern.previous</code>
            （只留一份，下次换版本会换掉它）；<code>Start.bat</code>{" "}
            恢复成官方原样。
            {s?.running ? " 正在跑的酒馆（连同三条桥）会先停掉。" : ""}
          </p>
        </div>
      </Modal>

      {/* ------------------------------------------------ 换回留着的那一版 */}
      <Modal
        open={kind === "swap"}
        onClose={() => setKind(null)}
        title={`换回留着的 ${s?.previous_version ?? "那一版"}？`}
        icon={<History size={16} />}
        size="wide"
        footer={
          <div className="flex flex-wrap justify-end gap-2">
            <Button onClick={() => setKind(null)}>取消</Button>
            <Button
              variant="primary"
              disabled={!s?.previous_version}
              onClick={() => void run("swap")}
              data-testid="tavern-swap-confirm"
            >
              换回 {s?.previous_version ?? ""}
            </Button>
          </div>
        }
      >
        <div data-testid="tavern-swap-dialog">
          <p>
            现在是 {s?.version ?? "?"}，留着的是 {s?.previous_version ?? "?"}
            。两份对调：换下来的这一份整份留在 <code>sillytavern.previous</code>
            ，再点一次就换回来。
          </p>
          <fieldset className="tv-choice mt-3">
            <legend className="notice">数据用哪一份</legend>
            <label className="tv-choice-row">
              <input
                type="radio"
                name="tavern-swap-data"
                checked={carryData}
                onChange={() => setCarryData(true)}
              />
              <span>
                <strong>带着现在的数据过去</strong>
                <span className="notice block">
                  聊天、角色卡、世界书还是现在这些。留着那份原来自己的数据先备份到{" "}
                  <code>
                    %LOCALAPPDATA%\ClaudeIpGate\tavern-rollback\&lt;时间&gt;
                  </code>
                  。
                  {s?.previous_version &&
                  s.version &&
                  compareVersions(s.previous_version, s.version) < 0
                    ? " 它比现在旧：现在这版改过的数据它可能读不懂。"
                    : ""}
                </span>
              </span>
            </label>
            <label className="tv-choice-row">
              <input
                type="radio"
                name="tavern-swap-data"
                checked={!carryData}
                onChange={() => setCarryData(false)}
              />
              <span>
                <strong>用它自己留着的数据</strong>
                <span className="notice block">
                  回到它被换下来那一刻的样子；那之后的新聊天留在换下来的这一份里（不删）。
                </span>
              </span>
            </label>
          </fieldset>
          {s?.running && (
            <p className="notice notice--warn mt-3">
              正在跑的酒馆（连同三条桥）会先停掉。
            </p>
          )}
        </div>
      </Modal>

      {/* ------------------------------------------------ 卸载 */}
      <ConfirmDialog
        open={kind === "uninstall"}
        onCancel={() => setKind(null)}
        onConfirm={() => void uninstall()}
        title="卸载面板装的 SillyTavern？"
        confirmLabel="备份后卸载"
        loading={busy === "uninstall"}
        danger
      >
        <p>
          先把 <code>data\</code>、<code>config.yaml</code>
          （连同插件与全局扩展里你自己加的）整份复制到{" "}
          <code>%LOCALAPPDATA%\ClaudeIpGate\tavern-uninstall\&lt;时间&gt;</code>
          ，核过备份再删 <span className="mono">{s?.root}</span>{" "}
          和留着的那一版； 酒馆配置里指着它的路径清空。
        </p>
        <p className="notice mt-2">
          {s?.running ? "正在跑的酒馆（连同三条桥）会先停掉。" : ""}
          你自己的那份酒馆（不是面板装的）面板一个字都不删。
        </p>
      </ConfirmDialog>
    </>
  );

  return { open, busy, dialogs };
}

// ------------------------------------------------------------------ 小块

function Release({
  latest,
  asking,
  error,
}: {
  latest: TavernRelease | null;
  asking: boolean;
  error: string;
}) {
  if (asking && !latest) return <p className="notice">正在问 GitHub…</p>;
  if (error)
    return (
      <p className="notice notice--danger">
        {error}（仍然可以试着装：装的时候会再问一次。）
      </p>
    );
  if (!latest) return null;
  return (
    <Row>
      <span>要装的版本</span>
      <span className="break-all">
        SillyTavern {latest.version}（标签 {latest.tag}）·{" "}
        <ExternalLink href={latest.page_url}>发布页</ExternalLink>
      </span>
    </Row>
  );
}

function NodeChoice({
  nodeShort,
  winget,
  installNode,
  setInstallNode,
  have,
  need,
}: {
  nodeShort: boolean;
  winget: boolean;
  installNode: boolean;
  setInstallNode: (v: boolean) => void;
  have: string | null;
  need: string | null;
}) {
  if (!nodeShort)
    return (
      <Row>
        <span>Node.js</span>
        <span className="break-all">
          {have ?? "版本读不出"}
          {need ? `（要求 ${need}）` : ""}
        </span>
      </Row>
    );
  return (
    <div className="mt-2" data-testid="tavern-node-choice">
      <p className="notice notice--warn">
        {have
          ? `本机的 Node.js ${have} 不够新${need ? `（要求 ${need}）` : ""}。`
          : "本机没有 Node.js —— SillyTavern 是 Node 程序，没有它起不来。"}
      </p>
      {winget ? (
        <Checkbox checked={installNode} onChange={setInstallNode}>
          先用 winget 装 Node.js LTS（官方包 OpenJS.NodeJS.LTS，会弹一次 UAC）
        </Checkbox>
      ) : (
        <p className="notice mt-1">
          本机没有 winget：到{" "}
          <ExternalLink href={NODE_PAGE}>nodejs.org</ExternalLink> 下载 LTS
          版装上再来。
        </p>
      )}
    </div>
  );
}

/** 「卸载」那颗按钮（SillyTavern 卡的最后一块）。样式跟其余危险按钮一致。 */
export function UninstallButton({
  onClick,
  disabled,
}: {
  onClick: () => void;
  disabled: boolean;
}) {
  return (
    <Button
      size="sm"
      variant="danger"
      icon={<Trash2 size={12} />}
      disabled={disabled}
      onClick={onClick}
    >
      卸载面板装的那份
    </Button>
  );
}
