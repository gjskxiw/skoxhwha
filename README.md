# SecAxis

把散落在各处的脚本、JAR 和绿色软件收进一个面板里统一启动的 Windows 工具箱。
Tauri 2 + Vue 3 + Rust，单文件绿色版，约 5 MB。

## 功能

- **工具卡片**：分组 + 搜索，双击启动；支持终端 Python / Java / exe、GUI Java / exe 与网页链接
- **环境绑定**：Java / Python 类工具必须绑定手动配置的 JDK / Python，避免跑错版本
- **参数安全**：参数按 token 逐个引用，含空格的参数不丢，`& | ; >` 只是参数内容；终端类工具必须经 cmd 保窗，所以 `%` 与不成对的双引号会被拒绝（cmd 会先展开变量再解析特殊字符）
- **管理员运行**：仅 GUI exe 支持右键「以管理员身份运行」
- **托盘与主题**：托盘常驻、深色 / 浅色主题、配置导出 / 导入
- **配置便携**：数据存于 exe 同目录 `data\`，不可写时回退 `%APPDATA%\SecAxis\data`

## 使用

1. 顶栏 **Java 环境 / Python 环境** 按钮添加运行环境（选目录，自动校验并命名）
2. 右上角**新建工具**：选类型、目标文件，按需绑定运行环境与启动参数
3. 双击卡片启动；右键可编辑、打开工作目录、删除（GUI exe 可提权运行）

## 构建

需要 Node.js、pnpm 与 Rust（MSVC 工具链）：

```bash
pnpm install
pnpm tauri build   # 产物：src-tauri/target/release/secaxis.exe
```
