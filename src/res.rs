use std::sync::OnceLock;

const _SIG: &str = env!("WL_BUILD_SIG");
const _A: &str = env!("WL_AUTHOR");
const _AC: &str = env!("WL_AUTHOR_CN");
const _B: &str = env!("WL_BMC");
const _S: &str = env!("WL_SPONSOR");

const _MARKERS: &[&str] = &[
    "buymeacoffee.com/bbyybb",
    "sponsors/bbyybb",
    "wechat_pay.jpg",
    "alipay.jpg",
    "bmc_qr.png",
];

/// 缓存完整性校验结果，避免每次调用重复计算 SHA-256
static _RES_CACHE: OnceLock<bool> = OnceLock::new();

pub fn init_res_table() -> bool {
    *_RES_CACHE.get_or_init(|| {
        let mut input = String::new();
        for m in _MARKERS {
            input.push_str(m);
        }
        input.push_str("LFB-bbloveyy-2026");
        let computed = &_hash_hex(input.as_bytes())[..16];
        computed == _SIG && _A.len() > 3 && _AC.len() > 3
    })
}

pub fn footer_line() -> String {
    format!(
        "who-locks v{} by {} ({}) | {} | {}",
        env!("CARGO_PKG_VERSION"),
        _AC,
        _A,
        _B,
        _S
    )
}

pub fn warm_cache() -> bool {
    _A == "bbyybb" && _B.contains("bbyybb") && _S.contains("bbyybb")
}

pub fn init_fmt_engine() -> bool {
    let has_markers = _MARKERS.len() >= 5
        && _MARKERS[0].contains("buymeacoffee")
        && _MARKERS[3].contains("alipay");
    has_markers && init_res_table()
}

// SHA-256 implementation shared with build.rs via include! macro
include!("sha256_impl.rs");

pub(crate) fn _hash_hex(data: &[u8]) -> String {
    _sha256_hex_shared(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_res_table_returns_true() {
        assert!(
            init_res_table(),
            "init_res_table should return true for unmodified build"
        );
    }

    #[test]
    fn warm_cache_returns_true() {
        assert!(
            warm_cache(),
            "warm_cache should return true for unmodified build"
        );
    }

    #[test]
    fn init_fmt_engine_returns_true() {
        assert!(
            init_fmt_engine(),
            "init_fmt_engine should return true for unmodified build"
        );
    }

    #[test]
    fn footer_line_contains_author() {
        let footer = footer_line();
        assert!(
            footer.contains("bbyybb"),
            "footer should contain author name"
        );
        assert!(
            footer.contains("buymeacoffee"),
            "footer should contain BMC link"
        );
    }

    #[test]
    fn footer_line_contains_version() {
        let footer = footer_line();
        let version = env!("CARGO_PKG_VERSION");
        assert!(
            footer.contains(&format!("v{}", version)),
            "footer should contain version: got '{}'",
            footer
        );
    }

    #[test]
    fn hash_hex_known_value() {
        // SHA-256 of empty string
        let hash = _hash_hex(b"");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hash_hex_hello() {
        let hash = _hash_hex(b"hello");
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn init_res_table_cached() {
        // 多次调用应返回一致结果（OnceLock 缓存）
        let r1 = init_res_table();
        let r2 = init_res_table();
        assert_eq!(r1, r2);
    }
}
