<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "@lucide/vue";

const appWindow = getCurrentWindow();
const maximized = ref(false);

let unlisten: (() => void) | undefined;
let raf = 0;

async function refreshMaximized() {
  try {
    maximized.value = await appWindow.isMaximized();
  } catch {
    // 忽略：仅用于图标状态
  }
}

/** resize 事件风暴下每帧最多查一次，避免拖动窗口边缘时产生大量 IPC 往返 */
function onResized() {
  if (raf) return;
  raf = requestAnimationFrame(() => {
    raf = 0;
    void refreshMaximized();
  });
}

onMounted(async () => {
  unlisten = await appWindow.onResized(onResized);
  await refreshMaximized();
});

onBeforeUnmount(() => {
  if (raf) cancelAnimationFrame(raf);
  unlisten?.();
});

// 焦点环走 inset：这三个按钮贴着窗口边缘，外扩的 ring 会被窗口边界裁掉
const ctrlBtn =
  "grid w-12 place-items-center text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-hidden focus-visible:inset-ring-2 focus-visible:inset-ring-ring";
const closeBtn =
  "grid w-12 place-items-center text-muted-foreground transition-colors hover:bg-destructive hover:text-white focus-visible:outline-hidden focus-visible:inset-ring-2 focus-visible:inset-ring-ring";
</script>

<template>
  <!-- data-tauri-drag-region="deep"：整个标题栏可拖动，按钮/输入框等交互元素自动豁免；双击拖拽区切换最大化（Tauri 内置） -->
  <!-- bg-sidebar 而不是 bg-card：浅色主题下 --card 与 --background 同为纯白，标题栏会和内容区糊成一片；
       用 sidebar 后标题栏与侧栏在两套主题里都是同一层「外壳」，内容区自然沉下去 -->
  <header
    class="flex h-12 shrink-0 select-none items-stretch border-b bg-sidebar"
    data-tauri-drag-region="deep"
  >
    <slot />

    <!-- 窗口控制按钮 -->
    <div class="flex shrink-0 items-stretch border-l border-border">
      <button
        type="button"
        :class="ctrlBtn"
        title="最小化"
        aria-label="最小化"
        @click="appWindow.minimize()"
      >
        <Minus class="size-4" />
      </button>
      <button
        type="button"
        :class="ctrlBtn"
        :title="maximized ? '还原' : '最大化'"
        :aria-label="maximized ? '还原' : '最大化'"
        @click="appWindow.toggleMaximize()"
      >
        <Copy v-if="maximized" class="size-3.5" />
        <Square v-else class="size-3.5" />
      </button>
      <button
        type="button"
        :class="closeBtn"
        title="关闭"
        aria-label="关闭窗口"
        @click="appWindow.close()"
      >
        <X class="size-4" />
      </button>
    </div>
  </header>
</template>
