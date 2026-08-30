use adw::prelude::*;
use gtk::glib;
use nodeinnet_p2p::p2p::{FileDifference, SharedResource, SyncAction, SyncFileInfo};
use relm4::prelude::*;
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;
use uuid::Uuid;

const FETCH_COOLDOWN: Duration = Duration::from_millis(1500);

#[derive(Clone)]
pub struct DiffItem {
    pub peer_id: String,
    pub peer_name: String,
    pub peer_os: String,
    pub folder_name: String,
    pub relative_path: String,
    pub local_state: Option<SyncFileInfo>,
    pub remote_state: Option<SyncFileInfo>,
    pub action: SyncAction,
}

#[derive(Debug, Clone)]
pub enum SyncOp {
    DeleteRemote {
        peer_id: String,
        resource_id: String,
        path: String,
    },
    Download {
        peer_id: String,
        resource_id: String,
        file_path: String,
        transfer_id: Uuid,
    },
    Upload {
        peer_id: String,
        resource_id: String,
        target_path: String,
        file_name: String,
        total_size: u64,
        transfer_id: Uuid,
        permissions: Option<u32>,
        local_file_path: PathBuf,
    },
}

pub struct SyncFolderInit {
    pub show_toolbar: bool,
    pub config: client_config::AppConfig,
}

#[derive(Debug)]
pub enum SyncFolderInput {
    SetTarget {
        peer_id: String,
        resource_id: String,
    },
    RequestFetch,
    LocalScanReady {
        generation: u64,
        states: BTreeMap<String, Vec<SyncFileInfo>>,
    },
    FetchCooldownOver,
    PeerStates {
        resource_id: String,
        available: BTreeSet<String>,
        states: BTreeMap<String, Vec<SyncFileInfo>>,
    },
    NodesUpdated(Vec<nodeinnet_p2p::NodeInfo>),
    SetPolicy(u32),
    SetItemAction {
        index: usize,
        action: SyncAction,
    },
    SetSearch(String),
    Execute,
    TransferProgress {
        file_name: String,
        fraction: f64,
    },
}

#[derive(Debug)]
pub enum SyncFolderOutput {
    Back,
    RequestSyncStates { target_folder_names: Vec<String> },
    Execute(Vec<SyncOp>),
}

#[derive(Clone, Copy, PartialEq)]
enum FetchState {
    Idle,
    Scanning,
    WaitingForPeers,
}

pub struct SyncFolderModel {
    show_toolbar: bool,
    config: client_config::AppConfig,
    current_peer_id: String,
    current_resource_id: String,
    nodes: Vec<nodeinnet_p2p::NodeInfo>,
    local_states_cache: Option<BTreeMap<String, Vec<SyncFileInfo>>>,
    diff_items: Vec<DiffItem>,
    unmatched_folders: Vec<String>,
    policy: u32,
    search: String,
    fetch_state: FetchState,
    sync_enabled: bool,
    global_bar_visible: bool,
    file_progress: HashMap<String, f64>,
    list_generation: u64,
    fetch_generation: u64,
    cooldown_gate: Rc<Cell<bool>>,
}

pub struct SyncFolderWidgets {
    header: adw::HeaderBar,
    fetch_btn: gtk::Button,
    sync_btn: gtk::Button,
    policy_combo: gtk::DropDown,
    policy_handler: glib::SignalHandlerId,
    counters_label: gtk::Label,
    search_entry: gtk::SearchEntry,
    global_progress_bar: gtk::ProgressBar,
    unmatched_folders_box: gtk::Box,
    results_list: gtk::ListBox,
    row_progress_bars: HashMap<String, Vec<gtk::ProgressBar>>,
    row_action_labels: HashMap<usize, gtk::Label>,
    rendered_generation: u64,
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / 1048576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

fn format_date(ms: u64) -> String {
    use chrono::TimeZone;
    let dt = chrono::Utc
        .timestamp_millis_opt(ms as i64)
        .unwrap()
        .with_timezone(&chrono::Local);
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn default_action_for(
    policy: u32,
    local: &Option<SyncFileInfo>,
    remote: &Option<SyncFileInfo>,
) -> SyncAction {
    match policy {
        0 => SyncAction::Ignore,
        1 => {
            if remote.is_none() {
                SyncAction::Push
            } else if local.is_none() {
                SyncAction::DeleteRemote
            } else {
                SyncAction::Push
            }
        }
        2 => {
            if remote.is_none() {
                SyncAction::DeleteLocal
            } else {
                SyncAction::Pull
            }
        }
        3 => {
            if remote.is_none() {
                SyncAction::Push
            } else if local.is_none() {
                SyncAction::Pull
            } else {
                let l_time = local.as_ref().unwrap().last_modified_ms;
                let r_time = remote.as_ref().unwrap().last_modified_ms;
                if l_time > r_time {
                    SyncAction::Push
                } else {
                    SyncAction::Pull
                }
            }
        }
        _ => SyncAction::Ignore,
    }
}

fn app_resources(config: &client_config::AppConfig) -> Vec<SharedResource> {
    config
        .get::<Vec<SharedResource>>("app.resources")
        .unwrap_or_default()
}

fn counters_markup(diffs: &[DiffItem]) -> String {
    let mut push_new = 0;
    let mut push_upd = 0;
    let mut pull_new = 0;
    let mut pull_upd = 0;
    let mut del_remote = 0;
    let mut del_local = 0;

    for item in diffs {
        match item.action {
            SyncAction::Push => {
                if item.remote_state.is_none() {
                    push_new += 1;
                } else {
                    push_upd += 1;
                }
            }
            SyncAction::Pull => {
                if item.local_state.is_none() {
                    pull_new += 1;
                } else {
                    pull_upd += 1;
                }
            }
            SyncAction::DeleteRemote => del_remote += 1,
            SyncAction::DeleteLocal => del_local += 1,
            _ => {}
        }
    }

    let mut markup = String::new();
    let mut add_sep = false;
    let mut maybe_sep = |markup: &mut String| {
        if add_sep {
            markup.push_str("   ");
        }
        add_sep = true;
    };

    if del_remote > 0 {
        maybe_sep(&mut markup);
        markup.push_str(&format!(
            "<span foreground=\"red\">{}</span>",
            crate::i18n::trf(
                "sync.status_remote_deleted",
                &[("count", &*(del_remote).to_string())]
            )
        ));
    }
    if del_local > 0 {
        maybe_sep(&mut markup);
        markup.push_str(&format!(
            "<span foreground=\"red\">{}</span>",
            crate::i18n::trf(
                "sync.status_local_deleted",
                &[("count", &*(del_local).to_string())]
            )
        ));
    }
    if push_new > 0 || push_upd > 0 {
        maybe_sep(&mut markup);
        markup.push_str("<span foreground=\"green\">");
        markup.push_str(&crate::i18n::tr("sync.status_to_remote"));
        if push_new > 0 {
            markup.push_str(&crate::i18n::trf(
                "sync.status_new",
                &[("count", &*(push_new).to_string())],
            ));
        }
        if push_new > 0 && push_upd > 0 {
            markup.push_str(", ");
        }
        if push_upd > 0 {
            markup.push_str(&format!(
                "<span foreground=\"orange\">{}</span>",
                crate::i18n::trf("sync.status_upd", &[("count", &*(push_upd).to_string())])
            ));
        }
        markup.push_str("</span>");
    }
    if pull_new > 0 || pull_upd > 0 {
        maybe_sep(&mut markup);
        markup.push_str("<span foreground=\"green\">");
        markup.push_str(&crate::i18n::tr("sync.status_to_local"));
        if pull_new > 0 {
            markup.push_str(&crate::i18n::trf(
                "sync.status_new",
                &[("count", &*(pull_new).to_string())],
            ));
        }
        if pull_new > 0 && pull_upd > 0 {
            markup.push_str(", ");
        }
        if pull_upd > 0 {
            markup.push_str(&format!(
                "<span foreground=\"orange\">{}</span>",
                crate::i18n::trf("sync.status_upd", &[("count", &*(pull_upd).to_string())])
            ));
        }
        markup.push_str("</span>");
    }

    markup
}

fn action_desc(action: SyncAction, l_none: bool, r_none: bool) -> (String, &'static str) {
    match action {
        SyncAction::DeleteLocal => (crate::i18n::tr("sync.action_desc_delete_local"), "error"),
        SyncAction::DeleteRemote => (crate::i18n::tr("sync.action_desc_delete_remote"), "error"),
        SyncAction::Push => {
            if r_none {
                (crate::i18n::tr("sync.action_desc_new_sent"), "success")
            } else {
                (crate::i18n::tr("sync.action_desc_replacement"), "warning")
            }
        }
        SyncAction::Pull => {
            if l_none {
                (crate::i18n::tr("sync.action_desc_new_received"), "success")
            } else {
                (crate::i18n::tr("sync.action_desc_replacement"), "warning")
            }
        }
        SyncAction::Ignore => (crate::i18n::tr("sync.action_desc_ignored"), "dim-label"),
        SyncAction::NoneSelect => (String::new(), ""),
    }
}

impl SyncFolderModel {
    fn arm_cooldown(&self, sender: &ComponentSender<Self>) {
        self.cooldown_gate.set(true);
        let gate = self.cooldown_gate.clone();
        let sender = sender.clone();
        glib::timeout_add_local(FETCH_COOLDOWN, move || {
            if gate.get() {
                gate.set(false);
                sender.input(SyncFolderInput::FetchCooldownOver);
            }
            glib::ControlFlow::Break
        });
    }

    fn start_scan(&mut self, sender: &ComponentSender<Self>) {
        self.fetch_state = FetchState::Scanning;
        self.diff_items.clear();
        self.list_generation += 1;
        self.fetch_generation += 1;
        let generation = self.fetch_generation;
        let resources = app_resources(&self.config);
        let sender = sender.clone();
        std::thread::spawn(move || {
            let states = node_functions::sync_folder::scan_local_resource_folders(&resources);
            sender.input(SyncFolderInput::LocalScanReady { generation, states });
        });
    }
}

impl SimpleComponent for SyncFolderModel {
    type Init = SyncFolderInit;
    type Input = SyncFolderInput;
    type Output = SyncFolderOutput;
    type Root = gtk::Box;
    type Widgets = SyncFolderWidgets;

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Vertical, 0)
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        crate::init_resources();

        let empty_title = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let header = adw::HeaderBar::builder()
            .show_start_title_buttons(false)
            .show_end_title_buttons(false)
            .title_widget(&empty_title)
            .build();

        let btn_back = gtk::Button::builder()
            .child(&gtk::Image::from_resource(
                "/com/sync-ui/gtk/arrow-left.svg",
            ))
            .tooltip_text(&*crate::i18n::tr("sync.back_to_services"))
            .build();
        btn_back.set_cursor_from_name(Some("pointer"));
        let sender_back = sender.clone();
        btn_back.connect_clicked(move |_| {
            let _ = sender_back.output(SyncFolderOutput::Back);
        });
        header.pack_start(&btn_back);

        header.pack_start(
            &gtk::Separator::builder()
                .orientation(gtk::Orientation::Vertical)
                .margin_top(8)
                .margin_bottom(8)
                .build(),
        );

        let fetch_btn = gtk::Button::builder()
            .child(&gtk::Image::from_resource("/com/sync-ui/gtk/download.svg"))
            .tooltip_text(&*crate::i18n::tr("sync.fetch_sync_state"))
            .build();
        fetch_btn.set_cursor_from_name(Some("pointer"));
        let sender_fetch = sender.clone();
        fetch_btn.connect_clicked(move |_| {
            sender_fetch.input(SyncFolderInput::RequestFetch);
        });
        header.pack_start(&fetch_btn);

        header.pack_start(
            &gtk::Separator::builder()
                .orientation(gtk::Orientation::Vertical)
                .margin_top(8)
                .margin_bottom(8)
                .build(),
        );

        let sync_btn = gtk::Button::builder()
            .child(&gtk::Image::from_resource("/com/sync-ui/gtk/refresh.svg"))
            .tooltip_text(&*crate::i18n::tr("sync.execute_selected_actions"))
            .sensitive(false)
            .css_classes(vec!["suggested-action"])
            .build();
        sync_btn.set_cursor_from_name(Some("pointer"));
        let sender_exec = sender.clone();
        sync_btn.connect_clicked(move |_| {
            sender_exec.input(SyncFolderInput::Execute);
        });
        header.pack_end(&sync_btn);

        let counters_label = gtk::Label::builder()
            .use_markup(true)
            .valign(gtk::Align::Center)
            .margin_end(12)
            .build();
        header.pack_end(&counters_label);

        header.pack_start(
            &gtk::Separator::builder()
                .orientation(gtk::Orientation::Vertical)
                .margin_top(8)
                .margin_bottom(8)
                .build(),
        );

        let policy_label = gtk::Label::builder()
            .label(&*crate::i18n::tr("sync.sync_policies"))
            .margin_start(8)
            .margin_end(8)
            .css_classes(vec!["dim-label"])
            .build();
        header.pack_start(&policy_label);

        let policy_model = gtk::StringList::new(&[
            &crate::i18n::tr("common_forms.strategy_none"),
            &crate::i18n::tr("common_forms.strategy_source"),
            &crate::i18n::tr("common_forms.strategy_remote"),
            &crate::i18n::tr("common_forms.strategy_twoway"),
        ]);
        let policy_combo = gtk::DropDown::builder()
            .model(&policy_model)
            .valign(gtk::Align::Center)
            .build();
        let sender_policy = sender.clone();
        let policy_handler = policy_combo.connect_selected_notify(move |cb| {
            sender_policy.input(SyncFolderInput::SetPolicy(cb.selected()));
        });
        header.pack_start(&policy_combo);

        header.set_visible(init.show_toolbar);
        root.append(&header);

        let content_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content_box.set_margin_top(12);
        content_box.set_margin_bottom(12);
        content_box.set_margin_start(12);
        content_box.set_margin_end(12);

        let global_progress_bar = gtk::ProgressBar::builder()
            .visible(false)
            .valign(gtk::Align::Center)
            .margin_bottom(8)
            .build();

        let results_list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(vec!["content"])
            .build();

        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .child(&results_list)
            .vexpand(true)
            .hexpand(true)
            .build();

        let search_entry = gtk::SearchEntry::builder()
            .margin_bottom(12)
            .hexpand(true)
            .build();
        let sender_search = sender.clone();
        search_entry.connect_search_changed(move |e| {
            sender_search.input(SyncFolderInput::SetSearch(e.text().to_string()));
        });

        let unmatched_folders_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        unmatched_folders_box.set_margin_bottom(8);
        unmatched_folders_box.set_visible(false);

        content_box.append(&unmatched_folders_box);
        content_box.append(&search_entry);
        content_box.append(&global_progress_bar);
        content_box.append(&scroll);
        root.append(&content_box);

        let model = SyncFolderModel {
            show_toolbar: init.show_toolbar,
            config: init.config,
            current_peer_id: String::new(),
            current_resource_id: String::new(),
            nodes: Vec::new(),
            local_states_cache: None,
            diff_items: Vec::new(),
            unmatched_folders: Vec::new(),
            policy: 0,
            search: String::new(),
            fetch_state: FetchState::Idle,
            sync_enabled: false,
            global_bar_visible: false,
            file_progress: HashMap::new(),
            list_generation: 1,
            fetch_generation: 0,
            cooldown_gate: Rc::new(Cell::new(false)),
        };

        let widgets = SyncFolderWidgets {
            header,
            fetch_btn,
            sync_btn,
            policy_combo,
            policy_handler,
            counters_label,
            search_entry,
            global_progress_bar,
            unmatched_folders_box,
            results_list,
            row_progress_bars: HashMap::new(),
            row_action_labels: HashMap::new(),
            rendered_generation: 0,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            SyncFolderInput::SetTarget {
                peer_id,
                resource_id,
            } => {
                self.current_peer_id = peer_id;
                self.current_resource_id = resource_id.clone();

                self.policy = 0;
                if let Some(res) = app_resources(&self.config)
                    .iter()
                    .find(|r| r.id == resource_id)
                {
                    if let Some(cfg) = &res.config {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(cfg) {
                            if let Some(p) = json.get("default_policy").and_then(|p| p.as_u64()) {
                                self.policy = p as u32;
                            }
                        }
                    }
                }

                self.local_states_cache = None;
                self.unmatched_folders.clear();
                self.file_progress.clear();
                self.global_bar_visible = false;
                self.sync_enabled = false;
                self.start_scan(&sender);
            }
            SyncFolderInput::RequestFetch => {
                self.start_scan(&sender);
            }
            SyncFolderInput::LocalScanReady { generation, states } => {
                if generation != self.fetch_generation {
                    return;
                }
                let target_folder_names: Vec<String> = states.keys().cloned().collect();
                self.local_states_cache = Some(states);
                self.fetch_state = FetchState::WaitingForPeers;
                let _ = sender.output(SyncFolderOutput::RequestSyncStates {
                    target_folder_names,
                });
                self.arm_cooldown(&sender);
            }
            SyncFolderInput::FetchCooldownOver => {
                self.fetch_state = FetchState::Idle;
            }
            SyncFolderInput::PeerStates {
                resource_id,
                available,
                states,
            } => {
                self.fetch_state = FetchState::Idle;
                self.sync_enabled = true;

                let (peer_id, peer_name, peer_os) = self
                    .nodes
                    .iter()
                    .find(|n| n.resources.iter().any(|r| r.id == resource_id))
                    .map(|n| (n.id.clone(), n.name.clone(), n.os.clone()))
                    .unwrap_or_else(|| {
                        (
                            crate::i18n::tr("sync.unknown"),
                            crate::i18n::tr("sync.unknown_peer"),
                            "unknown".to_string(),
                        )
                    });

                let local_cache = self.local_states_cache.clone().unwrap_or_default();
                let local_folder_names: HashSet<String> = local_cache.keys().cloned().collect();

                self.unmatched_folders = available
                    .iter()
                    .filter(|f| !local_folder_names.contains(*f))
                    .cloned()
                    .collect();

                let computed =
                    node_functions::sync_folder::compute_sync_diff(&local_cache, &states);
                for d in computed {
                    if !local_folder_names.contains(&d.folder_name)
                        || !available.contains(&d.folder_name)
                    {
                        continue;
                    }
                    if d.difference != FileDifference::Identical {
                        let action =
                            default_action_for(self.policy, &d.local_state, &d.remote_state);
                        self.diff_items.push(DiffItem {
                            peer_id: peer_id.clone(),
                            peer_name: peer_name.clone(),
                            peer_os: peer_os.clone(),
                            folder_name: d.folder_name,
                            relative_path: d.relative_path,
                            local_state: d.local_state,
                            remote_state: d.remote_state,
                            action,
                        });
                    }
                }
                self.diff_items.sort_by(|a, b| {
                    a.folder_name
                        .cmp(&b.folder_name)
                        .then(a.relative_path.cmp(&b.relative_path))
                });
                self.list_generation += 1;
            }
            SyncFolderInput::NodesUpdated(nodes) => {
                self.nodes = nodes;
            }
            SyncFolderInput::SetPolicy(policy) => {
                self.policy = policy;
                for item in self.diff_items.iter_mut() {
                    item.action = default_action_for(policy, &item.local_state, &item.remote_state);
                }
                self.list_generation += 1;
            }
            SyncFolderInput::SetItemAction { index, action } => {
                if let Some(item) = self.diff_items.get_mut(index) {
                    item.action = action;
                }
            }
            SyncFolderInput::SetSearch(text) => {
                self.search = text;
                self.list_generation += 1;
            }
            SyncFolderInput::Execute => {
                self.global_bar_visible = true;
                self.file_progress.clear();

                let nodes = self.nodes.clone();
                let mut local_folder_paths: HashMap<String, PathBuf> = HashMap::new();
                for res in app_resources(&self.config) {
                    if res.resource_type == nodeinnet_p2p::ResourceType::SyncFolder {
                        if let Some(cfg_str) = &res.config {
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(cfg_str) {
                                if let Some(p) = json.get("path").and_then(|v| v.as_str()) {
                                    local_folder_paths.insert(res.name.clone(), PathBuf::from(p));
                                }
                            }
                        }
                    }
                }

                let mut ops: Vec<SyncOp> = Vec::new();
                for item in &self.diff_items {
                    if item.action == SyncAction::Ignore {
                        continue;
                    }
                    let Some(target_node) = nodes.iter().find(|n| n.id == item.peer_id) else {
                        continue;
                    };
                    let remote_res_id = target_node
                        .resources
                        .iter()
                        .find(|r| {
                            r.resource_type == nodeinnet_p2p::ResourceType::SyncFolder
                                && r.name == item.folder_name
                        })
                        .map(|r| r.id.clone())
                        .unwrap_or_default();
                    let local_base_opt = local_folder_paths.get(&item.folder_name).cloned();

                    if item.action == SyncAction::Pull || item.action == SyncAction::Push {
                        let file_name = item
                            .relative_path
                            .split('/')
                            .next_back()
                            .unwrap_or("")
                            .to_string();
                        self.file_progress.insert(file_name, 0.0);
                    }

                    match item.action {
                        SyncAction::DeleteLocal => {
                            if let Some(local_base) = local_base_opt {
                                let path = local_base.join(&item.relative_path);
                                std::thread::spawn(move || {
                                    let _ = std::fs::remove_file(path);
                                });
                            }
                        }
                        SyncAction::DeleteRemote if !remote_res_id.is_empty() => {
                            ops.push(SyncOp::DeleteRemote {
                                peer_id: item.peer_id.clone(),
                                resource_id: remote_res_id,
                                path: item.relative_path.clone(),
                            });
                        }
                        SyncAction::Pull if !remote_res_id.is_empty() => {
                            ops.push(SyncOp::Download {
                                peer_id: item.peer_id.clone(),
                                resource_id: remote_res_id,
                                file_path: item.relative_path.clone(),
                                transfer_id: Uuid::new_v4(),
                            });
                        }
                        SyncAction::Push => {
                            if remote_res_id.is_empty() {
                                continue;
                            }
                            let Some(local_base) = local_base_opt else {
                                continue;
                            };
                            let path = local_base.join(&item.relative_path);
                            let Ok(meta) = std::fs::metadata(&path) else {
                                continue;
                            };
                            let parts: Vec<&str> = item.relative_path.split('/').collect();
                            let target_path = if parts.len() > 1 {
                                parts[..parts.len() - 1].join("/")
                            } else {
                                "/".to_string()
                            };
                            let file_name = parts.last().unwrap_or(&"").to_string();

                            #[cfg(unix)]
                            let permissions = {
                                use std::os::unix::fs::MetadataExt;
                                Some(meta.mode())
                            };
                            #[cfg(not(unix))]
                            let permissions = None;

                            ops.push(SyncOp::Upload {
                                peer_id: item.peer_id.clone(),
                                resource_id: remote_res_id,
                                target_path,
                                file_name,
                                total_size: meta.len(),
                                transfer_id: Uuid::new_v4(),
                                permissions,
                                local_file_path: path,
                            });
                        }
                        _ => {}
                    }
                }

                self.sync_enabled = false;
                let _ = sender.output(SyncFolderOutput::Execute(ops));
            }
            SyncFolderInput::TransferProgress {
                file_name,
                fraction,
            } => {
                if let Some(v) = self.file_progress.get_mut(&file_name) {
                    *v = fraction;
                }
            }
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, sender: ComponentSender<Self>) {
        widgets.header.set_visible(self.show_toolbar);

        let (fetch_sensitive, fetch_tip) = match self.fetch_state {
            FetchState::Idle => (true, "sync.fetch_sync_state"),
            FetchState::Scanning => (false, "sync.scanning_local_disk"),
            FetchState::WaitingForPeers => (false, "sync.waiting_for_peers"),
        };
        widgets.fetch_btn.set_sensitive(fetch_sensitive);
        widgets
            .fetch_btn
            .set_tooltip_text(Some(&crate::i18n::tr(fetch_tip)));

        widgets.policy_combo.block_signal(&widgets.policy_handler);
        widgets.policy_combo.set_selected(self.policy);
        widgets.policy_combo.unblock_signal(&widgets.policy_handler);

        widgets
            .counters_label
            .set_markup(&counters_markup(&self.diff_items));

        widgets
            .global_progress_bar
            .set_visible(self.global_bar_visible);
        if self.global_bar_visible && !self.file_progress.is_empty() {
            let sum: f64 = self.file_progress.values().sum();
            let total = sum / self.file_progress.len() as f64;
            widgets.global_progress_bar.set_fraction(total);
            if total >= 0.999 {
                widgets.global_progress_bar.add_css_class("success");
            } else {
                widgets.global_progress_bar.remove_css_class("success");
            }
        }

        if widgets.rendered_generation != self.list_generation {
            widgets.rendered_generation = self.list_generation;
            rebuild_rows(self, widgets, &sender);
        } else {
            for (idx, lbl) in widgets.row_action_labels.iter() {
                if let Some(item) = self.diff_items.get(*idx) {
                    let (text, css) = action_desc(
                        item.action,
                        item.local_state.is_none(),
                        item.remote_state.is_none(),
                    );
                    lbl.set_css_classes(&[css]);
                    lbl.set_label(&text);
                }
            }
        }

        for (file_name, fraction) in &self.file_progress {
            if let Some(bars) = widgets.row_progress_bars.get(file_name) {
                for bar in bars {
                    bar.set_visible(true);
                    bar.set_fraction(*fraction);
                }
            }
        }

        let visible_count = self
            .diff_items
            .iter()
            .filter(|d| row_matches(d, &self.search))
            .count();
        widgets
            .sync_btn
            .set_sensitive(self.sync_enabled && visible_count > 0);
    }
}

fn row_matches(d: &DiffItem, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let q = query.to_lowercase();
    d.relative_path.to_lowercase().contains(&q) || d.folder_name.to_lowercase().contains(&q)
}

fn rebuild_rows(
    model: &SyncFolderModel,
    widgets: &mut SyncFolderWidgets,
    sender: &ComponentSender<SyncFolderModel>,
) {
    widgets.row_progress_bars.clear();
    widgets.row_action_labels.clear();

    let ub = &widgets.unmatched_folders_box;
    while let Some(child) = ub.first_child() {
        ub.remove(&child);
    }
    if model.unmatched_folders.is_empty() {
        ub.set_visible(false);
    } else {
        ub.append(
            &gtk::Label::builder()
                .label(&*crate::i18n::tr("sync.unmatched_folders_alert"))
                .css_classes(vec!["dim-label"])
                .wrap(true)
                .xalign(0.0)
                .build(),
        );
        for uf in &model.unmatched_folders {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let name_lbl = gtk::Label::builder()
                .label(uf)
                .css_classes(vec!["bold"])
                .build();
            let copy_btn = gtk::Button::builder()
                .icon_name("edit-copy")
                .tooltip_text(&*crate::i18n::tr("sync.copy_folder_name"))
                .build();
            let uf_cl = uf.clone();
            copy_btn.connect_clicked(move |_| {
                if let Some(display) = gtk::gdk::Display::default() {
                    display.clipboard().set_text(&uf_cl);
                }
            });
            row.append(&name_lbl);
            row.append(&copy_btn);
            ub.append(&row);
        }
        ub.set_visible(true);
    }

    let list = &widgets.results_list;
    while let Some(row) = list.first_child() {
        list.remove(&row);
    }

    if model.diff_items.is_empty() {
        widgets.search_entry.set_visible(false);
        list.append(
            &gtk::Label::builder()
                .label(&*crate::i18n::tr("sync.all_synchronized"))
                .margin_top(24)
                .margin_bottom(24)
                .css_classes(vec!["title-2"])
                .build(),
        );
        return;
    }

    widgets.search_entry.set_visible(true);

    let shown: Vec<(usize, &DiffItem)> = model
        .diff_items
        .iter()
        .enumerate()
        .filter(|(_, d)| row_matches(d, &model.search))
        .collect();

    if shown.is_empty() {
        list.append(
            &gtk::Label::builder()
                .label(&*crate::i18n::tr("sync.no_files_match"))
                .margin_top(24)
                .margin_bottom(24)
                .css_classes(vec!["dim-label"])
                .build(),
        );
        return;
    }

    let default_action_text = if model.policy == 0 {
        crate::i18n::tr("sync.action_ignore")
    } else {
        crate::i18n::tr("sync.action_default")
    };

    list.append(
        &gtk::Label::builder()
            .label(&*crate::i18n::tr("sync.global_policies_preapplied"))
            .css_classes(vec!["dim-label"])
            .margin_top(12)
            .margin_bottom(12)
            .build(),
    );

    for (index, item) in shown {
        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row_box.set_margin_start(12);
        row_box.set_margin_end(12);
        row_box.set_margin_top(8);
        row_box.set_margin_bottom(8);

        let status_icon = gtk::Image::new();

        let peer_icon_name = match item.peer_os.to_lowercase().as_str() {
            "android" => "phone-symbolic",
            "windows" => "computer-symbolic",
            "macos" => "computer-apple-symbolic",
            _ => "go-home-symbolic",
        };
        let peer_icon = gtk::Image::from_icon_name(peer_icon_name);
        peer_icon.add_css_class("dim-label");
        let peer_label = gtk::Label::builder()
            .label(&item.peer_name)
            .css_classes(vec!["dim-label"])
            .build();
        let peer_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        peer_box.append(&peer_icon);
        peer_box.append(&peer_label);
        peer_box.set_valign(gtk::Align::Center);

        let mut info_text = format!("<b>[{}]</b> {}", item.folder_name, item.relative_path);
        let mut options: Vec<(String, SyncAction)> = Vec::new();

        match (&item.local_state, &item.remote_state) {
            (Some(l), Some(r)) => {
                status_icon.set_icon_name(Some("emblem-synchronizing-symbolic"));
                status_icon.add_css_class("warning");
                let diff_s = if l.last_modified_ms > r.last_modified_ms {
                    crate::i18n::tr("sync.diff_local_newer")
                } else {
                    crate::i18n::tr("sync.diff_remote_newer")
                };
                let l_md5_short = if l.md5_hash.len() > 8 {
                    &l.md5_hash[..8]
                } else {
                    &l.md5_hash
                };
                let r_md5_short = if r.md5_hash.len() > 8 {
                    &r.md5_hash[..8]
                } else {
                    &r.md5_hash
                };
                info_text.push_str(&crate::i18n::trf(
                    "sync.diff_modified_desc",
                    &[
                        ("diff", &*(diff_s).to_string()),
                        ("l_size", &*(format_size(l.size)).to_string()),
                        ("l_date", &*(format_date(l.last_modified_ms)).to_string()),
                        ("l_md5", &*(l_md5_short).to_string()),
                        ("r_size", &*(format_size(r.size)).to_string()),
                        ("r_date", &*(format_date(r.last_modified_ms)).to_string()),
                        ("r_md5", &*(r_md5_short).to_string()),
                    ],
                ));
                options.push((
                    crate::i18n::tr("sync.action_push_remote").to_string(),
                    SyncAction::Push,
                ));
                options.push((
                    crate::i18n::tr("sync.action_pull_remote").to_string(),
                    SyncAction::Pull,
                ));
                options.push((default_action_text.to_string(), SyncAction::Ignore));
            }
            (Some(l), None) => {
                status_icon.set_icon_name(Some("document-new-symbolic"));
                status_icon.add_css_class("success");
                info_text.push_str(&crate::i18n::trf(
                    "sync.diff_local_only_desc",
                    &[
                        ("size", &*(format_size(l.size)).to_string()),
                        ("date", &*(format_date(l.last_modified_ms)).to_string()),
                    ],
                ));
                options.push((
                    crate::i18n::tr("sync.action_push_remote").to_string(),
                    SyncAction::Push,
                ));
                options.push((
                    crate::i18n::tr("sync.action_delete_local").to_string(),
                    SyncAction::DeleteLocal,
                ));
                options.push((default_action_text.to_string(), SyncAction::Ignore));
            }
            (None, remote) => {
                status_icon.set_icon_name(Some("folder-download-symbolic"));
                status_icon.add_css_class("accent");
                if let Some(r) = remote {
                    info_text.push_str(&crate::i18n::trf(
                        "sync.diff_remote_only_desc",
                        &[
                            ("size", &*(format_size(r.size)).to_string()),
                            ("date", &*(format_date(r.last_modified_ms)).to_string()),
                        ],
                    ));
                }
                options.push((
                    crate::i18n::tr("sync.action_pull_remote").to_string(),
                    SyncAction::Pull,
                ));
                options.push((
                    crate::i18n::tr("sync.action_delete_remote").to_string(),
                    SyncAction::DeleteRemote,
                ));
                options.push((default_action_text.to_string(), SyncAction::Ignore));
            }
        }

        let info_label = gtk::Label::builder()
            .use_markup(true)
            .label(&info_text)
            .xalign(0.0)
            .build();

        let action_result_lbl = gtk::Label::builder()
            .use_markup(true)
            .xalign(0.0)
            .margin_top(2)
            .build();
        let (desc_text, desc_css) = action_desc(
            item.action,
            item.local_state.is_none(),
            item.remote_state.is_none(),
        );
        action_result_lbl.set_css_classes(&[desc_css]);
        action_result_lbl.set_label(&desc_text);
        widgets
            .row_action_labels
            .insert(index, action_result_lbl.clone());

        let progress_bar = gtk::ProgressBar::builder()
            .visible(false)
            .margin_top(4)
            .margin_bottom(4)
            .build();
        let file_name = item
            .relative_path
            .split('/')
            .next_back()
            .unwrap_or("")
            .to_string();
        widgets
            .row_progress_bars
            .entry(file_name)
            .or_default()
            .push(progress_bar.clone());

        let text_vbox = gtk::Box::new(gtk::Orientation::Vertical, 4);
        text_vbox.append(&info_label);
        text_vbox.append(&action_result_lbl);
        text_vbox.append(&peer_box);
        text_vbox.append(&progress_bar);
        text_vbox.set_hexpand(true);
        text_vbox.set_valign(gtk::Align::Center);

        let mut string_array = Vec::new();
        let mut active_idx = 0u32;
        for (idx, (desc, act)) in options.iter().enumerate() {
            string_array.push(desc.as_str());
            if *act == item.action {
                active_idx = idx as u32;
            }
        }

        let combo = gtk::DropDown::builder()
            .model(&gtk::StringList::new(&string_array))
            .valign(gtk::Align::Center)
            .selected(active_idx)
            .build();
        let options_owned = options.clone();
        let sender_row = sender.clone();
        combo.connect_selected_notify(move |cb| {
            let idx = cb.selected() as usize;
            if let Some((_, action)) = options_owned.get(idx) {
                sender_row.input(SyncFolderInput::SetItemAction {
                    index,
                    action: *action,
                });
            }
        });

        row_box.append(&status_icon);
        row_box.append(&text_vbox);
        row_box.append(&combo);
        list.append(&row_box);
    }
}
