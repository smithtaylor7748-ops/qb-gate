# 浏览器助手 · 真实 Chrome 手动走查（约 5 分钟）

用你自己平时用的 Chrome 走一遍。不需要打开 QB Gate 面板：用一个「独立测试桥」假扮面板，它不碰你的面板数据、不打开数据库，
出口 IP 也是假的（`203.0.113.10` / `203.0.113.11`，文档专用地址）。每一步写了「应该看到什么」，跟写的不一样就停下来，
把测试桥窗口最后几行和插件弹窗截个图发回来。

> 走查时 Claude 网页会被切到说明页。先把 Claude 里没发出去的字存一下。

## 一、装插件（1 分钟）

1. 以前装过旧版「QB Gate 浏览器助手」（0.1.0）的话，先在 `chrome://extensions` 里把它删掉。
2. 在 Chrome 地址栏打开 `chrome://extensions`，打开右上角「开发者模式」，点「加载已解压的扩展程序」，选仓库里的 `browser-extension` 文件夹。
   - 应该看到：多了「QB Gate 浏览器助手」，图标是 QB Gate 的盾牌锁，ID 是 `dffmajnemniojfblgfhenndeeelbjgoh`；自动打开了「三步用起来」引导页。
3. 点工具栏的拼图图标，把「QB Gate 浏览器助手」固定到工具栏（方便看图标上的小字）。

## 二、起测试桥并连上（1 分钟）

在 PowerShell 里进入仓库根目录（例如 `D:\claude-gate`），起测试桥。第一次要编译几分钟；Chrome 装在 Program Files 里的话，把路径换成
`C:\Program Files\Google\Chrome\Application\chrome.exe`：

```powershell
cargo run -p qb-app --example browser-link-dev -- --fresh --browser "$env:LOCALAPPDATA\Google\Chrome\Application\chrome.exe"
```

- 应该看到：Chrome 自动打开「连接 QB Gate 浏览器助手」页面，一两秒后变成「已连接」；测试桥窗口打印「已配对「Chrome …」」。**全程不用复制粘贴。**
- 点插件图标：写着「已连接，保护未开启」，主按钮「开启保护」。

## 三、开启保护（30 秒）

4. 在弹窗里点「开启保护」。
   - 应该看到：弹窗变成绿色「保护中：受保护网站可以打开」，图标上的小字是「放」。打开 `https://claude.ai`，正常打开。

## 四、面板退出、面板崩溃、浏览器重启（2 分钟）

5. 在测试桥窗口输入 `offline` 回车（假装面板正常退出）。
   - 应该看到：Claude 那个标签页换成「claude.ai 被拦下了」说明页，写着「连不上 QB Gate」「QB Gate 面板已退出」；图标小字变成「拦」。
6. 输入 `resume` 回车（面板回来了）。
   - 应该看到：半分钟内说明页自己回到 Claude 页面（插件按 1、2、5、10、20、30 秒的间隔重连；不想等就点说明页上的「重试连接」）。
7. 在测试桥窗口按 `Ctrl+C`（假装面板崩溃，不打招呼就没了）。
   - 应该看到：Claude 页又换成说明页。
8. 测试桥先别开。把 Chrome 整个退出（右上角 ⋮ →「退出」），再打开 Chrome，打开 `https://claude.ai`。
   - 应该看到：直接是说明页（浏览器刚重启、还没核实，所以拦着）。
9. 重新起测试桥，这次**不带** `--fresh`（沿用刚才的配对）：

```powershell
cargo run -p qb-app --example browser-link-dev
```

- 应该看到：不用再确认，半分钟内说明页自己回到 Claude 页面（插件自己连回来、核实过了；不想等就点「重试连接」）。

## 五、出口变了（1 分钟）

10. 在测试桥窗口输入 `change` 回车（出口换成白名单里的另一个 IP）。
    - 应该看到：几秒内 Claude 页换成说明页，写着「已拦截：出口 IP 变了（原来 203.0.113.10，现在 203.0.113.11）」。
      新开一个标签页打开 claude.ai，也直接是说明页；随便打开一个别的网站（比如 `https://www.bing.com`），照常打开。
11. 在说明页（或插件弹窗）点「重新放行」。
    - 应该看到：说明页自己回到 Claude 页面，图标小字回到「放」。

## 六、暂停保护（30 秒）

12. 在测试桥窗口输入 `unknown` 回车（查不到出口）。
    - 应该看到：Claude 页换成说明页，写着查不到出口。
13. 点插件图标里的「暂停保护」。
    - 应该看到：说明页自己回到 Claude 页面，图标上没有小字了；再开 claude.ai 也正常打开（暂停就是什么都不拦）。

## 收尾

在测试桥窗口按 `Ctrl+C` 关掉它。插件可以留着（保护已暂停，什么都不拦），也可以在 `chrome://extensions` 删掉。
以后装了新版 QB Gate，在面板「扩展 → 浏览器助手」点「连接浏览器」会重新连上真的面板。

测试桥窗口里还能输入：`allow`、`reject`、`hang`、`pause` / `unpause`、`gen`、`occupy` / `release`、`reconnect`、`status`、`help`
（说明在 `crates/qb-app/examples/browser-link-dev.rs` 文件头）。
