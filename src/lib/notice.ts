import { reactive } from "vue";

export type NoticeKind = "info" | "error";

interface Notice {
  text: string;
  kind: NoticeKind;
}

export const notice = reactive<{ current: Notice | null }>({ current: null });

let timer = 0;

/**
 * 底部状态条反馈（非浮动，不打断操作）。
 *
 * 界面已不用 toast：失败既要当场可见，又不该抢焦点，所以统一收口到这里。
 * 同一时刻只保留最新一条，到时自动收起；错误停留更久。
 */
export function notify(text: string, kind: NoticeKind = "info", ms = kind === "error" ? 8000 : 4000) {
  notice.current = { text, kind };
  window.clearTimeout(timer);
  timer = window.setTimeout(() => {
    notice.current = null;
  }, ms);
}

export function clearNotice() {
  window.clearTimeout(timer);
  notice.current = null;
}
