use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::config::{self, AppState, Config, Env, EnvKind};
use crate::icon;
use crate::launcher;
use crate::logging;
use crate::tray;

/// 导入结果：收敛了引用之后返回，让界面能提示「哪些引用被修正了」
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub config: Config,
    /// 引用收敛时做的修正说明（可能为空）
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub ok: bool,
    /// 阻断启动的原因（卡片标红）
    pub missing: String,
}

/// 保存前探测环境的结果
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvProbe {
    /// 建议的环境名（由版本信息推导，如 "JDK 17.0.9" / "Python 3.12.4"）
    pub name: String,
}

/// 托盘菜单只依赖分组与工具的 id/名称/分组归属，据此生成指纹避免无谓重建。
/// 工具按数组顺序（即插入序）遍历：没有排序字段，顺序变化只会来自增删。
fn tray_signature(cfg: &Config) -> String {
    let mut sig = String::new();
    for g in &cfg.groups {
        sig.push_str(&format!("g:{}|{};", g.id, g.name));
    }
    for t in &cfg.tools {
        sig.push_str(&format!(
            "t:{}|{}|{};",
            t.id,
            t.name,
            t.group_id.as_deref().unwrap_or("")
        ));
    }
    sig
}

/// 取配置（仅启动时调用一次）。
#[tauri::command]
pub fn get_state(state: State<'_, AppState>) -> Config {
    state.config().clone()
}

/// 界面已不弹通知，命令层失败只有 app.log 一个出口 —— 先落盘再把错误抛回前端。
fn log_err<T>(data_dir: &Path, res: Result<T, String>) -> Result<T, String> {
    if let Err(e) = &res {
        logging::error(data_dir, e);
    }
    res
}

#[tauri::command]
pub async fn save_config(
    app: AppHandle,
    state: State<'_, AppState>,
    mut config: Config,
) -> Result<Config, String> {
    config.settings.sanitize();
    let data_dir = state.data_dir.clone();
    let previous = tray_signature(&state.config());
    // 仅当托盘菜单内容（分组/工具）变化时才重建，避免每次保存都闪烁
    let tray_changed = tray_signature(&config) != previous;

    // 写盘（含 sync_all 落盘）放阻塞线程池，不占主线程
    let dir = data_dir.clone();
    let to_save = config.clone();
    let saved = tauri::async_runtime::spawn_blocking(move || config::save(&dir, &to_save))
        .await
        .map_err(|e| format!("保存配置失败: {e}"))
        .flatten();
    log_err(&data_dir, saved)?;

    *state.config() = config.clone();

    // 托盘/菜单是原生资源，统一回主线程构建（与启动时的路径一致）
    if tray_changed {
        let app_inner = app.clone();
        let cfg = config.clone();
        let dir = data_dir.clone();
        if let Err(e) = app.run_on_main_thread(move || {
            if let Err(e) = tray::rebuild(&app_inner, &cfg) {
                logging::error(&dir, &format!("重建托盘菜单失败: {e}"));
            }
        }) {
            logging::error(&data_dir, &format!("调度托盘重建失败: {e}"));
        }
    }
    Ok(config)
}

#[tauri::command]
pub async fn check_tools_status(
    state: State<'_, AppState>,
) -> Result<HashMap<String, CheckResult>, String> {
    let cfg = state.config().clone();
    // 逐个探测文件存在性，放到阻塞线程池，避免占用 UI/IPC 线程
    tauri::async_runtime::spawn_blocking(move || {
        cfg.tools
            .iter()
            .map(|t| match launcher::check_tool(t, &cfg) {
                Ok(()) => (
                    t.id.clone(),
                    CheckResult {
                        ok: true,
                        missing: String::new(),
                    },
                ),
                Err(missing) => (
                    t.id.clone(),
                    CheckResult { ok: false, missing },
                ),
            })
            .collect::<HashMap<_, _>>()
    })
    .await
    .map_err(|e| format!("状态检查失败: {e}"))
}

#[tauri::command]
pub async fn launch_tool(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    as_admin: bool,
) -> Result<(), String> {
    let (tool, config) = {
        let cfg = state.config();
        let tool = cfg
            .tools
            .iter()
            .find(|t| t.id == id)
            .cloned()
            .ok_or_else(|| "工具不存在".to_string())?;
        (tool, cfg.clone())
    };
    // 提权启动会阻塞到用户响应 UAC，放阻塞线程池，避免界面假死
    let handle = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        launcher::launch(&handle, &tool, &config, as_admin)
    })
    .await
    .map_err(|e| format!("启动失败: {e}"))?;
    if let Err(e) = &result {
        logging::error(&state.data_dir, &format!("启动工具失败: {e}"));
    }
    result
}

/// 在资源管理器里打开工具的工作目录（= 目标文件所在目录）。
/// 路径由后端解析，前端只传工具 id；Rust 侧调用 opener 不经过前端 capability。
#[tauri::command]
pub async fn open_tool_dir(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let dir = {
        let cfg = state.config();
        let tool = cfg
            .tools
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| "工具不存在".to_string())?;
        launcher::workdir_of(tool)
    };
    // ShellExecute 拉起资源管理器（冷启动可能上百毫秒），放阻塞线程池
    let data_dir = state.data_dir.clone();
    let opened = tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_opener::OpenerExt;
        if !dir.is_dir() {
            return Err(format!("打开工作目录失败：目录不存在: {}", dir.display()));
        }
        app.opener()
            .open_path(dir.display().to_string(), None::<&str>)
            .map_err(|e| format!("打开目录失败: {e}"))
    })
    .await
    .map_err(|e| format!("打开目录失败: {e}"))
    .flatten();
    log_err(&data_dir, opened)
}

/// 保存前探测：把「目录」当成环境跑一次 `java -version` / `python --version`。
/// 不要求环境已经存在于配置里，这样「添加」可以直接选目录、由版本信息自动命名。
/// 环境入库前必经此步，所以列表里不再需要单独的「验证」入口。
///
/// 失败原因先落 app.log 再抛回前端：界面只展示归类后的人话，
/// 原始输出（含版本串、退出码、OS 报错）留给排查时看。
#[tauri::command]
pub async fn probe_env(
    state: State<'_, AppState>,
    kind: EnvKind,
    path: String,
) -> Result<EnvProbe, String> {
    let data_dir = state.data_dir.clone();
    let probed = tauri::async_runtime::spawn_blocking(move || {
        let env = Env {
            id: String::new(),
            kind,
            name: String::new(),
            path,
        };
        let detail = launcher::validate_env(&env)?;
        let name = launcher::suggest_env_name(kind, &detail)
            .ok_or_else(|| format!("无法从版本信息中识别环境名称：{detail}"))?;
        // 原始版本输出只用于推导名称与失败消息，不再往前端传一份
        Ok(EnvProbe { name })
    })
    .await
    .map_err(|e| format!("探测失败: {e}"))
    .flatten();
    log_err(&data_dir, probed)
}

#[tauri::command]
pub async fn extract_icon(state: State<'_, AppState>, exe_path: String) -> Result<String, String> {
    if exe_path.trim().is_empty() {
        return Err("请先选择 EXE 文件".into());
    }
    let icons_dir = config::icons_dir(&state.data_dir);
    // GDI 取图 + PNG 编码 + 落盘，几毫秒到几十毫秒，不该占主线程
    tauri::async_runtime::spawn_blocking(move || icon::extract_exe_icon(&exe_path, &icons_dir))
        .await
        .map_err(|e| format!("提取图标失败: {e}"))?
}

fn read_icon(data_dir: &Path, name: &str) -> Result<String, String> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err("非法图标名".into());
    }
    let path = config::icons_dir(data_dir).join(name);
    let bytes = std::fs::read(&path).map_err(|e| format!("读取图标失败: {e}"))?;
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    // 图标一律由 extract_icon 产出 PNG
    Ok(format!("data:image/png;base64,{b64}"))
}

/// 批量取图标：避免启动时逐个 IPC 往返
#[tauri::command]
pub async fn get_icons(
    state: State<'_, AppState>,
    names: Vec<String>,
) -> Result<HashMap<String, String>, String> {
    let data_dir = state.data_dir.clone();
    // 逐个读盘 + base64 编码（数据量随图标数量线性增长）
    tauri::async_runtime::spawn_blocking(move || {
        names
            .into_iter()
            .filter_map(|name| read_icon(&data_dir, &name).ok().map(|data| (name, data)))
            .collect::<HashMap<_, _>>()
    })
    .await
    .map_err(|e| format!("读取图标失败: {e}"))
}

#[tauri::command]
pub async fn export_config(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let cfg = state.config().clone();
    let data_dir = state.data_dir.clone();
    let target = path.clone();
    let exported = tauri::async_runtime::spawn_blocking(move || {
        let json = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| format!("写入失败: {e}"))
    })
    .await
    .map_err(|e| format!("导出失败: {e}"))
    .flatten()
    .map_err(|e| format!("导出配置失败：{e}（{target}）"));
    log_err(&data_dir, exported)
}

/// 导入配置的大小上限。配置本身只有几十 KB，设上限是为了避免误选一个巨大的
/// 无关文件后被整体读进内存（read_to_string 是全量读取）。
const IMPORT_MAX_BYTES: u64 = 4 * 1024 * 1024;

/// 校验导入文件：必须是存在的普通文件、体积合理、扩展名为 .json。
fn prepare_import(path: &Path) -> Result<(), String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("读取失败: {e}"))?;
    if !meta.is_file() {
        return Err("请选择一个配置文件（不是目录）".into());
    }
    if meta.len() > IMPORT_MAX_BYTES {
        return Err(format!(
            "文件过大（{} 字节），配置文件不应超过 {} 字节",
            meta.len(),
            IMPORT_MAX_BYTES
        ));
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "json" {
        return Err(format!("只接受 .json 配置文件，当前是 .{ext}"));
    }
    Ok(())
}

/// 导入配置。
///
/// 安全约束（导入的 JSON 属于外部输入，且会立刻落盘生效）：
/// - 文件必须是 .json 且体积受限；
/// - 拒绝引号/换行/% 出现在 env.path 这类仍会拼进命令行的字段里；
/// - 引用收敛（归一化 envId / groupId），并逐条回报给界面。
/// 工具参数（args）不在这里拦截：终端类工具的参数由 launcher 逐 token 包引号交给 cmd，
/// 剩下的 % / 引号等由 check_tool 标红提示，导入时不必静默改写用户填的参数。
#[tauri::command]
pub async fn import_config(
    state: State<'_, AppState>,
    path: String,
) -> Result<ImportResult, String> {
    let path_for_log = path.clone();
    let parsed =
        tauri::async_runtime::spawn_blocking(move || -> Result<(Config, Vec<String>), String> {
            prepare_import(Path::new(&path))?;
            let text = std::fs::read_to_string(&path).map_err(|e| format!("读取失败: {e}"))?;
            let mut cfg = serde_json::from_str::<Config>(&text)
                .map_err(|e| format!("配置文件格式无效: {e}"))?;
            cfg.settings.sanitize();

            // 先剔除非法环境路径：这些字段会进入终端命令行，必须 fail-closed。
            // 被剔除的环境同时要从引用它的工具上摘掉，否则会变成"看着绑定了、其实没有"。
            let mut dropped_envs: HashSet<String> = HashSet::new();
            let mut env_notes: Vec<String> = Vec::new();
            cfg.envs.retain(|e| {
                match launcher::check_env_path(&format!("环境「{}」的路径", e.name), &e.path) {
                    Ok(()) => true,
                    Err(why) => {
                        env_notes.push(why);
                        dropped_envs.insert(e.id.clone());
                        false
                    }
                }
            });
            for t in &mut cfg.tools {
                if let Some(id) = t.env_id.as_deref() {
                    if dropped_envs.contains(id) {
                        t.env_id = None;
                    }
                }
            }

            // 别人导出的配置可能引用本机不存在的环境/分组，先收敛再交给前端
            let mut warnings = cfg.normalize();
            if !env_notes.is_empty() {
                warnings.insert(
                    0,
                    format!(
                        "已拒绝 {} 个路径不合法的运行环境（会破坏终端命令行）：{}",
                        env_notes.len(),
                        env_notes.join("；")
                    ),
                );
            }
            Ok((cfg, warnings))
        })
        .await
        .map_err(|e| format!("导入失败: {e}"))
        .flatten()
        .map_err(|e| format!("导入配置失败：{e}（{path_for_log}）"));
    let (cfg, warnings) = log_err(&state.data_dir, parsed)?;
    if warnings.is_empty() {
        logging::info(&state.data_dir, &format!("导入配置：{path_for_log}"));
    } else {
        logging::info(
            &state.data_dir,
            &format!("导入配置：{path_for_log}（{}）", warnings.join("；")),
        );
    }
    Ok(ImportResult {
        config: cfg,
        warnings,
    })
}

