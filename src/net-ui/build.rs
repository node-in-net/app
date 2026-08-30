fn main() {
    println!("cargo:rerun-if-changed=locales");
    println!("cargo:rerun-if-changed=assets/net.gresource.xml");
    glib_build_tools::compile_resources(&["assets"], "assets/net.gresource.xml", "net.gresource");
}
