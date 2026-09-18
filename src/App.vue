<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { Toaster, toast } from "vue-sonner";
import { Download, FolderPlus, Moon, RotateCcw, Search, Sun, TriangleAlert, Upload, Wrench } from "@lucide/vue";
import { open as openFileDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import javaLogo from "devicon/icons/java/java-original.svg";
import pythonLogo from "devicon/icons/python/python-original.svg";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { TooltipProvider } from "@/components/ui/tooltip";
import { api } from "@/lib/api";
import { confirmAction } from "@/lib/confirm";
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
  try {
    await initStore();
  } catch (e) {
    // 失败原因已记进 store.loadError，界面会显示重试入口
    toast.error("加载配置失败", { description: String(e) });
  }
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
  return [...list].sort((a, b) => a.sort - b.sort);
});

async function launch(tool: Tool, asAdmin = false) {
  try {
    await api.launchTool(tool.id, asAdmin);
    toast.success(asAdmin ? `已以管理员身份启动「${tool.name}」` : `已启动「${tool.name}」`);
    // 启动后刷新依赖状态（目标文件可能刚被补齐或删掉）
    await refreshStatuses();
  } catch (e) {
    toast.error(`「${tool.name}」启动失败`, { description: String(e) });
  }
}

/** 在资源管理器里打开工具的工作目录（= 目标文件所在目录，由后端解析） */
async function openToolDir(tool: Tool) {
  try {
    await api.openToolDir(tool.id);
  } catch (e) {
    toast.error("打开工作目录失败", { description: String(e) });
  }
}

async function deleteTool(tool: Tool) {
  const ok = await confirmAction(`确定删除工具「${tool.name}」？`, {
    title: "删除工具",
    confirmText: "删除",
  });
  if (!ok) return;
  commitConfig((d) => {
    d.tools = d.tools.filter((t) => t.id !== tool.id);
  })
    .then(() => toast.success("已删除"))
    .catch((e) => toast.error("删除失败", { description: String(e) }));
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
  try {
    // 主题由 store 的 watch 统一应用，等保存成功后再生效，失败时界面与磁盘不会脱节
    await commitConfig((d) => {
      d.settings.theme = next;
    });
    toast.success(`主题已切换为${THEME_LABEL[next]}`);
  } catch (e) {
    toast.error("保存失败", { description: String(e) });
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
    toast.success(`关闭窗口时：${CLOSE_LABEL[next]}`);
  } catch (e) {
    toast.error("保存失败", { description: String(e) });
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
    toast.success("配置已导出");
  } catch (e) {
    toast.error("导出失败", { description: String(e) });
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
      // 引用被收敛过就必须说出来：悬空的环境引用会让工具无法启动
      toast.warning("配置已导入，但有引用被修正", {
        description: warnings.join("\n"),
        duration: 8000,
      });
    } else {
      toast.success("配置已导入");
    }
  } catch (e) {
    toast.error("导入失败", { description: String(e) });
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

        <div class="ml-auto flex items-center gap-1.5 pl-2 pr-2">
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
            <Button size="sm" @click="toolFormRef?.openNew(store.activeGroupId)">
              新建工具
            </Button>
        </div>
      </TitleBar>

      <div class="flex min-h-0 min-w-0 flex-1">
        <GroupSidebar ref="groupSidebarRef" />

        <!-- 内容区 -->
        <main class="flex-1 overflow-y-auto p-4">
          <div v-if="visibleTools.length === 0" class="grid h-full place-items-center">
            <div class="text-center text-sm text-muted-foreground space-y-2">
              <Wrench class="mx-auto size-10 opacity-40" />
              <p>还没有工具，点击右上角「新建工具」添加</p>
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

      <ToolFormDialog ref="toolFormRef" />
      <EnvDialog ref="envDialogRef" />
      <ConfirmDialog />
    </div>

    <!-- offset 避开自定义标题栏，避免 toast 盖住搜索框 -->
    <Toaster position="top-center" rich-colors :duration="4000" :offset="64" />
  </TooltipProvider>
</template>
