import { invoke } from "@tauri-apps/api/core";
import type { CheckResult, Config, EnvKind, EnvProbe, ImportResult } from "./types";

export const api = {
  getState: () => invoke<Config>("get_state"),
  saveConfig: (config: Config) => invoke<Config>("save_config", { config }),
  checkToolsStatus: () => invoke<Record<string, CheckResult>>("check_tools_status"),
  /** asAdmin：本次以管理员身份启动（UAC 提权，仅 GUI exe），不是持久化属性 */
  launchTool: (id: string, asAdmin = false) =>
    invoke<void>("launch_tool", { id, asAdmin }),
  /** 在资源管理器里打开工具的工作目录（后端解析路径） */
  openToolDir: (id: string) => invoke<void>("open_tool_dir", { id }),
  /** 保存前探测：把目录当环境跑一次 --version，返回建议名称与原始输出 */
  probeEnv: (kind: EnvKind, path: string) => invoke<EnvProbe>("probe_env", { kind, path }),
  extractIcon: (exePath: string) => invoke<string>("extract_icon", { exePath }),
  getIcons: (names: string[]) => invoke<Record<string, string>>("get_icons", { names }),
  exportConfig: (path: string) => invoke<void>("export_config", { path }),
  importConfig: (path: string) => invoke<ImportResult>("import_config", { path }),
};
