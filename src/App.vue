<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { Download, FolderPlus, Moon, RotateCcw, Search, Sun, TriangleAlert, Upload, Wrench, X } from "@lucide/vue";
import { open as openFileDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import javaLogo from "devicon/icons/java/java-original.svg";
import pythonLogo from "devicon/icons/python/python-original.svg";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import { TooltipProvider } from "@/components/ui/tooltip";
import { api } from "@/lib/api";
import { confirmAction } from "@/lib/confirm";
import { clearNotice, notice, notify } from "@/lib/notice";
import { applyTheme, commitConfig, initStore, refreshStatuses, replaceConfig, store } from "@/lib/store";
import { type CloseAction, type ThemeMode, type Tool } from "@/lib/types";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import EnvDialog from "@/components/EnvDialog.vue";
import GroupSidebar from "@/components/GroupSidebar.vue";
import ToolCard from "@/components/ToolCard.vue";
import TitleBar from "@/components/TitleBar.vue";
import ToolFormDialog from "@/components/ToolFormDialog.vue";
import appLogo from "../ico.png";

const toolFormRef = ref<InstanceType<typeof ToolFormDialog>>();
const envDialogRef = ref<InstanceType<typeof EnvDialog>>();
const groupSidebarRef = ref<InstanceType<typeof GroupSidebar>>();

async function bootstrap() {
  // 失败原因记进 store.loadError，由全屏错误视图接管
  await initStore().catch(() => {});
}

onMounted(() => {
  void bootstrap();
  // 窗口重新获得焦点时刷新依赖状态（用户可能刚补齐了缺失的文件）
  window.addEventListener("focus", () => refreshStatuses());
});

watch(
  () => store.config.settings.theme,
  () => applyTheme(),
);

const visibleTools = computed<Tool[]>(() => {
  let list = store.config.tools;
  if (store.activeGroupId === "none") {
    list = list.filter((t) => !t.groupId);
  } else if (store.activeGroupId !== "all") {
    list = list.filter((t) => t.groupId === store.activeGroupId);
  }
  const q = store.search.trim().toLowerCase();
  if (q) {
    list = list.filter(
      (t) =>
        t.name.toLowerCase().includes(q) || t.description.toLowerCase().includes(q),
    );
  }
  // 卡片顺序 = 数组（插入）顺序，没有单独的排序字段
  return [...list];
});

/**
 * 空视图的三种成因要分开说：搜不到、这个分组是空的、真的一个工具都没有。
 * 之前三者共用「还没有工具，点击新建」，前两种会给错指引。
 */
const emptyReason = computed<"search" | "group" | "none" | null>(() => {
  if (visibleTools.value.length > 0) return null;
  if (store.search.trim()) return "search";
  if (store.activeGroupId !== "all") return "group";
  return "none";
});

const activeGroupName = computed(() => {
  if (store.activeGroupId === "none") return "「未分组」";
  const g = store.config.groups.find((x) => x.id === store.activeGroupId);
  return g ? `「${g.name}」` : "当前分组";
});

async function launch(tool: Tool, asAdmin = false) {
  try {
    await api.launchTool(tool.id, asAdmin);
  } catch (e) {
    // 原因同时由后端写进 data\logs\app.log
    notify(`「${tool.name}」启动失败：${String(e)}`, "error");
  }
  // 无论成败都刷新依赖状态（目标文件可能刚被补齐或删掉）
  await refreshStatuses();
}

/** 在资源管理器里打开工具的工作目录（= 目标文件所在目录，由后端解析） */
async function openToolDir(tool: Tool) {
  try {
    await api.openToolDir(tool.id);
  } catch (e) {
    // 后端消息已自带「打开工作目录失败：」前缀，别再叠一层
    notify(String(e), "error");
  }
}

async function deleteTool(tool: Tool) {
  const ok = await confirmAction(`确定删除工具「${tool.name}」？`, {
    title: "删除工具",
    confirmText: "删除",
  });
  if (!ok) return;
  try {
    await commitConfig((d) => {
      d.tools = d.tools.filter((t) => t.id !== tool.id);
    });
  } catch (e) {
    notify(`删除失败，配置未写入：${String(e)}`, "error");
  }
}

// —— 设置快捷按钮（点击即切换/操作，立即保存） ——
const THEME_LABEL: Record<ThemeMode, string> = {
  light: "浅色",
  dark: "深色",
};
const CLOSE_LABEL: Record<CloseAction, string> = {
  tray: "最小化到托盘",
  exit: "退出程序",
};

const themeIcon = computed(() =>
  store.config.settings.theme === "dark" ? Moon : Sun,
);

async function cycleTheme() {
  const order: ThemeMode[] = ["light", "dark"];
  const next = order[(order.indexOf(store.config.settings.theme) + 1) % order.length];
  // 主题由 store 的 watch 统一应用，保存成功后才生效，失败时界面与磁盘不会脱节
  try {
    await commitConfig((d) => {
      d.settings.theme = next;
    });
  } catch (e) {
    notify(`主题未能保存：${String(e)}`, "error");
  }
}

/** 关闭窗口行为开关：开 = 最小化到托盘，关 = 退出程序 */
async function setCloseToTray(tray: boolean) {
  const next: CloseAction = tray ? "tray" : "exit";
  if (next === store.config.settings.closeAction) return;
  try {
    await commitConfig((d) => {
      d.settings.closeAction = next;
    });
  } catch (e) {
    notify(`设置未能保存：${String(e)}`, "error");
  }
}

// 导入 / 导出共用：防止连点同时打开两个文件选择器
const cfgBusy = ref(false);

async function exportCfg() {
  if (cfgBusy.value) return;
  cfgBusy.value = true;
  try {
    const p = await saveFileDialog({
      defaultPath: "secaxis-config.json",
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (!p) return;
    await api.exportConfig(p);
    notify(`配置已导出到 ${p.split(/[\\/]/).pop()}`);
  } catch (e) {
    notify(`导出失败：${String(e)}`, "error");
  } finally {
    cfgBusy.value = false;
  }
}

async function importCfg() {
  if (cfgBusy.value) return;
  cfgBusy.value = true;
  try {
    const p = await openFileDialog({
      multiple: false,
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (typeof p !== "string") return;
    // 导入会整体替换现有工具/分组/环境并立刻落盘，且导入的工具点击即执行，
    // 所以必须先确认；配置文件来自外部，属于不可信输入
    const ok = await confirmAction(
      `导入将用「${p.split(/[\\/]/).pop()}」整体替换你现在的全部分组、工具与运行环境，且无法撤销。\n\n导入的工具在你双击时会按其配置启动程序，请确认文件来源可信。`,
      { title: "导入配置", confirmText: "导入并替换" },
    );
    if (!ok) return;
    const { config: cfg, warnings } = await api.importConfig(p);
    await replaceConfig(cfg);
    applyTheme();
    if (warnings.length > 0) {
      // 引用被收敛过就必须说出来：悬空的环境引用会让工具无法启动。
      // 完整清单同时由后端写进 app.log，状态条只放得下第一条。
      notify(
        warnings.length > 1
          ? `配置已导入，但有 ${warnings.length} 项引用被修正：${warnings[0]}……（详见日志）`
          : `配置已导入，但有引用被修正：${warnings[0]}`,
        "error",
        10000,
      );
    }
  } catch (e) {
    notify(`导入失败：${String(e)}`, "error");
  } finally {
    cfgBusy.value = false;
  }
}
</script>

<template>
  <TooltipProvider :delay-duration="300">
    <div v-if="!store.ready" class="grid h-screen place-items-center px-6">
      <div v-if="store.loadError" class="max-w-md space-y-3 text-center">
        <TriangleAlert class="mx-auto size-10 text-destructive" />
        <p class="text-sm font-medium">加载配置失败</p>
        <p class="text-xs break-all text-muted-foreground">{{ store.loadError }}</p>
        <Button size="sm" @click="bootstrap"><RotateCcw /> 重试</Button>
      </div>
      <div v-else class="text-sm text-muted-foreground">正在加载…</div>
    </div>

    <div v-else class="flex h-screen flex-col overflow-hidden">
      <!-- 自定义标题栏：应用名 + 搜索 + 快捷按钮 + 窗口控制 -->
      <TitleBar>
        <div class="flex shrink-0 items-center gap-2 pl-3">
          <img :src="appLogo" class="size-5" alt="SecAxis" />
          <span class="text-sm font-semibold tracking-wide">SecAxis</span>
        </div>

        <div class="relative ml-3 w-64 self-center">
          <Search class="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input v-model="store.search" placeholder="搜索工具…" class="h-8 pl-8" />
        </div>

        <!-- 竖分隔线把三类控件断开：运行环境 / 工具栏操作 / 主操作。
             原先 8 个控件按 gap-1.5 均分，唯一的主操作「新建工具」和「导出配置」权重一样。
             pl-4 是给左侧搜索框留的呼吸位：最小宽度 860 下两者原本只剩 8px 间隙 -->
        <div class="ml-auto flex items-center gap-1.5 pl-4 pr-2">
          <div class="flex items-center gap-1.5">
            <Button
              variant="ghost"
              size="icon"
              title="Java 环境"
              aria-label="Java 环境"
              @click="envDialogRef?.open('java')"
            >
              <img :src="javaLogo" class="size-4" alt="" />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              title="Python 环境"
              aria-label="Python 环境"
              @click="envDialogRef?.open('python')"
            >
              <img :src="pythonLogo" class="size-4" alt="" />
            </Button>
          </div>
          <Separator orientation="vertical" class="h-5!" />
          <div class="flex items-center gap-1.5">
            <Button
              variant="ghost"
              size="icon"
              title="新建分组"
              aria-label="新建分组"
              @click="groupSidebarRef?.openNewGroup()"
            >
              <FolderPlus />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              :title="`主题：${THEME_LABEL[store.config.settings.theme]}（点击切换）`"
              :aria-label="`主题：${THEME_LABEL[store.config.settings.theme]}，点击切换`"
              @click="cycleTheme"
            >
              <component :is="themeIcon" />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              title="导出配置"
              aria-label="导出配置"
              :disabled="cfgBusy"
              @click="exportCfg"
            >
              <Download />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              title="导入配置"
              aria-label="导入配置"
              :disabled="cfgBusy"
              @click="importCfg"
            >
              <Upload />
            </Button>
            <Switch
              :model-value="store.config.settings.closeAction === 'tray'"
              :title="`关闭窗口时：${CLOSE_LABEL[store.config.settings.closeAction]}（开：最小化到托盘，关：退出程序）`"
              :aria-label="`关闭窗口时：${CLOSE_LABEL[store.config.settings.closeAction]}（开：最小化到托盘，关：退出程序）`"
              @update:model-value="setCloseToTray"
            />
          </div>
          <Separator orientation="vertical" class="h-5!" />
          <Button size="sm" @click="toolFormRef?.openNew(store.activeGroupId)">
            新建工具
          </Button>
        </div>
      </TitleBar>

      <div class="flex min-h-0 min-w-0 flex-1">
        <GroupSidebar ref="groupSidebarRef" />

        <!-- 内容区 -->
        <main class="flex-1 overflow-y-auto p-4">
          <div v-if="emptyReason" class="grid h-full place-items-center">
            <div class="space-y-2 text-center text-sm text-muted-foreground">
              <Wrench class="mx-auto size-10 opacity-40" />
              <template v-if="emptyReason === 'search'">
                <p>没有匹配「{{ store.search.trim() }}」的工具</p>
                <Button variant="link" size="sm" @click="store.search = ''">清空搜索</Button>
              </template>
              <template v-else-if="emptyReason === 'group'">
                <p>{{ activeGroupName }}里还没有工具</p>
                <p class="text-xs">新建工具时选到这个分组，或在卡片右键「编辑」里改它的分组</p>
              </template>
              <template v-else>
                <p>还没有工具，点击右上角「新建工具」添加</p>
              </template>
            </div>
          </div>

          <div
            v-else
            class="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-3"
          >
            <ToolCard
              v-for="element in visibleTools"
              :key="element.id"
              :tool="element"
              @launch="launch"
              @edit="toolFormRef?.openEdit($event)"
              @delete="deleteTool"
              @launch-admin="launch($event, true)"
              @open-dir="openToolDir"
            />
          </div>
        </main>
      </div>

      <!-- 状态条：非浮动的结果反馈，只在有内容时占位，几秒后自动收起 -->
      <div
        v-if="notice.current"
        role="status"
        aria-live="polite"
        class="flex shrink-0 items-start gap-2 border-t px-4 py-1.5 text-xs"
        :class="
          notice.current.kind === 'error'
            ? 'border-destructive/30 bg-destructive/10 text-destructive'
            : 'bg-muted text-muted-foreground'
        "
      >
        <TriangleAlert
          v-if="notice.current.kind === 'error'"
          class="mt-0.5 size-3.5 shrink-0"
        />
        <span class="min-w-0 flex-1 break-words">{{ notice.current.text }}</span>
        <button
          type="button"
          class="mt-0.5 shrink-0 rounded opacity-70 transition-opacity hover:opacity-100 focus-visible:outline-hidden"
          title="关闭提示"
          aria-label="关闭提示"
          @click="clearNotice"
        >
          <X class="size-3.5" />
        </button>
      </div>

      <ToolFormDialog ref="toolFormRef" />
      <EnvDialog ref="envDialogRef" />
      <ConfirmDialog />
    </div>
  </TooltipProvider>
</template>
