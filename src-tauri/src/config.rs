use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowBounds {
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub maximized: bool,
}

fn default_theme() -> String {
    "light".into()
}
fn default_close_action() -> String {
    "exit".into()
}
fn default_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_theme")]
    pub theme: String, // light | dark
    #[serde(default = "default_close_action")]
    pub close_action: String, // exit | tray
}

impl Settings {
    /// 把未知/非法取值收敛为默认值，避免前端拿到空标签或非法状态。
    /// 注意：窗口位置已独立存于 window.json，不再属于 config。
    pub fn sanitize(&mut self) {
        if !["light", "dark"].contains(&self.theme.as_str()) {
            self.theme = default_theme();
        }
        if !["exit", "tray"].contains(&self.close_action.as_str()) {
            self.close_action = default_close_action();
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            close_action: default_close_action(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    #[default]
    TerminalPython,
    TerminalJava,
    TerminalExe,
    GuiJava,
    GuiExe,
    Web,
    /// 未知类型（更高版本写入 / 手工改错）：解析时容错，
    /// 避免单个非法值导致整份 config.json 被判为损坏而重置
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvKind {
    #[default]
    Java,
    Python,
    /// 未知类型：同上，容错解析
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Env {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub kind: EnvKind,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub tool_type: ToolKind,
    #[serde(default)]
    pub target: String, // .py / .jar / .exe / URL
    #[serde(default)]
    pub args: String,
    #[serde(default)]
    pub env_id: Option<String>,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub icon: Option<String>, // data/icons 下的文件名
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub sort: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub envs: Vec<Env>,
    #[serde(default)]
    pub groups: Vec<Group>,
    #[serde(default)]
    pub tools: Vec<Tool>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: default_version(),
            settings: Settings::default(),
            envs: Vec::new(),
            groups: Vec::new(),
            tools: Vec::new(),
        }
    }
}

impl Config {
    /// 引用收敛：把外部来源（导入的 JSON）里不成立的引用改成「未设置」，并去掉重复/空 id。
    /// 返回人可读的修正说明（可能为空），供界面提示。
    ///
    /// 为什么必须做：
    /// - 悬空的 envId 会让工具处于「看着绑定了某个 JDK，实际无法启动」的状态，
    ///   不如直接清掉绑定，让卡片明确提示需要重新选择环境；
    /// - 重复 id 会让卡片列表出现重复 key，启动时也只命中第一个；
    /// - 悬空的 groupId 会让工具从所有分组视图里消失（只剩「全部」能看到）。
    pub fn normalize(&mut self) -> Vec<String> {
        let mut notes = Vec::new();

        let before = self.envs.len();
        let mut env_ids: HashSet<String> = HashSet::new();
        self.envs
            .retain(|e| !e.id.is_empty() && env_ids.insert(e.id.clone()));
        if self.envs.len() != before {
            notes.push(format!(
                "已忽略 {} 个 id 重复或为空的运行环境",
                before - self.envs.len()
            ));
        }

        let before = self.groups.len();
        let mut group_ids: HashSet<String> = HashSet::new();
        self.groups
            .retain(|g| !g.id.is_empty() && group_ids.insert(g.id.clone()));
        if self.groups.len() != before {
            notes.push(format!(
                "已忽略 {} 个 id 重复或为空的分组",
                before - self.groups.len()
            ));
        }

        let before = self.tools.len();
        let mut tool_ids: HashSet<String> = HashSet::new();
        self.tools
            .retain(|t| !t.id.is_empty() && tool_ids.insert(t.id.clone()));
        if self.tools.len() != before {
            notes.push(format!(
                "已忽略 {} 个 id 重复或为空的工具",
                before - self.tools.len()
            ));
        }

        let mut dangling_env = 0usize;
        let mut dangling_group = 0usize;
        for t in &mut self.tools {
            if let Some(id) = t.env_id.as_deref() {
                if !env_ids.contains(id) {
                    t.env_id = None;
                    dangling_env += 1;
                }
            }
            if let Some(id) = t.group_id.as_deref() {
                if !group_ids.contains(id) {
                    t.group_id = None;
                    dangling_group += 1;
                }
            }
        }
        if dangling_env > 0 {
            notes.push(format!(
                "{dangling_env} 个工具绑定的运行环境不存在，已清除绑定（这些工具需重新选择环境）"
            ));
        }
        if dangling_group > 0 {
            notes.push(format!(
                "{dangling_group} 个工具所属的分组不存在，已移入「未分组」"
            ));
        }
        notes
    }
}

pub struct AppState {
    pub data_dir: PathBuf,
    pub config: Mutex<Config>,
}

impl AppState {
    /// 取配置锁；即使锁中毒也继续使用内部数据，避免直接 panic 崩掉应用
    pub fn config(&self) -> std::sync::MutexGuard<'_, Config> {
        self.config.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub fn config_path(data_dir: &Path) -> PathBuf {
    data_dir.join("config.json")
}

pub fn icons_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("icons")
}

pub fn window_path(data_dir: &Path) -> PathBuf {
    data_dir.join("window.json")
}

/// 窗口位置/尺寸独立存储：避免前端整体保存 config 时，
/// 把后端在关闭/隐藏时刚写入的窗口状态覆盖回旧值
pub fn load_window(data_dir: &Path) -> Option<WindowBounds> {
    let text = fs::read_to_string(window_path(data_dir)).ok()?;
    serde_json::from_str(&text).ok()
}

/// 滚动备份：save() 在覆盖写入前，把当前这份「已知可用」的配置复制到这里。
/// 主文件损坏时 load_or_default 会尝试用它兜回来。
pub fn backup_path(data_dir: &Path) -> PathBuf {
    data_dir.join("config.json.bak")
}

/// 临时文件路径：config.json -> config.json.tmp
fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".tmp");
    path.with_file_name(name)
}

/// 原子写：临时文件 → write → sync_all → rename 覆盖。
///
/// 相比 fs::write，多了 sync_all：配置是全部工具的唯一真源，
/// 断电/蓝屏时"元数据已落盘、数据还在缓存里"会留下 0 字节或半截 JSON。
fn write_atomic(path: &Path, bytes: &[u8], what: &str) -> Result<(), String> {
    use std::io::Write;
    let tmp = tmp_path(path);
    let mut file = fs::File::create(&tmp).map_err(|e| format!("写入{what}失败: {e}"))?;
    file.write_all(bytes)
        .map_err(|e| format!("写入{what}失败: {e}"))?;
    file.sync_all()
        .map_err(|e| format!("写入{what}失败: {e}"))?;
    drop(file);
    fs::rename(&tmp, path).map_err(|e| format!("保存{what}失败: {e}"))
}

pub fn save_window(data_dir: &Path, bounds: &WindowBounds) -> Result<(), String> {
    fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(bounds).map_err(|e| e.to_string())?;
    write_atomic(&window_path(data_dir), json.as_bytes(), "窗口状态")
}

/// 读取滚动备份；不存在或解析失败时返回 None
fn read_backup(data_dir: &Path) -> Option<Config> {
    let text = fs::read_to_string(backup_path(data_dir)).ok()?;
    serde_json::from_str::<Config>(&text).ok()
}

fn ensure_writable(dir: &Path) -> bool {
    if fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".write_probe");
    if fs::write(&probe, b"ok").is_ok() {
        let _ = fs::remove_file(&probe);
        true
    } else {
        false
    }
}

/// 便携优先：配置放在 exe 同目录 data\ 下；不可写时回退 %APPDATA%\SecAxis\data
pub fn resolve_data_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let data = dir.join("data");
            if ensure_writable(&data) {
                return data;
            }
        }
    }
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    let data = base.join("SecAxis").join("data");
    let _ = fs::create_dir_all(&data);
    data
}

pub fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 读取配置；主文件损坏时先用滚动备份兜回来，都不可用则回退默认配置。
pub fn load_or_default(data_dir: &Path) -> Config {
    let path = config_path(data_dir);
    match fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<Config>(&text) {
            Ok(mut c) => {
                c.settings.sanitize();
                c
            }
            Err(e) => {
                // 坏文件留档（.bak-<时间戳>），便于事后人工抢救
                let bak = data_dir.join(format!("config.json.bak-{}", unix_secs()));
                let _ = fs::rename(&path, &bak);
                // 优先用滚动备份兜回来：用户看到的仍然是自己的工具列表
                if let Some(mut restored) = read_backup(data_dir) {
                    restored.settings.sanitize();
                    // 立刻写回主文件，避免"这次能用、下次启动又是空的"
                    let _ = save(data_dir, &restored);
                    crate::logging::error(
                        data_dir,
                        &format!(
                            "配置解析失败（{e}），已从 config.json.bak 恢复；损坏文件备份至 {}",
                            bak.display()
                        ),
                    );
                    return restored;
                }
                crate::logging::error(
                    data_dir,
                    &format!(
                        "配置解析失败（{e}），已备份至 {} 并以默认配置启动",
                        bak.display()
                    ),
                );
                Config::default()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // 全新安装（或用户手工删掉了配置）：不是故障，不打 ERROR
            Config::default()
        }
        Err(e) => {
            // 读取失败（权限、磁盘错误等）以前是静默吞掉的，补一条 ERROR
            crate::logging::error(data_dir, &format!("读取配置失败（{e}），以默认配置启动"));
            Config::default()
        }
    }
}

pub fn save(data_dir: &Path, config: &Config) -> Result<(), String> {
    fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let path = config_path(data_dir);
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    // 覆盖之前先把当前这份（能正常读出来就说明是好的）滚动备份，
    // 这样即使下一次写入后文件损坏，也还有一份可用配置
    if path.is_file() {
        let _ = fs::copy(&path, backup_path(data_dir));
    }
    write_atomic(&path, json.as_bytes(), "配置")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 未知枚举值只应降级为 Unknown，不能让整份配置解析失败（否则用户工具列表会被重置）
    #[test]
    fn unknown_enum_values_are_tolerated() {
        // terminal 是旧版字段，现在应作为未知字段被静默忽略
        let json = r#"{
            "version": 1,
            "settings": { "terminal": "wt", "theme": "rainbow", "closeAction": "explode" },
            "envs": [ { "id": "e1", "kind": "ruby", "name": "R", "path": "C:\\r" } ],
            "groups": [ { "id": "g1", "name": "G" } ],
            "tools": [ { "id": "t1", "name": "T", "type": "future_kind", "target": "C:\\x" } ]
        }"#;
        let mut cfg: Config = serde_json::from_str(json).expect("未知值不应导致解析失败");
        assert_eq!(cfg.tools[0].tool_type, ToolKind::Unknown);
        assert_eq!(cfg.envs[0].kind, EnvKind::Unknown);
        cfg.settings.sanitize();
        assert_eq!(cfg.settings.theme, "light");
        assert_eq!(cfg.settings.close_action, "exit");
    }

    #[test]
    fn valid_settings_are_kept() {
        let mut s = Settings {
            theme: "dark".into(),
            close_action: "tray".into(),
        };
        s.sanitize();
        assert_eq!(s.theme, "dark");
        assert_eq!(s.close_action, "tray");
    }

    #[test]
    fn config_round_trip() {
        let cfg = Config::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(back.version, cfg.version);
        assert_eq!(back.settings.theme, cfg.settings.theme);
        assert!(back.tools.is_empty());
    }

    // ---- 导入配置的引用收敛 ----

    fn config_from(json: &str) -> Config {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn normalize_drops_dangling_refs_and_duplicates() {
        let mut cfg = config_from(
            r#"{
                "envs": [
                    { "id": "e1", "kind": "java", "name": "JDK17", "path": "C:\\jdk" },
                    { "id": "e1", "kind": "java", "name": "重复", "path": "C:\\x" }
                ],
                "groups": [ { "id": "g1", "name": "G" } ],
                "tools": [
                    { "id": "t1", "name": "A", "type": "gui_java", "target": "C:\\a.jar", "envId": "e1", "groupId": "g1" },
                    { "id": "t2", "name": "B", "type": "terminal_java", "target": "C:\\b.jar", "envId": "missing", "groupId": "nope" },
                    { "id": "t1", "name": "重复 id", "type": "web", "target": "https://x" }
                ]
            }"#,
        );

        let notes = cfg.normalize();

        assert_eq!(cfg.envs.len(), 1, "重复 id 的环境应被去掉");
        assert_eq!(cfg.tools.len(), 2, "重复 id 的工具应被去掉");
        // 正常引用保持不动
        assert_eq!(cfg.tools[0].env_id.as_deref(), Some("e1"));
        assert_eq!(cfg.tools[0].group_id.as_deref(), Some("g1"));
        // 悬空引用被置空，而不是留着让启动时静默回退
        assert_eq!(cfg.tools[1].env_id, None);
        assert_eq!(cfg.tools[1].group_id, None);
        assert!(
            notes.iter().any(|n| n.contains("运行环境不存在")),
            "应提示环境引用被修正: {notes:?}"
        );
        assert!(
            notes.iter().any(|n| n.contains("分组不存在")),
            "应提示分组引用被修正: {notes:?}"
        );
    }

    #[test]
    fn normalize_is_silent_for_healthy_config() {
        let mut cfg = config_from(
            r#"{
                "envs": [ { "id": "e1", "kind": "python", "name": "Py", "path": "C:\\py" } ],
                "groups": [ { "id": "g1", "name": "G" } ],
                "tools": [ { "id": "t1", "name": "A", "type": "terminal_python", "target": "C:\\a.py", "envId": "e1", "groupId": "g1" } ]
            }"#,
        );
        assert!(cfg.normalize().is_empty(), "合法配置不应产生任何修正说明");
        assert_eq!(cfg.tools[0].env_id.as_deref(), Some("e1"));
    }

    // ---- 配置持久化：原子写 / 滚动备份 / 损坏恢复 ----

    fn temp_data_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "secaxis-cfg-{tag}-{}-{}",
            std::process::id(),
            unix_secs()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn save_is_atomic_and_leaves_no_tmp_file() {
        let dir = temp_data_dir("atomic");
        save(&dir, &Config::default()).unwrap();
        assert!(config_path(&dir).is_file());
        assert!(
            !tmp_path(&config_path(&dir)).exists(),
            "临时文件必须已被 rename"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// 覆盖写入前要把上一份「已知可用」配置滚动到 config.json.bak
    #[test]
    fn save_keeps_previous_config_as_backup() {
        let dir = temp_data_dir("backup");
        let mut first = Config::default();
        first.settings.theme = "light".into();
        save(&dir, &first).unwrap();
        // 第一次保存时主文件还不存在，不应产生备份
        assert!(!backup_path(&dir).exists());

        let mut second = Config::default();
        second.settings.theme = "dark".into();
        save(&dir, &second).unwrap();

        let backup: Config = serde_json::from_str(&fs::read_to_string(backup_path(&dir)).unwrap())
            .expect("备份必须是可解析的配置");
        assert_eq!(backup.settings.theme, "light", "备份应是上一份配置");
        let current: Config =
            serde_json::from_str(&fs::read_to_string(config_path(&dir)).unwrap()).unwrap();
        assert_eq!(current.settings.theme, "dark");
        let _ = fs::remove_dir_all(&dir);
    }

    /// 主文件损坏时应从滚动备份恢复
    #[test]
    fn corrupt_config_is_restored_from_backup() {
        let dir = temp_data_dir("restore");
        let mut good = Config::default();
        good.settings.theme = "dark".into();
        save(&dir, &good).unwrap();
        // 需要一个「上一份」才有备份：再存一次同内容也会刷新备份
        save(&dir, &good).unwrap();
        assert!(backup_path(&dir).is_file());

        fs::write(config_path(&dir), b"{ this is not json").unwrap();

        let loaded = load_or_default(&dir);
        assert_eq!(
            loaded.settings.theme, "dark",
            "应从备份恢复出用户的真实配置"
        );
        // 恢复结果要写回主文件，避免下次启动又变回空配置
        let current: Config =
            serde_json::from_str(&fs::read_to_string(config_path(&dir)).unwrap()).unwrap();
        assert_eq!(current.settings.theme, "dark");
        let baks: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("config.json.bak-")
            })
            .collect();
        assert_eq!(baks.len(), 1, "坏文件应留档一份");
        let _ = fs::remove_dir_all(&dir);
    }
}
