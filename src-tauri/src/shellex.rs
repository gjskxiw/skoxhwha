use std::path::Path;

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 通过 ShellExecuteExW 以管理员身份启动（触发 UAC）
pub fn runas(file: &str, params: &str, dir: Option<&Path>) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    unsafe {
        let verb = to_wide("runas");
        let file_w = to_wide(file);
        let params_w = if params.is_empty() {
            None
        } else {
            Some(to_wide(params))
        };
        let dir_w = dir.map(|d| to_wide(&d.display().to_string()));

        let mut sei: SHELLEXECUTEINFOW = std::mem::zeroed();
        sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        sei.fMask = SEE_MASK_NOASYNC;
        sei.lpVerb = verb.as_ptr();
        sei.lpFile = file_w.as_ptr();
        sei.lpParameters = params_w.as_ref().map_or(std::ptr::null(), |p| p.as_ptr());
        sei.lpDirectory = dir_w.as_ref().map_or(std::ptr::null(), |p| p.as_ptr());
        sei.nShow = SW_SHOWNORMAL;

        if ShellExecuteExW(&mut sei) == 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(1223) {
                return Err("已取消：用户拒绝了管理员授权".into());
            }
            return Err(format!("管理员启动失败: {err}"));
        }
        Ok(())
    }
}
