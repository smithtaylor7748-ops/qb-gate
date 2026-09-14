import type { Channels } from "./generated/Channels";

/**
 * 事件通道名。**与 Rust 侧 `qb_contract::channels` 是同一份。**
 *
 * # 为什么不直接写字符串
 *
 * 打错一个字母不会有任何报错 —— 后端照样 emit，前端照样 listen，
 * 只是两边不是同一个通道。症状是「进度条不动」「切完账户托盘没刷新」，
 * 看起来像别的 bug，而且查起来要先怀疑到「名字打错了」才找得到。
 *
 * 值写在这里、类型来自 ts-rs 生成的 [`Channels`]：
 * Rust 那边一改名，这个对象的类型标注当场编译失败。
 */
export const CHANNELS: Channels = {
  task: "gate://task",
  workspace_changed: "workspace://changed",
  navigate: "workspace://navigate",
};
