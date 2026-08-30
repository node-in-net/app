fn main() {
    println!("cargo:rerun-if-changed=locales");
    println!("cargo:rerun-if-changed=assets/fm.gresource.xml");
    glib_build_tools::compile_resources(&["assets"], "assets/fm.gresource.xml", "fm.gresource");
}
