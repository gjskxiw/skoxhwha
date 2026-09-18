use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use windows_sys::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS,
};
use windows_sys::Win32::UI::Shell::ExtractIconExW;
use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 从 exe 提取大图标并保存为 PNG，返回文件名（保存在 icons_dir 下）
pub fn extract_exe_icon(exe_path: &str, icons_dir: &Path) -> Result<String, String> {
    let mut hasher = DefaultHasher::new();
    exe_path.hash(&mut hasher);
    let name = format!("exe-{:016x}.png", hasher.finish());
    unsafe { extract_impl(exe_path, &icons_dir.join(&name))? };
    Ok(name)
}

unsafe fn extract_impl(exe_path: &str, out_path: &Path) -> Result<(), String> {
    let wide = to_wide(exe_path);
    let mut large: HICON = std::ptr::null_mut();
    let mut small: HICON = std::ptr::null_mut();
    let n = ExtractIconExW(wide.as_ptr(), 0, &mut large, &mut small, 1);
    if n == 0 || (large.is_null() && small.is_null()) {
        return Err(format!("无法从该 exe 提取图标: {exe_path}"));
    }
    let hicon = if !large.is_null() { large } else { small };

    let mut info: ICONINFO = std::mem::zeroed();
    if GetIconInfo(hicon, &mut info) == 0 {
        // ExtractIconExW 会同时填充 large 与 small，两个句柄都必须释放
        if !large.is_null() {
            DestroyIcon(large);
        }
        if !small.is_null() {
            DestroyIcon(small);
        }
        return Err("读取图标信息失败".into());
    }

    let hdc = GetDC(std::ptr::null_mut());
    let mut result: Result<(), String> = Err("该 exe 未包含彩色图标".into());

    if !info.hbmColor.is_null() {
        let mut bm: BITMAP = std::mem::zeroed();
        let ok = GetObjectW(
            info.hbmColor,
            std::mem::size_of::<BITMAP>() as i32,
            &mut bm as *mut BITMAP as *mut _,
        );
        if ok != 0 && bm.bmWidth > 0 && bm.bmHeight > 0 {
            let w = bm.bmWidth as usize;
            let h = bm.bmHeight as usize;
            let mut buf = vec![0u8; w * h * 4];
            let mut bi: BITMAPINFO = std::mem::zeroed();
            bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bi.bmiHeader.biWidth = bm.bmWidth;
            bi.bmiHeader.biHeight = -bm.bmHeight; // 自上而下
            bi.bmiHeader.biPlanes = 1;
            bi.bmiHeader.biBitCount = 32;
            bi.bmiHeader.biCompression = BI_RGB;

            let lines = GetDIBits(
                hdc,
                info.hbmColor,
                0,
                bm.bmHeight as u32,
                buf.as_mut_ptr() as *mut _,
                &mut bi,
                DIB_RGB_COLORS,
            );
            if lines == bm.bmHeight {
                for px in buf.as_chunks_mut::<4>().0 {
                    px.swap(0, 2); // BGRA -> RGBA
                }
                // 无 alpha 通道的图标整体置为不透明
                let has_alpha = buf.iter().skip(3).step_by(4).any(|&a| a != 0);
                if !has_alpha {
                    for px in buf.as_chunks_mut::<4>().0 {
                        px[3] = 255;
                    }
                }
                if let Some(img) = image::RgbaImage::from_raw(w as u32, h as u32, buf) {
                    let save_result = (|| -> Result<(), String> {
                        if let Some(parent) = out_path.parent() {
                            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                        }
                        img.save(out_path).map_err(|e| e.to_string())
                    })();
                    match save_result {
                        Ok(()) => result = Ok(()),
                        Err(e) => result = Err(format!("保存图标失败: {e}")),
                    }
                }
            }
        }
    }

    if !info.hbmColor.is_null() {
        DeleteObject(info.hbmColor);
    }
    if !info.hbmMask.is_null() {
        DeleteObject(info.hbmMask);
    }
    if !large.is_null() {
        DestroyIcon(large);
    }
    if !small.is_null() {
        DestroyIcon(small);
    }
    ReleaseDC(std::ptr::null_mut(), hdc);
    result
}
