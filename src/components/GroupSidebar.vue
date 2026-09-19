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

const counts = computed<Record<string, number>>(() => {
  const c: Record<string, number> = { all: store.config.tools.length, none: 0 };
  for (const t of store.config.tools) {
    const key = t.groupId || "none";
    c[key] = (c[key] ?? 0) + 1;
  }
  return c;
});

// 新建 / 重命名分组
const groupDialogOpen = ref(false);
const groupDialogMode = ref<"new" | "rename">("new");
const renameTargetId = ref<string | null>(null);
const groupNameInput = ref("");
/** 保存中：防止连点「确定」把同一分组提交两次 */
const saving = ref(false);
/** 上一次提交被拒的原因，显示在输入框下方 */
const groupError = ref("");

// 改过名称，上一次的原因就过期了
watch(groupNameInput, () => {
  groupError.value = "";
});

function openNewGroup() {
  groupDialogMode.value = "new";
  renameTargetId.value = null;
  groupNameInput.value = "";
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
  saving.value = true;
  try {
    await commitConfig((d) => {
      if (isNewGroup) {
        d.groups.push({ id: uid("g"), name });
      } else if (targetId) {
        const g = d.groups.find((x) => x.id === targetId);
        if (g) g.name = name;
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
  <aside class="flex w-52 shrink-0 flex-col border-r bg-sidebar text-sidebar-foreground">
    <nav class="flex-1 space-y-[3px] overflow-y-auto px-2 pt-3 pb-2">
      <button
        class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-sm transition-colors"
        :class="store.activeGroupId === 'all' ? 'bg-sidebar-active font-medium text-sidebar-active-foreground' : 'hover:bg-sidebar-active/60'"
        @click="store.activeGroupId = 'all'"
      >
        全部
        <span class="ml-auto text-xs text-muted-foreground">{{ counts.all }}</span>
      </button>
      <button
        class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-sm transition-colors"
        :class="store.activeGroupId === 'none' ? 'bg-sidebar-active font-medium text-sidebar-active-foreground' : 'hover:bg-sidebar-active/60'"
        @click="store.activeGroupId = 'none'"
      >
        未分组
        <span class="ml-auto text-xs text-muted-foreground">{{ counts.none ?? 0 }}</span>
      </button>

      <template v-if="store.config.groups.length > 0">
        <Separator />
        <ContextMenu v-for="g in store.config.groups" :key="g.id">
          <ContextMenuTrigger as-child>
            <button
              class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-sm transition-colors"
              :class="store.activeGroupId === g.id ? 'bg-sidebar-active font-medium text-sidebar-active-foreground' : 'hover:bg-sidebar-active/60'"
              @click="store.activeGroupId = g.id"
            >
              <span class="truncate">{{ g.name }}</span>
              <span class="ml-auto text-xs text-muted-foreground">{{ counts[g.id] ?? 0 }}</span>
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
        <DialogFooter>
          <Button variant="outline" @click="groupDialogOpen = false">取消</Button>
          <Button :disabled="saving" @click="confirmGroup">确定</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </aside>
</template>
