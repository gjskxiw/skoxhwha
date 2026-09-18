use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::config::Config;
use crate::launcher;

pub const TRAY_ID: &str = "secaxis-tray";

/// 按配置重建托盘菜单（分组 -> 工具），保存配置后调用
pub fn rebuild(app: &AppHandle, config: &Config) -> tauri::Result<()> {
    let _ = app.remove_tray_by_id(TRAY_ID);

    let show = MenuItemBuilder::with_id("show", "显示主窗口").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "退出").build(app)?;
    let mut menu = MenuBuilder::new(app).item(&show);

    for g in &config.groups {
        let mut tools: Vec<_> = config
            .tools
            .iter()
            .filter(|t| t.group_id.as_deref() == Some(g.id.as_str()))
            .collect();
        if tools.is_empty() {
            continue;
        }
        tools.sort_by_key(|t| t.sort);
        let mut sub = SubmenuBuilder::new(app, &g.name);
        for t in tools {
            sub = sub.item(&MenuItemBuilder::with_id(format!("t:{}", t.id), &t.name).build(app)?);
        }
        menu = menu.item(&sub.build()?);
    }

    let ungrouped: Vec<_> = config
        .tools
        .iter()
        .filter(|t| t.group_id.as_deref().unwrap_or("").is_empty())
        .collect();
    if !ungrouped.is_empty() {
        let mut sub = SubmenuBuilder::new(app, "未分组");
        for t in ungrouped {
            sub = sub.item(&MenuItemBuilder::with_id(format!("t:{}", t.id), &t.name).build(app)?);
        }
        menu = menu.item(&sub.build()?);
    }

    let sep = PredefinedMenuItem::separator(app)?;
    let menu = menu.item(&sep).item(&quit).build()?;

    let _tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(
            app.default_window_icon()
                .cloned()
                .ok_or_else(|| tauri::Error::AssetNotFound("app icon".into()))?,
        )
        .tooltip("SecAxis")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "show" => show_main(app),
                "quit" => {
                    crate::persist_window_bounds(app);
                    app.exit(0);
                }
                _ => {
                    if let Some(tid) = id.strip_prefix("t:") {
                        // 提权启动可能阻塞到用户响应 UAC，放到后台线程，避免卡住托盘/主线程
                        let app = app.clone();
                        let tid = tid.to_string();
                        std::thread::spawn(move || {
                            if let Err(e) = launch_by_id(&app, &tid) {
                                let dir = app.state::<crate::config::AppState>().data_dir.clone();
                                crate::logging::error(&dir, &format!("托盘启动失败: {e}"));
                            }
                        });
                    }
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn launch_by_id(app: &AppHandle, id: &str) -> Result<(), String> {
    use tauri::Manager;
    let state = app.state::<crate::config::AppState>();
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
    // 托盘菜单只做普通启动；以管理员身份运行由卡片右键菜单发起
    launcher::launch(app, &tool, &config, false)
}
