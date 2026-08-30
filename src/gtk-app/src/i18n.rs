use std::sync::Once;

static INIT: Once = Once::new();

fn ensure() {
    INIT.call_once(|| {
        nodeinnet_i18n::register_locales!("../locales");
    });
}

pub(crate) fn tr(key: &str) -> String {
    ensure();
    nodeinnet_i18n::tr(key)
}

pub(crate) fn trf(key: &str, args: &[(&str, &str)]) -> String {
    ensure();
    nodeinnet_i18n::trf(key, args)
}
