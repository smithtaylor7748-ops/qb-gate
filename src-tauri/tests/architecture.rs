//! 架构约束。**这是整个分层重构唯一的防退化机制。**
//!
//! # 为什么需要它
//!
//! 体检时实测出 15 对循环依赖，其中 9 对缠着 `gate` —— 而没有任何东西
//! 阻止它们长回来。分层只要没有机器守着，半年后一定退化回去：
//! 每一次「就先这么调一下」单独看都合理，合起来就是现在这个样子。
//!
//! # 棘轮（ratchet）设计
//!
//! 现存的环列在 [`KNOWN_CYCLES`] 里。这条测试断言两件事：
//!
//! 1. **不许出现新的环** —— 名单外的环一出现就红；
//! 2. **名单不许有多余项** —— 某对环被断掉之后必须把它从名单里删掉，
//!    否则测试同样红。
//!
//! 第二条比第一条重要：没有它，名单会变成一张只增不减的「豁免清单」，
//! 而豁免清单就是分层烂掉的标准路径。
//!
//! # 它怎么判
//!
//! 扫 `src/` 下所有 `.rs`，**先剥掉注释与字符串**再找 `crate::<模块>`。
//! 不剥的话文档注释里一句 `` `use crate::gate` `` 就会被算成真依赖 ——
//! 第一版就踩了这个，报出两对根本不存在的环。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// 还没断干净的环。**只许变短，不许变长。**
///
/// A0 断掉三对（`config_io↔gate`、`gate↔plugins`、`gate↔sessions`）：把
/// `state_dir` 与 `log::write` 从 `gate` 挪到 `paths` / `audit` 之后，
/// 那三个模块对门禁的全部需求就消失了。
///
/// A1 再断两对（`gate↔killswitch`、`gate↔tray`）：把 `stop_managed` 与
/// `run_watchdog` 这两段跨域编排搬进 `usecase::gate_ops` 之后，`gate` 不再
/// 反过来调杀进程与托盘。
///
/// A2 断掉 `gate↔settings`：`settings::save` 原来自己调 `gate::invalidate_verdict`，
/// 现在唯一的保存路径是 `usecase::settings_ops::save`，`settings` 那边只剩
/// `pub(crate) fn write_only`。
///
/// A3 断掉 `config_io↔relay`：`secret`（DPAPI 封装）只依赖 `error`，是平台层的
/// 东西，却一直塞在 `relay` 里 —— 于是 config_io / repository / workspace /
/// extensions 四个模块为了加解密而依赖了中转站。搬到 `crate::secret` 即可。
///
/// 同步断掉 `operations↔startup`：`operations` 每次拿锁前要问「启动恢复完成没」，
/// 而那个状态住在 `startup` 里，`startup` 又要用 `operations` 的锁。就绪态本身
/// 不属于「启动流程」而属于「进程就绪」，搬到 `crate::readiness` 两边就都只向下了。
///
/// 以及 `install↔killswitch`：`signer_of`（问一个 exe 是谁签的）住在杀进程模块里，
/// 于是 install 的五处验签都依赖了 killswitch。搬到 `crate::signature` 即可。
///
/// 以及 `accounts↔plugins`：`bridge_data_dir` 是个零依赖的纯路径函数，
/// 却住在插件里，而 `AccountRoots::current` 要用它。搬到 `paths::tavern_bridge_dir`。
///
/// 最后一刀断两对（`extensions↔workspace`、`gate↔workspace`）：`environment_dir` 搬进
/// `config_io`（它本来就跟 `ensure_plain_path` 是一回事），`endpoint_base` 与
/// `normalize_base_url` 合并进新的 `endpoint`。两个模块都只是为了拼一个路径、
/// 规整一个地址，就把 `workspace` 拖成了公共依赖。
///
/// `config_io↔repository`：`commit_database`（跨 SQLite 与文件的两阶段提交）
/// 要拿 `&Repository`，让一个文件 I/O 模块反过来依赖了数据库。事务边界属于
/// 拥有数据库的那一层，搬进 `repository`；文件那半边仍由 `config_io::commit_inner` 做。
///
/// `accounts↔gate` 是唯一一对需要真正**依赖反转**的：`gate::hook` 原来自己去问
/// `accounts` 该装到哪个槽位，而 `accounts::switch` 切完又要回头叫 hook 跟过去。
/// 现在 `hook` 收一个由调用方填好的 `Scope`，两步由 `usecase::{hook_ops,account_ops}`
/// 绑在一起 —— 顺带把 `run_watchdog` 里那句 `tray::refresh` 也换成了回调，
/// 否则编排层会反过来依赖接口层（棘轮当场报出了 `tray ↔ usecase`）。
///
/// 最后一对 `gate ↔ install` 分两刀断：先把不该在那儿的东西下沉
/// （`acl` 与 `acl::locked` 到平台层、`Kind::target_kind` 从清单挪回门禁、
/// `verify_signature` 不再收 `&[Target]`），15 处引用削到 8 处；剩下的 8 处
/// 全是「装机前 `lock_all`、装完 `unlock_all`、升级期间开一个 `Maintenance`
/// 窗口」—— 那是**装机编排**而不是安装自己的事，整段搬进 `usecase::install_ops`。
///
/// `install` 一直被两层身份撕扯着：`inventory`/`detect` 是「Claude 装在哪」的
/// 低层事实，`winget`/`upgrade` 是高层操作。搬走那两段编排之后这条缝才对齐 ——
/// 拆 crate 时 `qb-install` 只留事实与单步操作，编排归 `qb-app`。
///
/// # 只看「互相依赖的两个」是不够的
///
/// 第一版只找 `a→b` 且 `b→a` 的**成对**环。那漏掉了全项目最大的一个：
///
/// ```text
/// domain → workspace → repository → profile → snapshot → gate → repository
/// ```
///
/// 11 个模块缠成一个强连通分量，任意两个之间都没有直接的互指，所以成对检测
/// 全程绿灯。它被发现纯属偶然 —— 拆 crate 时发现这 11 个模块怎么分都分不开。
///
/// 所以现在算的是**强连通分量**（Tarjan）：一个分量里有两个以上模块，
/// 就说明它们互相到得了，也就拆不进不同的 crate。
///
/// 那 11 个是这么拆开的，四刀都不是「加个 trait 绕一下」，而是把放错地方的
/// 东西搬回它该在的地方：
///
/// | 刀 | 边 | 为什么它一开始就不该存在 |
/// |---|---|---|
/// | 1 | `domain → {diagnostics, workspace, extensions}` | `export_types` 顺手导了别的模块的类型。汇总名单属于开发工具，不属于数据类型 |
/// | 2 | `repository → profile` | 迁移读的是 `profiles.json` 这份**历史文件**，形状该钉死在 v1，不该复用活着的类型 |
/// | 3 | `plugins → workspace` | 为了 40 行的一条「官方目录有没有中转残留」，把整个工作区编排拖了进来。规则本身零依赖，独立成 `residue` |
///
/// # 名单空了之后
///
/// **这不代表可以放松。** 这条测试真正的价值从现在才开始：这些环是一次性
/// 还完的旧账，而新的环是一行一行长出来的。名单空着的时候，任何一个新分量
/// 都会当场变红 —— 那一刻的正确反应是把共用的部分往下提或往上编排，
/// **不是**往这个数组里加一行。
const KNOWN_CYCLES: &[&[&str]] = &[];

#[test]
fn module_cycles_only_ever_shrink() {
    let found = cycles();
    let known: BTreeSet<Vec<String>> = KNOWN_CYCLES
        .iter()
        .map(|c| {
            let mut v: Vec<String> = c.iter().map(|s| s.to_string()).collect();
            v.sort();
            v
        })
        .collect();

    let added: Vec<_> = found.difference(&known).collect();
    assert!(
        added.is_empty(),
        "出现了新的循环依赖（强连通分量）：{added:?}
         分量里的模块互相到得了，也就拆不进不同的 crate。
         分层规则是「只许向下依赖」：要么改成单向，要么把共用的那部分
         提到更低的一层（像 A0 把 state_dir/log 提成 paths/audit 那样）。"
    );

    let stale: Vec<_> = known.difference(&found).collect();
    assert!(
        stale.is_empty(),
        "这些环已经断掉了，请从 KNOWN_CYCLES 里删掉：{stale:?}
         名单留着不删就会退化成一张只增不减的豁免清单。"
    );
}

/// 最底下那两层，每个模块只许依赖名单里那几个。
///
/// 地基层的意义就在于「谁都可以依赖它，它不依赖任何人」。一旦 `paths`
/// 反过来 use 了别的模块，A0 就白做了 —— 环会立刻长回来。
///
/// 名单是**白名单**而不是「不许依赖 X」：白名单加一项要经手一次，
/// 黑名单漏一项没人知道。
#[test]
fn the_foundation_layer_depends_on_nothing_above_it() {
    // 模块 → 它唯一允许依赖的那些。空数组 = 谁都不许依赖。
    const ALLOWED: &[(&str, &[&str])] = &[
        // L0 契约：纯形状，零依赖。
        ("domain", &[]),
        ("error_kind", &[]),
        // L1 地基。
        ("paths", &[]),
        ("sink", &[]),
        // `error` 要给出分类，而分类是契约的一部分（前端按它做分支）。
        ("error", &["error_kind"]),
        // `audit` 要知道 ip-gate.log 放哪。
        ("audit", &["paths"]),
    ];
    let deps = graph();
    for (m, allowed) in ALLOWED {
        let Some(d) = deps.get(*m) else { continue };
        let extra: Vec<_> = d
            .iter()
            .filter(|x| !allowed.contains(&x.as_str()))
            .collect();
        assert!(
            extra.is_empty(),
            "{m} 在最底下两层，只许依赖 {allowed:?}，实测还依赖了 {extra:?}"
        );
    }
}

/// 扫描器真的看到了每一个 crate。
///
/// 上面两条测试都是「没找到问题就算过」—— 扫描器一旦漏掉一个 crate，
/// 它们会安安静静地全绿。这条把「漏扫」本身变成一个会红的错误：
/// 三层各点一个名，少哪一层就说明 `roots()` 没跟上目录结构的变化。
#[test]
fn the_scanner_actually_covers_every_crate() {
    let g = graph();
    for (layer, module) in [
        ("qb-contract（L0）", "domain"),
        ("qb-foundation（L1）", "paths"),
        ("qb-platform（L2）", "config_io"),
        ("src-tauri（L4）", "gate"),
    ] {
        assert!(
            g.contains_key(module),
            "依赖图里没有 `{module}` —— {layer} 整个没被扫到。
             多半是新增/改名了 crate 目录，而 roots() 没跟上。"
        );
    }
}

/// 检测器自己的单测。
///
/// **没有这条，「查不出环」和「检测器坏了」长得一模一样。** 上一版的成对检测
/// 漏掉了一个 11 模块的分量，整整一轮都在报绿。这里喂一个手写的图进去：
/// 一个三元环、一条直链、一个自环，看它认不认得出来。
#[test]
fn the_cycle_detector_can_actually_find_a_cycle() {
    fn g(pairs: &[(&str, &[&str])]) -> BTreeMap<String, BTreeSet<String>> {
        pairs
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    v.iter().map(|x| x.to_string()).collect::<BTreeSet<_>>(),
                )
            })
            .collect()
    }

    // 三元环：谁都没有直接互指，成对检测看不见它。
    let found = components(&g(&[
        ("a", &["b"][..]),
        ("b", &["c"][..]),
        ("c", &["a"][..]),
        ("d", &["a"][..]),
    ]));
    assert_eq!(
        found,
        [vec!["a".to_string(), "b".to_string(), "c".to_string()]]
            .into_iter()
            .collect::<BTreeSet<_>>()
    );

    // 直链无环。
    assert!(components(&g(&[("a", &["b"][..]), ("b", &["c"][..]), ("c", &[][..])])).is_empty());

    // 自环不算「两个模块拆不开」—— 模块引用自己是正常的。
    assert!(components(&g(&[("a", &["a"][..])])).is_empty());
}

/// brace 导入真的被看见了。
///
/// `sessions.rs` 的第一行是 `use crate::{domain::*, error::…, repository::…}` ——
/// 一个 `crate::sessions` 之外的依赖，而且是本项目最常见的写法。展开器坏掉时
/// 这条边会消失，而 [`module_cycles_only_ever_shrink`] 照样全绿。
#[test]
fn brace_imports_are_not_invisible() {
    let g = graph();
    let deps = g.get("sessions").expect("sessions 模块应该在图里");
    assert!(
        deps.contains("domain") && deps.contains("repository"),
        "sessions 用 `use crate::{{domain::*, …, repository::…}}` 导入，
         这两条边却没进图 —— expand_groups 坏了。实测依赖：{deps:?}"
    );
}

/// 看门狗仍然走判定函数，没有自己写一套。
///
/// # 这条测试顶替了一道编译期护栏
///
/// `watchdog::{Tick, StopReason, decide}` 原来是 `pub(crate)`：谁把
/// `run_watchdog` 改成自己判、不再调 `decide`，这几个就成了 dead_code，
/// `cargo clippy --lib -- -D warnings` 当场失败。拆 crate 之后
/// `run_watchdog` 住在 `usecase::gate_ops`（src-tauri）、`decide` 住在
/// `qb-iplock`，只能放宽到 `pub` —— 而 `pub` 在 `pub mod` 里逃出了 dead_code
/// 分析，那道护栏就没了。
///
/// # 为什么它值得单独一条
///
/// 这条链**真的断过一次，而且毫无症状**：有一版 `run_watchdog` 自己写了
/// 「不通过就收」，`decide` 退化成只有单测在调。当时两档 `unknown_grace()`
/// 都是 `None`，自己写的那套算出来跟 `decide` 一样，20 条单测全过。
/// 代价是 CLAUDE.md 里那句「把宽限期加回来只要改 `unknown_grace()` 一个函数」
/// 变成了假的 —— 下一个人照做、跑通全部测试、以为宽限期回来了，
/// 而实际一秒都没加上，直到某次网络抖动误杀了正在用的 Claude 才露出来。
#[test]
fn the_watchdog_still_routes_through_the_judge() {
    // 不写死文件路径：`run_watchdog` 已经搬过两次家（`gate/mod.rs` →
    // `usecase/gate_ops.rs` → `qb-app`）。按内容找它，搬到哪儿都还钉得住。
    let body = module_files()
        .values()
        .flatten()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .map(|s| strip(&s))
        .find(|s| s.contains("pub async fn run_watchdog"))
        .expect("全 workspace 找不到 `pub async fn run_watchdog` —— 它改名了？");
    let at = body.find("pub async fn run_watchdog").unwrap();
    assert!(
        body[at..].contains("watchdog::decide("),
        "run_watchdog 里没有 `watchdog::decide(` —— 判定被搬回调用方自己写了。
         判定必须留在链路上，否则 `unknown_grace()` / `judge` 那边的改动
         对真实运行毫无作用，而所有测试照样全绿。"
    );
}

/// ⛔ 中转站的熔断**绝不许**触发官方账户槽位切换。
///
/// # 为什么这是一条测试而不是一条约定
///
/// 两件事在界面上长得很像（「这条不通了，换一条」），在后果上天差地别：
///
/// - 中转线路熔断 → 换一条**中转线路**。你自己买的 Key、第三方端点，
///   Anthropic 那边看不见，换来换去没有任何风险；
/// - 官方账户槽位切换 → 动的是**官方 OAuth 身份**。自动按额度 / 429 / 限流
///   切槽位，正是 CLAUDE.md ⛔ 表里「自动换号」那一条：
///   存在这条路径，「多槽位」的定位就从「管理你自己的账户」
///   变成了「规避限制」。
///
/// 所以熔断那一侧（`qb-station`）**不许认识账户**。这一条钉的是：
/// 它的依赖里没有 `qb-accounts`，也没有 `qb-launch`（槽位切换的执行方）。
/// 认不出来，就写不出那句调用 —— 不是靠自觉，是编译期就做不到。
///
/// `crates_only_depend_downwards` 其实已经顺带拦住了这件事（两者同为 L3，
/// 而 `ALLOWED_SIDEWAYS` 里没有这一对）。但那条测试的报错只会说
/// 「同层依赖没登记」，**读不出这里的利害**；而且只要有人往
/// `ALLOWED_SIDEWAYS` 里补一行，它就不响了。这一条不给那个出口。
#[test]
fn relay_breakers_can_never_reach_account_switching() {
    let all = crate_deps();
    let (_, station) = all
        .iter()
        .find(|(name, _)| name == "qb-station")
        .expect("qb-station 不在 workspace 里了？");
    for forbidden in ["qb-accounts", "qb-launch"] {
        assert!(
            !station.iter().any(|d| d == forbidden),
            "qb-station 依赖了 {forbidden} —— 中转站的熔断因此够得着官方账户槽位。\n\
             这两条路径必须在代码上隔死：熔断换的是中转线路，\n\
             按额度 / 429 自动切官方槽位是 CLAUDE.md 明确不做的「自动换号」。"
        );
    }
}

/// 每个 crate 在哪一层。**只许往下依赖。**
///
/// # 为什么 workspace 之外还要这一条
///
/// Cargo 只保证 crate 图无环，**不保证方向**：`qb-platform` 依赖
/// `qb-iplock` 一样编得过，只要反过来没有。层号把「谁在谁下面」写死，
/// 于是「平台层依赖领域层」这种事变成一条会红的测试，而不是一次代码评审。
const LAYERS: &[(&str, u8)] = &[
    // 契约在最底下：它零依赖，只描述形状。`qb-foundation` 要用它
    // （`GateError::kind()` 返回 `ErrorKind`），所以在它上面一层。
    ("qb-contract", 0),
    ("qb-foundation", 1),
    ("qb-platform", 2),
    ("qb-probe", 3),
    ("qb-relay", 3),
    ("qb-station", 3),
    ("qb-install", 3),
    ("qb-accounts", 3),
    ("qb-iplock", 3),
    ("qb-launch", 3),
    ("qb-extensions", 3),
    ("qb-sysenv", 3),
    ("qb-app", 4),
    ("qb-gate", 5), // src-tauri 这个包就叫 qb-gate（产出 qb-gate.exe）
];

/// L3 领域 crate 之间允许的横向依赖。**只许变短。**
///
/// # 为什么不是零
///
/// 原方案写的是「L2 横向零依赖」。实测之后这条做不到，而且**不该做到**：
///
/// | 边 | 为什么它是对的 |
/// |---|---|
/// | `qb-iplock → qb-install` | 「Claude 装在哪」全项目只有一张表（CLAUDE.md 的硬规矩）。门禁要锁哪些文件，只能问那张表 |
/// | `qb-accounts → qb-install` | 槽位要认得出官方客户端写在哪，同一张表 |
/// | `qb-launch → qb-iplock` | 起会话之前必须验 IP 解锁 —— 这正是门禁存在的意义 |
/// | `qb-extensions → qb-accounts` | 扩展装进账户槽位的目录里 |
/// | `qb-sysenv → qb-accounts` · `→ qb-probe` | 体检要读槽位配置、要问出口 IP |
///
/// 强行拆成「零横向」只有两条路：把 `inventory` 再拆一个 crate 出去（那是
/// 为了满足规则而拆，不是为了表达结构），或者把这些判断上提到 `qb-app`
/// （那会让 `qb-app` 变成新的巨石）。两条都比现在坏。
///
/// 所以规则改成**显式、无环、只增不减地缩**：横向边必须写在这张表里，
/// 加一条要先说服自己，删一条则随时欢迎。没写在表里的横向边当场报红。
const ALLOWED_SIDEWAYS: &[(&str, &str)] = &[
    ("qb-accounts", "qb-install"),
    ("qb-extensions", "qb-accounts"),
    ("qb-extensions", "qb-install"),
    ("qb-iplock", "qb-install"),
    ("qb-iplock", "qb-probe"),
    ("qb-launch", "qb-install"),
    ("qb-launch", "qb-iplock"),
    ("qb-sysenv", "qb-accounts"),
    ("qb-sysenv", "qb-probe"),
];

#[test]
fn crates_only_depend_downwards() {
    let layer: BTreeMap<&str, u8> = LAYERS.iter().copied().collect();
    let allowed: BTreeSet<(&str, &str)> = ALLOWED_SIDEWAYS.iter().copied().collect();
    let mut used: BTreeSet<(String, String)> = BTreeSet::new();
    let mut problems: Vec<String> = Vec::new();

    for (name, deps) in crate_deps() {
        let Some(&mine) = layer.get(name.as_str()) else {
            problems.push(format!(
                "{name} 不在 LAYERS 表里 —— 新 crate 要先决定它在哪一层"
            ));
            continue;
        };
        for dep in deps {
            let Some(&theirs) = layer.get(dep.as_str()) else {
                problems.push(format!("{name} 依赖了不在 LAYERS 表里的 {dep}"));
                continue;
            };
            if theirs < mine {
                continue; // 向下，永远可以
            }
            if theirs > mine {
                problems.push(format!(
                    "{name}（第 {mine} 层）依赖了更高层的 {dep}（第 {theirs} 层）"
                ));
                continue;
            }
            // 同层。必须在名单里。
            let pair = (name.clone(), dep.clone());
            if allowed.contains(&(name.as_str(), dep.as_str())) {
                used.insert(pair);
            } else {
                problems.push(format!(
                    "{name} → {dep} 是一条没登记的同层依赖。\n\
                     要么把共用的那部分下沉一层，要么把这条边加进 ALLOWED_SIDEWAYS\n\
                     并写清楚为什么它是对的"
                ));
            }
        }
    }

    assert!(
        problems.is_empty(),
        "crate 分层被破坏了：\n{}",
        problems.join("\n")
    );

    let stale: Vec<_> = allowed
        .iter()
        .filter(|(a, b)| !used.contains(&(a.to_string(), b.to_string())))
        .collect();
    assert!(
        stale.is_empty(),
        "这些同层依赖已经不存在了，请从 ALLOWED_SIDEWAYS 里删掉：{stale:?}\n\
         名单留着不删就会退化成一张只增不减的豁免清单。"
    );
}

/// 领域 crate 里不许出现 `tauri`。
///
/// **这是整个拆分最实在的那条收益**，而且是编译器管不着的那一半：Cargo
/// 不在乎谁依赖 tauri。没有这条测试，往 `qb-iplock` 的 Cargo.toml 里加一行
/// `tauri.workspace = true` 是一次没人会注意的改动，而它换来的是
/// 「领域代码写不出 `fn run_watchdog(…, app: AppHandle)`」这条保证整个失效，
/// 外加那些 crate 的测试重新开始链接 tao/wry（`events.rs` 里记的
/// `0xc0000139` 崩在 main 之前）。
#[test]
fn only_the_app_crate_knows_about_tauri() {
    let mut offenders: Vec<String> = Vec::new();
    for (name, _) in crate_deps() {
        if name == "qb-gate" {
            continue; // src-tauri 本体，就该有 tauri
        }
        let path = manifest_of(&name);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            if line.starts_with("tauri")
                && line
                    .trim_start_matches("tauri")
                    .starts_with(['-', '.', ' ', '='])
            {
                offenders.push(format!("{name}：{line}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "这些 crate 的 Cargo.toml 里出现了 tauri：\n{}\n\
         领域与编排层不许认识 UI 框架。要往界面报进度就收 `sink::ProgressSink`，\n\
         要发事件就收 `sink::EventSink`，要刷界面就收回调 —— `src-tauri` 那一层\n\
         负责把真实现填进去。",
        offenders.join("\n")
    );
}

/// 谁写 `settings.json`，谁就要作废设置缓存。
///
/// # 这条缓存带着一个陷阱
///
/// C4 给 `settings::load()` 加了进程内缓存（它在看门狗的热路径上，
/// 每 15 秒要被调好几次，每次都重读盘 + 重解析 JSON）。缓存是**写时失效**
/// 而不是带 TTL 的 —— 设置直接决定门禁锁哪些文件，那不是可以「过一会儿
/// 再生效」的东西。
///
/// 代价是：任何绕开 `settings::write_without_invalidating_the_gate_verdict`
/// 直接写那个文件的路径，都必须自己调一次 `invalidate_cache()`。
/// 目前有一处这样的路径（`gate::hook::set_hook` 把 `hook_enabled` 并进
/// 同一个事务里写）。
///
/// 漏掉的症状是**界面上开关翻了，实际行为没变** —— 不报错，只是不生效。
#[test]
fn settings_writers_invalidate_the_cache() {
    let mut offenders = Vec::new();
    for (module, paths) in module_files() {
        if module == "settings" {
            continue; // 缓存本体
        }
        for p in paths {
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            let body = strip(&text);
            if body.contains("settings::path()") && !body.contains("invalidate_cache") {
                offenders.push(p.display().to_string());
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "这些文件写 settings.json 却没作废设置缓存：
{}
         加一句 `settings::invalidate_cache()`，否则 `settings::load()` 会继续
         返回旧值 —— 界面上改了，实际行为没变，而且不报任何错。",
        offenders.join(
            "
"
        )
    );
}

/// ⛔ PowerShell 只能从 `process::powershell_std` / `powershell_tokio` 起。
///
/// # 漏掉的后果不是乱码，是功能静默失效
///
/// 面板拉起的 PowerShell 都带 `CREATE_NO_WINDOW`，没有控制台；没有控制台时
/// `[Console]::OutputEncoding` 会落到系统代码页上，编不出来的字符被换成一个
/// 字面的 `?` —— 字符在 PowerShell 那头就已经丢了，Rust 再怎么解码也回不来。
///
/// 2026-09-16 实测（zh-CN）：出站锁那一页整列网卡名都是 `???`，而那串问号会被
/// 原样送回去当 `-InterfaceAlias`，规则落在一个不存在的接口上，界面却报
/// 「已加 N 条」。**一个看起来生效、实际什么都没拦的功能。**
///
/// 这跟区域设置无关：`é` / `ü` / 西里尔字母在 CP437、CP850 上同样编不出来。
/// 开源出去之后这是每一台机器的问题，所以用测试守着，不靠自觉。
#[test]
fn every_powershell_call_pins_its_output_encoding() {
    /// 两处豁免，各有各的理由 —— 加第三处之前先想清楚。
    const ALLOWED: &[&str] = &[
        // 本体：`powershell_std` / `powershell_tokio` 就在这里。
        "process.rs",
        // 跑的是 Anthropic 自己的 install.ps1，走 `-File`：脚本内容是第三方的，
        // 前奏塞不进去。改成 `-Command "& '<路径>'"` 又会把一个我们控制不了的
        // 临时路径（可能带中文用户名、带单引号）拼进脚本串里，那是更大的口子。
        // 它的输出是英文安装日志，只进日志、不参与任何判断。
        "install_ops.rs",
    ];

    let mut offenders = Vec::new();
    for (_module, paths) in module_files() {
        for p in paths {
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            if !text.contains("Command::new(\"powershell\")") {
                continue;
            }
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if ALLOWED.contains(&name.as_str()) {
                continue;
            }
            offenders.push(p.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "这些文件自己拉 PowerShell，没走 `process::powershell_std` / `powershell_tokio`：
{}
         改成那两个 —— 它们把 `-NoProfile -NonInteractive` 与 UTF-8 前奏一起钉死。
         不钉的话，非 ASCII 的网卡名、用户名、路径会在 PowerShell 那头就变成 `?`，
         而这类 bug 不报错：界面照常显示、操作照常「成功」，只是作用在一个
         不存在的名字上。",
        offenders.join(
            "
"
        )
    );
}

// ---------------------------------------------------------------- 实现
/// 每个 workspace 成员声明了哪些**本项目内部**的依赖。
///
/// 从 Cargo.toml 读，不是从源码猜：依赖关系的真相源就是清单。
fn crate_deps() -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for root in roots() {
        let dir = root.parent().expect("src 目录一定有上级");
        let Ok(text) = std::fs::read_to_string(dir.join("Cargo.toml")) else {
            continue;
        };
        let mut name = String::new();
        let mut deps = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            if name.is_empty() {
                if let Some(rest) = line.strip_prefix("name = ") {
                    name = rest.trim_matches('"').to_string();
                }
            }
            if let Some(dep) = line.strip_suffix(".workspace = true") {
                if dep.starts_with("qb-") {
                    deps.push(dep.to_string());
                }
            }
        }
        if !name.is_empty() {
            deps.sort();
            out.push((name, deps));
        }
    }
    out
}

/// 某个 crate 的 Cargo.toml 在哪。
fn manifest_of(name: &str) -> PathBuf {
    for root in roots() {
        let dir = root.parent().expect("src 目录一定有上级");
        let manifest = dir.join("Cargo.toml");
        if std::fs::read_to_string(&manifest)
            .map(|t| t.contains(&format!("name = \"{name}\"")))
            .unwrap_or(false)
        {
            return manifest;
        }
    }
    PathBuf::new()
}

/// 每个 crate 的 `src/` 目录。
///
/// 拆 workspace 之前这里只有 `src-tauri/src` 一处。**只扫那一处的话，
/// 每搬走一个模块，这条测试的覆盖面就悄悄缩一圈** —— 模块搬进 `crates/`
/// 就从图里消失，它身上的环也跟着「消失」。那不是断环，是把环藏起来。
fn roots() -> Vec<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent().expect("src-tauri 一定有上级目录");
    let mut out = vec![manifest.join("src")];
    if let Ok(entries) = std::fs::read_dir(workspace.join("crates")) {
        let mut crates: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path().join("src"))
            .filter(|p| p.is_dir())
            .collect();
        crates.sort();
        out.extend(crates);
    }
    out
}

/// 模块名 → 它的全部源文件。**跨 crate 合成一张图。**
///
/// 模块名在整个 workspace 里必须唯一：重名的话两个模块会在图里合成一个点，
/// 环就被平均掉了。下面直接 panic 而不是悄悄合并 —— 这种退化没有症状。
fn module_files() -> BTreeMap<String, Vec<PathBuf>> {
    let mut out: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for root in roots() {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let (module, files) = if e.path().is_dir() {
                let mut files = Vec::new();
                collect(&e.path(), &mut files);
                (name, files)
            } else if let Some(stem) = name.strip_suffix(".rs") {
                // main.rs 是二进制入口，lib.rs 是 crate 根，都不是「模块」。
                // 顺带：crate 根不参与扫描，所以它里面那些 `pub use
                // qb_foundation::…` 的再导出不会变成假的依赖边。
                if stem == "main" || stem == "lib" {
                    continue;
                }
                (stem.to_string(), vec![e.path()])
            } else {
                continue;
            };
            assert!(
                !out.contains_key(&module),
                "模块名 `{module}` 在两个 crate 里都出现了。
                 重名会让两个模块在依赖图里合成一个点，它们之间的环就被
                 平均掉、再也报不出来 —— 改掉其中一个的名字。"
            );
            out.insert(module, files);
        }
    }
    out
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// 剥掉注释与字符串字面量。
///
/// **不剥就会误判**：文档注释里写一句 `` `use crate::gate` `` 解释「以前是怎样」，
/// 会被当成真的依赖。第一版没剥，报出 `paths ↔ gate` 和 `audit ↔ logging`
/// 两对根本不存在的环。
fn strip(src: &str) -> String {
    let b: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let next = b.get(i + 1).copied().unwrap_or('\0');
        if c == '/' && next == '/' {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && next == '*' {
            i += 2;
            while i + 1 < b.len() && !(b[i] == '*' && b[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(b.len());
        } else if c == '"' {
            i += 1;
            while i < b.len() && b[i] != '"' {
                // 跳过转义，免得 "\"" 提前收尾。
                if b[i] == '\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// 把 `use crate::{domain, repository::Repository}` 展开成
/// `crate::domain crate::repository::Repository`。
///
/// **不展开的话整个 brace 导入都是隐形的。** 这是第二个把真实依赖藏起来的坑
/// （第一个是没剥注释）：`use crate::{a, b, c}` 在图里一条边都不产生，
/// 而这恰恰是本项目最常见的导入写法 —— `diagnostics` 对 `workspace` 与
/// `repository` 的依赖就是这么整整一轮没被看见的。
fn expand_groups(text: &str) -> String {
    let mut t = text.to_string();
    // 每轮展开最外层的一批；嵌套的下一轮再来。几轮就收敛，给个上限防病态输入。
    for _ in 0..6 {
        let Some(i) = t.find("::{") else { break };
        // 往左吃掉前缀（`crate`、`qb_platform::config_io` 之类）
        let head: Vec<char> = t[..i].chars().collect();
        let mut j = head.len();
        while j > 0 && (head[j - 1].is_alphanumeric() || head[j - 1] == '_' || head[j - 1] == ':') {
            j -= 1;
        }
        let j = t[..i].char_indices().nth(j).map(|(b, _)| b).unwrap_or(0);
        let prefix: String = t[j..i].to_string();

        let mut depth = 1usize;
        let mut parts: Vec<String> = Vec::new();
        let mut start = i + 3;
        let mut k = start;
        let bytes: Vec<(usize, char)> = t[i + 3..]
            .char_indices()
            .map(|(b, c)| (b + i + 3, c))
            .collect();
        let mut end = None;
        for (b, c) in bytes {
            k = b;
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        parts.push(t[start..k].to_string());
                        end = Some(k);
                        break;
                    }
                }
                ',' if depth == 1 => {
                    parts.push(t[start..k].to_string());
                    start = k + 1;
                }
                _ => {}
            }
        }
        let Some(end) = end else { break };
        let _ = k;
        let expanded: String = parts
            .iter()
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .map(|p| format!(" {prefix}::{p} "))
            .collect();
        t = format!("{}{}{}", &t[..j], expanded, &t[end + 1..]);
    }
    t
}

fn graph() -> BTreeMap<String, BTreeSet<String>> {
    let files = module_files();
    let prefixes = prefixes();
    let mods: Vec<String> = files.keys().cloned().collect();
    let mut g = BTreeMap::new();
    for (m, paths) in &files {
        let text: String = paths
            .iter()
            .filter_map(|p| std::fs::read_to_string(p).ok())
            .map(|s| expand_groups(&strip(&s)))
            .collect();
        let mut deps = BTreeSet::new();
        for other in &mods {
            if other == m {
                continue;
            }
            // 同 crate 内写 `crate::<模块>`，跨 crate 写 `qb_xxx::<模块>`。
            // 两种都要认 —— 只认前者的话，模块一搬进别的 crate，
            // 指向它的边就全部从图里消失了。
            if mentions(&text, &prefixes, other) {
                deps.insert(other.clone());
            }
        }
        g.insert(m.clone(), deps);
    }
    g
}

/// 路径前缀：同 crate 内的 `crate::`，加上每个 workspace crate 的名字。
///
/// **从目录名推，不写死。** 写死的话每加一个 crate 都要记得来补一行，
/// 而忘了补的后果是指向那个 crate 的依赖边全部从图里消失 —— 测试照样全绿。
fn prefixes() -> Vec<String> {
    let mut out = vec!["crate::".to_string()];
    for root in roots().iter().skip(1) {
        if let Some(name) = root.parent().and_then(|p| p.file_name()) {
            out.push(format!("{}::", name.to_string_lossy().replace('-', "_")));
        }
    }
    out
}

/// `text` 里有没有以模块 `m` 结尾的路径前缀。
///
/// 后面必须跟非标识符字符，否则 `crate::paths` 会把 `crate::pathsomething`
/// 也算进来。
fn mentions(text: &str, prefixes: &[String], m: &str) -> bool {
    prefixes.iter().any(|prefix| {
        let needle = format!("{prefix}{m}");
        let mut from = 0;
        while let Some(at) = text[from..].find(&needle) {
            let end = from + at + needle.len();
            if text[end..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_')
            {
                return true;
            }
            from = end;
        }
        false
    })
}

/// 所有大小 > 1 的强连通分量，每个按模块名排序。
///
/// 用 Tarjan。迭代写法而不是递归 —— 模块数量不大，但测试二进制的栈
/// 不值得赌。
fn cycles() -> BTreeSet<Vec<String>> {
    components(&graph())
}

fn components(g: &BTreeMap<String, BTreeSet<String>>) -> BTreeSet<Vec<String>> {
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    let mut low: BTreeMap<String, usize> = BTreeMap::new();
    let mut on_stack: BTreeSet<String> = BTreeSet::new();
    let mut stack: Vec<String> = Vec::new();
    let mut counter = 0usize;
    let mut out = BTreeSet::new();
    let empty: BTreeSet<String> = BTreeSet::new();

    for root in g.keys() {
        if index.contains_key(root) {
            continue;
        }
        // (节点, 下一个要看的邻居序号)
        let mut work: Vec<(String, usize)> = vec![(root.clone(), 0)];
        while let Some((node, next)) = work.pop() {
            if next == 0 {
                index.insert(node.clone(), counter);
                low.insert(node.clone(), counter);
                counter += 1;
                stack.push(node.clone());
                on_stack.insert(node.clone());
            }
            let neighbors: Vec<String> = g.get(&node).unwrap_or(&empty).iter().cloned().collect();
            let mut descended = false;
            for (i, w) in neighbors.iter().enumerate().skip(next) {
                if !index.contains_key(w) {
                    work.push((node.clone(), i + 1));
                    work.push((w.clone(), 0));
                    descended = true;
                    break;
                } else if on_stack.contains(w) {
                    let v = low[&node].min(index[w]);
                    low.insert(node.clone(), v);
                }
            }
            if descended {
                continue;
            }
            if low[&node] == index[&node] {
                let mut comp = Vec::new();
                while let Some(w) = stack.pop() {
                    on_stack.remove(&w);
                    comp.push(w.clone());
                    if w == node {
                        break;
                    }
                }
                if comp.len() > 1 {
                    comp.sort();
                    out.insert(comp);
                }
            }
            if let Some((parent, _)) = work.last() {
                let v = low[parent].min(low[&node]);
                low.insert(parent.clone(), v);
            }
        }
    }
    out
}
