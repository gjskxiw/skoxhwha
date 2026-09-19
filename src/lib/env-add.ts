import { ref } from "vue";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { api } from "@/lib/api";
import { explainProbeError } from "@/lib/env-error";
import { commitConfig, uid } from "@/lib/store";
import type { Env, EnvKind } from "@/lib/types";

const PICKER_TITLE: Record<EnvKind, string> = {
  java: "选择 JDK 根目录",
  python: "选择 Python 所在目录",
  unknown: "选择目录",
};

/**
 * 「选目录 → 后端探测 → 用版本信息自动命名 → 入库」这条链路。
 *
 * 抽出来是因为两个入口都要它：环境弹窗里的「添加」，以及新建工具时发现一套环境都没有、
 * 就地补一个。后者尤其需要返回新建的环境，好让表单立刻把它选上。
 */
export function useAddEnv() {
  const adding = ref(false);
  /** 归类后的人话，空串表示没有失败 */
  const error = ref("");
  /** 原始错误，供 title 悬浮与排查用 */
  const rawError = ref("");

  /** 成功返回入库的环境；用户取消或探测失败返回 null */
  async function addEnv(kind: EnvKind): Promise<Env | null> {
    if (adding.value) return null;
    // 先立标记再开选择器：否则快速双击会弹出两个目录选择器、添加两个环境
    adding.value = true;
    error.value = "";
    rawError.value = "";
    try {
      const dir = await openFileDialog({ directory: true, title: PICKER_TITLE[kind] });
      if (typeof dir !== "string") return null; // 用户取消

      const probe = await api.probeEnv(kind, dir);
      const env: Env = { id: uid("e"), kind, name: probe.name, path: dir };
      await commitConfig((d) => {
        d.envs.push(env);
      });
      return env;
    } catch (e) {
      // 探测失败不入库：宁可列表里少一条，也不要「看着正常但根本不能用」的环境
      rawError.value = String(e);
      error.value = explainProbeError(rawError.value, kind);
      return null;
    } finally {
      adding.value = false;
    }
  }

  return { adding, error, rawError, addEnv };
}
