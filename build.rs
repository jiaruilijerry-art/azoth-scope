fn main() {
    println!("cargo:rerun-if-changed=assets/azoth-scope.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon_with_id("assets/azoth-scope.ico", "1")
            .compile()
            .expect("embed Azoth Scope icon");
    }
}
