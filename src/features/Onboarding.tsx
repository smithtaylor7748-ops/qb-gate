/**
 * 新手引导。
 *
 * 顺序是有讲究的，不能重排：
 *
 *   1. 先填出口 IP 并加白名单 —— 白名单为空时门禁**永远判不过**，
 *      这一步没做完，后面装的软件会被自己的锁关在门外。
 *   2. 再实测出口 IP 对不对 —— 填错了比没填更危险：界面显示「已配置」，
 *      而实际上每一轮巡检都会把 Claude 收掉。
 *   3. 然后看本机有没有装过。装过的那条路是**彻底清除**，不是覆盖安装 ——
 *      旧的登录态留着，换账户这件事从第一天起就是假的。
 *   4. 建槽位。
 *   5. 启动。
 *
 * 第 3 步的删除**面板自己不动手**，出提示词让 Codex 去做。这跟 DNS 排查、
 * 浏览器重装是同一条规矩：不可逆的破坏性操作，面板只负责把代价讲清楚、
 * 把指令给全，动手的是使用者自己启动的模型。面板替他删，出了事没人能复盘。
 */
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Check, ChevronRight, Play, RotateCw, Trash2 } from "lucide-react";

import { api } from "../lib/api";
import { R } from "../lib/resources";
import { invalidate, useResource, useSession } from "../lib/store";
import { markStep } from "../lib/progress";
import { slotName } from "../lib/slotName";
import { workspaceApi, useAction } from "../lib/workspace";
import { requestNewSlot } from "../pages/accounts/AccountDialogs";
import { BROWSER_REINSTALL_PROMPT, CLEAN_REINSTALL_PROMPT } from "../prompts";
import {
  Button,
  Card,
  CodeBlock,
  Collapsible,
  Metric,
  PageHeader,
  Row,
  useToast,
} from "../ui";

const STEPS = [
  "填写出口 IP",
  "实测核对",
  "清理或安装",
  "建立账户槽位",
  "启动",
] as const;

/** 只接受明确的 IPv4 / IPv6 字面量。留空、带端口、带网段一律拒绝。 */
function ipError(value: string): string | undefined {
  const v = value.trim();
  if (!v) return "请填写一个出口 IP";
  if (v.includes("/")) return "不要填网段，只填一个地址";
  const v4 = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(v);
  if (v4) {
    return v4.slice(1).every((n) => Number(n) <= 255)
      ? undefined
      : "IPv4 的每一段都不能大于 255";
  }
  if (/^[0-9a-fA-F:]+$/.test(v) && v.includes(":")) return undefined;
  return "看起来不是一个 IPv4 或 IPv6 地址";
}

export default function Onboarding() {
  const navigate = useNavigate();
  const toast = useToast();
  const action = useAction();
  const [step, setStep] = useSession("onboarding.step", 0);
  const [typed, setTyped] = useSession("onboarding.ip", "");
  const [touched, setTouched] = useState(false);

  const ip = useResource("ip", R.ip);
  const gate = useResource("gate", R.gate);
  const software = useResource("software", R.software);
  const traces = useResource("traces", R.traces);
  const accounts = useResource("accounts", R.accounts);

  const error = touched ? ipError(typed) : undefined;
  const measured = ip.data?.ip;
  const matches = !!measured && measured === typed.trim();

  const sw = software.data;
  const installedClaude = !!sw?.claudeCode.installed;
  const installedDesktop = !!sw?.claudeDesktop.installed;
  const hasTraces = (traces.data?.traces.length ?? 0) > 0;
  const chromeInstalled = !!traces.data?.chrome_installed;
  const needsCleanup = installedClaude || installedDesktop || hasTraces;
  const slots = accounts.data?.slots ?? [];

  async function saveIp() {
    setTouched(true);
    if (ipError(typed)) return;
    const value = typed.trim();
    const current = await api.allowlistRead();
    if (!current.includes(value)) {
      await api.allowlistWrite([...current, value]);
    }
    invalidate("gate");
    toast.ok(`已加入白名单：${value}`);
    setStep(1);
  }

  async function verify() {
    await ip.refresh();
    const r = await api.probeIp();
    if (r.ip === typed.trim()) {
      await markStep(
        "iplock",
        "passed",
        "low",
        `出口 IP ${r.ip} 已加入白名单并实测一致`,
      );
      toast.ok("实测一致");
      setStep(2);
    } else {
      toast.error(`实测是 ${r.ip}，与填写的不一致`);
    }
  }

  async function useMeasured() {
    if (!measured) return;
    setTyped(measured);
    const current = await api.allowlistRead();
    if (!current.includes(measured)) {
      await api.allowlistWrite([...current, measured]);
    }
    invalidate("gate");
    toast.ok(`已改用实测值 ${measured}`);
  }

  return (
    <>
      <PageHeader
        title="新手引导"
        sub="按顺序走一遍。每一步都可以停下，进度会留着。"
        actions={
          <Button
            variant="ghost"
            onClick={() => {
              void markStep(
                "accounts",
                "skipped",
                "unknown",
                "使用者跳过了新手引导",
              );
              navigate("/");
            }}
          >
            跳过引导
          </Button>
        }
      />

      <ol className="qb-onboarding-steps">
        {STEPS.map((name, i) => (
          <li
            key={name}
            className={i === step ? "active" : i < step ? "done" : ""}
          >
            <span>{i < step ? <Check size={13} /> : i + 1}</span>
            {name}
          </li>
        ))}
      </ol>

      {/* ---------------------------------------------- 1. 填 IP 并加白名单 */}
      {step === 0 && (
        <Card title="① 填写你的出口 IP">
          <p className="notice mb-3">
            门禁靠白名单放行。<strong>白名单为空时它永远判不过</strong>
            —— 先把这一条填上，后面装的软件才不会被自己的锁关在门外。
          </p>
          <label className="field-label" htmlFor="onboarding-ip">
            出口 IP
          </label>
          <input
            id="onboarding-ip"
            className="input"
            value={typed}
            placeholder="例如 203.0.113.7"
            onChange={(e) => setTyped(e.target.value)}
            onBlur={() => setTouched(true)}
          />
          {error && <p className="field-error">{error}</p>}
          {measured && (
            <p className="field-hint mt-2">
              面板刚才测到的是 <code>{measured}</code>
              。不确定就先填这个。
            </p>
          )}
          <div className="mt-3 flex gap-2">
            <Button
              variant="primary"
              icon={<ChevronRight size={14} />}
              loading={!!action.pending}
              onClick={() => void action.run("ip", saveIp)}
            >
              加入白名单并继续
            </Button>
            {measured && measured !== typed.trim() && (
              <Button onClick={() => setTyped(measured)}>填入实测值</Button>
            )}
          </div>
        </Card>
      )}

      {/* ---------------------------------------------- 2. 实测核对 */}
      {step === 1 && (
        <Card title="② 实测出口 IP 对不对" tone={matches ? "ok" : "accent"}>
          <p className="notice mb-3">
            填错了比没填更危险：界面上会显示「已配置」，而每一轮巡检都会把
            正在用的 Claude 收掉，未保存的对话会丢。所以这一步要实际测一次。
          </p>
          <div className="grid gap-2 sm:grid-cols-2">
            <Metric label="你填的">{typed.trim() || "—"}</Metric>
            <Metric label="实测">{measured ?? "未检测"}</Metric>
          </div>
          {measured && !matches && (
            <p className="notice notice--danger mt-3">
              两者不一致。要么改用实测值，要么确认你的代理/节点确实是你填的那个
              出口 —— 面板不替你猜哪个才对。
            </p>
          )}
          <div className="mt-3 flex flex-wrap gap-2">
            <Button
              variant="primary"
              icon={<RotateCw size={14} />}
              loading={!!action.pending}
              onClick={() => void action.run("verify", verify)}
            >
              实测并核对
            </Button>
            {measured && !matches && (
              <Button
                disabled={!!action.pending}
                onClick={() => void action.run("use", useMeasured)}
              >
                改用实测值 {measured}
              </Button>
            )}
            <Button variant="ghost" onClick={() => setStep(0)}>
              返回上一步
            </Button>
          </div>
        </Card>
      )}

      {/* ---------------------------------------------- 3. 清理或安装 */}
      {step === 2 && (
        <>
          <Card title="③ 这台机器的现状">
            <div className="grid gap-2 sm:grid-cols-3">
              <Metric label="Claude Code">
                {installedClaude ? "已安装" : "未安装"}
              </Metric>
              <Metric label="Claude 桌面端">
                {installedDesktop ? "已安装" : "未安装"}
              </Metric>
              <Metric label="登录痕迹">
                {traces.data
                  ? hasTraces
                    ? `${traces.data.traces.length} 处`
                    : traces.data.chrome_scanned
                      ? traces.data.chrome_files_locked > 0
                        ? `未发现（${traces.data.chrome_files_locked} 个没读开）`
                        : "未发现"
                      : "没扫成"
                  : "未检测"}
              </Metric>
            </div>
            {/* Chrome 开着也照扫了（见 `chrome.rs` 的「Chrome 开着照扫」），
                所以这里按**读开了几个**说话，不按「开没开」说话。 */}
            {traces.data && !traces.data.chrome_scanned && (
              <p className="notice notice--warn mt-3">
                一个 Chrome 资料文件都没读开，这一项没扫成。
                <strong>这不等于「没有痕迹」</strong>
                ——资料目录可能不在默认位置，或者被别的程序锁着。
              </p>
            )}
            {traces.data &&
              traces.data.chrome_scanned &&
              traces.data.chrome_files_locked > 0 && (
                <p className="notice notice--warn mt-3">
                  读开 {traces.data.chrome_files_read} 个，还有{" "}
                  {traces.data.chrome_files_locked} 个没读开
                  {traces.data.chrome_running ? "（Chrome 正开着）" : ""}。
                  <strong>上面的结论只覆盖读开的那部分</strong>
                  ——关掉 Chrome 再检测一次能扫全。
                </p>
              )}
            <div className="mt-3 flex gap-2">
              <Button
                disabled={!!action.pending}
                onClick={() =>
                  void action.run("scan", async () => {
                    await software.refresh();
                    await traces.refresh();
                  })
                }
              >
                重新检测
              </Button>
            </div>
          </Card>

          {needsCleanup ? (
            <Card title="彻底清除旧的 Claude 与浏览器" tone="danger">
              <p className="notice mb-3">
                这台机器上已经有 Claude 或登录痕迹。
                <strong>覆盖安装不解决问题</strong>
                ——旧的登录态留着，「换账户」这件事从第一天起就是假的。
              </p>
              <p className="notice notice--danger mb-3">
                删除**不可恢复**：浏览器的书签、保存的密码、自动填充、扩展及其数据、
                所有网站的 Cookie 与登录态会一起没，不只是 claude.ai。
                动手之前请自己先导出书签。
              </p>
              <p className="notice mb-3">
                面板自己不执行这些删除。下面两份提示词交给 Codex
                去做——不可逆的操作由你启动的模型执行，出了事有完整记录可以复盘。
              </p>

              <div className="mb-3 flex flex-wrap gap-2">
                <Button
                  variant="primary"
                  icon={<Play size={14} />}
                  disabled={!!action.pending}
                  onClick={() =>
                    void action.run(
                      "codex",
                      () => workspaceApi.launch("codex", "official", ""),
                      "Codex 已启动，把下面的提示词贴进去",
                    )
                  }
                >
                  启动 Codex
                </Button>
                {!sw?.codex.installed && (
                  <Button
                    disabled={!!action.pending}
                    onClick={() => navigate("/software")}
                  >
                    先去装 Codex
                  </Button>
                )}
              </div>

              <CodeBlock
                text={CLEAN_REINSTALL_PROMPT.body}
                caption={CLEAN_REINSTALL_PROMPT.title}
              />
              <div className="mt-3">
                <CodeBlock
                  text={BROWSER_REINSTALL_PROMPT.body}
                  caption={BROWSER_REINSTALL_PROMPT.title}
                />
              </div>

              <Collapsible className="mt-3" summary="为什么不让面板直接删？">
                <p className="notice">
                  删注册表项、删浏览器资料目录这类操作判断错一次就会误删无关数据，
                  而面板没有上下文去判断哪个目录是你真正在用的。让模型在你眼前
                  逐步执行、每一步都能叫停，比一个按钮背后藏着递归删除安全得多。
                </p>
              </Collapsible>

              <div className="mt-3 flex gap-2">
                <Button
                  variant="primary"
                  icon={<Check size={14} />}
                  onClick={() => setStep(3)}
                >
                  清理完了，继续
                </Button>
                <Button variant="ghost" onClick={() => setStep(3)}>
                  暂时跳过清理
                </Button>
              </div>
            </Card>
          ) : (
            <Card title="这台机器是干净的，直接装" tone="ok">
              <p className="notice mb-3">
                没有检测到 Claude 安装或登录痕迹
                {chromeInstalled ? "" : "，也没有装 Chrome"}
                。不需要删任何东西。
              </p>
              <Row
                side={
                  <Button
                    size="sm"
                    disabled={!!action.pending}
                    onClick={() => navigate("/software")}
                  >
                    去安装
                  </Button>
                }
              >
                <span>在「软件」页装 Claude Code 与桌面端。</span>
              </Row>
              {!chromeInstalled && (
                <Row
                  className="mt-2"
                  side={
                    <Button
                      size="sm"
                      icon={<Trash2 size={13} />}
                      disabled={!!action.pending}
                      onClick={() =>
                        void action.run(
                          "chrome",
                          api.chromeReinstall,
                          "已装上一个全新的 Chrome",
                        )
                      }
                    >
                      装一个新的
                    </Button>
                  }
                >
                  <span>没装过 Chrome，面板可以直接装一个干净的。</span>
                </Row>
              )}
              <div className="mt-3">
                <Button
                  variant="primary"
                  icon={<ChevronRight size={14} />}
                  onClick={() => setStep(3)}
                >
                  继续
                </Button>
              </div>
            </Card>
          )}
        </>
      )}

      {/* ---------------------------------------------- 4. 建槽位 */}
      {step === 3 && (
        <Card title="④ 建立一个账户槽位">
          <p className="notice mb-3">
            槽位就是一份独立的登录资料。每个账户一个槽位，切换时面板换的是指向，
            <strong>不复制凭证</strong>——任意时刻只有一个账户是激活的。
          </p>
          {slots.length > 0 ? (
            <div className="grid gap-2">
              {slots.map((s) => (
                <Row key={s.label} side={s.active ? "当前" : ""}>
                  <span>
                    {slotName(s.email, s.label)}
                    {s.logged_in ? "" : " · 尚未登录"}
                  </span>
                </Row>
              ))}
            </div>
          ) : (
            <p className="notice notice--warn">还没有任何槽位。</p>
          )}
          <div className="mt-3 flex gap-2">
            <Button variant="primary" onClick={requestNewSlot}>
              新建槽位
            </Button>
            <Button
              disabled={slots.length === 0}
              icon={<ChevronRight size={14} />}
              onClick={() => setStep(4)}
            >
              继续
            </Button>
          </div>
        </Card>
      )}

      {/* ---------------------------------------------- 5. 启动 */}
      {step === 4 && (
        <Card title="⑤ 启动" tone="ok">
          <p className="notice mb-3">
            门禁会在启动前验一次出口 IP。
            <strong>验不过一个进程都不会起</strong>
            ，不会出现「起来了但没受保护」。
          </p>
          <div className="flex flex-wrap gap-2">
            <Button
              variant="primary"
              icon={<Play size={14} />}
              loading={action.pending === "launch"}
              disabled={!!action.pending}
              onClick={() =>
                void action
                  .run(
                    "launch",
                    () => workspaceApi.launch("claude-code", "official", ""),
                    "Claude Code 已启动",
                  )
                  .then(async (r) => {
                    if (!r) return;
                    await markStep(
                      "accounts",
                      "passed",
                      "low",
                      "已完成新手引导并成功启动",
                    );
                    navigate("/");
                  })
              }
            >
              启动 Claude Code
            </Button>
            <Button
              disabled={!!action.pending}
              onClick={() =>
                void action.run(
                  "desktop",
                  () => workspaceApi.launch("claude-desktop", "official", ""),
                  "Claude 桌面端已启动",
                )
              }
            >
              启动桌面端
            </Button>
            <Button variant="ghost" onClick={() => navigate("/")}>
              先不启动，去面板
            </Button>
          </div>
          {gate.data?.allowlist.length === 0 && (
            <p className="notice notice--danger mt-3">
              白名单现在是空的，启动一定会失败。回到第一步补上。
            </p>
          )}
        </Card>
      )}

      <p className="qb-help-note">
        每一步都只做它写着的那件事。跳过的步骤会记在进度里，之后可以从设置页
        重新打开这份引导。
      </p>
    </>
  );
}
