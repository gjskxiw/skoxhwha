#!/usr/bin/env node
/**
 * 版本号一致性检查。
 *
 * 版本号在三处各写了一份，手工 bump 很容易漏改：
 *   - package.json            （npm 元数据）
 *   - src-tauri/tauri.conf.json（bundler 与 Windows 文件版本资源）
 *   - src-tauri/Cargo.toml     （Rust crate）
 * Cargo.lock 里 secaxis 那一条由 cargo 自行同步，不在此校验。
 *
 * 用法：pnpm check:version。已挂在 build 脚本最前面，
 * 所以 `pnpm tauri build` 会先校验；不想要这道拦截时把它从 build 里摘掉即可。
 */
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (p) => readFileSync(join(root, p), "utf8");

const pkgVersion = JSON.parse(read("package.json")).version;
const tauriVersion = JSON.parse(read("src-tauri/tauri.conf.json")).version;
const cargoMatch = /^version\s*=\s*"([^"]+)"/m.exec(read("src-tauri/Cargo.toml"));

if (!cargoMatch) {
  console.error("无法从 src-tauri/Cargo.toml 解析 version");
  process.exit(1);
}

const versions = {
  "package.json": pkgVersion,
  "src-tauri/tauri.conf.json": tauriVersion,
  "src-tauri/Cargo.toml": cargoMatch[1],
};

const unique = new Set(Object.values(versions));
if (unique.size !== 1) {
  console.error("版本号不一致：");
  for (const [file, v] of Object.entries(versions)) console.error(`  ${file}: ${v}`);
  process.exit(1);
}

console.log(`版本号一致：${pkgVersion}`);
