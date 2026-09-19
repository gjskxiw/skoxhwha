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
  /** 侧栏色标的键，"" 表示不上色；可选值见 GROUP_COLORS */
  color: string;
}

/**
 * 工具卡片。展示顺序 = `config.tools` 的数组顺序（新建时追加到末尾）：
 * 界面上不提供拖拽或上移下移，所以也没有单独的排序字段。
 */
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

/**
 * 分组色标：沿用工具类型徽章那套六色，深浅主题各一档。
 * 存的是键名（如 "blue"）而不是 class 字符串，Rust 侧 `Group.color` 同样只存键；
 * 导入的配置里出现未知键时这里查不到，就不画色点，不影响任何启动逻辑。
 */
export const GROUP_COLORS: Record<string, string> = {
  blue: "bg-blue-500 dark:bg-blue-400",
  amber: "bg-amber-500 dark:bg-amber-400",
  emerald: "bg-emerald-500 dark:bg-emerald-400",
  fuchsia: "bg-fuchsia-500 dark:bg-fuchsia-400",
  rose: "bg-rose-500 dark:bg-rose-400",
  cyan: "bg-cyan-500 dark:bg-cyan-400",
};

export const GROUP_COLOR_KEYS = Object.keys(GROUP_COLORS);

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
