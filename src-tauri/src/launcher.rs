use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use tauri::AppHandle;

use crate::config::{Config, Env, EnvKind, Tool, ToolKind};
use crate::shellex;

pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;
pub const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

fn cmd_path() -> String {
    std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".into())
}

pub fn bound_env<'a>(tool: &Tool, config: &'a Config) -> Option<&'a Env> {
    let id = tool.env_id.as_deref()?;
    config.envs.iter().find(|e| e.id == id)
}

/// 工具的工作目录：不再是可配置项，一律取目标文件所在目录。
/// cmd 分支的 cd /d 与 GUI 进程的 current_dir 都走这里；
/// 「打开工作目录」命令也复用它，保证与启动时用的是同一个目录。
///
/// 手改 / 导入的配置里目标可能是裸文件名（如 `a.exe`），此时「所在目录」没有目录成分
/// （`Path::new("a.exe").parent()` 是 `Some("")` 而不是 None），取当前工作目录，
/// 免得空串被拼成 `cd /d ""` 或传给「打开工作目录」。
pub fn workdir_of(tool: &Tool) -> PathBuf {
    match Path::new(&tool.target).parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    }
}

/// Java：env.path 为 JDK 根目录；Python：env.path 为 python.exe 所在目录。
/// 未知类型按 Python 风格处理；真正启动前 check_tool 会拒绝。
fn bin_dir(env: &Env) -> PathBuf {
    let p = PathBuf::from(&env.path);
    match env.kind {
        EnvKind::Java => p.join("bin"),
        _ => p,
    }
}

fn python_exe(env: &Env) -> PathBuf {
    bin_dir(env).join("python.exe")
}

/// 从 java -version / python --version 的输出推导环境显示名。
/// Java 取首个引号内的版本号，Python 直接取首行（本来就是「Python 3.12.4」）。
pub fn suggest_env_name(kind: EnvKind, output: &str) -> Option<String> {
    let first = output.lines().map(str::trim).find(|l| !l.is_empty())?;
    match kind {
        EnvKind::Java => {
            // openjdk version "17.0.9" 2023-10-17
            let raw = first.split_once('"')?.1.split_once('"')?.0.trim();
            if raw.is_empty() {
                return None;
            }
            Some(format!("JDK {raw}"))
        }
        EnvKind::Python => Some(first.to_string()),
        EnvKind::Unknown => None,
    }
}

fn java_exe(env: &Env) -> PathBuf {
    bin_dir(env).join("java.exe")
}

fn javaw_exe(env: &Env) -> PathBuf {
    bin_dir(env).join("javaw.exe")
}

/// 各工具类型要求的文件扩展名（None = 不是文件类工具，或不做限制）
fn required_ext(kind: ToolKind) -> Option<&'static str> {
    match kind {
        ToolKind::TerminalPython => Some("py"),
        ToolKind::TerminalJava | ToolKind::GuiJava => Some("jar"),
        ToolKind::TerminalExe | ToolKind::GuiExe => Some("exe"),
        ToolKind::Web | ToolKind::Unknown => None,
    }
}

/// 终端类工具：启动时要经过一次 cmd 解析，参数与路径受 cmd 的规则约束（见 launch_terminal）
fn is_terminal(kind: ToolKind) -> bool {
    matches!(
        kind,
        ToolKind::TerminalPython | ToolKind::TerminalJava | ToolKind::TerminalExe
    )
}

/// 取扩展名（小写、不含点）；无扩展名返回空串
fn file_ext(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// 双引号：既用来包参数，也因此会提前闭合别人的引号，参数里不允许出现。
const DOUBLE_QUOTE: char = '"';
/// 路径字段连单引号也一并拒绝：Windows 文件名不允许含双引号，环境目录名在实践里也不
/// 需要单引号，出现即视为手工编辑 / 导入的配置有问题。
const QUOTE_CHARS: [char; 2] = [DOUBLE_QUOTE, '\''];
/// 换行/回车：Shell 会把它当成语句分隔符，任何会进入命令行的字段都不该含它。
const LINE_BREAK_CHARS: [char; 2] = ['\r', '\n'];
/// `%`：cmd 解析命令行的顺序是「先展开 `%NAME%`、再识别特殊字符」，而双引号只保护后者、
/// 不保护前者 —— 引号里的 `%PATH%` 照样会被替换掉，替换出来的文本还要再过一遍分隔符解析。
/// 实测：`cmd /C ""dump.exe" "\"%PATH%\"""` 子进程收到的不是 `%PATH%` 而是整条 PATH。
/// 所以它既会让参数悄悄变样，也是二次注入原语，凡是进入 cmd 命令行的字段一律拒绝。
const PERCENT_CHAR: char = '%';

/// 校验一处「会进入命令行」的路径字段。
///
/// 背景：env.path 会以 `set "PATH={dir};%PATH%"` 的形式进入 cmd 命令行。
/// 工具参数（args）由 quote_windows_arg_forced 逐个包引号后交给 cmd（见 launch_terminal），
/// 但路径字段仍是字符串拼接，必须单独设防：引号会提前闭合 cmd 的引号，换行会变成
/// 语句分隔符，之后的内容都会变成裸命令。
///
/// 除 env.path 外，工具的 target（脚本 / JAR / EXE 路径）也会拼进 cmd 的 `cd /d "…"`，
/// 所以同一个函数也用于校验它。Windows 文件名本身不允许双引号，正常选用文件不会触发；
/// 这道校验拦住的是手工编辑 / 导入的配置。
pub fn check_env_path(what: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{what}为空"));
    }
    if let Some(c) = value.chars().find(|c| QUOTE_CHARS.contains(c)) {
        return Err(format!(
            "{what}含引号 {c:?}，会让终端命令的引号提前闭合，已拒绝执行：{value}"
        ));
    }
    if let Some(c) = value.chars().find(|c| LINE_BREAK_CHARS.contains(c)) {
        return Err(format!(
            "{what}含换行（{}），无法安全传给终端，已拒绝执行",
            c as u32
        ));
    }
    if value.contains(PERCENT_CHAR) {
        return Err(format!(
            "{what}含 %，终端会把它当作变量展开后再解析一次，已拒绝执行：{value}"
        ));
    }
    Ok(())
}

/// 校验终端类工具的参数 token（程序路径由 check_env_path 负责）。
///
/// 每个 token 都会被 quote_windows_arg_forced 整体包上双引号，因此 `& | < > ^ ; =` 只是
/// 参数内容；仍然无法安全表达的是：双引号（破坏参数边界）、换行（语句分隔符）、`%`（见
/// PERCENT_CHAR）。单引号对 cmd 与 CreateProcess 都不特殊，`--name=O'Brien` 要放行。
fn check_cmd_args(what: &str, args: &[String]) -> Result<(), String> {
    for arg in args {
        if arg.contains(PERCENT_CHAR) {
            return Err(format!(
                "{what}含 %，终端会把它当作变量展开后再解析一次，请去掉该字符：{arg}"
            ));
        }
        if arg.contains(DOUBLE_QUOTE) {
            // split_args 只把双引号当分组标记并剥掉，走到这里说明有代码绕过了它
            return Err(format!("{what}含双引号，无法安全交给终端：{arg}"));
        }
        if let Some(c) = arg.chars().find(|c| LINE_BREAK_CHARS.contains(c)) {
            return Err(format!("{what}含换行（{}），会被终端当成语句分隔符", c as u32));
        }
    }
    Ok(())
}

/// 按 Windows 命令行规则（CommandLineToArgvW / VC++ 运行时同款）给单个参数加引号。
///
/// 只用于不得不拼命令行的场合 —— 提权启动走 ShellExecuteEx，只能传一个参数字符串。
/// 普通启动一律交给 Command::args（由标准库负责引用），不经过这里。
pub fn quote_windows_arg(arg: &str) -> String {
    quote_arg(arg, false)
}

/// 同上，但即使参数不含空白也强制包引号。
///
/// 这是交给 cmd 的那一层：cmd 只看到一整行文本，裸的 `& | < >` 会被它当成分隔符，
/// 而引号内的这些字符是普通内容。多出来的引号由子进程的 CommandLineToArgvW 剥掉，
/// 参数内容不变（已在实测中逐字符验证）。
fn quote_windows_arg_forced(arg: &str) -> String {
    quote_arg(arg, true)
}

fn quote_arg(arg: &str, force: bool) -> String {
    let needs_quotes = arg.is_empty()
        || arg
            .chars()
            .any(|c| c == ' ' || c == '\t' || c == DOUBLE_QUOTE);
    if !force && !needs_quotes {
        return arg.to_string();
    }
    let mut out = String::with_capacity(arg.len() + 2);
    out.push(DOUBLE_QUOTE);
    let mut backslashes = 0usize;
    for c in arg.chars() {
        match c {
            '\\' => {
                backslashes += 1;
                out.push('\\');
            }
            DOUBLE_QUOTE => {
                // 引号前的反斜杠要翻倍，再补一个转义引号本身
                for _ in 0..backslashes {
                    out.push('\\');
                }
                backslashes = 0;
                out.push_str("\\\"");
            }
            _ => {
                backslashes = 0;
                out.push(c);
            }
        }
    }
    // 结尾的反斜杠会转义收尾引号，同样翻倍
    for _ in 0..backslashes {
        out.push('\\');
    }
    out.push('"');
    out
}

/// 把 argv 拼成单个字符串（仅提权路径使用）
fn join_windows_args(args: &[String]) -> String {
    args.iter()
        .map(|a| quote_windows_arg(a))
        .collect::<Vec<_>>()
        .join(" ")
}

/// 启动用的「程序」解析结果
struct LaunchExe {
    path: String,
    /// 解析过程中产生的问题描述（目前只有「找不到」）
    issue: Option<String>,
}

/// 取 Java / Python 类工具绑定的运行环境；未绑定或类型不符时直接拒绝。
/// 这类工具不再回退系统 PATH —— 系统里装着哪个版本不可控。
fn require_env<'a>(
    tool: &Tool,
    config: &'a Config,
    kind: EnvKind,
    label: &str,
) -> Result<&'a Env, String> {
    let env = bound_env(tool, config)
        .ok_or_else(|| format!("未绑定 {label} 环境：请先在顶栏添加并选择 {label} 环境"))?;
    if env.kind != kind {
        return Err(format!("环境「{}」不是 {label} 环境", env.name));
    }
    Ok(env)
}

/// 解析 Java / Python 类工具启动要用的可执行文件（环境已由 require_env 保证）。
///
/// 提权启动（ShellExecuteEx）继承的是注册表环境，进程内做的 PATH 注入不会生效，
/// 所以这里统一返回绝对路径，避免「普通启动能跑、以管理员身份运行报找不到」这种双标行为。
fn resolve_launch_exe(tool_type: ToolKind, env: &Env) -> LaunchExe {
    match tool_type {
        ToolKind::TerminalPython => {
            let p = python_exe(env);
            let path = p.display().to_string();
            LaunchExe {
                issue: (!p.is_file()).then(|| format!("Python 不存在: {path}")),
                path,
            }
        }
        ToolKind::TerminalJava | ToolKind::GuiJava => {
            let p = if tool_type == ToolKind::GuiJava {
                javaw_exe(env)
            } else {
                java_exe(env)
            };
            let path = p.display().to_string();
            LaunchExe {
                issue: (!p.is_file()).then(|| format!("Java 不存在: {path}")),
                path,
            }
        }
        _ => LaunchExe {
            path: String::new(),
            issue: Some("该工具类型不支持解析启动程序".into()),
        },
    }
}

/// 把工具类型 + 参数翻译成「可执行文件之后」的 argv 数组。
///
/// 这里只负责切分与排序，不做引用：终端类工具由 launch_terminal 逐 token 强制包引号
/// 后交给 cmd，GUI 类工具由 Command::args / join_windows_args 处理。
/// Python / Java 的参数里带上 target（脚本或 -jar 包），exe 的 target 就是可执行文件
/// 本身，不能再当作第一个参数重复传一遍。
fn command_arguments(tool: &Tool) -> Vec<String> {
    let mut out = Vec::new();
    match tool.tool_type {
        ToolKind::TerminalPython => {
            out.push(tool.target.clone());
            out.extend(split_args(&tool.args));
        }
        ToolKind::TerminalJava => {
            // JVM 参数排在 -jar 之前，与 launch_gui_java 保持同一套顺序
            let (jvm, rest) = split_java_args(&tool.args);
            out.extend(jvm);
            out.push("-jar".into());
            out.push(tool.target.clone());
            out.extend(rest);
        }
        _ => out.extend(split_args(&tool.args)),
    }
    out
}
/// 启动前依赖检查；Err 描述缺什么。
///
/// Java / Python 类工具必须绑定运行环境：未绑定或类型不符直接拒绝，
/// 不再回退系统 PATH。
pub fn check_tool(tool: &Tool, config: &Config) -> Result<(), String> {
    // 工具类型与目标文件的扩展名必须匹配。表单里也会校验一次，这里兜住手改 / 导入的
    // 配置，让卡片直接标红说明原因，而不是等到点启动才失败。
    if let Some(ext) = required_ext(tool.tool_type) {
        let target = tool.target.trim();
        if !target.is_empty() {
            let actual = file_ext(target);
            if actual != ext {
                let shown = if actual.is_empty() {
                    "无扩展名".to_string()
                } else {
                    format!(".{actual}")
                };
                return Err(format!(
                    "文件类型不符：该类型需要 .{ext} 文件，当前是{shown}"
                ));
            }
        }
    }
    // 运行环境的路径会进入终端命令行，含引号 / 换行 / % 时直接标红（详见 check_env_path）
    if let Some(env) = bound_env(tool, config) {
        check_env_path(format!("环境「{}」的路径", env.name).as_str(), &env.path)?;
    }
    // split_args 把双引号当作「这一段属于同一个参数」的标记，只有一半时后面所有内容都会被
    // 并进同一个参数 —— 与其悄悄传出一个奇怪的长参数，不如直接说明（GUI 类同样受影响）
    if tool.tool_type != ToolKind::Web
        && tool.tool_type != ToolKind::Unknown
        && tool.args.chars().filter(|c| *c == DOUBLE_QUOTE).count() % 2 != 0
    {
        return Err("启动参数的双引号没有配对".into());
    }
    // 终端类工具还要经过 cmd：强制包引号能压住 & | < >，压不住引号本身 / 换行 / %
    if is_terminal(tool.tool_type) {
        check_cmd_args("启动参数", &command_arguments(tool))?;
    }
    match tool.tool_type {
        ToolKind::Unknown => Err("未知的工具类型（可能由更高版本创建，当前版本无法启动）".into()),
        ToolKind::Web => {
            let url = tool.target.trim();
            if url.is_empty() {
                return Err("未配置 URL".into());
            }
            let lower = url.to_ascii_lowercase();
            if !(lower.starts_with("http://") || lower.starts_with("https://")) {
                return Err(format!("URL 必须以 http:// 或 https:// 开头: {url}"));
            }
            Ok(())
        }
        ToolKind::TerminalPython => {
            if tool.target.trim().is_empty() {
                return Err("未配置脚本路径".into());
            }
            check_env_path("脚本路径", &tool.target)?;
            if !Path::new(&tool.target).is_file() {
                return Err(format!("脚本不存在: {}", tool.target));
            }
            let env = require_env(tool, config, EnvKind::Python, "Python")?;
            let resolved = resolve_launch_exe(tool.tool_type, env);
            if let Some(issue) = resolved.issue {
                return Err(format!("{issue}（环境「{}」）", env.name));
            }
            Ok(())
        }
        ToolKind::TerminalJava | ToolKind::GuiJava => {
            if tool.target.trim().is_empty() {
                return Err("未配置 JAR 路径".into());
            }
            check_env_path("JAR 路径", &tool.target)?;
            if !Path::new(&tool.target).is_file() {
                return Err(format!("JAR 不存在: {}", tool.target));
            }
            let env = require_env(tool, config, EnvKind::Java, "Java")?;
            let resolved = resolve_launch_exe(tool.tool_type, env);
            if let Some(issue) = resolved.issue {
                return Err(format!("{issue}（环境「{}」）", env.name));
            }
            Ok(())
        }
        ToolKind::TerminalExe | ToolKind::GuiExe => {
            if tool.target.trim().is_empty() {
                return Err("未配置 EXE 路径".into());
            }
            check_env_path("EXE 路径", &tool.target)?;
            if !Path::new(&tool.target).is_file() {
                return Err(format!("EXE 不存在: {}", tool.target));
            }
            Ok(())
        }
    }
}
/// 按双引号切分参数（保留引号内空格）
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut has_token = false;
    for c in s.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                has_token = true;
            }
            ' ' | '\t' if !in_quotes => {
                if has_token {
                    out.push(std::mem::take(&mut cur));
                    has_token = false;
                }
            }
            _ => {
                cur.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        out.push(cur);
    }
    out
}

/// JVM 参数（-Xmx / -Dfoo=bar / -XX:… / --add-opens 等）排在 -jar 之前，其余作为程序参数。
/// 若用户在参数里又写了 `-jar xxx`，忽略之（target 字段才是权威来源）。
fn split_java_args(args: &str) -> (Vec<String>, Vec<String>) {
    let tokens = split_args(args);
    let mut jvm = Vec::new();
    let mut rest = Vec::new();
    let mut in_jvm = true;
    let mut skip_next = false;
    for t in tokens {
        if skip_next {
            skip_next = false; // 丢弃用户重复写的 jar 路径
            continue;
        }
        if in_jvm && is_jvm_arg(&t) {
            jvm.push(t);
            continue;
        }
        in_jvm = false;
        if t == "-jar" {
            skip_next = true;
            continue;
        }
        rest.push(t);
    }
    (jvm, rest)
}

/// 保守白名单判断 JVM 启动参数，避免把程序参数（如 --port）误吞为 JVM 参数
fn is_jvm_arg(t: &str) -> bool {
    t.starts_with("-X")
        || t.starts_with("-D")
        || t.starts_with("--add-")
        || t.starts_with("--enable-")
        || t.starts_with("--disable-")
        || matches!(
            t,
            "-ea" | "-da" | "-esa" | "-dsa" | "-server" | "-client" | "-verbose" | "-version"
        )
}

/// as_admin：本次是否以管理员身份启动。它不再是工具上的持久化属性，
/// 而是右键菜单「以管理员身份运行」在调用时传入的一次性选择；仅 GUI exe 支持。
pub fn launch(app: &AppHandle, tool: &Tool, config: &Config, as_admin: bool) -> Result<(), String> {
    if as_admin && tool.tool_type != ToolKind::GuiExe {
        return Err("只有「GUI exe」类型支持以管理员身份运行".into());
    }
    check_tool(tool, config)?;
    match tool.tool_type {
        ToolKind::Web => {
            use tauri_plugin_opener::OpenerExt;
            app.opener()
                .open_url(&tool.target, None::<&str>)
                .map_err(|e| format!("打开浏览器失败: {e}"))
        }
        ToolKind::TerminalPython | ToolKind::TerminalJava | ToolKind::TerminalExe => {
            launch_terminal(tool, config)
        }
        ToolKind::GuiJava => launch_gui_java(tool, config),
        ToolKind::GuiExe => launch_gui_exe(tool, as_admin),
        ToolKind::Unknown => Err("未知的工具类型，无法启动".into()),
    }
}

/// 组装交给 `cmd /K` 的那一行文本：PATH 注入 + 切工作目录 + 逐 token 强制引用的程序与参数。
///
/// 抽成纯函数是因为注入面就在这行文本上：cmd 拿到的是字符而不是 argv，所以只断言
/// command_arguments 返回的数组测不到它。参数里的 & | ; > 靠 quote_windows_arg_forced
/// 包进的引号退化为普通内容，% 与引号本身则由 check_cmd_args 拒绝。
fn terminal_cmd_line(
    program: &str,
    args: &[String],
    env_bin: Option<&Path>,
    workdir: &Path,
) -> String {
    let mut line = String::new();
    if let Some(dir) = env_bin {
        line.push_str(&format!("set \"PATH={};%PATH%\" && ", dir.display()));
    }
    line.push_str(&format!("cd /d \"{}\" && ", workdir.display()));
    line.push_str(&quote_windows_arg_forced(program));
    for a in args {
        line.push(' ');
        line.push_str(&quote_windows_arg_forced(a));
    }
    line
}

/// 终端类工具的启动器（固定 cmd /K，保留窗口）。
///
/// 保留窗口与「绑定环境的 bin 目录临时置顶 PATH」都必须由 cmd 来做，所以这一路无法绕开
/// Shell 解析 —— 做法是把整行文本自己拼出来（terminal_cmd_line），程序与参数逐 token
/// 强制包引号，再拒绝 cmd 唯一压不住的 % 与引号本身。
fn launch_terminal(tool: &Tool, config: &Config) -> Result<(), String> {
    let workdir = workdir_of(tool);
    // EXE 类工具没有运行环境，程序即目标文件；Java / Python 必绑环境（check_tool 已校验）
    let (program, env) = match tool.tool_type {
        ToolKind::TerminalExe => (tool.target.clone(), None),
        _ => {
            let env = bound_env(tool, config).ok_or_else(|| "未绑定运行环境".to_string())?;
            let exe = resolve_launch_exe(tool.tool_type, env);
            if let Some(issue) = exe.issue {
                return Err(issue);
            }
            (exe.path, Some(env))
        }
    };
    let env_bin = env.map(bin_dir);
    let line = terminal_cmd_line(&program, &command_arguments(tool), env_bin.as_deref(), &workdir);

    // raw_arg：整行由我们自己引用过，不能再让标准库加一层（它会把引号变成 cmd 看不懂的 \"）
    let mut cmd = Command::new(cmd_path());
    cmd.raw_arg(format!("/K {line}"))
        .creation_flags(CREATE_NEW_CONSOLE);
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("启动 cmd 失败: {e}"))
}

fn launch_gui_java(tool: &Tool, config: &Config) -> Result<(), String> {
    let workdir = workdir_of(tool);
    let env = require_env(tool, config, EnvKind::Java, "Java")?;
    let (jvm, rest) = split_java_args(&tool.args);
    let exe = resolve_launch_exe(tool.tool_type, env);
    if let Some(issue) = &exe.issue {
        return Err(issue.clone());
    }

    let mut args: Vec<String> = Vec::with_capacity(jvm.len() + rest.len() + 2);
    args.extend(jvm);
    args.push("-jar".into());
    args.push(tool.target.clone());
    args.extend(rest);

    let mut cmd = Command::new(&exe.path);
    cmd.args(&args).current_dir(&workdir);
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("启动 Java 程序失败: {e}"))
}

fn launch_gui_exe(tool: &Tool, admin: bool) -> Result<(), String> {
    let workdir = workdir_of(tool);
    let args = split_args(&tool.args);
    if admin {
        // 旧实现直接把 args 原样交给 ShellExecuteEx，既会破坏含空格的参数，
        // 也让参数里的引号有机会改变命令行结构；现在统一逐参数引用
        let params = join_windows_args(&args);
        shellex::runas(&tool.target, &params, Some(&workdir))
    } else {
        let mut cmd = Command::new(&tool.target);
        cmd.args(&args).current_dir(&workdir);
        cmd.spawn()
            .map(|_| ())
            .map_err(|e| format!("启动程序失败: {e}"))
    }
}

/// 环境验证：java -version / python --version
pub fn validate_env(env: &Env) -> Result<String, String> {
    let (exe, arg) = match env.kind {
        EnvKind::Java => (java_exe(env), "-version"),
        EnvKind::Python => (python_exe(env), "--version"),
        EnvKind::Unknown => return Err("未知的环境类型（可能由更高版本创建）".into()),
    };
    if !exe.is_file() {
        return Err(format!("可执行文件不存在: {}", exe.display()));
    }
    let output = Command::new(&exe)
        .arg(arg)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("执行失败: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let text = if stdout.is_empty() { stderr } else { stdout };
    if output.status.success() {
        Ok(text)
    } else if text.is_empty() {
        Err(format!("执行失败（退出码 {:?}）", output.status.code()))
    } else {
        Err(text)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Tool, ToolKind};

    fn tool(kind: ToolKind, target: &str) -> Tool {
        Tool {
            id: "t1".into(),
            name: "测试工具".into(),
            tool_type: kind,
            target: target.into(),
            args: String::new(),
            env_id: None,
            group_id: None,
            icon: None,
            description: String::new(),
        }
    }

    fn check(t: &Tool, cfg: &Config) -> Result<(), String> {
        check_tool(t, cfg)
    }

    #[test]
    fn split_args_handles_quotes_and_spaces() {
        assert!(split_args("").is_empty());
        assert_eq!(split_args("-v"), vec!["-v"]);
        assert_eq!(split_args("  a   b  "), vec!["a", "b"]);
        assert_eq!(
            split_args("\"C:\\Program Files\\x.exe\" -v"),
            vec!["C:\\Program Files\\x.exe", "-v"]
        );
        assert_eq!(split_args("--opt=\"a b\""), vec!["--opt=a b"]);
    }

    #[test]
    fn java_args_split_jvm_before_program_args() {
        let (jvm, rest) = split_java_args("-Xmx512m -Dfoo=bar -jar app.jar --port 8080");
        assert_eq!(jvm, vec!["-Xmx512m", "-Dfoo=bar"]);
        // 用户重复写的 -jar 及其路径被忽略（target 字段才是权威来源）
        assert_eq!(rest, vec!["--port", "8080"]);
    }

    #[test]
    fn java_args_do_not_swallow_program_args() {
        let (jvm, rest) = split_java_args("-Xmx512m --port 8080");
        assert_eq!(jvm, vec!["-Xmx512m"]);
        assert_eq!(rest, vec!["--port", "8080"]);
    }

    #[test]
    fn java_args_stop_jvm_at_first_plain_token() {
        let (jvm, rest) = split_java_args("app.jar -Xmx512m");
        assert!(jvm.is_empty());
        assert_eq!(rest, vec!["app.jar", "-Xmx512m"]);
    }

    /// 工具类型与目标文件扩展名必须匹配（先报类型不符，再报文件不存在）
    #[test]
    fn target_extension_must_match_tool_type() {
        let cfg = Config::default();
        assert!(check(&tool(ToolKind::GuiExe, "C:\\x\\a.py"), &cfg)
            .unwrap_err()
            .contains("需要 .exe"));
        assert!(
            check(&tool(ToolKind::TerminalPython, "C:\\x\\a.txt"), &cfg)
                .unwrap_err()
                .contains("需要 .py")
        );
        assert!(
            check(&tool(ToolKind::TerminalJava, "C:\\x\\a.exe"), &cfg)
                .unwrap_err()
                .contains("需要 .jar")
        );
        // 大小写不敏感
        assert!(
            check(&tool(ToolKind::TerminalExe, "C:\\x\\a.EXE"), &cfg)
                .unwrap_err()
                .contains("EXE 不存在")
        );
        // 完全没有扩展名
        assert!(
            check(&tool(ToolKind::TerminalExe, "C:\\x\\tool"), &cfg)
                .unwrap_err()
                .contains("无扩展名")
        );
    }

    /// Web 只接受 http / https
    #[test]
    fn web_url_must_be_http_or_https() {
        let cfg = Config::default();
        assert!(check(
            &tool(ToolKind::Web, "file:///C:/Windows/System32/calc.exe"),
            &cfg
        )
        .unwrap_err()
        .contains("http:// 或 https://"));
        assert!(check(&tool(ToolKind::Web, "https://example.com"), &cfg).is_ok());
        assert!(check(&tool(ToolKind::Web, "HTTP://EXAMPLE.COM"), &cfg).is_ok());
    }

    /// 工作目录不再可配置，恒为目标文件所在目录
    #[test]
    fn workdir_is_always_the_target_directory() {
        let t = tool(ToolKind::TerminalExe, "C:\\tools\\app.exe");
        assert_eq!(workdir_of(&t), PathBuf::from("C:\\tools"));
        let t = tool(ToolKind::TerminalPython, "D:\\work\\scan.py");
        assert_eq!(workdir_of(&t), PathBuf::from("D:\\work"));
    }

    #[test]
    fn unknown_tool_kind_is_rejected_instead_of_panicking() {
        let t = tool(ToolKind::Unknown, "C:\\x.exe");
        let cfg = Config::default();
        assert!(check(&t, &cfg).is_err());
    }

    #[test]
    fn suggest_env_name_parses_version_output() {
        let jdk17 = "openjdk version \"17.0.9\" 2023-10-17\nOpenJDK Runtime Environment Temurin-17.0.9+9 (build 17.0.9+9)";
        assert_eq!(
            suggest_env_name(EnvKind::Java, jdk17).as_deref(),
            Some("JDK 17.0.9")
        );

        // Java 8 的 1.x 写法原样保留
        let jdk8 =
            "java version \"1.8.0_401\"\nJava(TM) SE Runtime Environment (build 1.8.0_401-b10)";
        assert_eq!(
            suggest_env_name(EnvKind::Java, jdk8).as_deref(),
            Some("JDK 1.8.0_401")
        );

        let jdk11 = "openjdk version \"11.0.21\" 2023-10-17 LTS";
        assert_eq!(
            suggest_env_name(EnvKind::Java, jdk11).as_deref(),
            Some("JDK 11.0.21")
        );

        assert_eq!(
            suggest_env_name(EnvKind::Python, "Python 3.12.4").as_deref(),
            Some("Python 3.12.4")
        );

        // 拿不到引号里的版本号时不要瞎猜
        assert_eq!(suggest_env_name(EnvKind::Java, "not a jdk"), None);
        assert_eq!(suggest_env_name(EnvKind::Java, ""), None);
        assert_eq!(suggest_env_name(EnvKind::Unknown, "whatever"), None);
    }

    // ---- 终端类启动：整行文本自己引用，cmd 只能把它当参数 ----

    /// 含 Shell 元字符的参数：切分后仍是独立参数
    #[test]
    fn command_arguments_keep_shell_metacharacters_as_data() {
        let mut t = tool(ToolKind::TerminalExe, r"C:\tools\nmap.exe");
        t.args = r"-sV & calc.exe".into();
        let args = command_arguments(&t);
        assert_eq!(
            args,
            vec!["-sV", "&", "calc.exe"],
            "参数只应按空白切分: {args:?}"
        );
    }

    /// 目标 exe 本身就是可执行文件，不能又当作第一个参数传一遍
    /// （旧实现里 nmap 会收到 `nmap.exe -sV`，argv[1] 是自己的路径）
    #[test]
    fn terminal_exe_does_not_pass_target_twice() {
        let mut t = tool(ToolKind::TerminalExe, r"C:\tools\nmap.exe");
        t.args = "-sV".into();
        assert_eq!(command_arguments(&t), vec!["-sV"]);
        let line = terminal_cmd_line(
            r"C:\tools\nmap.exe",
            &command_arguments(&t),
            None,
            Path::new(r"C:\tools"),
        );
        assert_eq!(
            line,
            r#"cd /d "C:\tools" && "C:\tools\nmap.exe" "-sV""#,
            "命令行里 nmap.exe 只应出现一次"
        );
    }

    /// 每个 token 都被双引号包住：cmd 的分隔符解析在引号内不生效，
    /// 所以 & | > < ^ ; 只会成为参数内容。这里断言的是**交给 cmd 的字符串**，
    /// 只断言 argv 数组测不到这一层（旧实现就是在这里漏的）
    #[test]
    fn terminal_cmd_line_wraps_every_token_in_quotes() {
        let args: Vec<String> = vec!["-sV".into(), "&".into(), "calc.exe".into()];
        let line = terminal_cmd_line(
            r"C:\Program Files\Tools\nmap.exe",
            &args,
            None,
            Path::new(r"C:\tools"),
        );
        assert_eq!(
            line,
            r#"cd /d "C:\tools" && "C:\Program Files\Tools\nmap.exe" "-sV" "&" "calc.exe""#
        );
        // 参数各自成对引号包裹，且都排在 `&&` 之后（不再有任何裸的 cmd 分隔符）
        let tail = line.split_once("&& ").unwrap().1;
        assert_eq!(tail, r#""C:\Program Files\Tools\nmap.exe" "-sV" "&" "calc.exe""#);
        assert_eq!(tail.matches('"').count(), 8, "四个 token 各一对引号: {tail}");
        assert!(tail.contains("\"&\""), "与号必须在引号内: {tail}");
    }

    /// PATH 注入与切目录照常保留（这是这一路必须经过 cmd 的原因）
    #[test]
    fn terminal_cmd_line_keeps_path_injection_and_workdir() {
        let args = command_arguments(&{
            let mut t = tool(ToolKind::TerminalJava, r"C:\tools\app.jar");
            t.args = "--port 8080".into();
            t
        });
        let line = terminal_cmd_line(
            r"C:\jdk\bin\java.exe",
            &args,
            Some(Path::new(r"C:\jdk\bin")),
            Path::new(r"C:\tools"),
        );
        assert_eq!(
            line,
            r#"set "PATH=C:\jdk\bin;%PATH%" && cd /d "C:\tools" && "C:\jdk\bin\java.exe" "-jar" "C:\tools\app.jar" "--port" "8080""#
        );
    }

    /// 带空格的参数必须保持为一个整体
    #[test]
    fn command_arguments_preserve_spaced_option_values() {
        let mut t = tool(ToolKind::TerminalExe, r"C:\tools\t.exe");
        t.args = "--opt=\"a b\" \"c\\d e\"".into();
        let args = command_arguments(&t);
        assert_eq!(args, vec!["--opt=a b", r"c\d e"]);
    }

    /// Java 的 -jar 由后端生成 argv，用户写在 args 里的重复 -jar 要丢弃；
    /// JVM 参数必须保留并排在 -jar 之前（终端与 GUI 两类走同一套顺序）
    #[test]
    fn command_arguments_build_java_jar_argv() {
        let mut t = tool(ToolKind::TerminalJava, r"C:\app.jar");
        t.args = "-Xmx512m -jar other.jar --port 8080".into();
        assert_eq!(
            command_arguments(&t),
            vec!["-Xmx512m", "-jar", r"C:\app.jar", "--port", "8080"]
        );

        let mut g = tool(ToolKind::GuiJava, r"C:\app.jar");
        g.args = "-Xmx512m -jar other.jar --port 8080".into();
        let (jvm, rest) = split_java_args(&g.args);
        let mut gui = jvm;
        gui.push("-jar".into());
        gui.push(g.target.clone());
        gui.extend(rest);
        assert_eq!(gui, command_arguments(&t), "终端与 GUI 的 Java 参数顺序要一致");
    }

    /// 提权路径靠 quote_windows_arg 逐参数引用，规则要与 CommandLineToArgvW 一致
    #[test]
    fn quote_windows_arg_matches_argv_rules() {
        assert_eq!(quote_windows_arg("plain"), "plain");
        assert_eq!(quote_windows_arg(""), "\"\"");
        assert_eq!(quote_windows_arg("a b"), "\"a b\"");
        assert_eq!(quote_windows_arg("say \"hi\""), "\"say \\\"hi\\\"\"");
        // 结尾反斜杠在引号内要翻倍，否则会转义掉收尾引号
        assert_eq!(quote_windows_arg("a b\\"), "\"a b\\\\\"");
    }

    /// cmd 那一层还要额外强制包引号，否则裸的 & 会被当成分隔符
    #[test]
    fn quote_windows_arg_forced_always_quotes() {
        assert_eq!(quote_windows_arg_forced("plain"), "\"plain\"");
        assert_eq!(quote_windows_arg_forced("&"), "\"&\"");
        assert_eq!(quote_windows_arg_forced(""), "\"\"");
        // --out=C:\x\ 结尾的反斜杠同样要翻倍，否则子进程会吞掉收尾引号并串到下一个参数
        assert_eq!(quote_windows_arg_forced("a b\\"), "\"a b\\\\\"");
    }

    /// 终端参数里 cmd 压不住的字符必须拒绝：单引号可以放行（O'Brien 很常见）
    #[test]
    fn check_cmd_args_rejects_what_quoting_cannot_express() {
        let ok = |s: &[&str]| -> bool {
            check_cmd_args("启动参数", &s.iter().map(|x| x.to_string()).collect::<Vec<_>>()).is_ok()
        };
        assert!(ok(&["-sV", "--opt=a b", "https://x?a=1&b=2", "--name=O'Brien"]));
        assert!(!ok(&["%PATH%"]), "% 会被 cmd 展开后再解析一次");
        assert!(!ok(&["a\nb"]), "换行是语句分隔符");
        assert!(!ok(&["say \"hi\""]), "内层双引号会破坏参数边界");
    }

    /// 双引号不成对时 split_args 会把后面的内容并进同一个参数，必须提示而不是静默错切
    #[test]
    fn check_tool_rejects_unbalanced_quotes_in_args() {
        let cfg = Config::default();
        let mut t = tool(ToolKind::GuiExe, "C:\\x\\a.exe");
        t.args = "--msg=\"unclosed".into();
        assert!(check(&t, &cfg).unwrap_err().contains("双引号没有配对"));
        // 配对时不该被这道检查拦住（缺文件是另一回事）
        t.args = "--msg=\"ok\"".into();
        assert!(!check(&t, &cfg).unwrap_err().contains("双引号"));
    }

    /// 会进入命令行的路径字段：引号、换行与 % 必须拒绝（导入的配置可以塞任意字符串）
    #[test]
    fn env_path_rejects_quotes_line_breaks_and_percent() {
        assert!(check_env_path("环境路径", r"C:\jdk").is_ok());
        assert!(check_env_path("环境路径", "").is_err());
        assert!(check_env_path("环境路径", "C:\\a\\\"b").is_err());
        assert!(check_env_path("环境路径", "C:\\a'b").is_err());
        assert!(check_env_path("环境路径", "C:\\a\nb").is_err(), "换行必须被拒绝");
        assert!(
            check_env_path("环境路径", "C:\\%JDK_HOME%\\bin").is_err(),
            "% 会被 cmd 展开，必须被拒绝"
        );
    }

    /// Java / Python 类工具必须绑定运行环境：未绑定直接拒绝，不再回退系统 PATH
    #[test]
    fn unbound_env_is_rejected() {
        let cfg = Config::default();
        // 目标文件必须真实存在，否则会先报「JAR 不存在」而走不到环境检查
        let dir = std::env::temp_dir().join(format!("secaxis-argv-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let jar = dir.join("app.jar");
        std::fs::write(&jar, b"placeholder").unwrap();
        let target = jar.display().to_string();

        let t = tool(ToolKind::GuiJava, &target);
        let err = check(&t, &cfg).unwrap_err();
        assert!(err.contains("未绑定"), "未绑定环境必须被拒绝: {err}");

        // 已绑定环境（即便指向的 JDK 并不存在）报的是「Java 不存在」，而不是未绑定
        let mut cfg2 = cfg.clone();
        cfg2.envs.push(crate::config::Env {
            id: "e1".into(),
            kind: EnvKind::Java,
            name: "JDK".into(),
            path: r"C:\definitely-not-a-jdk".into(),
        });
        let mut t2 = tool(ToolKind::GuiJava, &target);
        t2.env_id = Some("e1".into());
        let err = check(&t2, &cfg2).unwrap_err();
        assert!(!err.contains("未绑定"), "{err}");
        assert!(err.contains("Java 不存在"), "{err}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}