import type { ReactNode } from "react";
import type { Tone } from "./labels";

interface Props {
  tone?: Tone;
  children: ReactNode;
  icon?: ReactNode;
  title?: string;
}

const CLASS: Record<Tone, string> = {
  default: "pill--neutral",
  ok: "pill--ok",
  warn: "pill--warn",
  danger: "pill--danger",
  accent: "pill--accent",
};

export default function Pill({
  tone = "default",
  children,
  icon,
  title,
}: Props) {
  return (
    <span className={`pill ${CLASS[tone]}`} title={title}>
      {icon}
      {children}
    </span>
  );
}
