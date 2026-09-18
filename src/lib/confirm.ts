import { reactive } from "vue";

interface ConfirmState {
  open: boolean;
  title: string;
  message: string;
  confirmText: string;
  destructive: boolean;
}

export const confirmState = reactive<ConfirmState>({
  open: false,
  title: "确认操作",
  message: "",
  confirmText: "确定",
  destructive: true,
});

let resolver: ((ok: boolean) => void) | null = null;

/** 应用内确认框：返回 Promise<boolean>，样式与应用一致且不阻塞渲染 */
export function confirmAction(
  message: string,
  opts: { title?: string; confirmText?: string; destructive?: boolean } = {},
): Promise<boolean> {
  confirmState.title = opts.title ?? "确认操作";
  confirmState.message = message;
  confirmState.confirmText = opts.confirmText ?? "确定";
  confirmState.destructive = opts.destructive ?? true;
  // 上一个确认框若还挂着，先按「取消」结掉，
  // 否则它的调用方会永远 await 不到结果（resolver 是模块级单例）
  resolver?.(false);
  confirmState.open = true;
  return new Promise((resolve) => {
    resolver = resolve;
  });
}

export function resolveConfirm(ok: boolean) {
  confirmState.open = false;
  resolver?.(ok);
  resolver = null;
}
