use client_core::{AppEventHandler, NetCmd};
use colored::*;
use nodeinnet_p2p::{NodeInfo, P2pMessage};

pub struct ConsoleEventHandler {
    pub ui_tx: std::sync::mpsc::Sender<AppEvent>,
}

#[async_trait::async_trait]
impl AppEventHandler for ConsoleEventHandler {
    async fn on_log(&self, msg: String) {
        let _ = self.ui_tx.send(AppEvent::Log(msg));
    }
    async fn on_connected(&self) {
        let _ = self.ui_tx.send(AppEvent::Connected);
    }
    async fn on_disconnected(&self) {
        let _ = self.ui_tx.send(AppEvent::Disconnected);
    }
    async fn on_update_nodes(&self, nodes: Vec<NodeInfo>) {
        let _ = self.ui_tx.send(AppEvent::UpdateNodes(nodes));
    }
    async fn on_download_complete(&self, path: std::path::PathBuf) {
        let _ = self.ui_tx.send(AppEvent::DownloadComplete(path));
    }
    async fn on_p2p_message(&self, msg: P2pMessage) {
        let _ = self.ui_tx.send(AppEvent::P2pMessageReceived(msg));
    }
    async fn on_p2p_disconnected(&self, peer_id: String) {
        let _ = self.ui_tx.send(AppEvent::PeerDisconnected(peer_id));
    }
    async fn on_p2p_connected(&self, peer_id: String) {
        let _ = self.ui_tx.send(AppEvent::Log(format!(
            "🟢 Connected to P2P peer: {}",
            peer_id
        )));
    }
}

pub enum AppEvent {
    Log(String),
    UpdateNodes(Vec<NodeInfo>),
    Connected,
    Disconnected,
    #[cfg_attr(debug_assertions, allow(dead_code))]
    TokenRotated(String),
    P2pMessageReceived(P2pMessage),
    PeerDisconnected(String),
    DownloadComplete(std::path::PathBuf),
    UploadUnanswered(uuid::Uuid),
}

const RECEIPT_WAIT: std::time::Duration = std::time::Duration::from_secs(60 * 60);

pub struct PendingUpload {
    pub path: std::path::PathBuf,
    pub node: String,
    pub accepted: bool,
}

pub type Uploads =
    std::sync::Arc<std::sync::Mutex<std::collections::HashMap<uuid::Uuid, PendingUpload>>>;

pub struct EventLoopContext {
    pub app_rx: std::sync::mpsc::Receiver<AppEvent>,
    pub refresh_token: String,
    pub net_tx: tokio::sync::mpsc::Sender<NetCmd>,
    pub active_downloads:
        std::sync::Arc<std::sync::Mutex<std::collections::HashMap<uuid::Uuid, std::path::PathBuf>>>,
    pub pending_uploads: Uploads,
    pub active_sync_strategy: std::sync::Arc<std::sync::Mutex<Option<(String, String, String)>>>,
    pub known_peers: std::collections::HashSet<String>,
    pub my_info: nodeinnet_p2p::NodeInfo,
    pub ws_base: String,
    pub my_node_id: String,
    pub exec_cmd: Option<String>,
    pub app_tx: std::sync::mpsc::Sender<AppEvent>,
    pub api_base: String,
    pub client_http: reqwest::Client,
    pub reconnect_attempt: usize,
    pub reconnect_timeouts: Vec<u64>,
    pub config: client_config::AppConfig,
}

pub fn run_event_loop(ctx: EventLoopContext) {
    let mut refresh_token = ctx.refresh_token;
    let net_tx = ctx.net_tx;
    let active_downloads = ctx.active_downloads;
    let pending_uploads = ctx.pending_uploads;
    let active_sync_strategy = ctx.active_sync_strategy;
    let mut known_peers = ctx.known_peers;
    let my_info = ctx.my_info;
    let ws_base = ctx.ws_base;
    let my_node_id = ctx.my_node_id;
    let exec_cmd = ctx.exec_cmd;
    let app_rx = ctx.app_rx;
    let app_tx = ctx.app_tx;
    let api_base = ctx.api_base;
    let client_http = ctx.client_http;
    let mut reconnect_attempt = ctx.reconnect_attempt;
    let reconnect_timeouts = ctx.reconnect_timeouts;
    let config = ctx.config;
    let mut transfer_failed = false;

    loop {
        if let Ok(event) = app_rx.recv() {
            match event {
                AppEvent::TokenRotated(new_token) => {
                    refresh_token = new_token.clone();
                    config.set("app.refresh_token", new_token);
                    config.save();
                    println!(
                        "{} Successfully persistent configuration with new Refresh Token!",
                        "🔑".green()
                    );
                }
                AppEvent::Log(msg) => {
                    if msg.starts_with("📁 [Config]")
                        || msg.contains("[WebSocket] Received NodesList")
                        || msg.contains("Failed to start TCP signaling server")
                        || msg.contains("Peer requested")
                        || msg.contains("Rejecting directory")
                        || msg.contains("RBAC")
                        || msg.contains("P2P-NODE KEYGEN")
                        || msg.contains("P2P peer")
                    {
                        let timestamp = chrono::Local::now().format("%H:%M:%S.%3f").to_string();
                        println!(
                            "{} [{}] {}",
                            "[SYS]".bright_black(),
                            timestamp.cyan(),
                            msg.white()
                        );
                    }
                }
                AppEvent::DownloadComplete(file_path) => {
                    println!(
                        "\n{} {} {}",
                        "📦".magenta().bold(),
                        "Update Downloaded!".green().bold(),
                        "Please install the new version from:".yellow()
                    );
                    println!("   {}\n", file_path.display());

                    println!("Would you like to install this update now? [y/N]");
                    let mut input = String::new();
                    if std::io::stdin().read_line(&mut input).is_ok() {
                        let input = input.trim().to_lowercase();
                        if input == "y" || input == "yes" {
                            if let Err(e) = common::installer::install_update(&file_path, false) {
                                println!("{} {}", "❌ Failed to install update:".red().bold(), e);
                            }
                        } else {
                            if let Some(parent) = file_path.parent() {
                                let _ = open::that(parent);
                            }
                        }
                    }
                }
                AppEvent::UpdateNodes(nodes) => {
                    let current_peer_ids: std::collections::HashSet<String> =
                        nodes.iter().map(|n| n.id.clone()).collect();
                    if current_peer_ids != known_peers {
                        let new_peers: Vec<_> = nodes
                            .iter()
                            .filter(|n| !known_peers.contains(&n.id))
                            .collect();

                        println!(
                            "\n{} {}",
                            "🌐 Active Node List Updated:".green().bold(),
                            nodes.len()
                        );
                        for node in &nodes {
                            println!(
                                "  {} [{}] - {} ({})",
                                "•".magenta(),
                                node.id.bright_blue(),
                                node.name.white(),
                                node.os.bright_black()
                            );
                        }

                        for new_peer in new_peers {
                            if new_peer.id != my_node_id {
                                let pk_len = new_peer.public_key.len().min(16);
                                let pk_display = if pk_len > 0 {
                                    &new_peer.public_key[..pk_len]
                                } else {
                                    "None"
                                };
                                println!(
                                    "  {} Received Public Key mapping for Handshake: {} = {}",
                                    "🔑".yellow().bold(),
                                    new_peer.name.cyan(),
                                    pk_display
                                );
                            }
                        }

                        known_peers = current_peer_ids;
                    }

                    if let Some(exec_cmd) = &exec_cmd {
                        let parts: Vec<&str> = exec_cmd.split_whitespace().collect();
                        if parts.len() == 2 && parts[0] == "resources" {
                            let target_node = parts[1];
                            if let Some(node) = nodes.iter().find(|n| n.id == target_node) {
                                println!(
                                    "\n{} Resources for node {}:",
                                    "📦".blue().bold(),
                                    target_node.cyan()
                                );
                                if let Ok(json) = serde_json::to_string_pretty(&node.resources) {
                                    println!("{}", json.white());
                                } else {
                                    println!("{} None found or failed to parse.", "⚠️".yellow());
                                }
                                std::process::exit(0);
                            }
                        }
                    }
                }
                AppEvent::Connected => {
                    println!(
                        "{}",
                        "✅ Connected Successfully to Signaling Server!"
                            .green()
                            .bold()
                    );
                    reconnect_attempt = 0;
                }
                AppEvent::Disconnected => {
                    println!("{}", "❌ Disconnected from server.".red().bold());

                    let timeout =
                        reconnect_timeouts[reconnect_attempt.min(reconnect_timeouts.len() - 1)];
                    reconnect_attempt += 1;

                    println!(
                        "{} {}",
                        format!("⏳ Reconnecting in {} seconds...", timeout)
                            .yellow()
                            .bold(),
                        format!("(Attempt {})", reconnect_attempt).bright_black()
                    );

                    let net_tx_clone = net_tx.clone();
                    let ws_base_clone = ws_base.to_string();
                    let api_base_clone = api_base.to_string();
                    let my_node_id_clone = my_node_id.clone();
                    let my_info_clone = my_info.clone();
                    let r_token_clone = refresh_token.clone();
                    let http_cli = client_http.clone();
                    let app_tx_clone = app_tx.clone();
                    let region = config.turn_region();

                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(timeout)).await;

                        if let Ok(resp) = http_cli
                            .post(format!("{}/account/refresh_token", api_base_clone))
                            .json(&nodeinnet_p2p::RefreshRequest {
                                refresh_token: r_token_clone.clone(),
                                region,
                            })
                            .send()
                            .await
                            && resp.status().is_success()
                            && let Ok(json) = resp.json::<serde_json::Value>().await
                        {
                            if let Some(new_refresh) = json["refresh_token"].as_str()
                                && new_refresh != r_token_clone
                            {
                                #[cfg(not(debug_assertions))]
                                {
                                    let _ = app_tx_clone
                                        .send(AppEvent::TokenRotated(new_refresh.to_string()));
                                }
                            }

                            if let Some(new_token) = json["access_token"].as_str() {
                                let new_ws_base = json["ws_url"].as_str().unwrap_or(&ws_base_clone);
                                let turn_credentials: Option<nodeinnet_p2p::rtc::TurnCredentials> =
                                    serde_json::from_value(json["turn"].clone()).unwrap_or(None);

                                let reconnect_url = format!(
                                    "{}?token={}&session_id={}",
                                    new_ws_base, new_token, my_node_id_clone
                                );
                                let _ = net_tx_clone
                                    .send(NetCmd::Connect(
                                        reconnect_url,
                                        my_info_clone,
                                        turn_credentials,
                                    ))
                                    .await;
                                return;
                            }
                        }

                        println!(
                            "{} Failed to acquire new token or reach server during reconnect routine.",
                            "⚠️".yellow()
                        );
                        let _ = app_tx_clone.send(AppEvent::Disconnected);
                    });
                }
                AppEvent::P2pMessageReceived(msg) => match msg {
                    P2pMessage::Ping { .. } | P2pMessage::Pong { .. } => {}
                    P2pMessage::SystemInfoResponse { resource_id, info } => {
                        if let Ok(json) = serde_json::to_string_pretty(&info) {
                            let formatted = format!(
                                "\n{} {} (sys_res_id={}):\n{}",
                                "📊 [TEST]".blue().bold(),
                                "System Info Response".green().bold(),
                                resource_id.cyan(),
                                json.white()
                            );
                            println!("{}", formatted);
                        }
                    }
                    P2pMessage::EntriesResponse {
                        resource_id,
                        path,
                        directories,
                        files,
                        ..
                    } => {
                        if let Ok(json_dirs) = serde_json::to_string_pretty(&directories)
                            && let Ok(json_files) = serde_json::to_string_pretty(&files)
                        {
                            let formatted = format!(
                                "\n{} {} (sys_res_id={}, path={}):\n{}\n{}",
                                "📂".blue().bold(),
                                "Entries Response".green().bold(),
                                resource_id.cyan(),
                                path.cyan(),
                                json_dirs.white(),
                                json_files.white()
                            );
                            println!("{}", formatted);
                            std::process::exit(0);
                        }
                    }
                    P2pMessage::MetadataResponse {
                        resource_id,
                        path,
                        metadata,
                        ..
                    } => {
                        let json = serde_json::to_string_pretty(&metadata).unwrap_or_default();
                        println!(
                            "\n{} {} (sys_res_id={}, path={}):\n{}",
                            "📄".blue().bold(),
                            "Metadata Response".green().bold(),
                            resource_id.cyan(),
                            path.cyan(),
                            json.white()
                        );
                        std::process::exit(0);
                    }
                    P2pMessage::SyncStateResponse {
                        resource_id,
                        folder_states,
                        ..
                    } => {
                        if let Some((target_peer, target_folder, strategy)) =
                            active_sync_strategy.lock().unwrap().clone()
                        {
                            if strategy == "diff_only" {
                                println!(
                                    "\n{} {} (Folder: {}):\n",
                                    "🔄".magenta().bold(),
                                    "Sync Diff Results".white().bold(),
                                    target_folder.cyan()
                                );

                                let local_cache =
                                    node_functions::sync_folder::scan_local_resource_folders(
                                        &config
                                            .get::<Vec<nodeinnet_p2p::SharedResource>>(
                                                "app.resources",
                                            )
                                            .unwrap_or_default(),
                                    );
                                let computed_diffs = node_functions::sync_folder::compute_sync_diff(
                                    &local_cache,
                                    &folder_states,
                                );

                                let mut mod_count = 0;
                                let mut loc_only_count = 0;
                                let mut rem_only_count = 0;

                                for diff in computed_diffs {
                                    if diff.difference
                                        == nodeinnet_p2p::p2p::FileDifference::Identical
                                    {
                                        continue;
                                    }
                                    let diff_symbol = match diff.difference {
                                        nodeinnet_p2p::p2p::FileDifference::Modified => {
                                            mod_count += 1;
                                            "⚠️ Mod".yellow()
                                        }
                                        nodeinnet_p2p::p2p::FileDifference::LocalOnly => {
                                            loc_only_count += 1;
                                            "📤 Loc".green()
                                        }
                                        nodeinnet_p2p::p2p::FileDifference::RemoteOnly => {
                                            rem_only_count += 1;
                                            "📥 Rem".blue()
                                        }
                                        _ => "".into(),
                                    };
                                    println!(" {} | {}", diff_symbol, diff.relative_path);
                                }

                                println!(
                                    "\n{} {} modified, {} local only, {} remote only.",
                                    "📊".magenta(),
                                    mod_count,
                                    loc_only_count,
                                    rem_only_count
                                );
                                std::process::exit(0);
                            } else {
                                println!(
                                    "\n{} Starting Sync Auto Strategy: {}",
                                    "🤖".magenta().bold(),
                                    strategy.cyan()
                                );
                                let local_cache =
                                    node_functions::sync_folder::scan_local_resource_folders(
                                        &config
                                            .get::<Vec<nodeinnet_p2p::SharedResource>>(
                                                "app.resources",
                                            )
                                            .unwrap_or_default(),
                                    );
                                let computed_diffs = node_functions::sync_folder::compute_sync_diff(
                                    &local_cache,
                                    &folder_states,
                                );

                                let mut actions_scheduled = 0;
                                let net_cmd_tx = net_tx.clone();

                                for diff in computed_diffs {
                                    if diff.difference
                                        == nodeinnet_p2p::p2p::FileDifference::Identical
                                        || diff.folder_name != target_folder
                                    {
                                        continue;
                                    }
                                    if !std::path::Path::new(&diff.relative_path)
                                        .components()
                                        .all(|c| matches!(c, std::path::Component::Normal(_)))
                                    {
                                        println!(
                                            "{} Skipping {}: it leads out of the sync folder",
                                            "⚠️".yellow(),
                                            diff.relative_path
                                        );
                                        continue;
                                    }
                                    if cfg!(windows)
                                        && std::path::Path::new(&diff.relative_path)
                                            .components()
                                            .any(|c| {
                                                node_functions::names::windows_refuses(
                                                    &c.as_os_str().to_string_lossy(),
                                                )
                                            })
                                    {
                                        println!(
                                            "{} Skipping {}: Windows cannot store this name",
                                            "⚠️".yellow(),
                                            diff.relative_path
                                        );
                                        continue;
                                    }

                                    let mut maybe_action = None;

                                    match strategy.as_str() {
                                        "pull" => {
                                            if diff.difference
                                                == nodeinnet_p2p::p2p::FileDifference::RemoteOnly
                                                || diff.difference
                                                    == nodeinnet_p2p::p2p::FileDifference::Modified
                                            {
                                                maybe_action =
                                                    Some(nodeinnet_p2p::p2p::SyncAction::Pull);
                                            }
                                        }
                                        "push" => {
                                            if diff.difference
                                                == nodeinnet_p2p::p2p::FileDifference::LocalOnly
                                                || diff.difference
                                                    == nodeinnet_p2p::p2p::FileDifference::Modified
                                            {
                                                maybe_action =
                                                    Some(nodeinnet_p2p::p2p::SyncAction::Push);
                                            }
                                        }
                                        "mirror_push" => match diff.difference {
                                            nodeinnet_p2p::p2p::FileDifference::Modified
                                            | nodeinnet_p2p::p2p::FileDifference::LocalOnly => {
                                                maybe_action =
                                                    Some(nodeinnet_p2p::p2p::SyncAction::Push);
                                            }
                                            nodeinnet_p2p::p2p::FileDifference::RemoteOnly => {
                                                maybe_action = Some(
                                                    nodeinnet_p2p::p2p::SyncAction::DeleteRemote,
                                                );
                                            }
                                            _ => {}
                                        },
                                        "mirror_pull" => match diff.difference {
                                            nodeinnet_p2p::p2p::FileDifference::Modified
                                            | nodeinnet_p2p::p2p::FileDifference::RemoteOnly => {
                                                maybe_action =
                                                    Some(nodeinnet_p2p::p2p::SyncAction::Pull);
                                            }
                                            nodeinnet_p2p::p2p::FileDifference::LocalOnly => {
                                                maybe_action = Some(
                                                    nodeinnet_p2p::p2p::SyncAction::DeleteLocal,
                                                );
                                            }
                                            _ => {}
                                        },
                                        _ => {
                                            let strat_rules: Vec<&str> =
                                                strategy.split(',').collect();

                                            if diff.difference
                                                != nodeinnet_p2p::p2p::FileDifference::Identical
                                            {
                                                if let (Some(local), Some(remote)) =
                                                    (&diff.local_state, &diff.remote_state)
                                                {
                                                    let loc_t = local.last_modified_ms;
                                                    let rem_t = remote.last_modified_ms;
                                                    if loc_t > rem_t {
                                                        if strat_rules
                                                            .contains(&"push-local-newermodifier")
                                                        {
                                                            maybe_action = Some(
                                                                    nodeinnet_p2p::p2p::SyncAction::Push,
                                                                );
                                                        } else if strat_rules
                                                            .contains(&"pull-local-newermodifier")
                                                        {
                                                            maybe_action = Some(
                                                                    nodeinnet_p2p::p2p::SyncAction::Pull,
                                                                );
                                                        }
                                                    } else {
                                                        if strat_rules
                                                            .contains(&"pull-remote-newermodifier")
                                                        {
                                                            maybe_action = Some(
                                                                    nodeinnet_p2p::p2p::SyncAction::Pull,
                                                                );
                                                        } else if strat_rules
                                                            .contains(&"push-remote-newermodifier")
                                                        {
                                                            maybe_action = Some(
                                                                    nodeinnet_p2p::p2p::SyncAction::Push,
                                                                );
                                                        }
                                                    }
                                                } else if diff.local_state.is_some() {
                                                    if strat_rules.contains(&"push-local-new") {
                                                        maybe_action = Some(
                                                            nodeinnet_p2p::p2p::SyncAction::Push,
                                                        );
                                                    } else if strat_rules
                                                        .contains(&"delete-local-new")
                                                    {
                                                        maybe_action = Some(nodeinnet_p2p::p2p::SyncAction::DeleteLocal);
                                                    }
                                                } else {
                                                    if strat_rules.contains(&"pull-remote-new") {
                                                        maybe_action = Some(
                                                            nodeinnet_p2p::p2p::SyncAction::Pull,
                                                        );
                                                    } else if strat_rules
                                                        .contains(&"delete-remote-new")
                                                    {
                                                        maybe_action = Some(nodeinnet_p2p::p2p::SyncAction::DeleteRemote);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if let Some(action) = maybe_action {
                                        actions_scheduled += 1;

                                        let mut resolved_local_path = None;
                                        if let Some(res) = config
                                            .get::<Vec<nodeinnet_p2p::SharedResource>>(
                                                "app.resources",
                                            )
                                            .unwrap_or_default()
                                            .iter()
                                            .find(|r| {
                                                r.name == target_folder
                                                    && r.resource_type
                                                        == nodeinnet_p2p::ResourceType::SyncFolder
                                            })
                                            && let Some(cfg_str) = &res.config
                                            && let Ok(json) =
                                                serde_json::from_str::<serde_json::Value>(cfg_str)
                                            && let Some(path_str) =
                                                json.get("path").and_then(|v| v.as_str())
                                        {
                                            resolved_local_path = Some(
                                                std::path::Path::new(path_str)
                                                    .join(&diff.relative_path),
                                            );
                                        }

                                        match action {
                                            nodeinnet_p2p::p2p::SyncAction::Pull => {
                                                let transfer_id = uuid::Uuid::new_v4();
                                                println!(
                                                    "{} Pulling {} ...",
                                                    "🔽".blue(),
                                                    diff.relative_path
                                                );

                                                if let Some(l_path) = &resolved_local_path {
                                                    active_downloads
                                                        .lock()
                                                        .unwrap()
                                                        .insert(transfer_id, l_path.clone());
                                                }

                                                let msg =
                                                        nodeinnet_p2p::P2pMessage::FileDownloadRequest {
                                                            resource_id: resource_id.clone(),
                                                            file_path: format!(
                                                                "/{}",
                                                                diff.relative_path
                                                            ),
                                                            transfer_id,
                                                            offset: 0,
                                                            length: None,
                                                        };
                                                tokio::spawn({
                                                    let n = net_cmd_tx.clone();
                                                    let p = target_peer.clone();
                                                    let m = msg;
                                                    async move {
                                                        let _ =
                                                            n.send(NetCmd::SendToPeer(p, m)).await;
                                                    }
                                                });
                                            }
                                            nodeinnet_p2p::p2p::SyncAction::Push => {
                                                println!(
                                                    "{} Pushing {} ...",
                                                    "🔼".blue(),
                                                    diff.relative_path
                                                );
                                                if let Some(l_path) = &resolved_local_path
                                                    && let Ok(meta) = std::fs::metadata(l_path)
                                                {
                                                    let transfer_id = uuid::Uuid::new_v4();
                                                    let size = meta.len();
                                                    let file_name = l_path
                                                        .file_name()
                                                        .unwrap_or_default()
                                                        .to_string_lossy()
                                                        .to_string();

                                                    pending_uploads.lock().unwrap().insert(
                                                        transfer_id,
                                                        PendingUpload {
                                                            path: l_path.clone(),
                                                            node: target_peer.clone(),
                                                            accepted: false,
                                                        },
                                                    );

                                                    let permissions = {
                                                        #[cfg(unix)]
                                                        {
                                                            use std::os::unix::fs::PermissionsExt;
                                                            std::fs::metadata(l_path)
                                                                .map(|m| {
                                                                    Some(m.permissions().mode())
                                                                })
                                                                .unwrap_or(None)
                                                        }
                                                        #[cfg(not(unix))]
                                                        {
                                                            None
                                                        }
                                                    };

                                                    let msg =
                                                            nodeinnet_p2p::P2pMessage::FileUploadRequest {
                                                                resource_id: resource_id.clone(),
                                                                target_path: format!(
                                                                    "/{}",
                                                                    diff.relative_path
                                                                ),
                                                                file_name,
                                                                total_size: size,
                                                                transfer_id,
                                                                permissions,
                                                            };
                                                    tokio::spawn({
                                                        let n = net_cmd_tx.clone();
                                                        let p = target_peer.clone();
                                                        let m = msg;
                                                        async move {
                                                            let _ = n
                                                                .send(NetCmd::SendToPeer(p, m))
                                                                .await;
                                                        }
                                                    });
                                                    watch_acceptance(app_tx.clone(), transfer_id);
                                                }
                                            }
                                            nodeinnet_p2p::p2p::SyncAction::DeleteLocal => {
                                                println!(
                                                    "{} Deleting Local {} ...",
                                                    "🗑️".red(),
                                                    diff.relative_path
                                                );
                                                if let Some(l_path) = &resolved_local_path
                                                    && l_path.exists()
                                                {
                                                    let _ = std::fs::remove_file(l_path);
                                                }
                                            }
                                            nodeinnet_p2p::p2p::SyncAction::DeleteRemote => {
                                                println!(
                                                    "{} Deleting Remote {} ...",
                                                    "🗑️".red(),
                                                    diff.relative_path
                                                );
                                                let msg =
                                                    nodeinnet_p2p::P2pMessage::DeleteEntryRequest {
                                                        resource_id: resource_id.clone(),
                                                        request_id: uuid::Uuid::new_v4(),
                                                        path: format!("/{}", diff.relative_path),
                                                    };
                                                tokio::spawn({
                                                    let n = net_cmd_tx.clone();
                                                    let p = target_peer.clone();
                                                    let m = msg;
                                                    async move {
                                                        let _ =
                                                            n.send(NetCmd::SendToPeer(p, m)).await;
                                                    }
                                                });
                                            }
                                            _ => {}
                                        }
                                    }
                                }

                                println!(
                                    "{} Scheduled {} operations. Keep the terminal open until transfers complete.",
                                    "✅".green(),
                                    actions_scheduled
                                );
                            }
                        } else {
                            let json =
                                serde_json::to_string_pretty(&folder_states).unwrap_or_default();
                            println!(
                                "\n{} {} (sys_res_id={}):\n{}",
                                "🔄".blue().bold(),
                                "Sync State Response".green().bold(),
                                resource_id.cyan(),
                                json.white()
                            );
                        }
                    }
                    P2pMessage::CreateDirectoryResponse {
                        resource_id,
                        parent_path,
                        result,
                        ..
                    } => {
                        let status = match result {
                            Ok(_) => "Success".green(),
                            Err(e) => format!("Error: {}", e).red(),
                        };
                        println!(
                            "\n{} {} (sys_res_id={}, parent_path={}): {}",
                            "📂".blue().bold(),
                            "Create Directory Response".green().bold(),
                            resource_id.cyan(),
                            parent_path.cyan(),
                            status
                        );
                        std::process::exit(if status.to_string().contains("Error") {
                            1
                        } else {
                            0
                        });
                    }
                    P2pMessage::DeleteEntryResponse {
                        resource_id,
                        parent_path,
                        result,
                        ..
                    } => {
                        let status = match result {
                            Ok(_) => "Success".green(),
                            Err(e) => format!("Error: {}", e).red(),
                        };
                        println!(
                            "\n{} {} (sys_res_id={}, parent_path={}): {}",
                            "🗑️".blue().bold(),
                            "Delete Entry Response".green().bold(),
                            resource_id.cyan(),
                            parent_path.cyan(),
                            status
                        );
                        std::process::exit(if status.to_string().contains("Error") {
                            1
                        } else {
                            0
                        });
                    }
                    P2pMessage::RenameEntryResponse {
                        resource_id,
                        parent_path,
                        result,
                        ..
                    } => {
                        let status = match result {
                            Ok(_) => "Success".green(),
                            Err(e) => format!("Error: {}", e).red(),
                        };
                        println!(
                            "\n{} {} (sys_res_id={}, parent_path={}): {}",
                            "📝".blue().bold(),
                            "Rename Entry Response".green().bold(),
                            resource_id.cyan(),
                            parent_path.cyan(),
                            status
                        );
                        std::process::exit(if status.to_string().contains("Error") {
                            1
                        } else {
                            0
                        });
                    }
                    P2pMessage::RegistryKeysResponse {
                        resource_id,
                        path,
                        subkeys,
                        values,
                        error,
                        ..
                    } => {
                        if let Some(err) = error {
                            println!(
                                "\n{} {} (sys_res_id={}, path={}): {}",
                                "📁".red().bold(),
                                "Registry Error".red().bold(),
                                resource_id.cyan(),
                                path.cyan(),
                                err.red()
                            );
                            std::process::exit(1);
                        } else {
                            let output = serde_json::json!({
                                "subkeys": subkeys,
                                "values": values
                            });
                            let json = serde_json::to_string_pretty(&output).unwrap_or_default();
                            println!(
                                "\n{} {} (sys_res_id={}, path={}):\n{}",
                                "📁".blue().bold(),
                                "Registry Keys".green().bold(),
                                resource_id.cyan(),
                                path.cyan(),
                                json.white()
                            );
                            std::process::exit(0);
                        }
                    }
                    P2pMessage::CreateRegistryKeyResponse {
                        resource_id,
                        parent_path,
                        result,
                        ..
                    } => {
                        let status = match result {
                            Ok(_) => "Success".green(),
                            Err(e) => format!("Error: {}", e).red(),
                        };
                        println!(
                            "\n{} {} (sys_res_id={}, parent_path={}): {}",
                            "📁".blue().bold(),
                            "Create Registry Key Response".green().bold(),
                            resource_id.cyan(),
                            parent_path.cyan(),
                            status
                        );
                        std::process::exit(if status.to_string().contains("Error") {
                            1
                        } else {
                            0
                        });
                    }
                    P2pMessage::DeleteRegistryEntryResponse {
                        resource_id,
                        parent_path,
                        result,
                        ..
                    } => {
                        let status = match result {
                            Ok(_) => "Success".green(),
                            Err(e) => format!("Error: {}", e).red(),
                        };
                        println!(
                            "\n{} {} (sys_res_id={}, parent_path={}): {}",
                            "🗑️".blue().bold(),
                            "Delete Registry Entry Response".green().bold(),
                            resource_id.cyan(),
                            parent_path.cyan(),
                            status
                        );
                        std::process::exit(if status.to_string().contains("Error") {
                            1
                        } else {
                            0
                        });
                    }
                    P2pMessage::SetRegistryValueResponse {
                        resource_id,
                        path,
                        result,
                        ..
                    } => {
                        let status = match result {
                            Ok(_) => "Success".green(),
                            Err(e) => format!("Error: {}", e).red(),
                        };
                        println!(
                            "\n{} {} (sys_res_id={}, path={}): {}",
                            "📝".blue().bold(),
                            "Set Registry Value Response".green().bold(),
                            resource_id.cyan(),
                            path.cyan(),
                            status
                        );
                        std::process::exit(if status.to_string().contains("Error") {
                            1
                        } else {
                            0
                        });
                    }
                    P2pMessage::FileTransferResponse {
                        transfer_id,
                        status,
                    } => match status {
                        nodeinnet_p2p::FileTransferStatus::Accepted { total_size } => {
                            let mut downloads = active_downloads.lock().unwrap();
                            if let Some(local_path) = downloads.get(&transfer_id) {
                                println!(
                                    "{} Download Accepted! Expecting {} bytes...",
                                    "🔽".blue(),
                                    total_size
                                );
                                if let Err(e) = std::fs::File::create(local_path) {
                                    println!("{} Error creating local file: {}", "❌".red(), e);
                                    transfer_failed = true;
                                    downloads.remove(&transfer_id);
                                    if downloads.is_empty()
                                        && pending_uploads.lock().unwrap().is_empty()
                                    {
                                        std::process::exit(1);
                                    }
                                }
                            } else if let Some(upload) = pending_uploads
                                .lock()
                                .unwrap()
                                .get_mut(&transfer_id)
                                .filter(|upload| !upload.accepted)
                            {
                                upload.accepted = true;
                                println!(
                                    "{} Upload Accepted! Streaming {} bytes...",
                                    "🔼".blue(),
                                    total_size
                                );
                                let uploads = pending_uploads.clone();
                                let net_tx = net_tx.clone();
                                let app_tx = app_tx.clone();
                                let (path, node) = (upload.path.clone(), upload.node.clone());
                                tokio::spawn(async move {
                                    let failure = match send_upload(
                                        &net_tx,
                                        &uploads,
                                        &node,
                                        &path,
                                        transfer_id,
                                        total_size,
                                    )
                                    .await
                                    {
                                        Ok(()) => {
                                            tokio::time::sleep(RECEIPT_WAIT).await;
                                            format!(
                                                "the device did not confirm the upload within {} min",
                                                RECEIPT_WAIT.as_secs() / 60
                                            )
                                        }
                                        Err(Some(reason)) => reason,
                                        Err(None) => return,
                                    };
                                    let _ = app_tx.send(AppEvent::P2pMessageReceived(
                                        P2pMessage::FileTransferReceipt {
                                            transfer_id,
                                            result: Err(failure),
                                        },
                                    ));
                                });
                            }
                        }
                        nodeinnet_p2p::FileTransferStatus::Rejected { reason } => {
                            println!("{} Transfer rejected: {}", "❌".red(), reason);
                            transfer_failed = true;
                            let mut downloads = active_downloads.lock().unwrap();
                            downloads.remove(&transfer_id);
                            pending_uploads.lock().unwrap().remove(&transfer_id);
                            if downloads.is_empty() && pending_uploads.lock().unwrap().is_empty() {
                                std::process::exit(1);
                            }
                        }
                    },
                    P2pMessage::FileChunk {
                        transfer_id,
                        offset,
                        data,
                    } => {
                        if let Some(local_path) = active_downloads.lock().unwrap().get(&transfer_id)
                        {
                            use std::io::{Seek, Write};
                            if let Ok(mut file) =
                                std::fs::OpenOptions::new().write(true).open(local_path)
                            {
                                let _ = file.seek(std::io::SeekFrom::Start(offset));
                                let _ = file.write_all(&data);
                            }
                        }
                    }
                    P2pMessage::FileTransferComplete { transfer_id, .. } => {
                        let mut downloads = active_downloads.lock().unwrap();
                        if downloads.remove(&transfer_id).is_some() {
                            println!("\n{} Download Completed Successfully!", "✅".green().bold());
                            if downloads.is_empty() && pending_uploads.lock().unwrap().is_empty() {
                                std::process::exit(if transfer_failed { 1 } else { 0 });
                            }
                        }
                    }
                    P2pMessage::FileTransferReceipt {
                        transfer_id,
                        result,
                    } => finish_upload(
                        &pending_uploads,
                        &active_downloads,
                        &mut transfer_failed,
                        transfer_id,
                        result,
                    ),
                    P2pMessage::TerminalOutput { data, .. } => {
                        use std::io::Write;
                        let mut stdout = std::io::stdout();
                        let _ = stdout.write_all(&data);
                        let _ = stdout.flush();
                    }
                    _ => {
                        println!("{} {:?}", "[P2P RECV]".magenta(), msg);
                    }
                },
                AppEvent::PeerDisconnected(peer_id) => {
                    println!("{} Peer {} disconnected.", "[DISCONNECT]".red(), peer_id);
                }
                AppEvent::UploadUnanswered(transfer_id) => {
                    let unanswered = pending_uploads
                        .lock()
                        .unwrap()
                        .get(&transfer_id)
                        .is_some_and(|upload| !upload.accepted);
                    if unanswered {
                        finish_upload(
                            &pending_uploads,
                            &active_downloads,
                            &mut transfer_failed,
                            transfer_id,
                            Err(format!(
                                "the device did not accept the upload within {} min",
                                RECEIPT_WAIT.as_secs() / 60
                            )),
                        );
                    }
                }
            }
        }
    }
}

fn finish_upload(
    uploads: &Uploads,
    downloads: &std::sync::Mutex<std::collections::HashMap<uuid::Uuid, std::path::PathBuf>>,
    transfer_failed: &mut bool,
    transfer_id: uuid::Uuid,
    result: Result<(), String>,
) {
    if uploads.lock().unwrap().remove(&transfer_id).is_none() {
        return;
    }
    match result {
        Ok(()) => println!("\n{} Upload Completed Successfully!", "✅".green().bold()),
        Err(reason) => {
            println!("\n{} Upload failed: {}", "❌".red(), reason);
            *transfer_failed = true;
        }
    }
    if downloads.lock().unwrap().is_empty() && uploads.lock().unwrap().is_empty() {
        std::process::exit(if *transfer_failed { 1 } else { 0 });
    }
}

pub fn watch_acceptance(app_tx: std::sync::mpsc::Sender<AppEvent>, transfer_id: uuid::Uuid) {
    tokio::spawn(async move {
        tokio::time::sleep(RECEIPT_WAIT).await;
        let _ = app_tx.send(AppEvent::UploadUnanswered(transfer_id));
    });
}

async fn queue_upload(
    net_tx: &tokio::sync::mpsc::Sender<NetCmd>,
    node: &str,
    msg: P2pMessage,
) -> Result<(), Option<String>> {
    match tokio::time::timeout(
        RECEIPT_WAIT,
        net_tx.send(NetCmd::SendToPeer(node.to_string(), msg)),
    )
    .await
    {
        Ok(Ok(())) => Ok(()),
        Ok(Err(_)) => Err(Some("the network stopped".to_string())),
        Err(_) => Err(Some(format!(
            "the upload stalled: nothing left the queue for {} min",
            RECEIPT_WAIT.as_secs() / 60
        ))),
    }
}

async fn send_upload(
    net_tx: &tokio::sync::mpsc::Sender<NetCmd>,
    uploads: &Uploads,
    node: &str,
    path: &std::path::Path,
    transfer_id: uuid::Uuid,
    total_size: u64,
) -> Result<(), Option<String>> {
    use tokio::io::AsyncReadExt;
    let mut unread = None;
    match tokio::fs::File::open(path).await {
        Ok(mut file) => {
            let mut buffer = vec![0u8; 16 * 1024];
            let mut offset = 0u64;
            loop {
                if !uploads.lock().unwrap().contains_key(&transfer_id) {
                    return Err(None);
                }
                match file.read(&mut buffer).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = P2pMessage::FileChunk {
                            transfer_id,
                            offset,
                            data: buffer[..n].to_vec(),
                        };
                        queue_upload(net_tx, node, chunk).await?;
                        offset += n as u64;

                        let progress = (offset as f64 / total_size as f64) * 100.0;
                        print!("\r{} Progress: {:.2}%", "[TRANS]".blue(), progress);
                        std::io::Write::flush(&mut std::io::stdout()).unwrap();
                    }
                    Err(e) => {
                        unread = Some(e.to_string());
                        break;
                    }
                }
            }
        }
        Err(e) => unread = Some(e.to_string()),
    }
    let complete = P2pMessage::FileTransferComplete {
        transfer_id,
        checksum: None,
    };
    queue_upload(net_tx, node, complete).await?;
    unread.map_or(Ok(()), |reason| Err(Some(reason)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done() {
            assert!(Instant::now() < deadline, "gave up waiting for {what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn remote(relative_path: &str) -> nodeinnet_p2p::p2p::SyncFileInfo {
        nodeinnet_p2p::p2p::SyncFileInfo {
            relative_path: relative_path.to_string(),
            md5_hash: "remote".to_string(),
            size: 3,
            last_modified_ms: 0,
        }
    }

    fn scratch() -> Scratch {
        Scratch(std::env::temp_dir().join(format!("console-app-sync-{}", uuid::Uuid::new_v4())))
    }

    fn sync_folder(name: &str, path: &std::path::Path) -> nodeinnet_p2p::SharedResource {
        nodeinnet_p2p::SharedResource {
            id: name.into(),
            name: name.into(),
            resource_type: nodeinnet_p2p::ResourceType::SyncFolder,
            config: Some(serde_json::json!({ "path": path }).to_string()),
            is_active: true,
            session_token: None,
        }
    }

    type Downloads =
        std::sync::Arc<std::sync::Mutex<std::collections::HashMap<uuid::Uuid, PathBuf>>>;

    fn start(
        resources: Vec<nodeinnet_p2p::SharedResource>,
        folder: &str,
        strategy: &str,
    ) -> (
        std::sync::mpsc::Sender<AppEvent>,
        Downloads,
        tokio::sync::mpsc::Receiver<NetCmd>,
    ) {
        let config = client_config::AppConfig::new("console-app-sync-test");
        config.set("app.resources", resources);

        let (app_tx, app_rx) = std::sync::mpsc::channel();
        let (net_tx, net_rx) = tokio::sync::mpsc::channel(16);
        let active_downloads: Downloads = Default::default();
        let ctx = EventLoopContext {
            app_rx,
            refresh_token: String::new(),
            net_tx,
            active_downloads: active_downloads.clone(),
            pending_uploads: Default::default(),
            active_sync_strategy: std::sync::Arc::new(std::sync::Mutex::new(Some((
                "peer".to_string(),
                folder.to_string(),
                strategy.to_string(),
            )))),
            known_peers: Default::default(),
            my_info: NodeInfo {
                id: "me".into(),
                name: "me".into(),
                os: std::env::consts::OS.into(),
                version: String::new(),
                app_type: String::new(),
                build_type: String::new(),
                public_key: String::new(),
                resources: vec![],
                is_online: true,
                last_used: 0,
                is_temporary: false,
            },
            ws_base: String::new(),
            my_node_id: "me".into(),
            exec_cmd: None,
            app_tx: app_tx.clone(),
            api_base: String::new(),
            client_http: reqwest::Client::new(),
            reconnect_attempt: 0,
            reconnect_timeouts: vec![1],
            config,
        };
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        std::thread::spawn(move || {
            rt.block_on(async move { run_event_loop(ctx) });
        });
        (app_tx, active_downloads, net_rx)
    }

    #[test]
    fn a_pull_cannot_write_outside_the_sync_folder() {
        let scratch = scratch();
        let root = scratch.0.join("docs");
        std::fs::create_dir_all(&root).unwrap();
        let ok = root.join("ok.txt");
        std::fs::write(&ok, b"old").unwrap();
        let above = scratch.0.join("above.txt");
        let elsewhere = scratch.0.join("elsewhere.txt");
        std::fs::write(&above, b"original").unwrap();
        std::fs::write(&elsewhere, b"original").unwrap();
        let (app_tx, active_downloads, _net_rx) =
            start(vec![sync_folder("docs", &root)], "docs", "pull");

        let response = P2pMessage::SyncStateResponse {
            resource_id: "docs".into(),
            available_folders: ["docs".to_string()].into(),
            folder_states: [(
                "docs".to_string(),
                vec![
                    remote("../above.txt"),
                    remote(&elsewhere.to_string_lossy()),
                    remote("ok.txt"),
                ],
            )]
            .into(),
        };
        app_tx.send(AppEvent::P2pMessageReceived(response)).unwrap();
        wait_for("the pull of ok.txt", || {
            active_downloads.lock().unwrap().values().any(|p| *p == ok)
        });

        let pulls = active_downloads.lock().unwrap().clone();
        let chunk = |transfer_id: uuid::Uuid, data: &[u8]| {
            app_tx
                .send(AppEvent::P2pMessageReceived(P2pMessage::FileChunk {
                    transfer_id,
                    offset: 0,
                    data: data.to_vec(),
                }))
                .unwrap();
        };
        for (transfer_id, path) in &pulls {
            if *path != ok {
                chunk(*transfer_id, b"pwned");
            }
        }
        for (transfer_id, path) in &pulls {
            if *path == ok {
                chunk(*transfer_id, b"new");
            }
        }
        wait_for("ok.txt to arrive", || {
            std::fs::read_to_string(&ok).unwrap() == "new"
        });

        assert_eq!(
            [
                std::fs::read_to_string(&above).unwrap(),
                std::fs::read_to_string(&elsewhere).unwrap(),
            ],
            ["original", "original"]
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_pull_skips_names_windows_would_rewrite() {
        let scratch = scratch();
        let root = scratch.0.join("docs");
        std::fs::create_dir_all(&root).unwrap();
        let ok = root.join("ok.txt");
        let (app_tx, active_downloads, _net_rx) =
            start(vec![sync_folder("docs", &root)], "docs", "pull");

        let response = P2pMessage::SyncStateResponse {
            resource_id: "docs".into(),
            available_folders: ["docs".to_string()].into(),
            folder_states: [(
                "docs".to_string(),
                vec![
                    remote("12:30.log"),
                    remote("notes."),
                    remote("sub/nul"),
                    remote("com1.txt"),
                    remote("ok.txt"),
                ],
            )]
            .into(),
        };
        app_tx.send(AppEvent::P2pMessageReceived(response)).unwrap();
        wait_for("the pull of ok.txt", || {
            active_downloads.lock().unwrap().values().any(|p| *p == ok)
        });

        assert_eq!(
            active_downloads
                .lock()
                .unwrap()
                .values()
                .collect::<Vec<_>>(),
            [&ok]
        );
    }

    #[test]
    fn a_pulled_new_file_arrives() {
        let scratch = scratch();
        let root = scratch.0.join("docs");
        std::fs::create_dir_all(&root).unwrap();
        let (app_tx, active_downloads, _net_rx) =
            start(vec![sync_folder("docs", &root)], "docs", "pull");

        let response = P2pMessage::SyncStateResponse {
            resource_id: "docs".into(),
            available_folders: ["docs".to_string()].into(),
            folder_states: [("docs".to_string(), vec![remote("new.txt")])].into(),
        };
        app_tx.send(AppEvent::P2pMessageReceived(response)).unwrap();
        let new = root.join("new.txt");
        wait_for("the pull of new.txt", || {
            active_downloads.lock().unwrap().values().any(|p| *p == new)
        });

        let transfer_id = *active_downloads.lock().unwrap().keys().next().unwrap();
        for msg in [
            P2pMessage::FileTransferResponse {
                transfer_id,
                status: nodeinnet_p2p::FileTransferStatus::Accepted { total_size: 3 },
            },
            P2pMessage::FileChunk {
                transfer_id,
                offset: 0,
                data: b"new".to_vec(),
            },
        ] {
            app_tx.send(AppEvent::P2pMessageReceived(msg)).unwrap();
        }
        wait_for("new.txt to arrive", || {
            std::fs::read(&new).is_ok_and(|bytes| bytes == b"new")
        });
    }

    #[test]
    fn a_mirror_pull_leaves_the_other_sync_folders_alone() {
        let scratch = scratch();
        let docs = scratch.0.join("docs");
        let notes = scratch.0.join("notes");
        for dir in [&docs, &notes] {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("README.md"), b"same").unwrap();
        }
        let resources = vec![sync_folder("docs", &docs), sync_folder("notes", &notes)];
        let mut peer = node_functions::sync_folder::scan_local_resource_folders(&resources);
        peer.retain(|name, _| name == "docs");
        let (app_tx, active_downloads, _net_rx) = start(resources, "docs", "mirror_pull");

        let marker = scratch.0.join("marker");
        std::fs::write(&marker, b"").unwrap();
        let after = uuid::Uuid::new_v4();
        active_downloads
            .lock()
            .unwrap()
            .insert(after, marker.clone());
        for msg in [
            P2pMessage::SyncStateResponse {
                resource_id: "docs".into(),
                available_folders: ["docs".to_string()].into(),
                folder_states: peer,
            },
            P2pMessage::FileChunk {
                transfer_id: after,
                offset: 0,
                data: b"done".to_vec(),
            },
        ] {
            app_tx.send(AppEvent::P2pMessageReceived(msg)).unwrap();
        }
        wait_for("the sync pass", || {
            std::fs::read(&marker).is_ok_and(|bytes| bytes == b"done")
        });

        assert!(docs.join("README.md").exists());
        assert!(notes.join("README.md").exists());
    }

    #[test]
    fn every_pull_lands_before_the_console_exits() {
        let Some(dir) = std::env::var_os("CONSOLE_APP_PULLS") else {
            let scratch = scratch();
            let root = scratch.0.join("docs");
            std::fs::create_dir_all(&root).unwrap();
            for name in ["a.bin", "b.bin"] {
                std::fs::write(root.join(name), b"old").unwrap();
            }
            let run = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "event_loop::tests::every_pull_lands_before_the_console_exits",
                ])
                .env("CONSOLE_APP_PULLS", &root)
                .output()
                .unwrap();
            assert_eq!(
                (
                    run.status.code(),
                    std::fs::read(root.join("b.bin")).unwrap()
                ),
                (Some(1), b"new b".to_vec()),
                "{}",
                String::from_utf8_lossy(&run.stdout)
            );
            return;
        };
        let root = PathBuf::from(dir);
        let (app_tx, active_downloads, _net_rx) =
            start(vec![sync_folder("docs", &root)], "docs", "pull");

        let response = P2pMessage::SyncStateResponse {
            resource_id: "docs".into(),
            available_folders: ["docs".to_string()].into(),
            folder_states: [(
                "docs".to_string(),
                ["a.bin", "b.bin", "sub/c.bin", "d.bin"]
                    .map(remote)
                    .to_vec(),
            )]
            .into(),
        };
        app_tx.send(AppEvent::P2pMessageReceived(response)).unwrap();
        wait_for("four pulls", || active_downloads.lock().unwrap().len() == 4);

        let id = |name: &str| {
            let pulls = active_downloads.lock().unwrap();
            *pulls.iter().find(|(_, p)| p.ends_with(name)).unwrap().0
        };
        let (a, b, c, d) = (id("a.bin"), id("b.bin"), id("c.bin"), id("d.bin"));
        let response = |transfer_id, status| P2pMessage::FileTransferResponse {
            transfer_id,
            status,
        };
        let accepted = nodeinnet_p2p::FileTransferStatus::Accepted { total_size: 5 };
        let chunk = |transfer_id, offset, data: &[u8]| P2pMessage::FileChunk {
            transfer_id,
            offset,
            data: data.to_vec(),
        };
        let complete = |transfer_id| P2pMessage::FileTransferComplete {
            transfer_id,
            checksum: None,
        };
        for msg in [
            response(a, accepted.clone()),
            response(b, accepted.clone()),
            chunk(a, 0, b"new a"),
            chunk(b, 0, b"ne"),
            response(c, accepted),
            response(
                d,
                nodeinnet_p2p::FileTransferStatus::Rejected {
                    reason: "gone".into(),
                },
            ),
            complete(a),
            chunk(b, 2, b"w b"),
            complete(b),
        ] {
            app_tx.send(AppEvent::P2pMessageReceived(msg)).unwrap();
        }
        wait_for("the console to exit", || false);
    }

    fn rerun(test: &str, var: &str, root: &std::path::Path) -> (Option<i32>, String) {
        let run = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--nocapture"])
            .env(var, root)
            .output()
            .unwrap();
        (
            run.status.code(),
            String::from_utf8_lossy(&run.stdout).into_owned(),
        )
    }

    fn next_sent(net_rx: &mut tokio::sync::mpsc::Receiver<NetCmd>) -> P2pMessage {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match net_rx.try_recv() {
                Ok(NetCmd::SendToPeer(_, msg)) => return msg,
                Ok(_) => {}
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                    assert!(Instant::now() < deadline, "the console sent nothing");
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => panic!("the console's network channel closed: {e}"),
            }
        }
    }

    fn stream_ends(net_rx: &mut tokio::sync::mpsc::Receiver<NetCmd>) -> Vec<u8> {
        let mut bytes = Vec::new();
        loop {
            match next_sent(net_rx) {
                P2pMessage::FileChunk { data, .. } => bytes.extend(data),
                P2pMessage::FileTransferComplete { .. } => return bytes,
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    const ANSWERED: &str = "the receipt goes out now";

    fn push_answered_with(dir: std::ffi::OsString, result: Result<(), String>) {
        let root = PathBuf::from(dir);
        let (app_tx, _downloads, mut net_rx) =
            start(vec![sync_folder("docs", &root)], "docs", "push");
        let empty = P2pMessage::SyncStateResponse {
            resource_id: "docs".into(),
            available_folders: ["docs".to_string()].into(),
            folder_states: [("docs".to_string(), vec![])].into(),
        };
        app_tx.send(AppEvent::P2pMessageReceived(empty)).unwrap();
        let P2pMessage::FileUploadRequest { transfer_id, .. } = next_sent(&mut net_rx) else {
            panic!("the push was not announced");
        };
        app_tx
            .send(AppEvent::P2pMessageReceived(
                P2pMessage::FileTransferResponse {
                    transfer_id,
                    status: nodeinnet_p2p::FileTransferStatus::Accepted { total_size: 3 },
                },
            ))
            .unwrap();
        assert_eq!(stream_ends(&mut net_rx), b"new");

        std::thread::sleep(Duration::from_millis(500));
        println!("{ANSWERED}");
        app_tx
            .send(AppEvent::P2pMessageReceived(
                P2pMessage::FileTransferReceipt {
                    transfer_id,
                    result,
                },
            ))
            .unwrap();
        wait_for("the console to exit", || false);
    }

    #[test]
    fn a_push_exits_zero_only_after_the_peer_confirms_it() {
        const VAR: &str = "CONSOLE_APP_PUSH_OK";
        let Some(dir) = std::env::var_os(VAR) else {
            let scratch = scratch();
            let root = scratch.0.join("docs");
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join("a.bin"), b"new").unwrap();
            let (code, stdout) = rerun(
                "event_loop::tests::a_push_exits_zero_only_after_the_peer_confirms_it",
                VAR,
                &root,
            );
            assert_eq!(
                (code, stdout.contains(ANSWERED)),
                (Some(0), true),
                "{stdout}"
            );
            return;
        };
        push_answered_with(dir, Ok(()));
    }

    #[test]
    fn a_push_the_peer_did_not_store_exits_one() {
        const VAR: &str = "CONSOLE_APP_PUSH_ERR";
        let Some(dir) = std::env::var_os(VAR) else {
            let scratch = scratch();
            let root = scratch.0.join("docs");
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join("a.bin"), b"new").unwrap();
            let (code, stdout) = rerun(
                "event_loop::tests::a_push_the_peer_did_not_store_exits_one",
                VAR,
                &root,
            );
            assert_eq!(
                (code, stdout.contains(ANSWERED)),
                (Some(1), true),
                "{stdout}"
            );
            assert!(stdout.contains("No space left on device"), "{stdout}");
            return;
        };
        push_answered_with(dir, Err("No space left on device (os error 28)".into()));
    }

    #[test]
    fn the_console_waits_for_every_push_and_pull() {
        const VAR: &str = "CONSOLE_APP_MIXED";
        let Some(dir) = std::env::var_os(VAR) else {
            let scratch = scratch();
            let root = scratch.0.join("docs");
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join("a.bin"), b"new a").unwrap();
            let (code, stdout) = rerun(
                "event_loop::tests::the_console_waits_for_every_push_and_pull",
                VAR,
                &root,
            );
            assert_eq!(
                (code, stdout.contains(ANSWERED)),
                (Some(0), true),
                "{stdout}"
            );
            assert_eq!(std::fs::read(root.join("b.bin")).unwrap(), b"new b");
            return;
        };
        let root = PathBuf::from(dir);
        let (app_tx, _downloads, mut net_rx) = start(
            vec![sync_folder("docs", &root)],
            "docs",
            "push-local-new,pull-remote-new",
        );
        let remote_b = P2pMessage::SyncStateResponse {
            resource_id: "docs".into(),
            available_folders: ["docs".to_string()].into(),
            folder_states: [("docs".to_string(), vec![remote("b.bin")])].into(),
        };
        app_tx.send(AppEvent::P2pMessageReceived(remote_b)).unwrap();
        let (mut push, mut pull) = (None, None);
        while push.is_none() || pull.is_none() {
            match next_sent(&mut net_rx) {
                P2pMessage::FileUploadRequest { transfer_id, .. } => push = Some(transfer_id),
                P2pMessage::FileDownloadRequest { transfer_id, .. } => pull = Some(transfer_id),
                other => panic!("unexpected {other:?}"),
            }
        }
        let (push, pull) = (push.unwrap(), pull.unwrap());
        let accepted = nodeinnet_p2p::FileTransferStatus::Accepted { total_size: 5 };
        let send = |msg| app_tx.send(AppEvent::P2pMessageReceived(msg)).unwrap();
        send(P2pMessage::FileTransferResponse {
            transfer_id: push,
            status: accepted.clone(),
        });
        assert_eq!(stream_ends(&mut net_rx), b"new a");
        for msg in [
            P2pMessage::FileTransferResponse {
                transfer_id: pull,
                status: accepted,
            },
            P2pMessage::FileChunk {
                transfer_id: pull,
                offset: 0,
                data: b"new b".to_vec(),
            },
            P2pMessage::FileTransferComplete {
                transfer_id: pull,
                checksum: None,
            },
            P2pMessage::FileTransferReceipt {
                transfer_id: uuid::Uuid::new_v4(),
                result: Err("not ours".into()),
            },
        ] {
            send(msg);
        }

        std::thread::sleep(Duration::from_millis(500));
        println!("{ANSWERED}");
        send(P2pMessage::FileTransferReceipt {
            transfer_id: push,
            result: Ok(()),
        });
        wait_for("the console to exit", || false);
    }

    #[test]
    fn a_push_whose_request_could_not_go_out_exits_one() {
        const VAR: &str = "CONSOLE_APP_PUSH_UNSENT";
        let Some(dir) = std::env::var_os(VAR) else {
            let scratch = scratch();
            let root = scratch.0.join("docs");
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join("a.bin"), b"new").unwrap();
            let (code, stdout) = rerun(
                "event_loop::tests::a_push_whose_request_could_not_go_out_exits_one",
                VAR,
                &root,
            );
            assert_eq!(code, Some(1), "{stdout}");
            assert!(stdout.contains("no connection to the device"), "{stdout}");
            return;
        };
        let root = PathBuf::from(dir);
        let (app_tx, _downloads, mut net_rx) =
            start(vec![sync_folder("docs", &root)], "docs", "push");
        let empty = P2pMessage::SyncStateResponse {
            resource_id: "docs".into(),
            available_folders: ["docs".to_string()].into(),
            folder_states: [("docs".to_string(), vec![])].into(),
        };
        app_tx.send(AppEvent::P2pMessageReceived(empty)).unwrap();
        let P2pMessage::FileUploadRequest { transfer_id, .. } = next_sent(&mut net_rx) else {
            panic!("the push was not announced");
        };
        app_tx
            .send(AppEvent::P2pMessageReceived(
                P2pMessage::FileTransferReceipt {
                    transfer_id,
                    result: Err("no connection to the device".into()),
                },
            ))
            .unwrap();
        wait_for("the console to exit", || false);
    }

    #[tokio::test(start_paused = true)]
    async fn a_push_nobody_accepts_is_given_up_only_after_an_hour() {
        let (app_tx, app_rx) = std::sync::mpsc::channel();
        let transfer_id = uuid::Uuid::new_v4();

        watch_acceptance(app_tx, transfer_id);
        tokio::time::sleep(RECEIPT_WAIT - Duration::from_secs(1)).await;
        assert!(app_rx.try_recv().is_err(), "the push was given up early");
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(matches!(
            app_rx.try_recv(),
            Ok(AppEvent::UploadUnanswered(id)) if id == transfer_id
        ));
    }
}
