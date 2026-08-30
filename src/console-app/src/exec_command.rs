use client_core::NetCmd;
use colored::*;

pub async fn execute_command(
    exec_cmd: String,
    net_tx_exec: tokio::sync::mpsc::Sender<NetCmd>,
    active_downloads_exec: std::sync::Arc<
        std::sync::Mutex<std::collections::HashMap<uuid::Uuid, std::path::PathBuf>>,
    >,
    pending_uploads_exec: std::sync::Arc<
        tokio::sync::Mutex<std::collections::HashMap<uuid::Uuid, (std::path::PathBuf, String)>>,
    >,
    active_sync_strategy_exec: std::sync::Arc<std::sync::Mutex<Option<(String, String, String)>>>,
) {
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    println!("{} Auto-executing command: {}", "🤖".cyan(), exec_cmd);
    let parts: Vec<&str> = exec_cmd.split_whitespace().collect();
    if parts.is_empty() {
        return;
    }

    let cmd = parts[0];
    match cmd {
        "call" => {
            if parts.len() >= 2 {
                let _ = net_tx_exec.send(NetCmd::Call(parts[1].to_string())).await;
            }
        }
        "help" => {
            println!(
                "\n{}",
                "========================================================="
                    .cyan()
                    .bold()
            );
            println!(
                "{} {}",
                "ℹ️".green().bold(),
                " NodeInNet Console CLI & Automator Help".cyan().bold()
            );
            println!(
                "{}\n",
                "========================================================="
                    .cyan()
                    .bold()
            );

            println!("{}:", "System Information".yellow().bold());
            println!(
                "  {} {} {}",
                "sysinfo".green(),
                "<node_id> <resource_id>".white(),
                "  Requests server OS and hardware context.".bright_black()
            );

            println!("\n{}:", "File Management".yellow().bold());
            println!(
                "  {} {} {}",
                "ls".green(),
                "<node> <res> <path>".white(),
                " Lists directory contents.".bright_black()
            );
            println!(
                "  {} {} {}",
                "mkdir".green(),
                "<node> <res> <p> <name>".white(),
                " Creates a directory.".bright_black()
            );
            println!(
                "  {} {} {}",
                "rm".green(),
                "<node> <res> <path>".white(),
                " Removes file/folder.".bright_black()
            );
            println!(
                "  {} {} {}",
                "mv".green(),
                "<node> <res> <src> <dst>".white(),
                " Renames file/folder.".bright_black()
            );
            println!(
                "  {} {} {}",
                "download".green(),
                "<node> <res> <rem> <loc>".white(),
                " Downloads file to local.".bright_black()
            );
            println!(
                "  {} {} {}",
                "upload".green(),
                "<node> <res> <loc> <rem>".white(),
                " Uploads local to remote.".bright_black()
            );

            println!(
                "\n{}:",
                "P2P Synchronizer (Automator Engine)".yellow().bold()
            );
            println!(
                "  {} {} {}",
                "sync_diff".green(),
                "<node_id> <folder_name>".white(),
                "  Scans and outputs file diff between strict folders.".bright_black()
            );
            println!(
                "  {} {} {}",
                "sync_auto".green(),
                "<node_id> <folder_name> <strtg>".white(),
                " Syncs automatically resolving conflicts.".bright_black()
            );
            println!(
                "     {}  {}  {}",
                "└─".bright_black(),
                "pull".cyan(),
                "Downloads missing/new files to server.".bright_black()
            );
            println!(
                "     {}  {}  {}",
                "└─".bright_black(),
                "push".cyan(),
                "Uploads missing/new files to peer.".bright_black()
            );
            println!(
                "     {}  {}  {}",
                "└─".bright_black(),
                "mirror_push".cyan(),
                "Server is master (Uploads missing, deletes remote extra).".bright_black()
            );
            println!(
                "     {}  {}  {}",
                "└─".bright_black(),
                "mirror_pull".cyan(),
                "Peer is master (Downloads missing, deletes local extra).".bright_black()
            );
            println!(
                "     {}  {}  {}",
                "└─".bright_black(),
                "Advanced Modifiers (comma-separated):".cyan(),
                "Allows explicit file resolution per context".bright_black()
            );
            println!(
                "         {} {} {}",
                "•".bright_black(),
                "pull-remote-new".green(),
                "Download if file is ONLY on remote".bright_black()
            );
            println!(
                "         {} {} {}",
                "•".bright_black(),
                "push-local-new".green(),
                "Upload if file is ONLY on local".bright_black()
            );
            println!(
                "         {} {} {}",
                "•".bright_black(),
                "delete-remote-new".red(),
                "Delete remote if file is missing locally".bright_black()
            );
            println!(
                "         {} {} {}",
                "•".bright_black(),
                "delete-local-new".red(),
                "Delete local if file is missing remotely".bright_black()
            );
            println!(
                "         {} {} {}",
                "•".bright_black(),
                "push-local-newermodifier".green(),
                "Upload if both exist but LOCAL is newer".bright_black()
            );
            println!(
                "         {} {} {}",
                "•".bright_black(),
                "pull-local-newermodifier".magenta(),
                "Download if both exist but LOCAL is newer (override)".bright_black()
            );
            println!(
                "         {} {} {}",
                "•".bright_black(),
                "pull-remote-newermodifier".green(),
                "Download if both exist but REMOTE is newer".bright_black()
            );
            println!(
                "         {} {} {}",
                "•".bright_black(),
                "push-remote-newermodifier".magenta(),
                "Upload if both exist but REMOTE is newer (override)".bright_black()
            );

            println!("\n{}:", "Other".yellow().bold());
            println!(
                "  {} {} {}",
                "shell".green(),
                "<node> <res>".white(),
                " Starts interactive RAW terminal session.".bright_black()
            );

            println!("\n{}", "Examples:".yellow().bold());
            println!(
                "  {} {}",
                "nodeinnet-console --exec \"sync_diff my-pc-1 images\"".white(),
                "       # Show which files differ from my-pc-1 under images".bright_black()
            );
            println!(
                "  {} {}",
                "nodeinnet-console --exec \"sync_auto laptop-2 docs mirror_push\"".white(),
                "      # Mirror docs outwards: local wins, remote extras are deleted"
                    .bright_black()
            );
            println!(
                "  {} {}",
                "nodeinnet-console --exec \"sync_auto laptop-2 videos pull\"".white(),
                "      # Pull new videos from laptop-2 only".bright_black()
            );
            println!(
                "  {} {}",
                "nodeinnet-console --exec \"ls desktop-3 fs_res_1 /usr/local\"".white(),
                "     # List a remote directory".bright_black()
            );

            std::process::exit(0);
        }
        "sysinfo" => {
            if parts.len() >= 3 {
                let node_id = parts[1].to_string();
                let resource_id = parts[2].to_string();
                println!(
                    "{} Requesting system info from {} resource {}",
                    "⏳".yellow(),
                    node_id,
                    resource_id
                );
                let _ = net_tx_exec
                    .send(NetCmd::SendToPeer(
                        node_id,
                        nodeinnet_p2p::P2pMessage::RequestSystemInfo { resource_id },
                    ))
                    .await;
            }
        }
        "ls" => {
            if parts.len() >= 4 {
                let node_id = parts[1].to_string();
                let resource_id = parts[2].to_string();
                let path = parts[3..].join(" ");
                println!(
                    "{} Requesting directory listing for {} from {} resource {}",
                    "📂".yellow(),
                    path,
                    node_id,
                    resource_id
                );
                let msg = nodeinnet_p2p::P2pMessage::RequestEntries {
                    request_id: uuid::Uuid::new_v4(),
                    resource_id,
                    path,
                };
                let _ = net_tx_exec.send(NetCmd::SendToPeer(node_id, msg)).await;
            }
        }
        "mkdir" => {
            if parts.len() >= 5 {
                let node_id = parts[1].to_string();
                let resource_id = parts[2].to_string();
                let parent_path = parts[3].to_string();
                let dir_name = parts[4..].join(" ");
                println!(
                    "{} Requesting to create directory {}/{} on {} resource {}",
                    "📂".yellow(),
                    parent_path,
                    dir_name,
                    node_id,
                    resource_id
                );
                let msg = nodeinnet_p2p::P2pMessage::CreateDirectoryRequest {
                    request_id: uuid::Uuid::new_v4(),
                    resource_id,
                    parent_path,
                    dir_name,
                    permissions: None,
                };
                let _ = net_tx_exec.send(NetCmd::SendToPeer(node_id, msg)).await;
            }
        }
        "rm" => {
            if parts.len() >= 4 {
                let node_id = parts[1].to_string();
                let resource_id = parts[2].to_string();
                let path = parts[3..].join(" ");
                println!(
                    "{} Requesting to delete {} on {} resource {}",
                    "🗑️".red(),
                    path,
                    node_id,
                    resource_id
                );
                let msg = nodeinnet_p2p::P2pMessage::DeleteEntryRequest {
                    request_id: uuid::Uuid::new_v4(),
                    resource_id,
                    path,
                };
                let _ = net_tx_exec.send(NetCmd::SendToPeer(node_id, msg)).await;
            }
        }
        "mv" => {
            if parts.len() >= 5 {
                let node_id = parts[1].to_string();
                let resource_id = parts[2].to_string();
                let path = parts[3].to_string();
                let new_path = parts[4].to_string();
                println!(
                    "{} Requesting to move {} to {} on {} resource {}",
                    "📝".yellow(),
                    path,
                    new_path,
                    node_id,
                    resource_id
                );
                let msg = nodeinnet_p2p::P2pMessage::RenameEntryRequest {
                    request_id: uuid::Uuid::new_v4(),
                    resource_id,
                    path,
                    new_path,
                };
                let _ = net_tx_exec.send(NetCmd::SendToPeer(node_id, msg)).await;
            }
        }
        "stat" => {
            if parts.len() >= 4 {
                let node_id = parts[1].to_string();
                let resource_id = parts[2].to_string();
                let path = parts[3..].join(" ");
                println!(
                    "{} Requesting metadata for {} from {} resource {}",
                    "📄".yellow(),
                    path,
                    node_id,
                    resource_id
                );
                let msg = nodeinnet_p2p::P2pMessage::RequestMetadata {
                    request_id: uuid::Uuid::new_v4(),
                    resource_id,
                    path,
                };
                let _ = net_tx_exec.send(NetCmd::SendToPeer(node_id, msg)).await;
            }
        }
        "registry_ls" => {
            if parts.len() >= 4 {
                let node_id = parts[1].to_string();
                let resource_id = parts[2].to_string();
                let path = parts[3..].join(" ");
                println!(
                    "{} Requesting Registry keys for {} from {} resource {}",
                    "📁".yellow(),
                    path,
                    node_id,
                    resource_id
                );
                let msg = nodeinnet_p2p::P2pMessage::RequestRegistryKeys {
                    request_id: uuid::Uuid::new_v4(),
                    resource_id,
                    path,
                };
                let _ = net_tx_exec.send(NetCmd::SendToPeer(node_id, msg)).await;
            }
        }
        "registry_mkdir" => {
            if parts.len() >= 5 {
                let node_id = parts[1].to_string();
                let resource_id = parts[2].to_string();
                let parent_path = parts[3].to_string();
                let key_name = parts[4..].join(" ");
                println!(
                    "{} Creating Registry Key {} under {} on {} resource {}",
                    "📁".yellow(),
                    key_name,
                    parent_path,
                    node_id,
                    resource_id
                );
                let msg = nodeinnet_p2p::P2pMessage::CreateRegistryKeyRequest {
                    request_id: uuid::Uuid::new_v4(),
                    resource_id,
                    parent_path,
                    key_name,
                };
                let _ = net_tx_exec.send(NetCmd::SendToPeer(node_id, msg)).await;
            }
        }
        "registry_rm" => {
            if parts.len() >= 5 {
                let node_id = parts[1].to_string();
                let resource_id = parts[2].to_string();
                let is_key = parts[3] == "true";
                let path = parts[4..].join(" ");
                println!(
                    "{} Deleting Registry Entry {} (IsKey={}) on {} resource {}",
                    "🗑️".red(),
                    path,
                    is_key,
                    node_id,
                    resource_id
                );
                let msg = nodeinnet_p2p::P2pMessage::DeleteRegistryEntryRequest {
                    request_id: uuid::Uuid::new_v4(),
                    resource_id,
                    path,
                    value_name: None,
                    is_key,
                };
                let _ = net_tx_exec.send(NetCmd::SendToPeer(node_id, msg)).await;
            }
        }
        "registry_set" => {
            if parts.len() >= 7 {
                let node_id = parts[1].to_string();
                let resource_id = parts[2].to_string();
                let path = parts[3].to_string();
                let value_name = parts[4].to_string();
                let vtype = parts[5].to_lowercase();
                let data = parts[6..].join(" ");

                let value_data = match vtype.as_str() {
                    "string" => nodeinnet_p2p::p2p::RegistryValueData::String(data),
                    "expandstring" => nodeinnet_p2p::p2p::RegistryValueData::ExpandString(data),
                    "dword" => {
                        nodeinnet_p2p::p2p::RegistryValueData::DWord(data.parse().unwrap_or(0))
                    }
                    "qword" => {
                        nodeinnet_p2p::p2p::RegistryValueData::QWord(data.parse().unwrap_or(0))
                    }
                    _ => {
                        println!("{} Unknown registry type: {}", "❌".red(), vtype);
                        std::process::exit(1);
                    }
                };

                println!(
                    "{} Setting Registry Value {} at {} to {:?} on {} resource {}",
                    "📝".blue(),
                    value_name,
                    path,
                    value_data,
                    node_id,
                    resource_id
                );
                let msg = nodeinnet_p2p::P2pMessage::SetRegistryValueRequest {
                    request_id: uuid::Uuid::new_v4(),
                    resource_id,
                    path,
                    value_name,
                    value_data,
                };
                let _ = net_tx_exec.send(NetCmd::SendToPeer(node_id, msg)).await;
            }
        }

        "shell" => {
            if parts.len() >= 3 {
                let node_id = parts[1].to_string();
                let resource_id = parts[2].to_string();

                println!(
                    "{} Requesting secure PTY session with {} resource {}",
                    "👾".green(),
                    node_id,
                    resource_id
                );
                let msg = nodeinnet_p2p::P2pMessage::StartTerminal {
                    resource_id: resource_id.clone(),
                };
                let _ = net_tx_exec
                    .send(NetCmd::SendToPeer(node_id.clone(), msg))
                    .await;

                crossterm::terminal::enable_raw_mode().expect("Failed to enable raw terminal mode");

                let net_tx_shell = net_tx_exec.clone();
                let n_id = node_id.clone();
                let r_id = resource_id.clone();

                tokio::task::spawn_blocking(move || {
                    use crossterm::event::{Event, KeyCode, KeyModifiers, read};
                    loop {
                        if let Ok(Event::Key(key)) = read() {
                            if key.code == KeyCode::Char('d')
                                && key.modifiers.contains(KeyModifiers::CONTROL)
                            {
                                let _ = net_tx_shell.blocking_send(NetCmd::SendToPeer(
                                    n_id.clone(),
                                    nodeinnet_p2p::P2pMessage::TerminalInput {
                                        resource_id: r_id.clone(),
                                        data: vec![0x04],
                                    },
                                ));
                            } else if key.code == KeyCode::Char('c')
                                && key.modifiers.contains(KeyModifiers::CONTROL)
                            {
                                let _ = net_tx_shell.blocking_send(NetCmd::SendToPeer(
                                    n_id.clone(),
                                    nodeinnet_p2p::P2pMessage::TerminalInput {
                                        resource_id: r_id.clone(),
                                        data: vec![0x03],
                                    },
                                ));
                            } else if key.code == KeyCode::Char('x')
                                && key.modifiers.contains(KeyModifiers::CONTROL)
                            {
                                let _ = crossterm::terminal::disable_raw_mode();
                                println!("\r\n{} Disconnecting from shell...", "🚪".yellow());
                                std::process::exit(0);
                            } else if key.code == KeyCode::Enter {
                                let _ = net_tx_shell.blocking_send(NetCmd::SendToPeer(
                                    n_id.clone(),
                                    nodeinnet_p2p::P2pMessage::TerminalInput {
                                        resource_id: r_id.clone(),
                                        data: vec![b'\r'],
                                    },
                                ));
                            } else if key.code == KeyCode::Backspace {
                                let _ = net_tx_shell.blocking_send(NetCmd::SendToPeer(
                                    n_id.clone(),
                                    nodeinnet_p2p::P2pMessage::TerminalInput {
                                        resource_id: r_id.clone(),
                                        data: vec![0x08],
                                    },
                                ));
                            } else if let KeyCode::Char(c) = key.code {
                                let _ = net_tx_shell.blocking_send(NetCmd::SendToPeer(
                                    n_id.clone(),
                                    nodeinnet_p2p::P2pMessage::TerminalInput {
                                        resource_id: r_id.clone(),
                                        data: vec![c as u8],
                                    },
                                ));
                            }
                        }
                    }
                });
            }
        }
        "download" => {
            if parts.len() >= 5 {
                let node_id = parts[1].to_string();
                let resource_id = parts[2].to_string();
                let remote_path = parts[3].to_string();
                let local_path = parts[4..].join(" ");
                let transfer_id = uuid::Uuid::new_v4();

                active_downloads_exec
                    .lock()
                    .unwrap()
                    .insert(transfer_id, std::path::PathBuf::from(local_path));

                println!(
                    "{} Requesting to download {} from {} resource {}",
                    "🔽".blue(),
                    remote_path,
                    node_id,
                    resource_id
                );
                let msg = nodeinnet_p2p::P2pMessage::FileDownloadRequest {
                    resource_id,
                    file_path: remote_path,
                    transfer_id,
                };
                let _ = net_tx_exec.send(NetCmd::SendToPeer(node_id, msg)).await;
            }
        }
        "upload" => {
            if parts.len() >= 5 {
                let node_id = parts[1].to_string();
                let resource_id = parts[2].to_string();
                let local_path_str = parts[3].to_string();
                let remote_path = parts[4..].join(" ");
                let local_path = std::path::PathBuf::from(local_path_str);

                if let Ok(meta) = std::fs::metadata(&local_path) {
                    let transfer_id = uuid::Uuid::new_v4();
                    pending_uploads_exec
                        .lock()
                        .await
                        .insert(transfer_id, (local_path.clone(), node_id.clone()));

                    let file_name = local_path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let total_size = meta.len();
                    println!(
                        "{} Requesting to upload {} ({} bytes) to {} resource {}",
                        "🔼".blue(),
                        file_name,
                        total_size,
                        node_id,
                        resource_id
                    );

                    let permissions = {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            std::fs::metadata(&local_path)
                                .map(|m| Some(m.permissions().mode()))
                                .unwrap_or(None)
                        }
                        #[cfg(not(unix))]
                        {
                            None
                        }
                    };

                    let msg = nodeinnet_p2p::P2pMessage::FileUploadRequest {
                        resource_id,
                        target_path: remote_path,
                        file_name,
                        total_size,
                        transfer_id,
                        permissions,
                    };
                    let _ = net_tx_exec.send(NetCmd::SendToPeer(node_id, msg)).await;
                } else {
                    println!("{} Local file not found: {:?}", "❌".red(), local_path);
                    std::process::exit(1);
                }
            }
        }
        "resources" => {}
        "sync_diff" | "sync_auto" => {
            if parts.len() >= 3 {
                let node_id = parts[1].to_string();
                let folder_name = parts[2].to_string();

                let strategy = if cmd == "sync_auto" && parts.len() >= 4 {
                    parts[3].to_string()
                } else {
                    "diff_only".to_string()
                };

                println!(
                    "{} Requesting SyncFolder Diff for '{}' (Strategy: {})",
                    "🔄".magenta(),
                    folder_name,
                    strategy
                );

                *active_sync_strategy_exec.lock().unwrap() =
                    Some((node_id.clone(), folder_name.clone(), strategy));

                let msg = nodeinnet_p2p::P2pMessage::SyncStateRequest {
                    resource_id: folder_name.clone(),
                    target_folder_names: vec![folder_name],
                };

                let _ = net_tx_exec.send(NetCmd::SendToPeer(node_id, msg)).await;
            } else {
                println!(
                    "{} Usage: sync_diff <node_name> <folder_name> | sync_auto <node_name> <folder_name> <strategy>",
                    "❌".red()
                );
                std::process::exit(1);
            }
        }
        _ => {
            println!("{} Unknown command: {}", "⚠️".yellow(), cmd);
        }
    }
}
