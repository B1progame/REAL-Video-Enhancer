use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let icon = manifest_dir.join("../icons/logo-v3.ico");
    println!("cargo:rerun-if-changed={}", icon.display());

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let icon = icon
            .to_str()
            .expect("the application icon path must be valid UTF-8");
        let mut resources = winres::WindowsResource::new();
        resources
            .set_icon(icon)
            .set("FileDescription", "REAL Video Enhancer")
            .set("ProductName", "REAL Video Enhancer")
            .set("CompanyName", "B1progame");
        resources
            .compile()
            .expect("failed to embed the REAL Video Enhancer Windows icon");
    }
}
