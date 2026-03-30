//! CLI 集成测试
//!
//! 通过 `std::process::Command` 调用编译后的二进制文件，
//! 测试命令行模式的端到端行为。

use std::fs;
use std::process::Command;

/// 获取编译后的二进制路径
fn bin_path() -> String {
    env!("CARGO_BIN_EXE_who-locks").to_string()
}

#[test]
fn cli_help_shows_usage() {
    let output = Command::new(bin_path())
        .arg("--help")
        .output()
        .expect("failed to run binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("PATHS") || stdout.contains("paths") || stdout.contains("usage"),
        "help should mention PATHS or usage, got: {}",
        stdout
    );
    assert!(output.status.success());
}

#[test]
fn cli_nonexistent_path_returns_error() {
    let output = Command::new(bin_path())
        .arg("/nonexistent/path/that/does/not/exist_12345")
        .output()
        .expect("failed to run binary");

    // 不存在的路径应返回非零退出码
    assert!(!output.status.success());
}

#[test]
fn cli_scan_empty_temp_dir_text_format() {
    let tmp = tempdir_for_test("cli_scan_text");

    let output = Command::new(bin_path())
        .arg(tmp.as_str())
        .output()
        .expect("failed to run binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // 空目录应无占用，输出应包含扫描相关信息
    // 成功扫描应返回 0
    assert!(
        output.status.success(),
        "scanning empty dir should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // stdout 可能为空（无占用结果）或包含扫描统计信息
    // 关键是不应 panic 或出错
    let _ = stdout;

    cleanup_tempdir(&tmp);
}

#[test]
fn cli_scan_empty_temp_dir_json_format() {
    let tmp = tempdir_for_test("cli_scan_json");

    let output = Command::new(bin_path())
        .arg(tmp.as_str())
        .arg("-f")
        .arg("json")
        .output()
        .expect("failed to run binary");

    assert!(
        output.status.success(),
        "JSON scan should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    // JSON 输出应为有效 JSON（以 [ 或 { 开头）
    let trimmed = stdout.trim();
    if !trimmed.is_empty() {
        assert!(
            trimmed.starts_with('[') || trimmed.starts_with('{'),
            "JSON output should start with [ or {{, got: {}",
            &trimmed[..trimmed.len().min(100)]
        );
    }

    cleanup_tempdir(&tmp);
}

#[test]
fn cli_no_recursive_flag() {
    let tmp = tempdir_for_test("cli_no_rec");
    // 创建子目录
    let sub = format!("{}/subdir", tmp);
    fs::create_dir_all(&sub).unwrap();

    let output = Command::new(bin_path())
        .arg(tmp.as_str())
        .arg("--no-recursive")
        .output()
        .expect("failed to run binary");

    assert!(
        output.status.success(),
        "no-recursive scan should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    cleanup_tempdir(&tmp);
}

#[test]
fn cli_depth_option() {
    let tmp = tempdir_for_test("cli_depth");

    let output = Command::new(bin_path())
        .arg(tmp.as_str())
        .arg("-d")
        .arg("1")
        .output()
        .expect("failed to run binary");

    assert!(
        output.status.success(),
        "depth-limited scan should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    cleanup_tempdir(&tmp);
}

#[test]
fn cli_exclude_option() {
    let tmp = tempdir_for_test("cli_exclude");

    let output = Command::new(bin_path())
        .arg(tmp.as_str())
        .arg("-e")
        .arg("*.log,temp*")
        .output()
        .expect("failed to run binary");

    assert!(
        output.status.success(),
        "exclude scan should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    cleanup_tempdir(&tmp);
}

#[test]
fn cli_multiple_paths() {
    let tmp1 = tempdir_for_test("cli_multi1");
    let tmp2 = tempdir_for_test("cli_multi2");

    let output = Command::new(bin_path())
        .arg(tmp1.as_str())
        .arg(tmp2.as_str())
        .output()
        .expect("failed to run binary");

    assert!(
        output.status.success(),
        "multi-path scan should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    cleanup_tempdir(&tmp1);
    cleanup_tempdir(&tmp2);
}

// --- 辅助函数 ---

/// 创建临时测试目录
fn tempdir_for_test(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("who-locks-test-{}-{}", name, std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir.to_string_lossy().to_string()
}

/// 清理临时目录
fn cleanup_tempdir(path: &str) {
    let _ = fs::remove_dir_all(path);
}
