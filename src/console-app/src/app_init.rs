use crate::event_loop::{AppEvent, ConsoleEventHandler};

use client_core::{AppEventHandler, NetCmd};
use colored::*;
use nodeinnet_p2p::{NodeInfo, SharedResource};
use std::sync::mpsc;
use tokio::sync::mpsc as tokio_mpsc;

pub async fn start_application(args: crate::cli::Args) {
    std::panic::set_hook(Box::new(|info| {
        let payload = info.payload();
        let msg = if let Some(s) = payload.downcast_ref::<&str>() {
            *s
        } else if let Some(s) = payload.downcast_ref::<String>() {
            s.as_str()
        } else {
            "Unknown panic payload"
        };

        let location = info.location().unwrap();
        let panic_info = format!(
            "\n\n======================================================\n\
             PANIC (console app)\n\
             ======================================================\n\
             Message : {}\n\
             File    : {}\n\
             Line    : {}\n\
             ======================================================\n",
            msg,
            location.file(),
            location.line()
        );

        eprintln!("{}", panic_info.red().bold());
        std::process::exit(1);
    }));
    #[cfg(debug_assertions)]
    let app_title = format!(
        " 🚀 NodeInNet Console Node v{} [DEV] starting...",
        app_version::APP_VERSION
    );
    #[cfg(not(debug_assertions))]
    let app_title = format!(
        " 🚀 NodeInNet Console Node v{} starting...",
        app_version::APP_VERSION
    );

    println!(
        "{}",
        "==============================================="
            .blue()
            .bold()
    );
    println!("{}", app_title.green().bold());
    println!(
        "{}",
        "==============================================="
            .blue()
            .bold()
    );

    if args.watch_bin {
        println!(
            "{} Binary modification monitoring enabled. Will exit automatically upon update.",
            "[SYS]".bright_black()
        );

        tokio::spawn(async move {
            if let Ok(exe_path) = std::env::current_exe()
                && let Ok(initial_meta) = std::fs::metadata(&exe_path)
                && let Ok(initial_time) = initial_meta.modified()
            {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(300)).await;

                    if let Ok(current_meta) = std::fs::metadata(&exe_path)
                        && let Ok(current_time) = current_meta.modified()
                        && current_time != initial_time
                    {
                        println!(
                            "\n{} Binary update detected! Exiting process for service restart...",
                            "⚠️ [SYS]".yellow().bold()
                        );
                        std::process::exit(0);
                    }
                }
            }
        });
    }

    let api_base_owned =
        std::env::var("NODEINNET_API").unwrap_or_else(|_| nodeinnet_p2p::API_BASE.to_string());
    let api_base = api_base_owned.as_str();

    let config = client_config::AppConfig::new("console-app");
    let mut private_key_b64 = config.get::<String>("app.private_key_b64");
    let mut public_key_b64 = config.get::<String>("app.public_key_b64");

    if private_key_b64.is_none() || public_key_b64.is_none() {
        let (priv_key, pub_key) = nodeinnet_p2p::crypto::generate_ed25519_keypair();
        private_key_b64 = Some(priv_key.clone());
        public_key_b64 = Some(pub_key.clone());
        config.set("app.private_key_b64", priv_key);
        config.set("app.public_key_b64", pub_key);
        config.save();
        println!(
            "{} Generated new local Ed25519 keypair for Zero-Trust P2P.",
            "🔐".green()
        );
    }

    let my_node_id = if let Some(id) = config.get::<String>("app.node_id") {
        id
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        config.set("app.node_id", id.clone());
        config.save();
        id
    };

    let my_public_key = public_key_b64.unwrap();
    let resources = config
        .get::<Vec<nodeinnet_p2p::SharedResource>>("app.resources")
        .unwrap_or_default();

    let _my_info = NodeInfo {
        id: my_node_id.clone(),
        name: hostname::get()
            .ok()
            .and_then(|s| s.into_string().ok())
            .unwrap_or_else(|| "Console Node".to_string()),
        os: std::env::consts::OS.to_string(),
        version: app_version::APP_VERSION.to_string(),
        app_type: app_version::APP_TYPE.to_string(),
        build_type: app_version::BUILD_TYPE.to_string(),
        public_key: my_public_key.clone(),
        resources,
        is_online: true,
        last_used: 0,
        is_temporary: args.guest,
    };

    let mut refresh_token = config
        .get::<String>("app.refresh_token")
        .unwrap_or_default();

    let client_http = reqwest::Client::new();

    if refresh_token.is_empty() {
        println!(
            "\n{}",
            "==============================================="
                .blue()
                .bold()
        );
        println!("{}", "Authentication Required".yellow().bold());
        println!(
            "{}\n",
            "==============================================="
                .blue()
                .bold()
        );

        print!("Email/Login: ");
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
        let mut login = String::new();
        std::io::stdin().read_line(&mut login).unwrap();
        let login = login.trim().to_string();

        let password = rpassword::prompt_password("Password: ").unwrap();

        println!("{} Authenticating...", "🔄".bright_black());

        if let Ok(resp) = client_http
            .post(format!("{}/account/login", api_base))
            .json(&nodeinnet_p2p::LoginRequest {
                login: login.clone(),
                password,
                region: config.turn_region(),
            })
            .send()
            .await
        {
            if resp.status().is_success() {
                let json: nodeinnet_p2p::LoginResponse = resp.json().await.unwrap();
                refresh_token = json.refresh_token.clone();
                config.set("app.refresh_token", refresh_token.clone());
                config.set("app.account_login", login.clone());
                config.set_turn_region(json.turn_region);
                config.save();
                println!("{} Login successful! Token saved.", "✅".green().bold());
            } else {
                println!("{} Invalid login or password.", "❌".red().bold());
                std::process::exit(1);
            }
        } else {
            println!("{} Failed to connect to server.", "❌".red().bold());
            std::process::exit(1);
        }
    }

    let (app_tx, app_rx) = mpsc::channel::<AppEvent>();
    let (net_tx, net_rx) = tokio_mpsc::channel::<NetCmd>(32);
    let net_tx_bg = net_tx.clone();

    let my_hostname = hostname::get()
        .ok()
        .and_then(|s| s.into_string().ok())
        .unwrap_or_else(|| "Console Node".to_string());

    println!("{} {}", "📌 Node ID:".cyan().bold(), my_node_id);
    println!("{} {}", "🔑 Target Env:".cyan().bold(), api_base);

    let reconnect_timeouts = [5, 10, 20, 30, 60, 120];
    let mut reconnect_attempt = 0;

    let (access_token, my_info, ws_base, turn_credentials) = loop {
        println!(
            "{} Authenticating via Refresh Token...",
            "🔄".bright_black()
        );

        let _login_for_auth = config
            .get::<String>("app.account_login")
            .unwrap_or_default();
        let auth_res =
            client_core::auth::refresh_access_token(api_base, &refresh_token, config.turn_region())
                .await;

        match auth_res {
            Ok(refresh_resp) => {
                config.set_turn_region(refresh_resp.turn_region);
                println!(
                    "[{}] [APP LOG] 🛠️ Received Auth Config -> Premium: {}, TURN Provided: {}",
                    chrono::Local::now().format("%H:%M:%S"),
                    refresh_resp.premium,
                    refresh_resp.turn.is_some()
                );
                if let Some(turn) = &refresh_resp.turn {
                    println!(
                        "[{}] [APP LOG] 🛠️ TURN Details -> Username: {}, URI: {:?}",
                        chrono::Local::now().format("%H:%M:%S"),
                        turn.username,
                        turn.uris
                    );
                }

                if refresh_resp.refresh_token != refresh_token {
                    #[cfg(not(debug_assertions))]
                    {
                        refresh_token = refresh_resp.refresh_token.clone();
                        config.set("app.refresh_token", refresh_token.clone());
                        config.save();
                        println!(
                            "{} Persisted fresh rotation of Refresh Token!",
                            "🔑".green().bold()
                        );
                    }
                }

                let mut existing_device_id = None;
                let mut needs_config_save = false;

                #[derive(serde::Serialize)]
                struct ResourceWrapper {
                    display_name: Option<String>,
                    os: Option<String>,
                    app_type: Option<String>,
                    version: Option<String>,
                    resources: Vec<SharedResource>,
                }

                if let Some(device) = refresh_resp.devices.iter().find(|d| d.name == my_node_id) {
                    existing_device_id = Some(device.id.clone());
                }

                let stored = config.get::<Vec<SharedResource>>("app.resources");
                let has_local_config = stored.is_some();
                let mut my_resources = stored.unwrap_or_default();

                let added_system_info = crate::setup::ensure_system_info_shared(&mut my_resources);

                let needs_interactive_setup = args.setup || !has_local_config;
                let mut needs_server_sync = has_local_config && added_system_info;
                if needs_server_sync {
                    config.set("app.resources", my_resources.clone());
                    needs_config_save = true;
                }

                if needs_interactive_setup {
                    crate::setup::run_interactive_setup(&mut my_resources, &config);
                    config.set("app.resources", my_resources.clone());
                    needs_config_save = true;
                    needs_server_sync = true;
                } else if existing_device_id.is_none() {
                    needs_server_sync = true;
                }

                let peer_name = config
                    .get::<String>("app-console-name")
                    .unwrap_or_else(|| format!("{} (CLI)", my_hostname));

                if needs_server_sync && !args.guest {
                    let wrapper = ResourceWrapper {
                        display_name: Some(peer_name.clone()),
                        os: Some(std::env::consts::OS.to_string()),
                        app_type: Some(app_version::APP_TYPE.to_string()),
                        version: Some(app_version::APP_VERSION.to_string()),
                        resources: my_resources.iter().map(|r| r.without_config()).collect(),
                    };
                    let bson_bytes = bson::ser::serialize_to_vec(&wrapper).unwrap_or_default();

                    let mut req = client_http.post(format!("{}/account/devices", api_base));
                    if let Some(ref dev_id) = existing_device_id {
                        req = client_http.put(format!("{}/account/devices/{}", api_base, dev_id));
                    }

                    let res = req
                        .bearer_auth(&refresh_resp.access_token)
                        .json(&serde_json::json!({
                            "name": my_node_id,
                            "resources": bson_bytes
                        }))
                        .send()
                        .await;

                    match res {
                        Ok(resp) if resp.status().is_success() => {
                            println!(
                                "{} Device configuration saved to server!",
                                "✅".green().bold()
                            );
                        }
                        Ok(resp) => {
                            let status = resp.status();
                            let err_body = resp.text().await.unwrap_or_default();
                            println!(
                                "{} Warning: Could not save device config. HTTP {}: {}",
                                "⚠️".yellow().bold(),
                                status,
                                err_body
                            );
                        }
                        Err(e) => {
                            println!(
                                "{} Warning: Could not save device config. Request failed: {}",
                                "⚠️".yellow().bold(),
                                e
                            );
                        }
                    }
                }

                if needs_config_save {
                    config.save();
                }

                for r in my_resources.iter_mut() {
                    let Some(base) = nodeinnet_p2p::resource_id_base(&r.resource_type) else {
                        continue;
                    };
                    r.id = format!("{base}-{my_node_id}");
                }

                let my_info = NodeInfo {
                    id: my_node_id.clone(),
                    name: peer_name,
                    os: std::env::consts::OS.to_string(),
                    version: format!("{}-cli", app_version::APP_VERSION),
                    app_type: app_version::APP_TYPE.to_string(),
                    build_type: app_version::BUILD_TYPE.to_string(),
                    public_key: my_public_key.clone(),
                    resources: my_resources,
                    is_online: true,
                    last_used: 0,
                    is_temporary: args.guest,
                };

                reconnect_attempt = 0;
                break (
                    refresh_resp.access_token,
                    my_info,
                    refresh_resp.ws_url,
                    refresh_resp.turn,
                );
            }
            Err(e) => {
                if e.contains("401") || e.contains("403") {
                    println!(
                        "\n{} Refresh token is invalid or expired (401/403).",
                        "❌".red().bold()
                    );
                    println!("Please restart the application to log in again.");
                    config.set("app.refresh_token", "".to_string());
                    config.save();
                    std::process::exit(1);
                }

                let timeout =
                    reconnect_timeouts[reconnect_attempt.min(reconnect_timeouts.len() - 1)];
                reconnect_attempt += 1;
                println!(
                    "{} Auth request failed! Server offline or network error: {}. Retrying in {}s...",
                    "❌".red().bold(),
                    e,
                    timeout
                );
                tokio::time::sleep(tokio::time::Duration::from_secs(timeout)).await;
            }
        }
    };

    println!(
        "{} Successfully retrieved short-lived Access Token!",
        "✅".green().bold()
    );

    let ws_base = std::env::var("NODEINNET_WS").unwrap_or(ws_base);
    let ws_url = format!(
        "{}?token={}&session_id={}",
        ws_base, access_token, my_node_id
    );
    println!(
        "{} {}",
        "🌍 Connecting to:".cyan().bold(),
        ws_url.replace(&access_token, "REDACTED")
    );

    let handler: std::sync::Arc<dyn AppEventHandler> = std::sync::Arc::new(ConsoleEventHandler {
        ui_tx: app_tx.clone(),
    });
    let priv_key_str = private_key_b64.unwrap_or_default();

    p2p_node::set_app_version(app_version::APP_VERSION);
    p2p_handlers::install(
        p2p_handlers::Capabilities::ALL,
        p2p_handlers::HostSettings {
            app_launcher: None,
            screencast_restore_token: config.get::<String>(client_config::SCREENCAST_TOKEN_KEY),
            on_screencast_restore_token: Some({
                let config = config.clone();
                std::sync::Arc::new(move |token: String| {
                    config.set(client_config::SCREENCAST_TOKEN_KEY, token);
                    config.save();
                })
            }),
        },
    );

    client_core::network::start_network_thread(
        net_rx,
        net_tx_bg,
        handler,
        my_info.clone(),
        priv_key_str,
        std::sync::Arc::new(client_config::ConfigPeerStore::new(config.clone())),
        config
            .get::<bool>(client_config::LOCAL_DISCOVERY_KEY)
            .unwrap_or(false),
    );

    net_tx
        .send(NetCmd::Connect(
            ws_url.clone(),
            my_info.clone(),
            turn_credentials,
        ))
        .await
        .unwrap();

    let active_downloads = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::<
        uuid::Uuid,
        std::path::PathBuf,
    >::new()));
    let pending_uploads =
        std::sync::Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::<
            uuid::Uuid,
            (std::path::PathBuf, String),
        >::new()));

    let active_sync_strategy =
        std::sync::Arc::new(std::sync::Mutex::new(None::<(String, String, String)>));

    if let Some(exec_cmd) = args.exec.clone() {
        let net_tx_exec = net_tx.clone();
        let active_downloads_exec = active_downloads.clone();
        let pending_uploads_exec = pending_uploads.clone();
        let active_sync_strategy_exec = active_sync_strategy.clone();
        tokio::spawn(async move {
            crate::exec_command::execute_command(
                exec_cmd,
                net_tx_exec,
                active_downloads_exec,
                pending_uploads_exec,
                active_sync_strategy_exec,
            )
            .await;
        });
    }

    let known_peers: std::collections::HashSet<String> = std::collections::HashSet::new();

    let stdin_net_tx = net_tx.clone();
    let config_stdin = config.clone();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let stdin = std::io::stdin();
        for line in stdin.lock().lines().map_while(Result::ok) {
            let cmd = line.trim();
            if cmd == "reload" {
                println!("♻️ Reloading configuration from disk...");
                let new_resources = config_stdin
                    .get::<Vec<nodeinnet_p2p::SharedResource>>("app.resources")
                    .unwrap_or_default();
                if !new_resources.is_empty() {
                    let _ = stdin_net_tx.blocking_send(NetCmd::ReloadResources(new_resources));
                    println!("✅ Resources reloaded!");
                } else {
                    let _ = stdin_net_tx.blocking_send(NetCmd::ReloadResources(vec![]));
                    println!("✅ Resources reloaded (empty)!");
                }
            }
        }
    });

    let ctx = crate::event_loop::EventLoopContext {
        app_rx,
        refresh_token,
        net_tx,
        active_downloads,
        pending_uploads,
        active_sync_strategy,
        known_peers,
        my_info,
        ws_base: ws_base.to_string(),
        my_node_id,
        exec_cmd: args.exec.clone(),
        app_tx,
        api_base: api_base.to_string(),
        client_http: client_http.clone(),
        reconnect_attempt,
        reconnect_timeouts: reconnect_timeouts.to_vec(),
        config,
    };
    crate::event_loop::run_event_loop(ctx);
}
