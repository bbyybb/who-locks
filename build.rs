use std::collections::HashMap;
use std::fs;
use std::io::Read;

fn main() {
    // ========================================================
    // 编译时完整性校验 + 签名注入
    // ========================================================

    let sealed_files: HashMap<&str, &str> = HashMap::from([
        (
            "docs/wechat_pay.jpg",
            "686b9d5bba59d6831580984cb93804543f346d943f2baf4a94216fd13438f1e6",
        ),
        (
            "docs/alipay.jpg",
            "510155042b703d23f7eeabc04496097a7cc13772c5712c8d0716bab5962172dd",
        ),
        (
            "docs/bmc_qr.png",
            "bfd20ef305007c3dacf30dde49ce8f0fe4d7ac3ffcc86ac1f83bc1e75cccfcd6",
        ),
    ]);

    let readme_markers = [
        "buymeacoffee.com/bbyybb",
        "sponsors/bbyybb",
        "wechat_pay.jpg",
        "alipay.jpg",
        "bmc_qr.png",
    ];

    // --- 校验二维码图片哈希 ---
    for (path, expected_hash) in &sealed_files {
        println!("cargo:rerun-if-changed={}", path);

        let mut file = match fs::File::open(path) {
            Ok(f) => f,
            Err(_) => {
                panic!(
                    "\n\n========================================\n\
                     ERROR: Donation file missing: {}\n\
                     Please do not remove author attribution files.\n\
                     ========================================\n",
                    path
                );
            }
        };

        let mut buf = Vec::new();
        file.read_to_end(&mut buf).unwrap();

        let hash = sha256_hex(&buf);
        if hash != *expected_hash {
            panic!(
                "\n\n========================================\n\
                 ERROR: Donation file tampered: {}\n\
                 Expected: {}\n\
                 Got:      {}\n\
                 Please do not modify author attribution files.\n\
                 ========================================\n",
                path, expected_hash, hash
            );
        }
    }

    // --- 校验 README.md ---
    println!("cargo:rerun-if-changed=README.md");
    let readme = fs::read_to_string("README.md").unwrap_or_default();
    for marker in &readme_markers {
        if !readme.contains(marker) {
            panic!(
                "\n\n========================================\n\
                 ERROR: README.md is missing donation marker: {}\n\
                 Please do not remove author attribution from README.\n\
                 ========================================\n",
                marker
            );
        }
    }

    // --- 校验 FUNDING.yml ---
    println!("cargo:rerun-if-changed=.github/FUNDING.yml");
    let funding = fs::read_to_string(".github/FUNDING.yml").unwrap_or_default();
    if !funding.contains("bbyybb") {
        panic!(
            "\n\n========================================\n\
             ERROR: .github/FUNDING.yml is missing or tampered.\n\
             Please do not remove author attribution.\n\
             ========================================\n"
        );
    }

    // ========================================================
    // 编译时签名注入：将标记哈希和签名注入到二进制中
    // 运行时代码可以通过 env! 宏读取这些值进行自校验
    // ========================================================

    // 计算所有标记的组合签名
    let mut sig_input = String::new();
    for marker in &readme_markers {
        sig_input.push_str(marker);
    }
    sig_input.push_str("LFB-bbloveyy-2026");
    let sig = &sha256_hex(sig_input.as_bytes())[..16];

    // 注入到编译环境，运行时通过 env!("WL_BUILD_SIG") 读取
    println!("cargo:rustc-env=WL_BUILD_SIG={}", sig);
    println!("cargo:rustc-env=WL_AUTHOR=bbyybb");
    println!("cargo:rustc-env=WL_AUTHOR_CN=白白LOVE尹尹");
    println!("cargo:rustc-env=WL_BMC=buymeacoffee.com/bbyybb");
    println!("cargo:rustc-env=WL_SPONSOR=github.com/sponsors/bbyybb");

    println!("cargo:rerun-if-changed=src/sha256_impl.rs");

    // 校验 build.rs 自身的关键函数是否被清空
    // 通过检查自身源码中是否包含必要的校验关键词
    println!("cargo:rerun-if-changed=build.rs");
    let self_src = fs::read_to_string("build.rs").unwrap_or_default();
    let self_markers = [
        "sha256_hex",
        "sealed_files",
        "readme_markers",
        "WL_BUILD_SIG",
    ];
    for m in &self_markers {
        if self_src.matches(m).count() < 2 {
            panic!(
                "\n\n========================================\n\
                 ERROR: build.rs integrity check failed.\n\
                 The build script appears to have been tampered with.\n\
                 ========================================\n"
            );
        }
    }

    // ========================================================
    // Windows: 将应用图标嵌入 .exe 资源
    // ========================================================
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/icon.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.compile().expect("Failed to compile Windows resources");
    }
}

// SHA-256 implementation shared with src/res.rs via include! macro
include!("src/sha256_impl.rs");

fn sha256_hex(data: &[u8]) -> String {
    _sha256_hex_shared(data)
}

#[allow(dead_code)]
fn sha256(data: &[u8]) -> [u8; 32] {
    _sha256_shared(data)
}
