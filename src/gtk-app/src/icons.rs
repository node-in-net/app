const RESOURCE_PREFIX: &str = "/com/nodeinnet/gtk";

pub fn image(name: &str, size: i32) -> gtk::Image {
    let img = gtk::Image::from_resource(&format!("{RESOURCE_PREFIX}/{name}.svg"));
    img.set_pixel_size(size);
    img
}

pub fn os_icon_name(os: &str) -> &'static str {
    match os {
        "macos" | "darwin" => "os-mac",
        "windows" => "os-win",
        "android" => "os-android",
        "ios" => "os-iphone",
        "linux" => "os-linux",
        _ => "desktop-pc",
    }
}

pub fn os_pretty(os: &str) -> &'static str {
    match os {
        "macos" | "darwin" => "macOS",
        "windows" => "Windows",
        "android" => "Android",
        "ios" => "iOS",
        "linux" => "Linux",
        _ => "Desktop",
    }
}

pub fn desktop_hero_name(os: &str) -> &'static str {
    match os {
        "macos" | "darwin" => "desktop-mac",
        _ => "desktop-pc",
    }
}
