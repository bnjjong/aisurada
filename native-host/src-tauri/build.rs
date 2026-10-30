fn main() {
    println!("cargo:rerun-if-changed=secret-names.txt");
    println!("cargo:rerun-if-changed=secret-names.sample.txt");
    let real = std::path::Path::new("secret-names.txt").is_file();
    let source = if real {
        "secret-names.txt"
    } else {
        "secret-names.sample.txt"
    };
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-env=WITHLOG_SECRET_TABLE={root}/{source}");
    println!(
        "cargo:rustc-env=WITHLOG_SECRET_TABLE_SOURCE={}",
        if real { "real" } else { "sample" }
    );
    tauri_build::build()
}
