export type ToolType =
  | "terminal_python"
  | "terminal_java"
  | "terminal_exe"
  | "gui_java"
  | "gui_exe"
  | "web"
  /** 后端容错用：更高版本写入的未知类型，不参与新建 */
  | "unknown";

/** 后端 #[serde(other)] 容错：配置里出现未知 kind 时会被序列化成 "unknown" */
export type EnvKind = "java" | "python" | "unknown";
export type ThemeMode = "light" | "dark";
export type CloseAction = "exit" | "tray";

export interface Settings {
  theme: ThemeMode;
  closeAction: CloseAction;
}

export interface Env {
  id: string;
  kind: EnvKind;
  name: string;
  path: string;
}

export interface Group {
  id: string;
  name: string;
}

export interface Tool {
  id: string;
  name: string;
  type: ToolType;
  target: string;
  args: string;
  envId: string | null;
  groupId: string | null;
  icon: string | null;
  description: string;
  sort: number;
}

export interface Config {
  version: number;
  settings: Settings;
  envs: Env[];
  groups: Group[];
  tools: Tool[];
}

export interface CheckResult {
  ok: boolean;
  /** 阻断启动的原因（卡片标红） */
  missing: string;
}

/** 保存前探测环境的结果 */
export interface EnvProbe {
  /** 建议的环境名（由版本信息推导，如 "JDK 17.0.9" / "Python 3.12.4"） */
  name: string;
  /** 原始版本输出 */
  detail: string;
}

/** import_config 的返回：收敛引用后可能带修正说明 */
export interface ImportResult {
  config: Config;
  /** 引用被修正的说明（可能为空） */
  warnings: string[];
}

export const TOOL_TYPE_LABEL: Record<ToolType, string> = {
  terminal_python: "Terminal Python",
  terminal_java: "Terminal Java",
  terminal_exe: "Terminal exe",
  gui_java: "Gui Java",
  gui_exe: "Gui exe",
  web: "Web Link",
  unknown: "Unknown",
};

/**
 * 工具类型徽章的配色：六种类型按色相均匀分开（蓝 / 琥珀 / 翠绿 / 品红 / 玫红 / 青），
 * 深浅色主题各一套。配成「浅底 + 同色系文字 + 同色系描边」，在卡片网格里一眼可辨。
 * 换色只改这里即可，无需动组件。
 */
export const TOOL_TYPE_BADGE: Record<ToolType, string> = {
  terminal_python:
    "border-blue-200 bg-blue-50 text-blue-700 dark:border-blue-900 dark:bg-blue-950/50 dark:text-blue-300",
  terminal_java:
    "border-amber-200 bg-amber-50 text-amber-700 dark:border-amber-900 dark:bg-amber-950/50 dark:text-amber-300",
  terminal_exe:
    "border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-900 dark:bg-emerald-950/50 dark:text-emerald-300",
  gui_java:
    "border-fuchsia-200 bg-fuchsia-50 text-fuchsia-700 dark:border-fuchsia-900 dark:bg-fuchsia-950/50 dark:text-fuchsia-300",
  gui_exe:
    "border-rose-200 bg-rose-50 text-rose-700 dark:border-rose-900 dark:bg-rose-950/50 dark:text-rose-300",
  web: "border-cyan-200 bg-cyan-50 text-cyan-700 dark:border-cyan-900 dark:bg-cyan-950/50 dark:text-cyan-300",
  unknown: "border-border bg-muted text-muted-foreground",
};

export function envKindForType(type: ToolType): EnvKind | null {
  switch (type) {
    case "terminal_python":
      return "python";
    case "terminal_java":
    case "gui_java":
      return "java";
    default:
      return null;
  }
}
