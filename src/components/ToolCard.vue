<script setup lang="ts">
import { computed } from "vue";
import {
  AppWindow,
  CircleHelp,
  FolderOpen,
  Globe,
  Play,
  Pencil,
  ShieldCheck,
  SquareTerminal,
  TriangleAlert,
  Trash2,
} from "@lucide/vue";
import javaLogo from "devicon/icons/java/java-original.svg";
import pythonLogo from "devicon/icons/python/python-original.svg";
import { Badge } from "@/components/ui/badge";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { store } from "@/lib/store";
import { TOOL_TYPE_BADGE, TOOL_TYPE_LABEL, type Tool, type ToolType } from "@/lib/types";

const props = defineProps<{ tool: Tool }>();
const emit = defineEmits<{
  launch: [tool: Tool];
  edit: [tool: Tool];
  delete: [tool: Tool];
  /** 以管理员身份启动（UAC 提权），一次性动作而非开关 */
  launchAdmin: [tool: Tool];
  /** 在资源管理器里打开工作目录 */
  openDir: [tool: Tool];
}>();

// 双击启动（单击不触发，避免误启动）
function onCardDblClick() {
  emit("launch", props.tool);
}

/** Java / Python 类工具用 Devicon 原生 logo */
const TYPE_LOGO: Partial<Record<ToolType, string>> = {
  terminal_python: pythonLogo,
  terminal_java: javaLogo,
  gui_java: javaLogo,
};

const TYPE_ICON: Partial<Record<ToolType, unknown>> = {
  terminal_exe: SquareTerminal,
  gui_exe: AppWindow,
  web: Globe,
  unknown: CircleHelp,
};

const typeLogo = computed(() => TYPE_LOGO[props.tool.type]);
const typeIcon = computed(() => TYPE_ICON[props.tool.type]);
/** 网页类型没有文件路径，也就没有工作目录 */
const isWeb = computed(() => props.tool.type === "web");
/** 提权启动只对 GUI exe 开放，其他类型不显示入口 */
const canRunAsAdmin = computed(() => props.tool.type === "gui_exe");
const status = computed(() => store.statuses[props.tool.id]);
// 提取过图标的工具用自定义图标，其余回退到下面的类型图标
const iconSrc = computed(() => (props.tool.icon ? store.icons[props.tool.icon] : undefined));
const envName = computed(() => {
  const env = store.config.envs.find((e) => e.id === props.tool.envId);
  return env?.name;
});
/**
 * 分组归属。颜色留给侧栏，卡片只带文字，避免和类型徽章那套六色抢语义；
 * 放在副行开头而不是徽章行，是因为 220px 的窄列放不下「类型 + 分组 + 环境」三枚徽章。
 * 已经在看这个分组时不显示：一屏卡片重复同一个组名只是噪声，需要它的是「全部」和搜索结果。
 */
const groupName = computed(() => {
  const id = props.tool.groupId;
  if (!id || id === store.activeGroupId) return undefined;
  return store.config.groups.find((g) => g.id === id)?.name;
});
/** 副行全文，同时用作悬停 title（截断时仍能看到完整分组名 + 描述/路径） */
const subline = computed(() =>
  [groupName.value, props.tool.description || props.tool.target].filter(Boolean).join(" · "),
);
</script>

<template>
  <ContextMenu>
    <ContextMenuTrigger as-child>
      <div
        class="flex cursor-pointer flex-col gap-3 rounded-xl border bg-card p-4 transition-[border-color,box-shadow,translate] hover:-translate-y-px hover:border-foreground/25 hover:shadow-md focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px] focus-visible:outline-hidden"
        :class="status && !status.ok ? 'border-destructive/60' : ''"
        role="button"
        tabindex="0"
        title="双击启动"
        :aria-label="`双击启动 ${tool.name}`"
        @dblclick="onCardDblClick"
        @keydown.enter.prevent="emit('launch', tool)"
        @keydown.space.prevent="emit('launch', tool)"
      >
        <div class="flex items-start gap-3">
          <div
            class="grid size-10 shrink-0 place-items-center overflow-hidden rounded-lg bg-muted/60 inset-ring inset-ring-border"
          >
            <img v-if="iconSrc" :src="iconSrc" class="size-6 object-contain" alt="" />
            <img v-else-if="typeLogo" :src="typeLogo" class="size-6 object-contain" alt="" />
            <component :is="typeIcon" v-else class="size-5 text-muted-foreground" />
          </div>
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-1">
              <span class="truncate text-sm font-medium" :title="tool.name">{{ tool.name }}</span>
              <Tooltip v-if="status && !status.ok">
                <TooltipTrigger as-child>
                  <TriangleAlert class="size-4 shrink-0 text-destructive" />
                </TooltipTrigger>
                <TooltipContent side="bottom">{{ status.missing }}</TooltipContent>
              </Tooltip>
            </div>
            <div class="mt-0.5 truncate text-xs text-muted-foreground" :title="subline">
              <template v-if="groupName"><span class="text-secondary-foreground">{{ groupName }}</span> · </template>{{ tool.description || tool.target }}
            </div>
          </div>
        </div>

        <div class="flex flex-wrap items-center gap-1.5">
          <Badge variant="outline" :class="[TOOL_TYPE_BADGE[tool.type], 'text-[10px] font-normal']">
            {{ TOOL_TYPE_LABEL[tool.type] }}
          </Badge>
          <Badge v-if="envName" variant="outline" class="text-[10px] font-normal">
            {{ envName }}
          </Badge>
        </div>
      </div>
    </ContextMenuTrigger>
    <ContextMenuContent>
      <ContextMenuItem @click="$emit('launch', tool)"><Play /> 启动</ContextMenuItem>
      <ContextMenuItem v-if="canRunAsAdmin" @click="$emit('launchAdmin', tool)">
        <ShieldCheck /> 以管理员身份运行
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem @click="$emit('edit', tool)"><Pencil /> 编辑</ContextMenuItem>
      <ContextMenuItem v-if="!isWeb" @click="$emit('openDir', tool)">
        <FolderOpen /> 打开工作目录
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem variant="destructive" @click="$emit('delete', tool)">
        <Trash2 /> 删除
      </ContextMenuItem>
    </ContextMenuContent>
  </ContextMenu>
</template>
