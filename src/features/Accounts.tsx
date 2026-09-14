/**
 * 官方账户 —— 第一项，同时就是仪表盘。
 *
 * 旧版把槽位列在「总览」和「账户」两页，两处重复；这一页把槽位、切换、启动、
 * 评分、门禁状态、会话、日志收在一处，低频操作（新建槽位 / 合并桥接 /
 * 官方目录残留迁移）在槽位块里就地展开，不再弹去别的页。
 */
import { Navigate } from "react-router-dom";
import { R } from "../lib/resources";
import { useResource } from "../lib/store";
import Home from "./overview/Home";

export default function Accounts() {
  const accounts = useResource("accounts", R.accounts);
  // 真·第一次：一个槽位都没有。这时候直接进引导 —— 空着的面板没什么可看的，
  // 而引导的第一步（加白名单）不做完，后面装什么都会被自己的锁挡住。
  // 只在「确实读到了账户数据且为空」时跳，读取中不跳，免得闪一下。
  if (accounts.data && accounts.data.slots.length === 0) {
    return <Navigate to="/onboarding" replace />;
  }
  return <Home />;
}
