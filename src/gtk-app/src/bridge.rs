use app_core::session::AuthRpc;
use app_core::workspace::{DeviceInfo, ServiceKind};
pub struct AcceptAnyAuth;

#[async_trait::async_trait(?Send)]
impl AuthRpc for AcceptAnyAuth {
    async fn login(&self, _login: String, _password: String) -> Result<(), String> {
        Ok(())
    }
    async fn register_device(&self, _node_id: String, _device_name: String) -> Result<(), String> {
        Ok(())
    }
}

pub fn mock_devices() -> Vec<DeviceInfo> {
    let dev = |id: &str, name: &str, os: &str, online: bool| DeviceInfo {
        id: id.into(),
        name: name.into(),
        os: os.into(),
        online,
        link: app_core::workspace::LinkState::Idle,
        services: vec![
            ServiceKind::SystemInfo,
            ServiceKind::Files,
            ServiceKind::Terminal,
            ServiceKind::Desktop,
            ServiceKind::Network,
        ],
    };
    vec![
        dev("server-1", "server-1", "linux", true),
        dev("laptop-win32", "laptop-win32", "windows", true),
        dev("pixel-9", "Pixel 9 Pro XL", "android", false),
        dev("laptop-1", "laptop-1", "linux", false),
    ]
}
