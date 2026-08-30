fn main() {
    println!("cargo:rerun-if-changed=locales");
    println!("cargo:rerun-if-changed=assets/sync.gresource.xml");
    glib_build_tools::compile_resources(&["assets"], "assets/sync.gresource.xml", "sync.gresource");
}
