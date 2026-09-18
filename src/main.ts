import { createApp } from "vue";
import App from "./App.vue";
import "./assets/index.css";

createApp(App).mount("#app");

// 禁用 WebView2 默认右键菜单（刷新 / 另存为 / 检查等浏览器菜单），
// 输入框也不再例外 —— 需要粘贴用 Ctrl+V。
//
// 但必须放行自定义右键菜单所在的区域：reka-ui 的 ContextMenuTrigger 在 contextmenu
// 事件里先 await nextTick()、之后才判断 event.defaultPrevented，只有该标志为 false
// 才会打开菜单。这里若一律 preventDefault，卡片和分组上的自定义菜单就会被一并拦掉。
// 放行后自定义菜单自己会调 preventDefault，原生菜单同样不会冒出来。
document.addEventListener("contextmenu", (e) => {
  const el = e.target instanceof Element ? e.target : null;
  if (el?.closest('[data-slot="context-menu-trigger"]')) return;
  e.preventDefault();
});
