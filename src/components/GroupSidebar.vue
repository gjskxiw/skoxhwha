<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { Separator } from "@/components/ui/separator";
import { confirmAction } from "@/lib/confirm";
import { notify } from "@/lib/notice";
import { commitConfig, uid, store } from "@/lib/store";
import { GROUP_COLORS, GROUP_COLOR_KEYS } from "@/lib/types";
import { cn } from "@/lib/utils";

const counts = computed<Record<string, number>>(() => {
  const c: Record<string, number> = { all: store.config.tools.length, none: 0 };
  for (const t of store.config.tools) {
    const key = t.groupId || "none";
    c[key] = (c[key] ?? 0) + 1;
  }
  return c;
});

/**
 * 侧栏一行。选中态同时用三种线索表达 —— 左侧色条、底色、字重 ——
 * 因为深色主题下 hover 与选中只差一档底色明度，扫视时容易分不清当前在看哪个组。
 */
function rowClass(active: boolean) {
  return cn(
    "relative flex w-full items-center gap-2 rounded-md py-1.5 pr-2 pl-4 text-sm transition-colors",
    "focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring/50",
    active
      ? "bg-sidebar-accent font-medium text-sidebar-accent-foreground"
      : "hover:bg-sidebar-accent/60",
  );
}

/** 色标槽位固定占位，保证分组名左右对齐；未上色时留空 */
function dotClass(color: string) {
  return GROUP_COLORS[color] ?? "bg-transparent";
}

// 新建 / 重命名分组
const groupDialogOpen = ref(false);
const groupDialogMode = ref<"new" | "rename">("new");
const renameTargetId = ref<string | null>(null);
const groupNameInput = ref("");
const groupColorInput = ref("");
/** 保存中：防止连点「确定」把同一分组提交两次 */
const saving = ref(false);
/** 上一次提交被拒的原因，显示在输入框下方 */
const groupError = ref("");

// 改过名称，上一次的原因就过期了
watch(groupNameInput, () => {
  groupError.value = "";
});

/** 色板按钮：选中的一圈 ring，未选中的略淡 */
function swatchClass(selected: boolean, colorClass: string) {
  return cn(
    "size-5 shrink-0 rounded-full border transition",
    colorClass || "border-dashed border-muted-foreground/60 bg-transparent",
    selected
      ? "ring-ring/70 ring-2 ring-offset-2 ring-offset-background"
      : "opacity-75 hover:opacity-100",
  );
}

function openNewGroup() {
  groupDialogMode.value = "new";
  renameTargetId.value = null;
  groupNameInput.value = "";
  groupColorInput.value = "";
  groupError.value = "";
  groupDialogOpen.value = true;
}

defineExpose({ openNewGroup });

function openRenameGroup(id: string) {
  const g = store.config.groups.find((x) => x.id === id);
  if (!g) return;
  groupDialogMode.value = "rename";
  renameTargetId.value = id;
  groupNameInput.value = g.name;
  groupColorInput.value = GROUP_COLORS[g.color] ? g.color : "";
  groupError.value = "";
  groupDialogOpen.value = true;
}

async function confirmGroup() {
  // 除了保存中，弹窗已进入关闭动画（groupDialogOpen 已置 false）时也不接受再次提交：
  // 关闭动画有 200ms，按钮仍可点到，光靠 saving 会在保存完成后的动画窗口里漏掉第二击
  if (saving.value || !groupDialogOpen.value) return;
  const name = groupNameInput.value.trim();
  if (!name) {
    groupError.value = "请填写分组名称";
    return;
  }
  const isNewGroup = groupDialogMode.value === "new";
  const targetId = renameTargetId.value;
  const color = groupColorInput.value;
  saving.value = true;
  try {
    await commitConfig((d) => {
      if (isNewGroup) {
        d.groups.push({ id: uid("g"), name, color });
      } else if (targetId) {
        const g = d.groups.find((x) => x.id === targetId);
        if (g) {
          g.name = name;
          g.color = color;
        }
      }
    });
  } catch {
    // 写盘失败时弹窗保持打开，别丢掉用户刚输入的名字
    groupError.value = "保存失败，分组未写入";
    return;
  } finally {
    saving.value = false;
  }
  groupDialogOpen.value = false;
}

async function deleteGroup(id: string) {
  const g = store.config.groups.find((x) => x.id === id);
  if (!g) return;
  const used = store.config.tools.filter((t) => t.groupId === id).length;
  const msg =
    used > 0
      ? `删除分组「${g.name}」？其中 ${used} 个工具将移动到「未分组」。`
      : `删除分组「${g.name}」？`;
  const ok = await confirmAction(msg, { title: "删除分组", confirmText: "删除" });
  if (!ok) return;
  try {
    await commitConfig((d) => {
      d.groups = d.groups.filter((x) => x.id !== id);
      for (const t of d.tools) {
        if (t.groupId === id) t.groupId = null;
      }
    });
    if (store.activeGroupId === id) store.activeGroupId = "all";
  } catch (e) {
    notify(`删除分组失败，配置未写入：${String(e)}`, "error");
  }
}
</script>

<template>
  <aside class="flex w-52 shrink-0 flex-col border-r border-sidebar-border bg-sidebar text-sidebar-foreground">
    <nav class="flex-1 space-y-[3px] overflow-y-auto px-2 pt-3 pb-2">
      <button
        :class="rowClass(store.activeGroupId === 'all')"
        :aria-current="store.activeGroupId === 'all' ? 'true' : undefined"
        @click="store.activeGroupId = 'all'"
      >
        <span
          v-if="store.activeGroupId === 'all'"
          aria-hidden="true"
          class="absolute bottom-1.5 left-0 w-[3px] rounded-full bg-primary top-1.5"
        />
        <span aria-hidden="true" class="size-2 shrink-0" />
        全部
        <span
          v-if="counts.all > 0"
          class="ml-auto rounded-full border border-sidebar-border px-1.5 text-xs tabular-nums text-muted-foreground"
        >{{ counts.all }}</span>
      </button>
      <button
        :class="rowClass(store.activeGroupId === 'none')"
        :aria-current="store.activeGroupId === 'none' ? 'true' : undefined"
        @click="store.activeGroupId = 'none'"
      >
        <span
          v-if="store.activeGroupId === 'none'"
          aria-hidden="true"
          class="absolute bottom-1.5 left-0 w-[3px] rounded-full bg-primary top-1.5"
        />
        <span aria-hidden="true" class="size-2 shrink-0" />
        未分组
        <span
          v-if="counts.none > 0"
          class="ml-auto rounded-full border border-sidebar-border px-1.5 text-xs tabular-nums text-muted-foreground"
        >{{ counts.none }}</span>
      </button>

      <template v-if="store.config.groups.length > 0">
        <Separator />
        <ContextMenu v-for="g in store.config.groups" :key="g.id">
          <ContextMenuTrigger as-child>
            <button
              :class="rowClass(store.activeGroupId === g.id)"
              :aria-current="store.activeGroupId === g.id ? 'true' : undefined"
              @click="store.activeGroupId = g.id"
            >
              <span
                v-if="store.activeGroupId === g.id"
                aria-hidden="true"
                class="absolute bottom-1.5 left-0 w-[3px] rounded-full bg-primary top-1.5"
              />
              <span
                aria-hidden="true"
                class="size-2 shrink-0 rounded-full"
                :class="dotClass(g.color)"
              />
              <span class="truncate">{{ g.name }}</span>
              <span
                v-if="counts[g.id]"
                class="ml-auto rounded-full border border-sidebar-border px-1.5 text-xs tabular-nums text-muted-foreground"
              >{{ counts[g.id] }}</span>
            </button>
          </ContextMenuTrigger>
          <ContextMenuContent>
            <ContextMenuItem @click="openRenameGroup(g.id)">重命名</ContextMenuItem>
            <ContextMenuItem variant="destructive" @click="deleteGroup(g.id)">删除分组</ContextMenuItem>
          </ContextMenuContent>
        </ContextMenu>
      </template>
    </nav>

    <!-- 分组新建/重命名 -->
    <Dialog v-model:open="groupDialogOpen">
      <DialogContent class="sm:max-w-xs">
        <DialogHeader>
          <DialogTitle>{{ groupDialogMode === "new" ? "新建分组" : "重命名分组" }}</DialogTitle>
        </DialogHeader>
        <Input v-model="groupNameInput" @keydown.enter="confirmGroup" />
        <p v-if="groupError" role="alert" class="text-xs text-destructive">{{ groupError }}</p>
        <div class="space-y-1.5">
          <span class="text-xs text-muted-foreground">色标</span>
          <div class="flex items-center gap-1.5">
            <button
              type="button"
              title="不上色"
              aria-label="不上色"
              :aria-pressed="groupColorInput === ''"
              :class="swatchClass(groupColorInput === '', '')"
              @click="groupColorInput = ''"
            />
            <button
              v-for="key in GROUP_COLOR_KEYS"
              :key="key"
              type="button"
              :title="key"
              :aria-label="`色标 ${key}`"
              :aria-pressed="groupColorInput === key"
              :class="swatchClass(groupColorInput === key, GROUP_COLORS[key])"
              @click="groupColorInput = key"
            />
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" @click="groupDialogOpen = false">取消</Button>
          <Button :disabled="saving" @click="confirmGroup">确定</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </aside>
</template>
