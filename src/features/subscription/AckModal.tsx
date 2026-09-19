import { AlertTriangle, ShieldCheck } from "lucide-react";
import { Button, Modal } from "../../ui";
import { AddressNotice, ScopeNotice } from "./Notices";

/** 首次进入时的知情说明。确认记在本机 localStorage，文案改了就把键升一号。 */
export const ACK_KEY = "qb_subscription_ack_v2";

export function readAck(): boolean {
  try {
    return localStorage.getItem(ACK_KEY) === "true";
  } catch {
    return false;
  }
}

export function writeAck(): void {
  try {
    localStorage.setItem(ACK_KEY, "true");
  } catch {
    // 私密模式或被禁用：这次会话里照常解锁，下次再问一遍。
  }
}

interface Props {
  open: boolean;
  /** 已确认过的可以随手关掉；第一次必须点确认。 */
  acknowledged: boolean;
  onConfirm: () => void;
  onClose: () => void;
}

export function AckModal({ open, acknowledged, onConfirm, onClose }: Props) {
  return (
    <Modal
      open={open}
      onClose={() => {
        if (acknowledged) onClose();
      }}
      title="订阅指南 · 开始之前先读这一页"
      icon={
        <ShieldCheck size={18} className="text-accent" aria-hidden="true" />
      }
      size="wide"
      dismissible={acknowledged}
      footer={
        <Button variant="primary" onClick={onConfirm}>
          我读完了，进入指南
        </Button>
      }
    >
      <div className="qb-sub-ack">
        <p className="qb-sub-para">
          这是个人经验整理，不是官方指引，也不是任何人的推荐做法。四件事先说清楚：
        </p>

        <ScopeNotice />

        <AddressNotice />

        <div className="qb-sub-alert qb-sub-alert--ok" role="note">
          <ShieldCheck size={20} aria-hidden="true" />
          <div className="qb-sub-alert-body">
            <p className="qb-sub-alert-title">
              本面板不做任何代充、代付、账号买卖或拼车
            </p>
            <p>
              QB Gate
              是开源的本机门禁与环境工具，这一页只是资料。所有注册、充值、订阅动作都由你在
              Apple、Google
              或服务商官网自己完成；本面板不读取、不中转、不上传你的付款信息与凭据。
            </p>
          </div>
        </div>

        <div className="qb-sub-alert qb-sub-alert--warn" role="note">
          <AlertTriangle size={20} aria-hidden="true" />
          <div className="qb-sub-alert-body">
            <p className="qb-sub-alert-title">不承诺成功，不承诺不封号</p>
            <p>
              服务商的风控是多维度、持续变化的。本页帮你避开已知的拒付与税费坑，但不构成「必定成功」或「不会被限制」的承诺；请遵守所在地法律与各平台条款。
            </p>
          </div>
        </div>
      </div>
    </Modal>
  );
}
