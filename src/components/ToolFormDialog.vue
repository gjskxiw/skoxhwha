<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { FolderOpen } from "@lucide/vue";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { api } from "@/lib/api";
import { commitConfig, store, uid } from "@/lib/store";
import {
  TOOL_TYPE_LABEL,
  envKindForType,
  type EnvKind,
  type Tool,
  type ToolType,
} from "@/lib/types";

const open = ref(false);
const isNew = ref(true);
/** 保存中（gui_exe 会先提取图标），避免重复提交 */
const saving = ref(false);
/** 上一次保存被拒的原因，显示在弹窗底部 */
const formError = ref("");

const form = reactive<Tool>({
  id: "",
  name: "",
  type: "terminal_python",
  target: "",
  args: "",
  envId: null,
  groupId: null,
  icon: null,
  description: "",
  sort: 0,
});

function emptyTool(): Tool {
  return {
    id: uid("t"),
    name: "",
    type: "terminal_python",
    target: "",
    args: "",
    envId: null,
    groupId: null,
    icon: null,
    description: "",
    sort: 0,
  };
}

function openNew(groupId?: string | null) {
  isNew.value = true;
  Object.assign(form, emptyTool());
  form.groupId =
    groupId && groupId !== "all" && groupId !== "none" ? groupId : null;
  formError.value = "";
  open.value = true;
}

function openEdit(tool: Tool) {
  isNew.value = false;
  Object.assign(form, JSON.parse(JSON.stringify(tool)));
  formError.value = "";
  open.value = true;
}

defineExpose({ openNew, openEdit });

const needFile = computed(() => form.type !== "web");
const envKind = computed<EnvKind | null>(() => envKindForType(form.type));

const envOptions = computed(() =>
  envKind.value ? store.config.envs.filter((e) => e.kind === envKind.value) : [],
);
/** 该类型还没有任何可用的运行环境：需要先去顶栏配置 */
const envMissing = computed(() => !!envKind.value && envOptions.value.length === 0);
/** Java / Python 类工具必须绑定环境，未选中时表单标红并阻止保存 */
const envInvalid = computed(() => !!envKind.value && !form.envId);
const envPlaceholder = computed(() =>
  envMissing.value ? "请配置环境变量" : "请选择运行环境",
);

// 绑定环境（__none__ = 未选中；没有任何环境时 SelectValue 显示占位文案）
const envSelect = computed<string>({
  get: () => form.envId ?? "__none__",
  set: (v) => {
    form.envId = v === "__none__" ? null : v;
  },
});

const groupSelect = computed<string>({
  get: () => form.groupId ?? "__none__",
  set: (v) => {
    form.groupId = v === "__none__" ? null : v;
  },
});

watch(
  () => form.type,
  () => {
    if (form.envId) {
      const env = store.config.envs.find((e) => e.id === form.envId);
      if (!env || env.kind !== envKind.value) form.envId = null;
    }
  },
);

// 改过任何字段，上一次显示的被拒原因就过期了
watch(form, () => {
  formError.value = "";
});

const TARGET_FILTERS: Record<ToolType, { name: string; extensions: string[] }[]> = {
  terminal_python: [{ name: "Python 脚本", extensions: ["py"] }],
  terminal_java: [{ name: "JAR", extensions: ["jar"] }],
  terminal_exe: [{ name: "EXE", extensions: ["exe"] }],
  gui_java: [{ name: "JAR", extensions: ["jar"] }],
  gui_exe: [{ name: "EXE", extensions: ["exe"] }],
  web: [],
  unknown: [],
};

/** 可新建的类型（排除后端容错用的 unknown） */
const TYPE_OPTIONS = (Object.entries(TOOL_TYPE_LABEL) as [ToolType, string][]).filter(
  ([key]) => key !== "unknown",
);

async function pickTarget() {
  if (form.type === "web") return;
  const selected = await openFileDialog({
    multiple: false,
    filters: TARGET_FILTERS[form.type],
  });
  if (typeof selected === "string") form.target = selected;
}

/** 该类型要求的扩展名：直接取文件选择器声明的第一个，避免两处规则各写一份而漂移 */
function requiredExt(type: ToolType): string | undefined {
  return TARGET_FILTERS[type]?.[0]?.extensions?.[0];
}

/** 取扩展名（小写、不含点）；无扩展名返回空串。前导点的文件名（如 .gitignore）不算扩展名 */
function fileExt(path: string): string {
  const base = path.replace(/\\/g, "/").split("/").pop() ?? "";
  const i = base.lastIndexOf(".");
  return i > 0 ? base.slice(i + 1).toLowerCase() : "";
}

function validate(): string | null {
  if (!form.name.trim()) return "请填写名称";
  const target = form.target.trim();
  if (!target) {
    return form.type === "web" ? "请填写 URL" : "请选择文件路径";
  }
  if (form.type === "web") {
    return /^https?:\/\//i.test(target) ? null : "URL 需以 http:// 或 https:// 开头";
  }
  // 与后端 check_tool 同一套规则：类型决定扩展名
  const ext = requiredExt(form.type);
  if (ext) {
    const actual = fileExt(target);
    if (actual !== ext) {
      return `该类型需要 .${ext} 文件${actual ? `，当前是 .${actual}` : "（当前文件没有扩展名）"}`;
    }
  }
  // 启动参数：双引号是「这一段算一个参数」的标记，不成对会被错切；
  // 终端类还要经过 cmd，% 会被它当变量展开（& | > 已由后端逐参数包引号，无需限制）
  if ((form.args.match(/"/g) ?? []).length % 2 !== 0) return "启动参数的双引号没有配对";
  if (form.type.startsWith("terminal_") && form.args.includes("%")) {
    return "终端类工具的启动参数不能含 %，cmd 会把它当作变量展开";
  }
  // Java / Python 类工具必须绑定运行环境（后端不再回退系统 PATH）
  if (envInvalid.value) {
    return envMissing.value
      ? "请先点击顶栏的 Java / Python 按钮配置运行环境"
      : "请选择运行环境";
  }
  return null;
}

async function save() {
  // 同分组弹窗：关闭动画期间按钮仍可点，必须用 open 状态兜住第二次提交
  if (saving.value || !open.value) return;
  const err = validate();
  if (err) {
    formError.value = err;
    return;
  }
  formError.value = "";
  saving.value = true;
  try {
    const snapshot: Tool = JSON.parse(JSON.stringify(form));

    // gui_exe：图标由用户指定的 exe 自动提取（表单里已不再提供任何图标操作）。
    // 只在「新工具 / 类型变了 / 目标文件变了 / 还没有图标」时才提取：
    // 图标文件名是目标路径的哈希，其余情况重提取只是白跑一次 GDI + PNG 编码。
    // 提取失败就置空，卡片会回退到内置的类型图标。
    const prev = isNew.value
      ? null
      : (store.config.tools.find((t) => t.id === snapshot.id) ?? null);
    const typeChanged = !!prev && prev.type !== snapshot.type;
    const targetChanged = !prev || prev.target !== snapshot.target;
    if (snapshot.type === "gui_exe") {
      if (!snapshot.icon || typeChanged || targetChanged) {
        try {
          snapshot.icon = await api.extractIcon(snapshot.target);
        } catch {
          snapshot.icon = null;
        }
      }
    } else if (typeChanged) {
      // 从 gui_exe 改成别的类型：旧图标是那个 exe 的图标，留着会张冠李戴
      snapshot.icon = null;
    }

    await commitConfig((draft) => {
      if (isNew.value) {
        snapshot.sort = Math.max(-1, ...draft.tools.map((t) => t.sort)) + 1;
        draft.tools.push(snapshot);
      } else {
        const idx = draft.tools.findIndex((t) => t.id === snapshot.id);
        if (idx >= 0) draft.tools[idx] = snapshot;
      }
    });
  } catch {
    // 写盘失败时弹窗保持打开，让用户改完再试，而不是静默丢掉这次编辑
    formError.value = "保存失败，配置未写入";
    return;
  } finally {
    saving.value = false;
  }
  open.value = false;
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="flex max-h-[85vh] flex-col gap-0 overflow-hidden p-0 sm:max-w-xl">
      <DialogHeader class="px-6 pb-4 pt-5 text-left">
        <DialogTitle>{{ isNew ? "新建工具" : "编辑工具" }}</DialogTitle>
      </DialogHeader>

      <div class="min-h-0 flex-1 space-y-4 overflow-y-auto px-6 pb-5">
        <div class="space-y-1.5">
          <Label class="text-muted-foreground">名称</Label>
          <Input v-model="form.name" placeholder="工具名称" />
        </div>

        <div class="grid grid-cols-2 gap-3">
          <div class="space-y-1.5">
            <Label class="text-muted-foreground">类型</Label>
            <Select v-model="form.type">
              <SelectTrigger class="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem v-for="[key, label] in TYPE_OPTIONS" :key="key" :value="key">
                  {{ label }}
                </SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="space-y-1.5">
            <Label class="text-muted-foreground">分组</Label>
            <Select v-model="groupSelect">
              <SelectTrigger class="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="__none__">未分组</SelectItem>
                <SelectItem v-for="g in store.config.groups" :key="g.id" :value="g.id">
                  {{ g.name }}
                </SelectItem>
              </SelectContent>
            </Select>
          </div>
        </div>
        <div class="space-y-1.5">
          <Label class="text-muted-foreground">{{ form.type === "web" ? "URL" : "文件路径" }}</Label>
          <div v-if="form.type === 'web'">
            <Input v-model="form.target" placeholder="http(s)://" />
          </div>
          <div v-else class="flex gap-1.5">
            <Input v-model="form.target" placeholder="可执行文件路径" />
            <Button variant="outline" size="icon" title="选择文件" @click="pickTarget">
              <FolderOpen />
            </Button>
          </div>
        </div>

        <div v-if="needFile" class="space-y-1.5">
          <Label class="text-muted-foreground">启动参数</Label>
          <Input v-model="form.args" placeholder="--help" />
        </div>

        <div v-if="envKind" class="space-y-1.5">
          <Label class="text-muted-foreground">运行环境</Label>
          <Select v-model="envSelect">
            <SelectTrigger
              class="w-full"
              :aria-invalid="envInvalid || undefined"
            >
              <SelectValue :placeholder="envPlaceholder" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem v-for="e in envOptions" :key="e.id" :value="e.id">
                {{ e.name }}
              </SelectItem>
            </SelectContent>
          </Select>
        </div>

        <div class="space-y-1.5">
          <Label class="text-muted-foreground">描述</Label>
          <Input v-model="form.description" placeholder="工具介绍" />
        </div>
      </div>

      <DialogFooter class="border-t px-6 py-3">
        <p v-if="formError" role="alert" class="mr-auto self-center text-xs text-destructive">
          {{ formError }}
        </p>
        <Button variant="outline" @click="open = false">取消</Button>
        <Button :disabled="saving" @click="save">保存</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
