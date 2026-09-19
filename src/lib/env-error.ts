import type { EnvKind } from "./types";

const EXE: Record<EnvKind, string> = {
  java: "java.exe",
  python: "python.exe",
  unknown: "可执行文件",
};

/**
 * 把后端 `launcher::validate_env` 的原始错误归成一句人话。
 *
 * 分类靠的是消息前缀，后端改了措辞就会退化成最后一档「没能通过校验」——
 * 所以原始文本不能丢：它挂在错误行的 title 上供悬浮查看，同时由命令层写进 app.log。
 * `launcher.rs` 里有一条测试钉住这些前缀，两边谁改都会红。
 */
export function explainProbeError(raw: string, kind: EnvKind): string {
  const exe = EXE[kind];
  if (raw.startsWith("可执行文件不存在")) {
    return kind === "java"
      ? `这里没找到 ${exe}：要选的是 JDK 的安装根目录（${exe} 在它的 bin 子目录里）。`
      : `这里没找到 ${exe}：要选的是 Python 的安装目录（${exe} 就在这一层）。`;
  }
  if (raw.startsWith("无法从版本信息中识别环境名称")) return `${exe} 能跑，但输出里认不出版本号。`;
  if (raw.startsWith("执行失败（退出码")) return `${exe} 没能正常运行。`;
  if (raw.startsWith("执行失败")) return `${exe} 无法启动（文件损坏或被安全软件拦截）。`;
  if (raw.startsWith("未知的环境类型")) return raw;
  return "这个目录没能通过环境校验。";
}
