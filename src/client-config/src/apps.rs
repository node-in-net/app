use crate::AppConfig;
use serde::{Deserialize, Serialize};

pub const KEY: &str = "proxied_apps";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ProxiedApp {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub exec_cmd: String,
    pub icon_name: Option<String>,
    #[serde(default)]
    pub allow_remote_launch: bool,
}

impl ProxiedApp {
    pub fn new(name: String, exec_cmd: String, icon_name: Option<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            exec_cmd,
            icon_name,
            allow_remote_launch: false,
        }
    }
}

fn backfill_ids(apps: &mut [ProxiedApp]) -> bool {
    let mut changed = false;
    for app in apps.iter_mut() {
        if app.id.is_empty() {
            app.id = uuid::Uuid::new_v4().to_string();
            changed = true;
        }
    }
    changed
}

pub fn load(config: &AppConfig) -> Vec<ProxiedApp> {
    let Some(mut apps) = config.get::<Vec<ProxiedApp>>(KEY) else {
        return Vec::new();
    };
    if backfill_ids(&mut apps) {
        save(config, &apps);
    }
    apps
}

pub fn save(config: &AppConfig, apps: &[ProxiedApp]) {
    config.set(KEY, apps);
    config.save();
}

pub fn upsert(config: &AppConfig, app: ProxiedApp) {
    let mut apps = load(config);
    apps.retain(|a| a.id != app.id && a.exec_cmd != app.exec_cmd);
    apps.push(app);
    save(config, &apps);
}

pub fn remove_by_id(config: &AppConfig, id: &str) -> bool {
    let mut apps = load(config);
    let before = apps.len();
    apps.retain(|a| a.id != id);
    if apps.len() == before {
        return false;
    }
    save(config, &apps);
    true
}

pub fn set_remote_launch(config: &AppConfig, id: &str, allowed: bool) -> bool {
    let mut apps = load(config);
    let Some(app) = apps.iter_mut().find(|a| a.id == id) else {
        return false;
    };
    app.allow_remote_launch = allowed;
    save(config, &apps);
    true
}

pub fn consented(config: &AppConfig) -> Vec<ProxiedApp> {
    load(config)
        .into_iter()
        .filter(|a| a.allow_remote_launch)
        .collect()
}

pub fn find_by_id(config: &AppConfig, id: &str) -> Option<ProxiedApp> {
    load(config).into_iter().find(|a| a.id == id)
}

pub fn find_by_name(config: &AppConfig, name: &str) -> Option<ProxiedApp> {
    load(config).into_iter().find(|a| a.name == name)
}

pub fn import_legacy(config: &AppConfig, path: &std::path::Path) -> Vec<ProxiedApp> {
    let Ok(data) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(mut apps) = serde_json::from_str::<Vec<ProxiedApp>>(&data) else {
        return Vec::new();
    };
    backfill_ids(&mut apps);
    save(config, &apps);
    let _ = std::fs::remove_file(path);
    apps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_config(who: &str) -> AppConfig {
        let cfg = AppConfig::new(&format!("_apps_unit_tests_{who}"));
        cfg.set(KEY, &Vec::<ProxiedApp>::new());
        cfg
    }

    #[test]
    fn entries_stored_before_ids_existed_survive_the_upgrade() {
        let cfg = scratch_config("upgrade");
        cfg.set(
            KEY,
            &serde_json::json!([
                { "name": "Firefox", "exec_cmd": "/usr/bin/firefox", "icon_name": "firefox" },
                { "name": "Chromium", "exec_cmd": "/usr/bin/chromium", "icon_name": null },
            ]),
        );

        let apps = load(&cfg);

        assert_eq!(apps.len(), 2, "a missing field must not empty the list");
        assert!(apps.iter().all(|a| !a.id.is_empty()), "ids are backfilled");
        assert!(
            apps.iter().all(|a| !a.allow_remote_launch),
            "nobody is granted remote launch by an upgrade"
        );
        assert_ne!(apps[0].id, apps[1].id);

        assert_eq!(load(&cfg), apps);
    }

    #[test]
    fn removal_is_by_id_so_a_shared_name_is_safe() {
        let cfg = scratch_config("remove");
        let a = ProxiedApp::new("Browser".into(), "/usr/bin/firefox".into(), None);
        let b = ProxiedApp::new("Browser".into(), "/usr/bin/chromium".into(), None);
        upsert(&cfg, a.clone());
        upsert(&cfg, b.clone());

        assert!(remove_by_id(&cfg, &a.id));

        let left = load(&cfg);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].exec_cmd, "/usr/bin/chromium", "the namesake stayed");
        assert!(
            !remove_by_id(&cfg, &a.id),
            "removing it twice is not an error"
        );
    }

    #[test]
    fn the_same_command_added_twice_is_one_entry() {
        let cfg = scratch_config("dedup");
        upsert(
            &cfg,
            ProxiedApp::new("Firefox".into(), "/usr/bin/firefox".into(), None),
        );
        upsert(
            &cfg,
            ProxiedApp::new("Firefox again".into(), "/usr/bin/firefox".into(), None),
        );

        let apps = load(&cfg);
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "Firefox again", "the later add wins");
    }

    #[test]
    fn consent_is_per_entry_and_off_until_set() {
        let cfg = scratch_config("consent");
        let a = ProxiedApp::new("Firefox".into(), "/usr/bin/firefox".into(), None);
        let b = ProxiedApp::new("Chromium".into(), "/usr/bin/chromium".into(), None);
        upsert(&cfg, a.clone());
        upsert(&cfg, b.clone());
        assert!(consented(&cfg).is_empty());

        assert!(set_remote_launch(&cfg, &a.id, true));
        let allowed = consented(&cfg);
        assert_eq!(allowed.len(), 1);
        assert_eq!(allowed[0].id, a.id, "only the entry that was ticked");

        assert!(set_remote_launch(&cfg, &a.id, false));
        assert!(
            consented(&cfg).is_empty(),
            "revocation takes effect at once"
        );
        assert!(!set_remote_launch(&cfg, "no-such-id", true));
    }
}
