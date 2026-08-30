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
}

pub struct EventLoopContext {
    pub app_rx: std::sync::mpsc::Receiver<AppEvent>,
    pub refresh_token: String,
    pub net_tx: tokio::sync::mpsc::Sender<NetCmd>,
    pub active_downloads:
        std::sync::Arc<std::sync::Mutex<std::collections::HashMap<uuid::Uuid, std::path::PathBuf>>>,
    pub pending_uploads: std::sync::Arc<
        tokio::sync::Mutex<std::collections::HashMap<uuid::Uuid, (std::path::PathBuf, String)>>,
    >,
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
                                    {
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

                                                    let pu = pending_uploads.clone();
                                                    let l_path_clone = l_path.clone();
                                                    let t_peer = target_peer.clone();
                                                    tokio::spawn(async move {
                                                        pu.lock().await.insert(
                                                            transfer_id,
                                                            (l_path_clone, t_peer),
                                                        );
                                                    });

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
                            let mut is_upload = false;
                            let mut upload_node = String::new();
                            let mut upload_path = std::path::PathBuf::new();

                            if let Some((local_path, node_id)) =
                                pending_uploads.blocking_lock().remove(&transfer_id)
                            {
                                is_upload = true;
                                upload_node = node_id;
                                upload_path = local_path;
                            }

                            if is_upload {
                                println!(
                                    "{} Upload Accepted! Streaming {} bytes...",
                                    "🔼".blue(),
                                    total_size
                                );
                                let net_tx_upload = net_tx.clone();
                                tokio::spawn(async move {
                                    if let Ok(mut file) = tokio::fs::File::open(&upload_path).await
                                    {
                                        use tokio::io::AsyncReadExt;
                                        let mut buffer = vec![0u8; 16 * 1024];
                                        let mut offset = 0u64;
                                        loop {
                                            match file.read(&mut buffer).await {
                                                Ok(0) => break,
                                                Ok(n) => {
                                                    let chunk =
                                                        nodeinnet_p2p::P2pMessage::FileChunk {
                                                            transfer_id,
                                                            offset,
                                                            data: buffer[..n].to_vec(),
                                                        };
                                                    let _ = net_tx_upload
                                                        .send(NetCmd::SendToPeer(
                                                            upload_node.clone(),
                                                            chunk,
                                                        ))
                                                        .await;
                                                    offset += n as u64;

                                                    let progress =
                                                        (offset as f64 / total_size as f64) * 100.0;
                                                    print!(
                                                        "\r{} Progress: {:.2}%",
                                                        "[TRANS]".blue(),
                                                        progress
                                                    );
                                                    std::io::Write::flush(&mut std::io::stdout())
                                                        .unwrap();
                                                }
                                                Err(_) => break,
                                            }
                                        }
                                        let comp =
                                            nodeinnet_p2p::P2pMessage::FileTransferComplete {
                                                transfer_id,
                                                checksum: None,
                                            };
                                        let _ = net_tx_upload
                                            .send(NetCmd::SendToPeer(upload_node, comp))
                                            .await;
                                        println!(
                                            "\n{} Upload Completed Successfully!",
                                            "✅".green().bold()
                                        );
                                        std::process::exit(0);
                                    }
                                });
                            } else if let Some(local_path) =
                                active_downloads.lock().unwrap().get(&transfer_id)
                            {
                                println!(
                                    "{} Download Accepted! Expecting {} bytes...",
                                    "🔽".blue(),
                                    total_size
                                );
                                if let Err(e) = std::fs::File::create(local_path) {
                                    println!("{} Error creating local file: {}", "❌".red(), e);
                                    std::process::exit(1);
                                }
                            }
                        }
                        nodeinnet_p2p::FileTransferStatus::Rejected { reason } => {
                            println!("{} Transfer rejected: {}", "❌".red(), reason);
                            std::process::exit(1);
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
                        if active_downloads
                            .lock()
                            .unwrap()
                            .remove(&transfer_id)
                            .is_some()
                        {
                            println!("\n{} Download Completed Successfully!", "✅".green().bold());
                            std::process::exit(0);
                        }
                    }
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
            }
        }
    }
}
