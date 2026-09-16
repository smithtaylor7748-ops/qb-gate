/**
 * 客户端配置那几个开关的判定。
 *
 * 这些测试钉的是**读和写必须是同一个断言的两面**。0.17.0 的第一版读得太松
 * （只看「键在不在」），后果是一个把 `CLAUDE_CODE_EFFORT_LEVEL` 设成
 * `"medium"` 的人会看到「最大强度思考」勾着，取消勾选把他的设置整个删掉。
 */

import { describe, expect, it } from "vitest";

import {
  applyModels,
  CODEX_COMPACT_LIMIT,
  CODEX_ONE_M,
  compose,
  hasOneM,
  jsonProblem,
  LEGACY_SUBAGENT_KEY,
  MODEL_ROLES,
  QUICK_SWITCHES,
  readModels,
  removeTomlInt,
  setOneM,
  setTomlInt,
  setTomlStr,
  stripOneM,
  tomlInt,
  tomlStr,
  type ConfigObject,
} from "./clientConfig";

const sw = (id: string) => QUICK_SWITCHES.find((s) => s.id === id)!;

describe("六个快捷开关", () => {
  it("读回来的状态跟刚写进去的一致", () => {
    // 这是最基本的那条：写完再读必须一样。不一样的话界面会在保存之后
    // 自己把勾弹回去，看起来像「保存没生效」。
    for (const s of QUICK_SWITCHES) {
      const on: ConfigObject = {};
      s.apply(on, true);
      expect(s.read(on), `${s.id} 打开后读不出来`).toBe(true);
      s.apply(on, false);
      expect(s.read(on), `${s.id} 关掉后还读成开着`).toBe(false);
    }
  });

  it("关掉最后一个 env 键时连 env 一起删", () => {
    // ⛔ 留一个空的 `env: {}` 不影响客户端运行，但会让「跟随表单」算出来的
    // JSON 跟手写的那份永远不相等，于是界面一直停在「已手改」。
    const o: ConfigObject = {};
    sw("disableAutoUpgrade").apply(o, true);
    expect(o.env).toEqual({ DISABLE_AUTOUPDATER: "1" });
    sw("disableAutoUpgrade").apply(o, false);
    expect("env" in o).toBe(false);
  });

  it("env 里还有别的键时不删 env", () => {
    const o: ConfigObject = { env: { SOMETHING_ELSE: "x" } };
    sw("disableAutoUpgrade").apply(o, true);
    sw("disableAutoUpgrade").apply(o, false);
    expect(o.env).toEqual({ SOMETHING_ELSE: "x" });
  });

  it("⛔ 思考等级设成别的档位不算「最大强度」开着", () => {
    // 松判定的后果：他一取消勾选，medium 就没了，而他以为自己只是关掉了
    // 一个本来就没开的开关。
    const medium: ConfigObject = {
      env: { CLAUDE_CODE_EFFORT_LEVEL: "medium" },
    };
    expect(sw("effortMax").read(medium)).toBe(false);
    const max: ConfigObject = { env: { CLAUDE_CODE_EFFORT_LEVEL: "max" } };
    expect(sw("effortMax").read(max)).toBe(true);
  });

  it("⛔ 真的配了署名模板不算「隐藏署名」开着", () => {
    const real: ConfigObject = {
      attribution: { commit: "Co-Authored-By: 某某", pr: "由某某生成" },
    };
    expect(sw("hideAttribution").read(real)).toBe(false);
    // 两项都得是空串才算隐藏 —— 只空了一半不算。
    const half: ConfigObject = { attribution: { commit: "", pr: "x" } };
    expect(sw("hideAttribution").read(half)).toBe(false);
  });

  it("数字 1 和字符串 1 都算开着", () => {
    // 手写 JSON 的人两种都会写。只认字符串的话，写了数字的人会看到
    // 开关是关的，一勾一取消反而把他的设置改了。
    expect(
      sw("disableAutoUpgrade").read({ env: { DISABLE_AUTOUPDATER: 1 } }),
    ).toBe(true);
    expect(
      sw("disableAutoUpgrade").read({ env: { DISABLE_AUTOUPDATER: "1" } }),
    ).toBe(true);
  });

  it("Tool Search 写 true，但 1 也读作开着", () => {
    const o: ConfigObject = {};
    sw("enableToolSearch").apply(o, true);
    expect((o.env as ConfigObject).ENABLE_TOOL_SEARCH).toBe("true");
    expect(
      sw("enableToolSearch").read({ env: { ENABLE_TOOL_SEARCH: "1" } }),
    ).toBe(true);
  });

  it("默认勾上的只有「禁用自动升级」", () => {
    // 客户端自己升级出来的新副本没有 Deny ACE，是现成的绕过入口 ——
    // 这一项默认开着是安全决定，不是偏好。
    const on = QUICK_SWITCHES.filter((s) => s.defaultOn).map((s) => s.id);
    expect(on).toEqual(["disableAutoUpgrade"]);
  });
});

describe("1M 上下文标记", () => {
  it("大小写都认得出来", () => {
    // 手写的人会写小写。认不出来的话，勾选框显示成没勾，
    // 一勾就变成 `x[1m][1M]`。
    expect(hasOneM("claude-opus-5[1M]")).toBe(true);
    expect(hasOneM("claude-opus-5[1m]")).toBe(true);
    expect(hasOneM("claude-opus-5")).toBe(false);
  });

  it("加了再去掉回到原样，不会叠加", () => {
    const base = "claude-opus-5";
    const marked = setOneM(base, true);
    expect(marked).toBe("claude-opus-5[1M]");
    expect(setOneM(marked, true)).toBe("claude-opus-5[1M]");
    expect(setOneM(marked, false)).toBe(base);
    expect(stripOneM(marked)).toBe(base);
  });

  it("空模型名加不上标记", () => {
    // 加上去会变成一个只有 `[1M]` 的模型名，上游回一个看不懂的 400。
    expect(setOneM("", true)).toBe("");
    expect(setOneM("   ", true)).toBe("");
  });
});

describe("模型映射", () => {
  it("空串删键，不写空值", () => {
    // ⛔ 写空值的话客户端会拿空字符串当模型名去请求。
    const o: ConfigObject = { env: { ANTHROPIC_MODEL: "x" } };
    applyModels(o, { ANTHROPIC_MODEL: "" });
    expect("env" in o).toBe(false);
  });

  it("⛔ 每次写都删掉旧的 ANTHROPIC_SMALL_FAST_MODEL", () => {
    // 两个键都留着的话，客户端读哪个取决于它自己的优先级 —— 而那个优先级
    // 我们看不见，于是「我明明改了，子代理还在走旧模型」查不出来。
    const o: ConfigObject = {
      env: { [LEGACY_SUBAGENT_KEY]: "claude-haiku-4-5" },
    };
    applyModels(o, { CLAUDE_CODE_SUBAGENT_MODEL: "claude-haiku-4-5" });
    const env = o.env as ConfigObject;
    expect(LEGACY_SUBAGENT_KEY in env).toBe(false);
    expect(env.CLAUDE_CODE_SUBAGENT_MODEL).toBe("claude-haiku-4-5");
  });

  it("写进去再读出来是同一份", () => {
    const map: Record<string, string> = {};
    for (const r of MODEL_ROLES) {
      map[r.key] = `${r.key.toLowerCase()}-model`;
      if (r.nameKey) map[r.nameKey] = `${r.label} 显示名`;
    }
    const o: ConfigObject = {};
    applyModels(o, map);
    expect(readModels(o)).toEqual(map);
  });

  it("五个角色带显示名，主模型和子代理不带", () => {
    // 形状照 cc-switch 的 `ClaudeModelEnvField`：只有四个 DEFAULT 档
    // 有 `_NAME`。给主模型也加一个的话，写进去的键客户端不认。
    const withName = MODEL_ROLES.filter((r) => r.nameKey).map((r) => r.key);
    expect(withName).toEqual([
      "ANTHROPIC_DEFAULT_OPUS_MODEL",
      "ANTHROPIC_DEFAULT_SONNET_MODEL",
      "ANTHROPIC_DEFAULT_FABLE_MODEL",
      "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    ]);
  });
});

describe("跟随表单", () => {
  it("compose 是幂等的 —— 同样的开关算两次结果一样", () => {
    // 不幂等的话，界面会在「跟随」状态下把自己判成「已手改」。
    const on = { disableAutoUpgrade: true, effortMax: true };
    const models = { ANTHROPIC_MODEL: "claude-opus-5" };
    const once = compose("{}", on, models);
    expect(compose(once, on, models)).toBe(once);
  });

  it("坏 JSON 当空对象处理，不抛", () => {
    // 抛出去的话整个弹窗白屏，而人只是少打了一个引号。
    expect(() => compose("{ bad", {}, {})).not.toThrow();
  });
});

describe("JSON 报错分类", () => {
  it("四类手误各说各的", () => {
    // ⛔ 只说「格式错误」等于没说 —— 人还得自己一行行找。
    expect(jsonProblem('{"a":1,}')).toContain("尾逗号");
    expect(jsonProblem("{a:1}")).toContain("键没加引号");
    expect(jsonProblem('{"a":{"b":1}')).toContain("括号不配对");
    expect(jsonProblem('{"a": example.com}')).toContain("没加引号");
  });

  it("合法 JSON 不报错", () => {
    expect(jsonProblem('{"env":{"X":"1"}}')).toBeNull();
  });
});

describe("Codex 的 TOML 顶级字段", () => {
  const toml = [
    'model = "gpt-5"',
    'model_reasoning_effort = "high"',
    "",
    "[model_providers.qb_relay]",
    "model_context_window = 123",
    "",
  ].join("\n");

  it("⛔ 只看第一个 [section] 之前的部分", () => {
    // section 底下的同名键改错了地方，症状是「我明明设了，它没生效」。
    expect(tomlInt(toml, "model_context_window")).toBeUndefined();
    expect(tomlStr(toml, "model_reasoning_effort")).toBe("high");
  });

  it("新字段插在顶级区末尾，不掉进 section 里", () => {
    const out = setTomlInt(toml, "model_context_window", CODEX_ONE_M);
    const lines = out.split("\n");
    const at = lines.findIndex((l) => l.startsWith("model_context_window ="));
    const section = lines.findIndex((l) => l.startsWith("["));
    expect(at).toBeGreaterThanOrEqual(0);
    expect(at).toBeLessThan(section);
    expect(tomlInt(out, "model_context_window")).toBe(CODEX_ONE_M);
  });

  it("已有的字段改而不是再插一行", () => {
    const once = setTomlInt(toml, "model_auto_compact_token_limit", 1);
    const twice = setTomlInt(
      once,
      "model_auto_compact_token_limit",
      CODEX_COMPACT_LIMIT,
    );
    const n = twice
      .split("\n")
      .filter((l) => l.startsWith("model_auto_compact_token_limit")).length;
    expect(n).toBe(1);
    expect(tomlInt(twice, "model_auto_compact_token_limit")).toBe(
      CODEX_COMPACT_LIMIT,
    );
  });

  it("删掉之后读不到，再删一次不报错", () => {
    const added = setTomlInt(toml, "model_context_window", CODEX_ONE_M);
    const gone = removeTomlInt(added, "model_context_window");
    expect(tomlInt(gone, "model_context_window")).toBeUndefined();
    expect(() => removeTomlInt(gone, "model_context_window")).not.toThrow();
  });

  it("字符串字段写、改、删", () => {
    let t = setTomlStr("", "model_reasoning_effort", "low");
    expect(tomlStr(t, "model_reasoning_effort")).toBe("low");
    t = setTomlStr(t, "model_reasoning_effort", "medium");
    expect(tomlStr(t, "model_reasoning_effort")).toBe("medium");
    t = setTomlStr(t, "model_reasoning_effort", null);
    expect(tomlStr(t, "model_reasoning_effort")).toBeUndefined();
  });
});
