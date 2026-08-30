use serde::{Deserialize, Serialize};

pub mod error;
pub mod installer;

pub use error::AppError;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpdatePackage {
    pub app_type: String,
    pub build_type: String,
    pub version: String,
    pub url: String,
    #[serde(default)]
    pub md5: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum UpdatesManifest {
    Wrapped { packages: Vec<UpdatePackage> },
    Bare(Vec<UpdatePackage>),
}

impl UpdatesManifest {
    pub fn into_packages(self) -> Vec<UpdatePackage> {
        match self {
            Self::Wrapped { packages } => packages,
            Self::Bare(packages) => packages,
        }
    }
}

pub static NET_TIMEOUT_SECS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(20);

pub fn set_net_timeout_secs(secs: u64) {
    NET_TIMEOUT_SECS.store(secs, std::sync::atomic::Ordering::Relaxed);
}

pub fn net_timeout() -> Option<std::time::Duration> {
    match NET_TIMEOUT_SECS.load(std::sync::atomic::Ordering::Relaxed) {
        0 => None,
        s => Some(std::time::Duration::from_secs(s)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_accepts_both_shapes() {
        let wrapped = r#"{"packages":[{"app_type":"mobile","build_type":"apk","version":"0.5.523","url":"/download/x.apk","md5":""}],"updateTs":1}"#;
        let bare = r#"[{"app_type":"gui","build_type":"deb","version":"0.5.523","url":"/download/x.deb"}]"#;

        let w: UpdatesManifest = serde_json::from_str(wrapped).unwrap();
        assert_eq!(w.into_packages()[0].build_type, "apk");

        let b: UpdatesManifest = serde_json::from_str(bare).unwrap();
        assert_eq!(b.into_packages()[0].build_type, "deb");
    }
}
