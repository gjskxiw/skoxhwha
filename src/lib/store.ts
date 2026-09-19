import { reactive } from "vue";
import { api } from "./api";
import type { CheckResult, Config, ThemeMode } from "./types";

interface Store {
  ready: boolean;
  /** 初始化失败原因；非 null 时界面显示重试入口，而不是永远停在「正在加载…」 */
  loadError: string | null;
  config: Config;
  statuses: Record<string, CheckResult>;
  icons: Record<string, string>; // icon 文件名 -> dataURL
  activeGroupId: string; // "all" | "none" | group.id
  search: string;
}

export const store = reactive<Store>({
  ready: false,
  loadError: null,
  config: {
    version: 1,
    settings: { theme: "light", closeAction: "exit" },
    envs: [],
    groups: [],
    tools: [],
  },
  statuses: {},
  icons: {},
  activeGroupId: "all",
  search: "",
});

export async function initStore() {
  try {
    store.config = await api.getState();
    store.ready = true;
    store.loadError = null;
    applyTheme();
    await Promise.all([refreshStatuses(), loadIcons()]);
  } catch (e) {
    store.loadError = String(e);
    throw e;
  }
}

/**
 * 在配置深拷贝上做修改，保存成功后才写回 store。
 * 避免「先改本地再保存」在保存失败时，界面留下一条后端并不存在的改动。
 */
export async function commitConfig(mutate: (draft: Config) => void) {
  const draft: Config = JSON.parse(JSON.stringify(store.config));
  mutate(draft);
  await replaceConfig(draft);
}

/**
 * 依赖状态只由 check_tool 真正读到的输入决定，据此决定保存后要不要重扫。
 *
 * 工具侧只要身份（id 变了必须重扫，否则新工具在 statuses 里没有条目）、类型、目标路径、
 * 启动参数（引号是否配对、终端类的 %）和环境绑定；环境侧要 kind / path，外加 name ——
 * 失败文案里会带「环境「X」」。工具名、分组、图标、描述改了都不影响结论，不必重扫：
 * 100 个工具时一轮全量重扫约 200ms，只为改个名字是白付的。
 *
 * 这份字段清单与后端 launcher::check_tool 的输入是一对契约，那边多读一个字段，
 * 这里就得跟着加（launcher.rs 的注释里也留了提醒）。
 */
function statusKey(c: Config): string {
  return JSON.stringify([
    c.tools.map((t) => [t.id, t.type, t.target, t.args, t.envId]),
    c.envs.map((e) => [e.id, e.kind, e.path, e.name]),
  ]);
}

/** 用给定配置整体替换后端与本地（导入用）；成功才写回 store */
export async function replaceConfig(next: Config) {
  const refresh = statusKey(next) !== statusKey(store.config);
  store.config = await api.saveConfig(JSON.parse(JSON.stringify(next)));
  pruneIcons();
  await Promise.all(refresh ? [refreshStatuses(), loadIcons()] : [loadIcons()]);
}

/** 丢掉不再被任何工具引用的图标缓存，避免删掉工具后 data URL 一直挂在内存里 */
function pruneIcons() {
  const used = new Set(
    store.config.tools.map((t) => t.icon).filter((name): name is string => !!name),
  );
  for (const name of Object.keys(store.icons)) {
    if (!used.has(name)) delete store.icons[name];
  }
}

let inflight: Promise<void> | null = null;
let pending = false;

/**
 * 刷新依赖状态（启动、启动工具后、窗口重新获得焦点时调用）。
 *
 * 并发调用不再被直接丢弃：正在刷新时记一个 pending 标记，结束后补跑一次。
 * 否则「刚启动完工具的那次刷新」会被同时进行的焦点刷新吞掉，
 * 卡片上的缺依赖标红会停留在旧状态。
 */
export async function refreshStatuses(): Promise<void> {
  if (inflight) {
    pending = true;
    return inflight;
  }
  inflight = doRefresh().finally(() => {
    inflight = null;
    if (pending) {
      pending = false;
      void refreshStatuses();
    }
  });
  return inflight;
}

async function doRefresh() {
  try {
    store.statuses = await api.checkToolsStatus();
  } catch {
    // 刷新失败时保留上一次结果
  }
}

async function loadIcons() {
  const missing = [
    ...new Set(
      store.config.tools
        .map((t) => t.icon)
        .filter((name): name is string => !!name && !store.icons[name]),
    ),
  ];
  if (missing.length === 0) return;
  try {
    // 一次 IPC 取回全部缺失图标，避免逐个往返
    Object.assign(store.icons, await api.getIcons(missing));
  } catch {
    // 图标缺失时回退到类型图标
  }
}

export function applyTheme() {
  const theme: ThemeMode = store.config.settings.theme;
  const dark = theme === "dark";
  const html = document.documentElement;
  if (html.classList.contains("dark") === dark) return;
  // 切换瞬间禁用所有过渡，让新旧配色一步到位；
  // 否则带 transition-colors 的元素会以旧主题颜色渐变 150ms，视觉上闪一下
  html.classList.add("theme-transition-off");
  html.classList.toggle("dark", dark);
  void html.offsetWidth; // 强制在禁用过渡期间完成样式重算
  html.classList.remove("theme-transition-off");
}

export function uid(prefix: string): string {
  return `${prefix}_${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`;
}
