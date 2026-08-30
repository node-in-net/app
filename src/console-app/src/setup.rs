use colored::*;
use nodeinnet_p2p::{ResourceType, SharedResource};
use std::io::{self, Write};

const SERVICES: [(ResourceType, &str, &str); 3] = [
    (ResourceType::Filesystem, "fs-home", "Files"),
    (ResourceType::Terminal, "terminal", "Terminal"),
    (ResourceType::SharedNetwork, "network", "Network"),
];

pub fn run_interactive_setup(
    resources: &mut Vec<SharedResource>,
    config: &client_config::AppConfig,
) {
    let host_name = hostname::get()
        .ok()
        .and_then(|s| s.into_string().ok())
        .unwrap_or_else(|| "Console Node".to_string());
    let mut peer_name = config
        .get::<String>("app-console-name")
        .unwrap_or_else(|| format!("{} (CLI)", host_name));

    loop {
        println!(
            "\n{}",
            "==============================================="
                .blue()
                .bold()
        );
        println!(
            "{} {}",
            "⚙️".cyan().bold(),
            " NodeInNet Console — Interactive Setup".cyan().bold()
        );
        println!(
            "{}",
            "==============================================="
                .blue()
                .bold()
        );
        println!(
            "\n{} {}",
            "Device name:".yellow().bold(),
            peer_name.bright_white()
        );

        println!(
            "\n{}",
            "Services (shared with your other devices):".yellow().bold()
        );
        for (i, (rt, _, label)) in SERVICES.iter().enumerate() {
            let on = resources
                .iter()
                .any(|r| &r.resource_type == rt && r.is_active);
            let mark = if on { "x".green() } else { " ".normal() };
            let extra = if *rt == ResourceType::Filesystem && on {
                format!("   ({} folder(s))", file_shares(resources).len())
            } else {
                String::new()
            };
            println!(
                "  {}. [{}] {}{}",
                i + 1,
                mark,
                label.bright_white(),
                extra.dimmed()
            );
        }

        println!("\n{}", "Menu:".yellow().bold());
        println!("  1-4.  Toggle a service on/off");
        println!("  5.    Manage Files folders (add / remove shared folders)");
        println!("  6.    Change device name");
        println!("  7.    Save & Exit");

        let choice = prompt_string("\nChoose an option: ");
        match choice.trim() {
            "1" | "2" | "3" | "4" => {
                let idx = choice.trim().parse::<usize>().unwrap() - 1;
                toggle_service(resources, idx);
            }
            "5" => manage_files_shares(resources),
            "6" => {
                let new_name =
                    prompt_string(&format!("Enter new device name [current: {}]: ", peer_name));
                if !new_name.trim().is_empty() {
                    peer_name = new_name.trim().to_string();
                    config.set("app-console-name", peer_name.clone());
                    config.save();
                    println!("{} Device name updated.", "✅".green());
                }
            }
            "7" => {
                println!("\n{}", "💾 Saved. Exiting setup...".green().bold());
                break;
            }
            _ => println!("{}", "❌ Invalid choice, please try again.".red()),
        }
    }
}

fn toggle_service(resources: &mut Vec<SharedResource>, idx: usize) {
    let (rt, id, label) = &SERVICES[idx];
    if let Some(pos) = resources.iter().position(|r| &r.resource_type == rt) {
        resources.remove(pos);
        println!("{} {} is now OFF.", "○".red(), label);
    } else {
        let cfg = (*rt == ResourceType::Filesystem).then(|| shares_json(&[]));
        resources.push(SharedResource {
            id: id.to_string(),
            name: label.to_string(),
            resource_type: rt.clone(),
            config: cfg,
            is_active: true,
            session_token: None,
        });
        println!("{} {} is now ON.", "●".green(), label);
        if *rt == ResourceType::Filesystem {
            println!(
                "  {}",
                "Use 'Manage Files folders' to add shared folders.".dimmed()
            );
        }
    }
}

fn file_shares(resources: &[SharedResource]) -> Vec<(String, String)> {
    resources
        .iter()
        .find(|r| r.resource_type == ResourceType::Filesystem)
        .and_then(|r| r.config.as_ref())
        .and_then(|c| serde_json::from_str::<serde_json::Value>(c).ok())
        .and_then(|v| v.get("shares").and_then(|s| s.as_array()).cloned())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| {
                    Some((
                        s.get("name")?.as_str()?.to_string(),
                        s.get("path")?.as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn shares_json(shares: &[(String, String)]) -> String {
    let list: Vec<_> = shares
        .iter()
        .map(|(n, p)| serde_json::json!({ "name": n, "path": p }))
        .collect();
    serde_json::json!({ "shares": list }).to_string()
}

fn set_file_shares(resources: &mut Vec<SharedResource>, shares: &[(String, String)]) {
    if let Some(r) = resources
        .iter_mut()
        .find(|r| r.resource_type == ResourceType::Filesystem)
    {
        r.config = Some(shares_json(shares));
    } else {
        resources.push(SharedResource {
            id: "fs-home".into(),
            name: "Files".into(),
            resource_type: ResourceType::Filesystem,
            config: Some(shares_json(shares)),
            is_active: true,
            session_token: None,
        });
    }
}

fn manage_files_shares(resources: &mut Vec<SharedResource>) {
    loop {
        let mut shares = file_shares(resources);
        println!("\n{}", "--- Files: shared folders ---".cyan().bold());
        if shares.is_empty() {
            println!("  [no folders shared yet]");
        } else {
            for (i, (name, path)) in shares.iter().enumerate() {
                println!("  {}. {}  →  {}", i + 1, name.bright_white(), path.dimmed());
            }
        }
        println!("\n  a.      Add a folder");
        println!("  r.      Remove a folder");
        println!("  Enter.  Back");

        let choice = prompt_string("\nChoose: ");
        match choice.trim() {
            "a" | "A" => {
                let path = prompt_string("Absolute path to the folder to share: ");
                let path = path.trim().to_string();
                if path.is_empty() {
                    continue;
                }
                let default = std::path::Path::new(&path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "share".into());
                let base = prompt_res_name(&default);
                let mut name = base.clone();
                let mut n = 2;
                while shares.iter().any(|(nm, _)| nm == &name) {
                    name = format!("{base} {n}");
                    n += 1;
                }
                shares.push((name, path));
                set_file_shares(resources, &shares);
                println!("{} Folder shared.", "✅".green());
            }
            "r" | "R" => {
                if shares.is_empty() {
                    continue;
                }
                let idx_str = prompt_string("Number of the folder to remove: ");
                match idx_str.trim().parse::<usize>() {
                    Ok(idx) if idx >= 1 && idx <= shares.len() => {
                        let (name, _) = shares.remove(idx - 1);
                        set_file_shares(resources, &shares);
                        println!("{} Removed '{}'.", "🗑️".red(), name);
                    }
                    _ => println!("{}", "❌ Invalid number.".red()),
                }
            }
            "" => break,
            _ => println!("{}", "❌ Invalid choice.".red()),
        }
    }
}

fn prompt_res_name(default_name: &str) -> String {
    let raw = prompt_string(&format!("Share name [default: {}]: ", default_name));
    if raw.trim().is_empty() {
        default_name.to_string()
    } else {
        raw.trim().to_string()
    }
}

fn prompt_string(prompt: &str) -> String {
    print!("{}", prompt);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim_end().to_string()
}

pub fn ensure_system_info_shared(resources: &mut Vec<SharedResource>) -> bool {
    let already = resources
        .iter()
        .any(|r| r.resource_type == ResourceType::SystemInfo && r.is_active);
    if already {
        return false;
    }
    resources.retain(|r| r.resource_type != ResourceType::SystemInfo);
    resources.push(SharedResource {
        id: "sysinfo".into(),
        name: "System information".into(),
        resource_type: ResourceType::SystemInfo,
        config: None,
        is_active: true,
        session_token: None,
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn res(rt: ResourceType, id: &str, is_active: bool) -> SharedResource {
        SharedResource {
            id: id.into(),
            name: id.into(),
            resource_type: rt,
            config: None,
            is_active,
            session_token: None,
        }
    }

    #[test]
    fn a_fresh_node_starts_sharing_it() {
        let mut r = Vec::new();
        assert!(ensure_system_info_shared(&mut r));
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].resource_type, ResourceType::SystemInfo);
        assert!(r[0].is_active);
    }

    #[test]
    fn an_existing_node_is_left_alone() {
        let mut r = vec![
            res(ResourceType::Filesystem, "fs-home", true),
            res(ResourceType::SystemInfo, "sysinfo", true),
        ];
        assert!(!ensure_system_info_shared(&mut r));
        assert_eq!(r.len(), 2, "nothing should have been added");
    }

    #[test]
    fn a_disabled_entry_is_replaced_not_duplicated() {
        let mut r = vec![res(ResourceType::SystemInfo, "sysinfo", false)];
        assert!(ensure_system_info_shared(&mut r));
        assert_eq!(
            r.iter()
                .filter(|x| x.resource_type == ResourceType::SystemInfo)
                .count(),
            1,
            "the disabled entry must be gone, not shadowed"
        );
        assert!(r[0].is_active);
    }

    #[test]
    fn the_other_services_survive() {
        let mut r = vec![
            res(ResourceType::Filesystem, "fs-home", true),
            res(ResourceType::Terminal, "terminal", true),
        ];
        ensure_system_info_shared(&mut r);
        assert!(
            r.iter()
                .any(|x| x.resource_type == ResourceType::Filesystem)
        );
        assert!(r.iter().any(|x| x.resource_type == ResourceType::Terminal));
    }
}
