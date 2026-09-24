import { describe, expect, it } from "vitest";

import { applyTaskProgress, stateOf, type TaskProgress } from "./tasks";

function progress(p: Partial<TaskProgress> & { task: string }): TaskProgress {
  return { phase: "", step: 0, total: 0, done: false, ...p };
}

describe("applyTaskProgress", () => {
  it("只带日志的事件不许把当前段的标题抹掉", () => {
    // `events::Reporter::log` 发出来的 `phase` 是空串。照抄过去，界面就退回
    // 「启动中…」—— **日志刷得越勤，能看见的信息越少**。
    // 酒馆那条最长要等三分钟，正是最需要那句标题的地方。
    const task = "tavern-start-test-log";
    applyTaskProgress(
      progress({ task, phase: "等酒馆 HTTP 服务就绪", step: 5, total: 5 }),
    );
    applyTaskProgress(progress({ task, step: 5, total: 5, log: "已等 8 秒" }));

    const s = stateOf(task);
    expect(s.phase).toBe("等酒馆 HTTP 服务就绪");
    expect(s.log).toEqual(["已等 8 秒"]);
  });

  it("带了新标题就换成新的", () => {
    const task = "tavern-start-test-phase";
    applyTaskProgress(progress({ task, phase: "启动酒馆", step: 4, total: 5 }));
    applyTaskProgress(progress({ task, phase: "等它应答", step: 5, total: 5 }));
    expect(stateOf(task).phase).toBe("等它应答");
  });

  it("done 与 error 照常落下来", () => {
    const task = "tavern-start-test-done";
    applyTaskProgress(progress({ task, phase: "跑着", step: 1, total: 5 }));
    applyTaskProgress(
      progress({ task, step: 5, total: 5, done: true, error: "超时了" }),
    );
    const s = stateOf(task);
    expect(s.running).toBe(false);
    expect(s.finished).toBe(true);
    expect(s.error).toBe("超时了");
    // 出错那一条也没带标题 —— 标题仍然是最后一次说清楚的那句。
    expect(s.phase).toBe("跑着");
  });
});
