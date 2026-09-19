<script setup lang="ts">
import { ref } from "vue";
import { LoaderCircle, Trash2 } from "@lucide/vue";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { api } from "@/lib/api";
import { confirmAction } from "@/lib/confirm";
import { notify } from "@/lib/notice";
import { commitConfig, store, uid } from "@/lib/store";
import type { Env, EnvKind } from "@/lib/types";

const open = ref(false);
const activeKind = ref<EnvKind>("java");
/** 正在走「选目录 → 探测 → 添加」的流程 */
const adding = ref(false);
/** 上一次探测失败的原因，显示在「添加」按钮下方 */
const addError = ref("");

function openDialog(kind: EnvKind = "java") {
  activeKind.value = kind;
  addError.value = "";
  open.value = true;
}
defineExpose({ open: openDialog });

function envsOf(kind: EnvKind) {
  return store.config.envs.filter((e) => e.kind === kind);
}

function usedCount(envId: string) {
  return store.config.tools.filter((t) => t.envId === envId).length;
}

/**
 * 添加环境：直接弹目录选择器，选完由后端跑一次 --version 验证，
 * 用版本信息自动命名后直接入库 —— 不需要用户先想名字、也不需要先保存再验证。
 * 探测失败就不入库，避免出现「看着正常但根本不能用」的环境。
 *
 * 正因为入库前已经验证过，这里不再提供「验证」按钮；
 * 名称由版本自动生成，也不再提供「编辑」—— 选错了删掉重加即可。用不上的环境
 * 留在列表里没有任何意义，而它一旦失效，工具启动时会直接报错。
 */
async function startNew() {
  if (adding.value) return;
  // 先把标记立起来再开选择器：否则快速双击会弹出两个目录选择器、添加两个环境
  adding.value = true;
  addError.value = "";
  try {
    const dir = await openFileDialog({
      directory: true,
      title: activeKind.value === "java" ? "选择 JDK 根目录" : "选择 Python 所在目录",
    });
    if (typeof dir !== "string") return; // 用户取消

    const probe = await api.probeEnv(activeKind.value, dir);
    await commitConfig((d) => {
      d.envs.push({
        id: uid("e"),
        kind: activeKind.value,
        name: probe.name,
        path: dir,
      });
    });
  } catch (e) {
    // 探测失败不入库，原因就地显示；成功后列表里出现新条目即是反馈
    addError.value = `这个目录不是有效的环境：${String(e)}；换一个目录再试。`;
  } finally {
    adding.value = false;
  }
}

async function removeEnv(env: Env) {
  const used = usedCount(env.id);
  const msg =
    used > 0
      ? `删除环境「${env.name}」？${used} 个工具将失去绑定，需重新选择环境。`
      : `删除环境「${env.name}」？`;
  const ok = await confirmAction(msg, { title: "删除环境", confirmText: "删除" });
  if (!ok) return;
  try {
    await commitConfig((d) => {
      d.envs = d.envs.filter((x) => x.id !== env.id);
      for (const t of d.tools) {
        if (t.envId === env.id) t.envId = null;
      }
    });
  } catch (e) {
    notify(`删除环境失败，配置未写入：${String(e)}`, "error");
  }
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="flex max-h-[85vh] flex-col gap-0 overflow-hidden p-0 sm:max-w-xl">
      <DialogHeader class="px-6 pb-4 pt-5 text-left">
        <DialogTitle>{{ activeKind === "java" ? "Java 环境（JDK）" : "Python 环境" }}</DialogTitle>
      </DialogHeader>

      <div class="min-h-0 flex-1 space-y-2 overflow-y-auto px-6 pb-5">
        <div class="mb-3 flex items-center justify-between">
          <span class="text-xs text-muted-foreground">
            {{ envsOf(activeKind).length }} 套环境
          </span>
          <Button variant="outline" size="sm" :disabled="adding" @click="startNew">
            <LoaderCircle v-if="adding" class="animate-spin" />
            {{ adding ? "识别中…" : "添加" }}
          </Button>
        </div>

        <p v-if="addError" role="alert" class="-mt-1 mb-3 break-words text-xs text-destructive">
          {{ addError }}
        </p>

        <div
          v-if="envsOf(activeKind).length === 0"
          class="rounded-md border border-dashed p-3 text-xs text-muted-foreground"
        >
          {{ activeKind === "java" ? "点「添加」选择 Java 根目录" : "点「添加」选择 Python 所在目录" }}
        </div>

        <div v-for="env in envsOf(activeKind)" :key="env.id" class="rounded-lg border p-3">
          <div class="flex items-center gap-2">
            <div class="min-w-0 flex-1">
              <div class="flex items-baseline gap-1.5">
                <span class="truncate text-sm font-medium">{{ env.name }}</span>
                <span
                  v-if="usedCount(env.id) > 0"
                  class="shrink-0 text-[10px] text-muted-foreground"
                >
                  {{ usedCount(env.id) }} 个工具使用
                </span>
              </div>
              <div class="truncate text-xs text-muted-foreground" :title="env.path">
                {{ env.path }}
              </div>
            </div>
            <Button
              variant="ghost"
              size="icon"
              class="size-8 text-destructive"
              title="删除"
              @click="removeEnv(env)"
            >
              <Trash2 />
            </Button>
          </div>
        </div>
      </div>
    </DialogContent>
  </Dialog>
</template>
