<script setup lang="ts">
import { computed, ref } from "vue";
import { LoaderCircle, Plus, Trash2 } from "@lucide/vue";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { confirmAction } from "@/lib/confirm";
import { useAddEnv } from "@/lib/env-add";
import { notify } from "@/lib/notice";
import { commitConfig, store } from "@/lib/store";
import { envKindForType, type Env, type EnvKind } from "@/lib/types";

const open = ref(false);
const activeKind = ref<EnvKind>("java");
const { adding, error: addError, rawError, addEnv } = useAddEnv();
/** 「N 个工具未绑定」点开后才列出具体是哪些工具 */
const showUnbound = ref(false);

const KINDS: { kind: EnvKind; label: string }[] = [
  { kind: "java", label: "Java（JDK）" },
  { kind: "python", label: "Python" },
];

/**
 * 顶栏只有一个入口，进来时停在上次看的那一类。
 * （早年是 Java / Python 两个按钮各自带 kind 进来，弹窗内不能切换时才需要那参数）
 */
function openDialog() {
  addError.value = "";
  showUnbound.value = false;
  open.value = true;
}
defineExpose({ open: openDialog });

function switchKind(kind: EnvKind) {
  activeKind.value = kind;
  addError.value = "";
  showUnbound.value = false;
}

const envs = computed(() => store.config.envs.filter((e) => e.kind === activeKind.value));
/** 空态引导语。整句放在 computed 里，是为了让中英文之间的空格可控 —— 模板里拼接插值会丢掉词间空格 */
const emptyHint = computed(() =>
  `还没有${activeKind.value === "java" ? " JDK " : " Python "}环境。选一次安装目录，程序会跑一次版本命令自动命名。`,
);
/** 这类环境下还没绑运行环境的工具 —— Java / Python 类不绑就根本启动不了，是打开这个面板的真正原因 */
const unbound = computed(() =>
  store.config.tools.filter((t) => envKindForType(t.type) === activeKind.value && !t.envId),
);

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
  const env = await addEnv(activeKind.value);
  if (env) showUnbound.value = false;
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
      <DialogHeader class="px-6 pb-3 pt-5 text-left">
        <DialogTitle>运行环境</DialogTitle>
        <!-- 两类环境经常要对照着看，切换就放在弹窗里，不必关掉再回顶栏点另一个按钮 -->
        <div class="inline-flex self-start rounded-md border p-0.5" role="group" aria-label="环境类别">
          <button
            v-for="k in KINDS"
            :key="k.kind"
            type="button"
            class="rounded-sm px-3 py-1 text-xs transition-colors focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring/50"
            :class="
              activeKind === k.kind
                ? 'bg-accent font-medium text-accent-foreground'
                : 'text-muted-foreground hover:text-foreground'
            "
            :aria-pressed="activeKind === k.kind"
            @click="switchKind(k.kind)"
          >
            {{ k.label }}
          </button>
        </div>
      </DialogHeader>

      <div class="min-h-0 flex-1 space-y-2 overflow-y-auto px-6 pb-5">
        <div class="flex items-center justify-between gap-3">
          <p class="text-xs text-muted-foreground">
            {{ envs.length }} 套环境
            <template v-if="unbound.length">
              ·
              <button
                type="button"
                class="underline decoration-dotted underline-offset-2 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring/50"
                :aria-expanded="showUnbound"
                @click="showUnbound = !showUnbound"
              >
                {{ unbound.length }} 个工具未绑定
              </button>
            </template>
          </p>
          <!-- 列表为空时「添加」在下面的空态里，这里再放一个是多余的 -->
          <Button v-if="envs.length" variant="outline" size="sm" :disabled="adding" @click="startNew">
            <LoaderCircle v-if="adding" class="animate-spin" />
            {{ adding ? "识别中…" : "添加" }}
          </Button>
        </div>

        <!-- 归类后的人话；后端原文挂在 title 上，同时已由命令层写进 app.log -->
        <p v-if="addError" role="alert" class="break-words text-xs text-destructive" :title="rawError">
          {{ addError }}
        </p>

        <ul v-if="showUnbound && unbound.length" class="rounded-md border bg-muted/30 px-3 py-2">
          <li v-for="t in unbound" :key="t.id" class="truncate text-xs text-muted-foreground" :title="t.name">
            {{ t.name }}
          </li>
        </ul>

        <!-- 空态即操作区：引导语和按钮放一起，而不是让「点上面的添加」指向别处 -->
        <div
          v-if="!envs.length"
          class="flex flex-col items-center gap-2 rounded-md border border-dashed px-4 py-6 text-center"
        >
          <p class="text-xs text-muted-foreground">{{ emptyHint }}</p>
          <Button size="sm" :disabled="adding" @click="startNew">
            <LoaderCircle v-if="adding" class="animate-spin" />
            <Plus v-else />
            {{ adding ? "识别中…" : "添加环境" }}
          </Button>
        </div>

        <!-- 一个容器 + divide-y：每行自带边框时，五套环境就是一摞卡片，边框把每行都抬成独立对象 -->
        <div v-else class="divide-y overflow-hidden rounded-lg border">
          <div v-for="env in envs" :key="env.id" class="flex items-center gap-2 px-3 py-2.5">
            <div class="min-w-0 flex-1">
              <div class="flex items-baseline gap-1.5">
                <span class="truncate text-sm font-medium">{{ env.name }}</span>
                <span v-if="usedCount(env.id) > 0" class="shrink-0 text-[11px] text-muted-foreground">
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
              class="size-8 text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
              :aria-label="`删除 ${env.name}`"
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
