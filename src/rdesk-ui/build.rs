fn main() {
    println!("cargo:rerun-if-changed=locales");
    println!("cargo:rerun-if-changed=assets/rdesk.gresource.xml");
    glib_build_tools::compile_resources(
        &["assets"],
        "assets/rdesk.gresource.xml",
        "rdesk.gresource",
    );
}
