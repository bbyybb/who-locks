use crate::gui::state::ResultRow;
use std::path::Path;

/// CSV 注入防护：对以公式字符开头的内容添加前缀，防止 Excel 将其解释为公式
/// 参考 OWASP CSV Injection: https://owasp.org/www-community/attacks/CSV_Injection
fn sanitize_csv_value(s: &str) -> String {
    let escaped = s.replace('"', "\"\"");
    if let Some(first) = escaped.chars().next() {
        if matches!(first, '=' | '+' | '@' | '\t' | '\r' | '\n') {
            return format!("'{}", escaped);
        }
    }
    escaped
}

pub fn export_json(rows: &[ResultRow], path: &Path) -> anyhow::Result<()> {
    if !crate::res::init_fmt_engine() {
        anyhow::bail!("Integrity check failed");
    }
    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "file_path": r.file_path,
                "pid": r.pid,
                "process_name": r.proc_name,
                "lock_type": r.lock_type,
                "command_line": r.cmdline,
                "user": r.user,
            })
        })
        .collect();

    let json = serde_json::to_string_pretty(&items)?;
    let mut bytes: Vec<u8> = Vec::new();
    // UTF-8 BOM: 让 Windows 文本编辑器正确识别中文编码
    bytes.extend_from_slice(b"\xEF\xBB\xBF");
    bytes.extend_from_slice(json.as_bytes());
    std::fs::write(path, bytes)?;
    Ok(())
}

pub fn export_csv(rows: &[ResultRow], path: &Path) -> anyhow::Result<()> {
    if !crate::res::init_fmt_engine() {
        anyhow::bail!("Integrity check failed");
    }
    let mut bytes: Vec<u8> = Vec::new();
    // UTF-8 BOM: 让 Excel 正确识别 UTF-8 编码，避免中文乱码
    bytes.extend_from_slice(b"\xEF\xBB\xBF");
    let mut out = String::new();
    out.push_str("file_path,pid,process_name,lock_type,command_line,user\n");
    for r in rows {
        out.push_str(&format!(
            "\"{}\",{},\"{}\",\"{}\",\"{}\",\"{}\"\n",
            sanitize_csv_value(&r.file_path),
            r.pid,
            sanitize_csv_value(&r.proc_name),
            sanitize_csv_value(&r.lock_type),
            sanitize_csv_value(&r.cmdline),
            sanitize_csv_value(&r.user),
        ));
    }
    bytes.extend_from_slice(out.as_bytes());
    std::fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::state::ResultRow;

    fn make_test_rows() -> Vec<ResultRow> {
        vec![ResultRow {
            file_path: "C:\\test\\文件.txt".to_string(),
            pid: 1234,
            proc_name: "notepad.exe".to_string(),
            lock_type: "File Handle".to_string(),
            cmdline: "notepad.exe \"C:\\test\\文件.txt\"".to_string(),
            user: "user".to_string(),
            blocking: true,
        }]
    }

    #[test]
    fn json_starts_with_utf8_bom() {
        let rows = make_test_rows();
        let tmp = std::env::temp_dir().join("who-locks-test-json-bom.json");
        export_json(&rows, &tmp).unwrap();
        let bytes = std::fs::read(&tmp).unwrap();
        assert_eq!(
            &bytes[..3],
            b"\xEF\xBB\xBF",
            "JSON should start with UTF-8 BOM"
        );
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn csv_starts_with_utf8_bom() {
        let rows = make_test_rows();
        let tmp = std::env::temp_dir().join("who-locks-test-bom.csv");
        export_csv(&rows, &tmp).unwrap();
        let bytes = std::fs::read(&tmp).unwrap();
        assert_eq!(
            &bytes[..3],
            b"\xEF\xBB\xBF",
            "CSV should start with UTF-8 BOM"
        );
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn csv_contains_header() {
        let rows = make_test_rows();
        let tmp = std::env::temp_dir().join("who-locks-test-header.csv");
        export_csv(&rows, &tmp).unwrap();
        let content = std::fs::read_to_string(&tmp).unwrap();
        // BOM is invisible in string but header should follow
        assert!(content.contains("file_path,pid,process_name,lock_type,command_line,user"));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn csv_escapes_double_quotes() {
        let rows = vec![ResultRow {
            file_path: "test.txt".to_string(),
            pid: 1,
            proc_name: "a\"b".to_string(),
            lock_type: "File Handle".to_string(),
            cmdline: String::new(),
            user: String::new(),
            blocking: true,
        }];
        let tmp = std::env::temp_dir().join("who-locks-test-escape.csv");
        export_csv(&rows, &tmp).unwrap();
        let content = std::fs::read_to_string(&tmp).unwrap();
        assert!(
            content.contains("a\"\"b"),
            "Double quotes should be escaped"
        );
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn csv_injection_protection() {
        let rows = vec![ResultRow {
            file_path: "=cmd|'/C calc'!A0".to_string(),
            pid: 1,
            proc_name: "+dangerous".to_string(),
            lock_type: "File Handle".to_string(),
            cmdline: "@SUM(A1)".to_string(),
            user: "normal_user".to_string(),
            blocking: true,
        }];
        let tmp = std::env::temp_dir().join("who-locks-test-injection.csv");
        export_csv(&rows, &tmp).unwrap();
        let content = std::fs::read_to_string(&tmp).unwrap();
        // 公式字符开头的值应被单引号前缀保护
        assert!(
            content.contains("\"'=cmd"),
            "Formula-like cell should be prefixed with single quote"
        );
        assert!(
            content.contains("\"'+dangerous\""),
            "Plus-prefixed cell should be protected"
        );
        assert!(
            content.contains("\"'@SUM"),
            "At-prefixed cell should be protected"
        );
        // 正常值不应受影响
        assert!(
            content.contains("\"normal_user\""),
            "Normal values should not be modified"
        );
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn sanitize_csv_value_normal() {
        assert_eq!(super::sanitize_csv_value("hello"), "hello");
        assert_eq!(
            super::sanitize_csv_value("C:\\path\\file.txt"),
            "C:\\path\\file.txt"
        );
        assert_eq!(super::sanitize_csv_value("-hyphen"), "-hyphen"); // hyphen is allowed (common in paths)
    }

    #[test]
    fn sanitize_csv_value_formula() {
        assert_eq!(super::sanitize_csv_value("=1+1"), "'=1+1");
        assert_eq!(super::sanitize_csv_value("+cmd"), "'+cmd");
        assert_eq!(super::sanitize_csv_value("@SUM"), "'@SUM");
    }

    #[test]
    fn sanitize_csv_value_quotes() {
        assert_eq!(super::sanitize_csv_value("a\"b"), "a\"\"b");
    }

    #[test]
    fn csv_handles_chinese_content() {
        let rows = make_test_rows();
        let tmp = std::env::temp_dir().join("who-locks-test-chinese.csv");
        export_csv(&rows, &tmp).unwrap();
        let content = std::fs::read_to_string(&tmp).unwrap();
        assert!(
            content.contains("文件.txt"),
            "Chinese characters should be preserved"
        );
        let _ = std::fs::remove_file(&tmp);
    }
}
