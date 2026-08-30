fn main() {
    println!("cargo:rerun-if-changed=locales");
    println!("cargo:rerun-if-changed=assets/sysinfo.gresource.xml");
    glib_build_tools::compile_resources(
        &["assets"],
        "assets/sysinfo.gresource.xml",
        "sysinfo.gresource",
    );
}
