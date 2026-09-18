mod commands;
mod config;
mod icon;
mod launcher;
mod logging;
mod shellex;
mod tray;

use std::sync::Mutex;

use tauri::{Manager, WindowEvent};

use config::AppState;

/// 把主窗口尺寸/最大化状态持久化到 data\window.json
/// （启动时一律屏幕居中，不恢复上次坐标）
fn persist_window_bounds(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let state = app.state::<AppState>();
    let maximized = window.is_maximized().unwrap_or(false);
    let bounds = if maximized {
        // 最大化时保留上次的窗口尺寸，只记标记
        let mut b = config::load_window(&state.data_dir).unwrap_or(config::WindowBounds {
            width: 1100.0,
            height: 720.0,
            maximized: true,
        });
        b.maximized = true;
        Some(b)
    } else {
        match window.inner_size() {
            Ok(s) => Some(config::WindowBounds {
                width: s.width as f64,
                height: s.height as f64,
                maximized: false,
            }),
            _ => None,
        }
    };
    if let Some(b) = bounds {
        let _ = config::save_window(&state.data_dir, &b);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_dir = config::resolve_data_dir();
    // 便携目录不可写时会回退 %APPDATA%，把实际选中的位置记下来，
    // 便于排查「把 exe 换到只读目录后配置像是丢了」
    logging::info(
        &data_dir,
        &format!(
            "启动 SecAxis v{}（数据目录 {}）",
            env!("CARGO_PKG_VERSION"),
            data_dir.display()
        ),
    );
    let cfg = config::load_or_default(&data_dir);
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            data_dir: data_dir.clone(),
            config: Mutex::new(cfg.clone()),
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::save_config,
            commands::check_tools_status,
            commands::launch_tool,
            commands::open_tool_dir,
            commands::probe_env,
            commands::extract_icon,
            commands::get_icons,
            commands::export_config,
            commands::import_config,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            // 托盘只是锦上添花：构建失败（例如拿不到默认窗口图标）不该阻止应用启动
            if let Err(e) = tray::rebuild(&handle, &cfg) {
                let dir = handle.state::<AppState>().data_dir.clone();
                logging::error(&dir, &format!("构建托盘失败（应用继续启动）: {e}"));
            }

            if let Some(window) = handle.get_webview_window("main") {
                // 启动时：尺寸沿用上次记录，位置一律屏幕居中（不再恢复上次坐标）。
                // 窗口在配置里设为 visible:false，等尺寸/位置都算好后再显示，
                // 否则会先以默认尺寸居中显示一次、再套用记忆尺寸重排，出现肉眼可见的跳动。
                let data_dir = handle.state::<AppState>().data_dir.clone();
                match config::load_window(&data_dir) {
                    Some(b) if b.maximized => {
                        let _ = window.maximize();
                    }
                    Some(b) => {
                        if b.width >= 200.0 && b.height >= 200.0 {
                            let _ = window.set_size(tauri::PhysicalSize::new(
                                b.width as u32,
                                b.height as u32,
                            ));
                        }
                        let _ = window.center();
                    }
                    None => {
                        let _ = window.center();
                    }
                }
                let _ = window.show();

                let h = handle.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        let app = h.clone();
                        let close_to_tray = {
                            let state = app.state::<AppState>();
                            let guard = state.config();
                            guard.settings.close_action == "tray"
                        };
                        if close_to_tray {
                            api.prevent_close();
                            // 只是隐藏窗口，进程还活着：fsync 落盘不必卡住关窗，丢后台线程即可
                            let bg = app.clone();
                            std::thread::spawn(move || persist_window_bounds(&bg));
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.hide();
                            }
                        } else {
                            // 即将退出进程，必须同步写完，否则记录丢失
                            persist_window_bounds(&app);
                        }
                    }
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
