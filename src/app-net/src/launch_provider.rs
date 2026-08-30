use crate::SocksManager;
use client_core::launcher::{refusal, AppLaunchProvider};
use nodeinnet_p2p::p2p::{LaunchableApp, RemoteAppSession};
use std::sync::Arc;
use uuid::Uuid;

const MAX_REMOTE_SESSIONS: usize = 8;

pub struct HostedLauncher {
    socks: Arc<SocksManager>,
    config: client_config::AppConfig,
    net_tx: tokio::sync::mpsc::Sender<client_core::NetCmd>,
    router: Arc<app_core::webvpn::WebVpnRouter>,
}

impl HostedLauncher {
    pub fn new(
        socks: Arc<SocksManager>,
        config: client_config::AppConfig,
        net_tx: tokio::sync::mpsc::Sender<client_core::NetCmd>,
        router: Arc<app_core::webvpn::WebVpnRouter>,
    ) -> Self {
        Self {
            socks,
            config,
            net_tx,
            router,
        }
    }

    fn own_network_resource(&self) -> Option<String> {
        crate::default_shared_resources(&self.config)
            .into_iter()
            .find(|r| r.resource_type == nodeinnet_p2p::ResourceType::SharedNetwork && r.is_active)
            .map(|r| r.id)
    }
}

impl AppLaunchProvider for HostedLauncher {
    fn view(&self, peer_id: &str) -> Option<(Vec<LaunchableApp>, Vec<RemoteAppSession>)> {
        self.own_network_resource()?;
        let apps = client_config::apps::consented(&self.config)
            .into_iter()
            .map(|a| LaunchableApp {
                id: a.id,
                name: a.name,
                icon_name: a.icon_name.filter(|i| {
                    !i.contains('/') && !i.contains('\\') && !std::path::Path::new(i).is_absolute()
                }),
            })
            .collect();
        Some((apps, self.socks.sessions_for(peer_id)))
    }

    fn launch(
        &self,
        peer_id: &str,
        egress: &str,
        session_id: Uuid,
        app_id: &str,
    ) -> Result<(), &'static str> {
        self.own_network_resource().ok_or(refusal::NETWORK_OFF)?;
        let app =
            client_config::apps::find_by_id(&self.config, app_id).ok_or(refusal::UNKNOWN_APP)?;
        if !app.allow_remote_launch {
            return Err(refusal::NO_CONSENT);
        }
        if self.socks.sessions_for(peer_id).len() >= MAX_REMOTE_SESSIONS {
            return Err(refusal::TOO_MANY);
        }
        self.socks
            .launch_session(
                peer_id,
                egress,
                &app.exec_cmd,
                self.net_tx.clone(),
                self.router.clone(),
                Some(session_id),
                Some(app.id),
            )
            .map(|_| ())
            .map_err(|_| refusal::SPAWN_FAILED)
    }

    fn stop(&self, peer_id: &str, session_id: Uuid) -> Result<(), &'static str> {
        if self.socks.terminate_session(peer_id, session_id) {
            Ok(())
        } else {
            Err(refusal::UNKNOWN_SESSION)
        }
    }
}
