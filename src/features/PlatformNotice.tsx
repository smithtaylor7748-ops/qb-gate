import { usePlatform } from "../lib/platform";

export default function PlatformNotice() {
  const platform = usePlatform();
  if (platform?.os !== "macos") return null;
  return (
    <p className="notice notice--warn mb-3" role="note">
      macOS 版：从面板启动的受管会话会检查出口 IP，并由看门狗监督。 从 Finder
      或其他终端自行启动的软件不受此限制；本版不提供系统执行锁、Windows
      防火墙或注册表修复。
    </p>
  );
}
