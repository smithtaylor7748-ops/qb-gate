import type { ErrorKind } from "./generated/ErrorKind";

/**
 * 后端抛过来的错误。
 *
 * # 为什么不是普通的 Error
 *
 * 后端的错误过 IPC 时是 `{ kind, message }`（见 `qb_foundation::error`）。
 * `message` 是给人看的中文，**随时可能改**；`kind` 是给代码看的，改它要改两边。
 *
 * 在这之前，所有错误过来都是一个裸中文字符串，页面想做条件处理只能
 * `message.includes("重复")`。那条路一定会烂：改文案的人不知道某个页面正靠
 * 那几个字做分支，症状是错误处理**静默失效** —— 不报错，只是不再生效。
 *
 * 所以：**判断看 `kind`，显示看 `message`。**
 */
export class IpcError extends Error {
  readonly kind: ErrorKind;

  constructor(kind: ErrorKind, message: string) {
    super(message);
    this.name = "IpcError";
    this.kind = kind;
  }

  /** 这是使用者自己点的取消，不是故障 —— 别弹红色报错。 */
  get cancelled(): boolean {
    return this.kind === "cancelled";
  }
}

/**
 * 把 `invoke` 抛出来的任何东西收敛成 [`IpcError`]。
 *
 * 三种来源都要认：
 *
 * | 来源 | 形状 |
 * |---|---|
 * | `GateError` | `{ kind, message }` |
 * | Tauri 自己的失败（参数反序列化、命令不存在） | 一个字符串 |
 * | 别的意外 | 什么都可能 |
 *
 * 后两种归到 `other` —— 它们确实没有分类，硬编一个只会骗人。
 */
export function toIpcError(e: unknown): IpcError {
  if (e instanceof IpcError) return e;
  if (
    e !== null &&
    typeof e === "object" &&
    "kind" in e &&
    "message" in e &&
    typeof (e as { message: unknown }).message === "string"
  ) {
    const { kind, message } = e as { kind: unknown; message: string };
    return new IpcError(
      typeof kind === "string" ? (kind as ErrorKind) : "other",
      message,
    );
  }
  if (typeof e === "string") return new IpcError("other", e);
  if (e instanceof Error) return new IpcError("other", e.message);
  return new IpcError("other", String(e));
}
