use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceKind {
    Files,
    Terminal,
    Desktop,
    Network,
    Registry,
    #[serde(rename = "sysinfo")]
    SystemInfo,
}

impl ServiceKind {
    pub fn id(self) -> &'static str {
        match self {
            ServiceKind::Files => "files",
            ServiceKind::Terminal => "terminal",
            ServiceKind::Desktop => "desktop",
            ServiceKind::Network => "network",
            ServiceKind::Registry => "registry",
            ServiceKind::SystemInfo => "sysinfo",
        }
    }

    pub fn from_id(s: &str) -> Option<Self> {
        [
            ServiceKind::Files,
            ServiceKind::Terminal,
            ServiceKind::Desktop,
            ServiceKind::Network,
            ServiceKind::Registry,
            ServiceKind::SystemInfo,
        ]
        .into_iter()
        .find(|k| k.id() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkState {
    #[default]
    Idle,
    Connecting,
    Connected,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub os: String,
    pub online: bool,
    #[serde(default)]
    pub link: LinkState,
    #[serde(default)]
    pub services: Vec<ServiceKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountInfo {
    pub resource_id: String,
    pub name: String,
    pub url: String,
    pub port: u16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RailViewMode {
    #[default]
    List,
    Icons,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapView {
    #[default]
    Hidden,
    Peek,
    Full,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServicePanes {
    pub open: Vec<ServiceKind>,
    pub focused: Option<ServiceKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    DevicesChanged,
    SelectionChanged { device: Option<String> },
    ServicesChanged { device: String },
    TabsChanged,
    MapChanged { view: MapView },
    RailModeChanged { mode: RailViewMode },
    ThemeChanged { theme: Theme },
    MountsChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub devices: Vec<DeviceInfo>,
    pub selected: Option<String>,
    pub open_tabs: Vec<String>,
    pub panes: ServicePanes,
    pub map: MapView,
    pub rail_mode: RailViewMode,
    pub theme: Theme,
    #[serde(default)]
    pub mounts: Vec<MountInfo>,
    #[serde(default)]
    pub server_online: bool,
}

#[derive(Debug, Default)]
pub struct Workspace {
    devices: Vec<DeviceInfo>,
    selected: Option<String>,
    open_tabs: Vec<String>,
    panes: HashMap<String, ServicePanes>,
    map: MapView,
    rail_mode: RailViewMode,
    theme: Theme,
    mounts: Vec<MountInfo>,
    server_online: bool,
    events: Vec<Event>,
}

impl Workspace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn devices(&self) -> &[DeviceInfo] {
        &self.devices
    }

    pub fn selected_device(&self) -> Option<&DeviceInfo> {
        let id = self.selected.as_deref()?;
        self.devices.iter().find(|d| d.id == id)
    }

    pub fn open_tabs(&self) -> &[String] {
        &self.open_tabs
    }

    pub fn panes(&self) -> ServicePanes {
        self.selected
            .as_deref()
            .and_then(|id| self.panes.get(id))
            .cloned()
            .unwrap_or_default()
    }

    pub fn map(&self) -> MapView {
        self.map
    }

    pub fn rail_mode(&self) -> RailViewMode {
        self.rail_mode
    }

    pub fn theme(&self) -> Theme {
        self.theme
    }

    pub fn set_server_online(&mut self, online: bool) -> bool {
        if self.server_online == online {
            return false;
        }
        self.server_online = online;
        self.events.push(Event::DevicesChanged);
        true
    }

    pub fn mounts(&self) -> &[MountInfo] {
        &self.mounts
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    pub fn snapshot(&self) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            devices: self.devices.clone(),
            selected: self.selected.clone(),
            open_tabs: self.open_tabs.clone(),
            panes: self.panes(),
            map: self.map,
            rail_mode: self.rail_mode,
            theme: self.theme,
            mounts: self.mounts.clone(),
            server_online: self.server_online,
        }
    }

    pub fn apply_snapshot(&mut self, devices: Vec<DeviceInfo>) {
        self.devices = devices;
        self.panes
            .retain(|id, _| self.devices.iter().any(|d| &d.id == id));
        let tabs_before = self.open_tabs.len();
        self.open_tabs
            .retain(|id| self.devices.iter().any(|d| &d.id == id));
        if self.open_tabs.len() != tabs_before {
            self.events.push(Event::TabsChanged);
        }
        if let Some(sel) = self.selected.clone() {
            if !self.devices.iter().any(|d| d.id == sel) {
                self.selected = None;
                self.events.push(Event::SelectionChanged { device: None });
            }
        }
        self.events.push(Event::DevicesChanged);
    }

    pub fn apply_online(&mut self, dev: DeviceInfo) {
        match self.devices.iter_mut().find(|d| d.id == dev.id) {
            Some(d) => {
                *d = DeviceInfo {
                    online: true,
                    ..dev
                };
            }
            None => self.devices.push(DeviceInfo {
                online: true,
                ..dev
            }),
        }
        self.events.push(Event::DevicesChanged);
    }

    pub fn apply_offline(&mut self, id: &str) {
        let Some(d) = self.devices.iter_mut().find(|d| d.id == id) else {
            return;
        };
        d.online = false;
        if self.panes.remove(id).is_some() {
            self.events.push(Event::ServicesChanged {
                device: id.to_string(),
            });
        }
        self.events.push(Event::DevicesChanged);
    }

    pub fn apply_info_changed(&mut self, dev: DeviceInfo) {
        if let Some(d) = self.devices.iter_mut().find(|d| d.id == dev.id) {
            *d = dev;
            self.events.push(Event::DevicesChanged);
        }
    }

    pub fn select_device(&mut self, id: &str) -> bool {
        if !self.devices.iter().any(|d| d.id == id) {
            return false;
        }
        if !self.open_tabs.iter().any(|t| t == id) {
            self.open_tabs.push(id.to_string());
            self.events.push(Event::TabsChanged);
        }
        if self.selected.as_deref() != Some(id) {
            self.selected = Some(id.to_string());
            self.events.push(Event::SelectionChanged {
                device: Some(id.to_string()),
            });
        }
        true
    }

    pub fn close_tab(&mut self, id: &str) -> bool {
        let Some(pos) = self.open_tabs.iter().position(|t| t == id) else {
            return false;
        };
        self.open_tabs.remove(pos);
        self.events.push(Event::TabsChanged);
        if self.panes.remove(id).is_some() {
            self.events.push(Event::ServicesChanged {
                device: id.to_string(),
            });
        }
        if self.selected.as_deref() == Some(id) {
            self.selected = self.open_tabs.last().cloned();
            self.events.push(Event::SelectionChanged {
                device: self.selected.clone(),
            });
        }
        true
    }

    pub fn open_service(&mut self, kind: ServiceKind) -> bool {
        let Some(dev) = self.selected_device() else {
            return false;
        };
        if !dev.online {
            return false;
        }
        let id = dev.id.clone();
        let panes = self.panes.entry(id.clone()).or_default();
        if !panes.open.contains(&kind) {
            panes.open.push(kind);
        }
        panes.focused = Some(kind);
        self.events.push(Event::ServicesChanged { device: id });
        true
    }

    pub fn focus_service(&mut self, kind: ServiceKind) -> bool {
        let Some(id) = self.selected.clone() else {
            return false;
        };
        let Some(panes) = self.panes.get_mut(&id) else {
            return false;
        };
        if !panes.open.contains(&kind) {
            return false;
        }
        if panes.focused != Some(kind) {
            panes.focused = Some(kind);
            self.events.push(Event::ServicesChanged { device: id });
        }
        true
    }

    pub fn close_service(&mut self, kind: ServiceKind) -> bool {
        let Some(id) = self.selected.clone() else {
            return false;
        };
        let Some(panes) = self.panes.get_mut(&id) else {
            return false;
        };
        let Some(pos) = panes.open.iter().position(|k| *k == kind) else {
            return false;
        };
        panes.open.remove(pos);
        if panes.focused == Some(kind) {
            panes.focused = panes.open.last().copied();
        }
        if panes.open.is_empty() {
            self.panes.remove(&id);
        }
        self.events.push(Event::ServicesChanged { device: id });
        true
    }

    pub fn map_toggle_peek(&mut self) {
        self.set_map(match self.map {
            MapView::Hidden => MapView::Peek,
            MapView::Peek => MapView::Hidden,
            MapView::Full => MapView::Peek,
        });
    }

    pub fn map_open_full(&mut self) {
        self.set_map(MapView::Full);
    }

    pub fn map_close(&mut self) {
        self.set_map(MapView::Hidden);
    }

    fn set_map(&mut self, view: MapView) {
        if self.map != view {
            self.map = view;
            self.events.push(Event::MapChanged { view });
        }
    }

    pub fn set_rail_mode(&mut self, mode: RailViewMode) {
        if self.rail_mode != mode {
            self.rail_mode = mode;
            self.events.push(Event::RailModeChanged { mode });
        }
    }

    pub fn set_theme(&mut self, theme: Theme) {
        if self.theme != theme {
            self.theme = theme;
            self.events.push(Event::ThemeChanged { theme });
        }
    }

    pub fn add_mount(&mut self, mount: MountInfo) {
        match self
            .mounts
            .iter_mut()
            .find(|m| m.resource_id == mount.resource_id)
        {
            Some(m) => *m = mount,
            None => self.mounts.push(mount),
        }
        self.events.push(Event::MountsChanged);
    }

    pub fn remove_mount(&mut self, resource_id: &str) -> bool {
        let before = self.mounts.len();
        self.mounts.retain(|m| m.resource_id != resource_id);
        let removed = self.mounts.len() != before;
        if removed {
            self.events.push(Event::MountsChanged);
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(id: &str, online: bool) -> DeviceInfo {
        DeviceInfo {
            id: id.to_string(),
            name: format!("name-{id}"),
            os: "linux".to_string(),
            online,
            link: LinkState::Idle,
            services: Vec::new(),
        }
    }

    fn ws_with(devs: &[(&str, bool)]) -> Workspace {
        let mut ws = Workspace::new();
        ws.apply_snapshot(devs.iter().map(|(id, on)| dev(id, *on)).collect());
        ws.take_events();
        ws
    }

    #[test]
    fn select_opens_tab_once() {
        let mut ws = ws_with(&[("a", true), ("b", true)]);
        ws.select_device("a");
        ws.select_device("b");
        ws.select_device("a");
        assert_eq!(ws.open_tabs(), ["a", "b"]);
        assert_eq!(
            ws.take_events()
                .iter()
                .filter(|e| **e == Event::TabsChanged)
                .count(),
            2
        );
    }

    #[test]
    fn close_tab_falls_back_to_last_remaining() {
        let mut ws = ws_with(&[("a", true), ("b", true), ("c", true)]);
        ws.select_device("a");
        ws.select_device("b");
        ws.select_device("c");
        ws.take_events();
        assert!(ws.close_tab("c"));
        assert_eq!(ws.open_tabs(), ["a", "b"]);
        assert_eq!(ws.selected_device().unwrap().id, "b");
        assert!(ws.take_events().contains(&Event::TabsChanged));
    }

    #[test]
    fn close_tab_drops_panes_and_selection() {
        let mut ws = ws_with(&[("a", true)]);
        ws.select_device("a");
        ws.open_service(ServiceKind::Files);
        ws.take_events();
        assert!(ws.close_tab("a"));
        assert_eq!(ws.open_tabs(), [] as [&str; 0]);
        assert!(ws.selected_device().is_none());
        ws.select_device("a");
        assert_eq!(ws.panes().open, vec![]);
    }

    #[test]
    fn close_unknown_tab_is_noop() {
        let mut ws = ws_with(&[("a", true)]);
        assert!(!ws.close_tab("ghost"));
        assert!(ws.take_events().is_empty());
    }

    #[test]
    fn snapshot_prunes_vanished_tabs() {
        let mut ws = ws_with(&[("a", true), ("b", true)]);
        ws.select_device("a");
        ws.select_device("b");
        ws.take_events();
        ws.apply_snapshot(vec![dev("b", true)]);
        assert_eq!(ws.open_tabs(), ["b"]);
        assert!(ws.take_events().contains(&Event::TabsChanged));
    }

    #[test]
    fn snapshot_replaces_devices() {
        let mut ws = ws_with(&[("a", true), ("b", false)]);
        assert_eq!(ws.devices().len(), 2);
        ws.apply_snapshot(vec![dev("c", true)]);
        assert_eq!(ws.devices().len(), 1);
        assert_eq!(ws.devices()[0].id, "c");
        assert_eq!(ws.take_events(), vec![Event::DevicesChanged]);
    }

    #[test]
    fn snapshot_drops_selection_of_vanished_device() {
        let mut ws = ws_with(&[("a", true)]);
        ws.select_device("a");
        ws.take_events();
        ws.apply_snapshot(vec![dev("b", true)]);
        assert!(ws.selected_device().is_none());
        assert!(ws
            .take_events()
            .contains(&Event::SelectionChanged { device: None }));
    }

    #[test]
    fn snapshot_keeps_selection_of_surviving_device() {
        let mut ws = ws_with(&[("a", true)]);
        ws.select_device("a");
        ws.take_events();
        ws.apply_snapshot(vec![dev("a", true), dev("b", true)]);
        assert_eq!(ws.selected_device().unwrap().id, "a");
    }

    #[test]
    fn online_delta_inserts_new_device() {
        let mut ws = ws_with(&[("a", true)]);
        ws.apply_online(dev("b", false));
        assert_eq!(ws.devices().len(), 2);
        assert!(ws.devices().iter().find(|d| d.id == "b").unwrap().online);
    }

    #[test]
    fn online_delta_updates_existing_device() {
        let mut ws = ws_with(&[("a", false)]);
        ws.apply_online(dev("a", false));
        assert!(ws.devices()[0].online);
        assert_eq!(ws.devices().len(), 1);
    }

    #[test]
    fn offline_delta_marks_offline_and_closes_panes() {
        let mut ws = ws_with(&[("a", true)]);
        ws.select_device("a");
        ws.open_service(ServiceKind::Files);
        ws.take_events();

        ws.apply_offline("a");
        assert!(!ws.devices()[0].online);
        assert!(ws.panes().open.is_empty());
        let ev = ws.take_events();
        assert!(ev.contains(&Event::ServicesChanged { device: "a".into() }));
        assert!(ev.contains(&Event::DevicesChanged));
    }

    #[test]
    fn stale_offline_delta_for_unknown_id_is_ignored() {
        let mut ws = ws_with(&[("a", true)]);
        ws.apply_offline("ghost");
        assert!(ws.take_events().is_empty());
    }

    #[test]
    fn info_changed_renames_known_device_only() {
        let mut ws = ws_with(&[("a", true)]);
        let mut renamed = dev("a", true);
        renamed.name = "renamed".into();
        ws.apply_info_changed(renamed);
        assert_eq!(ws.devices()[0].name, "renamed");

        ws.take_events();
        ws.apply_info_changed(dev("ghost", true));
        assert_eq!(ws.devices().len(), 1);
        assert!(ws.take_events().is_empty());
    }

    #[test]
    fn select_unknown_device_fails() {
        let mut ws = ws_with(&[("a", true)]);
        assert!(!ws.select_device("ghost"));
        assert!(ws.take_events().is_empty());
    }

    #[test]
    fn reselecting_same_device_emits_nothing() {
        let mut ws = ws_with(&[("a", true)]);
        ws.select_device("a");
        ws.take_events();
        ws.select_device("a");
        assert!(ws.take_events().is_empty());
    }

    #[test]
    fn switching_devices_keeps_each_ones_open_panes() {
        let mut ws = ws_with(&[("a", true), ("b", true)]);
        ws.select_device("a");
        ws.open_service(ServiceKind::Files);
        ws.open_service(ServiceKind::Terminal);

        ws.select_device("b");
        assert!(ws.panes().open.is_empty(), "b starts clean");
        ws.open_service(ServiceKind::Desktop);

        ws.select_device("a");
        assert_eq!(
            ws.panes().open,
            vec![ServiceKind::Files, ServiceKind::Terminal]
        );
        assert_eq!(ws.panes().focused, Some(ServiceKind::Terminal));
    }

    #[test]
    fn files_then_terminal_then_desktop_tiling_flow() {
        let mut ws = ws_with(&[("a", true)]);
        ws.select_device("a");

        ws.open_service(ServiceKind::Files);
        assert_eq!(ws.panes().focused, Some(ServiceKind::Files));

        ws.open_service(ServiceKind::Terminal);
        assert_eq!(
            ws.panes().open,
            vec![ServiceKind::Files, ServiceKind::Terminal]
        );

        ws.open_service(ServiceKind::Desktop);
        let panes = ws.panes();
        assert_eq!(panes.focused, Some(ServiceKind::Desktop));
        assert_eq!(
            panes.open,
            vec![
                ServiceKind::Files,
                ServiceKind::Terminal,
                ServiceKind::Desktop
            ]
        );
    }

    #[test]
    fn reopening_an_open_service_only_refocuses() {
        let mut ws = ws_with(&[("a", true)]);
        ws.select_device("a");
        ws.open_service(ServiceKind::Files);
        ws.open_service(ServiceKind::Terminal);
        ws.open_service(ServiceKind::Files);
        let panes = ws.panes();
        assert_eq!(panes.open, vec![ServiceKind::Files, ServiceKind::Terminal]);
        assert_eq!(panes.focused, Some(ServiceKind::Files));
    }

    #[test]
    fn focus_service_promotes_thumbnail() {
        let mut ws = ws_with(&[("a", true)]);
        ws.select_device("a");
        ws.open_service(ServiceKind::Files);
        ws.open_service(ServiceKind::Desktop);
        assert!(ws.focus_service(ServiceKind::Files));
        assert_eq!(ws.panes().focused, Some(ServiceKind::Files));
        assert!(!ws.focus_service(ServiceKind::Network), "not open → no-op");
    }

    #[test]
    fn close_focused_falls_back_to_last_opened() {
        let mut ws = ws_with(&[("a", true)]);
        ws.select_device("a");
        ws.open_service(ServiceKind::Files);
        ws.open_service(ServiceKind::Terminal);
        ws.open_service(ServiceKind::Desktop);

        ws.close_service(ServiceKind::Desktop);
        assert_eq!(ws.panes().focused, Some(ServiceKind::Terminal));

        ws.close_service(ServiceKind::Files);
        assert_eq!(ws.panes().focused, Some(ServiceKind::Terminal));

        ws.close_service(ServiceKind::Terminal);
        assert!(ws.panes().open.is_empty());
        assert_eq!(ws.panes().focused, None);
    }

    #[test]
    fn open_service_requires_selected_online_device() {
        let mut ws = ws_with(&[("a", false)]);
        assert!(!ws.open_service(ServiceKind::Files), "nothing selected");
        ws.select_device("a");
        assert!(!ws.open_service(ServiceKind::Files), "device offline");
        assert!(ws
            .take_events()
            .iter()
            .all(|e| !matches!(e, Event::ServicesChanged { .. })));
    }

    #[test]
    fn map_peek_full_back_transitions() {
        let mut ws = Workspace::new();
        assert_eq!(ws.map(), MapView::Hidden);
        ws.map_toggle_peek();
        assert_eq!(ws.map(), MapView::Peek);
        ws.map_open_full();
        assert_eq!(ws.map(), MapView::Full);
        ws.map_toggle_peek();
        assert_eq!(ws.map(), MapView::Peek);
        ws.map_close();
        assert_eq!(ws.map(), MapView::Hidden);
        ws.map_toggle_peek();
        ws.map_toggle_peek();
        assert_eq!(ws.map(), MapView::Hidden);
    }

    #[test]
    fn chrome_toggles_emit_once_per_change() {
        let mut ws = Workspace::new();
        ws.set_rail_mode(RailViewMode::Icons);
        ws.set_rail_mode(RailViewMode::Icons);
        ws.set_theme(Theme::Dark);
        assert_eq!(
            ws.take_events(),
            vec![
                Event::RailModeChanged {
                    mode: RailViewMode::Icons
                },
                Event::ThemeChanged { theme: Theme::Dark },
            ]
        );
    }

    fn mount(id: &str, port: u16) -> MountInfo {
        MountInfo {
            resource_id: id.to_string(),
            name: format!("drive-{id}"),
            url: format!("dav://localhost:{port}/"),
            port,
        }
    }

    #[test]
    fn add_mount_appends_and_notifies() {
        let mut ws = Workspace::new();
        ws.add_mount(mount("r1", 8001));
        assert_eq!(ws.mounts().len(), 1);
        assert_eq!(ws.mounts()[0].port, 8001);
        assert_eq!(ws.take_events(), vec![Event::MountsChanged]);
        assert_eq!(ws.snapshot().mounts.len(), 1);
    }

    #[test]
    fn add_mount_is_idempotent_on_resource_id() {
        let mut ws = Workspace::new();
        ws.add_mount(mount("r1", 8001));
        ws.add_mount(mount("r1", 8002));
        assert_eq!(ws.mounts().len(), 1);
        assert_eq!(ws.mounts()[0].port, 8002);
    }

    #[test]
    fn remove_mount_drops_and_reports() {
        let mut ws = Workspace::new();
        ws.add_mount(mount("r1", 8001));
        ws.add_mount(mount("r2", 8002));
        ws.take_events();
        assert!(ws.remove_mount("r1"));
        assert_eq!(ws.mounts().len(), 1);
        assert_eq!(ws.mounts()[0].resource_id, "r2");
        assert_eq!(ws.take_events(), vec![Event::MountsChanged]);
        assert!(!ws.remove_mount("ghost"), "unknown id is a no-op");
        assert!(ws.take_events().is_empty());
    }

    #[test]
    fn mounts_changed_serializes_snake_case() {
        let json = serde_json::to_string(&Event::MountsChanged).unwrap();
        assert!(json.contains(r#""event":"mounts_changed""#), "{json}");
    }

    #[test]
    fn events_serialize_with_snake_case_tags() {
        let ev = Event::MapChanged {
            view: MapView::Full,
        };
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains(r#""event":"map_changed""#), "{json}");
        assert!(json.contains(r#""view":"full""#), "{json}");
    }
}

#[cfg(test)]
mod service_kind_tests {
    use super::ServiceKind;

    const ALL: [ServiceKind; 6] = [
        ServiceKind::Files,
        ServiceKind::Terminal,
        ServiceKind::Desktop,
        ServiceKind::Network,
        ServiceKind::Registry,
        ServiceKind::SystemInfo,
    ];

    #[test]
    fn the_id_matches_what_serde_writes() {
        for kind in ALL {
            let encoded = serde_json::to_string(&kind).expect("ServiceKind serialises");
            assert_eq!(
                encoded.trim_matches('"'),
                kind.id(),
                "{kind:?} serialises differently from its id()"
            );
        }
    }

    #[test]
    fn every_id_parses_back() {
        for kind in ALL {
            assert_eq!(ServiceKind::from_id(kind.id()), Some(kind));
        }
        assert_eq!(ServiceKind::from_id("nonsense"), None);
    }
}
