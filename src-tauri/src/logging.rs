//! 极简文件日志。
//!
//! release 构建带 `windows_subsystem = "windows"`，没有控制台，eprintln! 的输出
//! 无人可见；这里把关键错误/事件写入 data\logs\app.log，便于排查
//! （托盘启动失败、配置损坏、窗口恢复异常等）。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// 单文件上限，超出后轮转为 app.log.1
const MAX_LOG_BYTES: u64 = 1024 * 1024;

pub fn info(data_dir: &Path, msg: &str) {
    write_log(data_dir, "INFO ", msg);
}

pub fn error(data_dir: &Path, msg: &str) {
    write_log(data_dir, "ERROR", msg);
}

fn write_log(data_dir: &Path, level: &str, msg: &str) {
    let dir = data_dir.join("logs");
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("app.log");
    if fs::metadata(&path)
        .map(|m| m.len() > MAX_LOG_BYTES)
        .unwrap_or(false)
    {
        let _ = fs::rename(&path, dir.join("app.log.1"));
    }
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let _ = writeln!(file, "[{}] {level} {msg}", fmt_utc(now_secs()));
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// UTC 时间格式化（避免为此引入 chrono 依赖）
fn fmt_utc(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    let (y, mo, d) = civil_from_days(days);
    format!(
        "{y:04}-{mo:02}-{d:02} {:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Howard Hinnant 的 civil_from_days 算法：把「1970-01-01 起的天数」还原为年月日
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_epoch() {
        assert_eq!(fmt_utc(0), "1970-01-01 00:00:00Z");
        assert_eq!(fmt_utc(1_700_000_000), "2023-11-14 22:13:20Z");
    }

    #[test]
    fn leap_year_boundary() {
        assert_eq!(fmt_utc(951_782_400), "2000-02-29 00:00:00Z");
    }
}
