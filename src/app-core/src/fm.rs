use fm_core::rpc::{FileSystemRpc, RemoteFileEntry};
use serde::{Deserialize, Serialize};
use std::rc::Rc;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    #[default]
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: u64,
    pub permissions: Option<u32>,
}

impl From<&RemoteFileEntry> for FileEntry {
    fn from(e: &RemoteFileEntry) -> Self {
        Self {
            name: e.name.clone(),
            is_dir: e.is_dir,
            size: e.size,
            modified: e.modified,
            permissions: e.permissions,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Crumb {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceTab {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelState {
    pub resources: Vec<ResourceTab>,
    pub active_resource: Option<usize>,
    pub cwd: String,
    pub breadcrumb: Vec<Crumb>,
    pub entries: Vec<FileEntry>,
    pub cursor: usize,
    pub selection: Vec<usize>,
    pub can_back: bool,
    pub can_forward: bool,
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mounted_as: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferKind {
    Copy,
    Move,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnConflict {
    Replace,
    Skip,
    KeepBoth,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferItem {
    pub name: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferFrom {
    OtherPane(TransferKind),
    Clipboard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferPlan {
    pub kind: TransferKind,
    pub items: Vec<TransferItem>,
    pub conflicts: Vec<String>,
    pub from: String,
    pub to: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardView {
    pub kind: TransferKind,
    pub count: usize,
    pub side: Side,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    PanelUpdated { side: Side, panel: PanelState },
    ActivePanel { side: Side },
    ClipboardChanged { clipboard: Option<ClipboardView> },
}

struct Clipboard {
    kind: TransferKind,
    provider: Rc<dyn FileSystemRpc>,
    label: String,
    side: Side,
    cwd: String,
    items: Vec<TransferItem>,
}

struct Source {
    kind: TransferKind,
    provider: Rc<dyn FileSystemRpc>,
    cwd: String,
    items: Vec<TransferItem>,
    side: Option<Side>,
}

#[derive(Default)]
struct Panel {
    providers: Vec<Rc<dyn FileSystemRpc>>,
    back: Vec<String>,
    forward: Vec<String>,
    state: PanelState,
}

#[derive(Default)]
pub struct FileManager {
    left: Panel,
    right: Panel,
    active: Side,
    events: Vec<Event>,
    clipboard: Option<Clipboard>,
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn free_name(name: &str, taken: &[String]) -> String {
    if !taken.iter().any(|t| t == name) {
        return name.to_string();
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (2..)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|c| !taken.iter().any(|t| t == c))
        .unwrap_or_else(|| name.to_string())
}

fn join(cwd: &str, name: &str) -> String {
    if cwd.ends_with('/') {
        format!("{cwd}{name}")
    } else {
        format!("{cwd}/{name}")
    }
}

async fn ensure_dir(
    provider: &Rc<dyn FileSystemRpc>,
    parent: &str,
    name: &str,
) -> Result<(), common::AppError> {
    match provider
        .create_directory(parent.to_string(), name.to_string(), None)
        .await
    {
        Ok(()) => Ok(()),
        Err(e) => match provider.list_dir(join(parent, name)).await {
            Ok(_) => Ok(()),
            Err(_) => Err(e),
        },
    }
}

async fn copy_entry(
    src_provider: &Rc<dyn FileSystemRpc>,
    dst_provider: &Rc<dyn FileSystemRpc>,
    src_path: &str,
    dst_parent: &str,
    dst_name: &str,
    is_dir: bool,
) -> Result<(), common::AppError> {
    if !is_dir {
        let bytes = src_provider.read_file(src_path.to_string(), None).await?;
        return dst_provider
            .write_file(join(dst_parent, dst_name), bytes, None, None)
            .await;
    }

    let mut queue = vec![(
        src_path.to_string(),
        dst_parent.to_string(),
        dst_name.to_string(),
    )];
    while let Some((from, to_parent, dir_name)) = queue.pop() {
        ensure_dir(dst_provider, &to_parent, &dir_name).await?;
        let here = join(&to_parent, &dir_name);
        for entry in src_provider.list_dir(from.clone()).await? {
            let child = join(&from, &entry.name);
            if entry.is_dir {
                queue.push((child, here.clone(), entry.name.clone()));
            } else {
                let bytes = src_provider.read_file(child, None).await?;
                dst_provider
                    .write_file(join(&here, &entry.name), bytes, None, None)
                    .await?;
            }
        }
    }
    Ok(())
}

impl FileManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn panel(&self, side: Side) -> &PanelState {
        &self.panel_ref(side).state
    }

    pub fn active(&self) -> Side {
        self.active
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    fn panel_ref(&self, side: Side) -> &Panel {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }

    fn other(side: Side) -> Side {
        match side {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }

    fn panel_mut(&mut self, side: Side) -> &mut Panel {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        }
    }

    fn provider(&self, side: Side) -> Option<Rc<dyn FileSystemRpc>> {
        let p = self.panel_ref(side);
        p.state
            .active_resource
            .and_then(|i| p.providers.get(i))
            .cloned()
    }

    pub fn active_provider(&self, side: Side) -> Option<Rc<dyn FileSystemRpc>> {
        self.provider(side)
    }

    pub fn set_mounted(&mut self, side: Side, url: Option<String>) {
        self.panel_mut(side).state.mounted_as = url;
        self.emit(side);
    }

    fn emit(&mut self, side: Side) {
        let p = self.panel_mut(side);
        p.state.can_back = !p.back.is_empty();
        p.state.can_forward = !p.forward.is_empty();
        let panel = p.state.clone();
        self.events.push(Event::PanelUpdated { side, panel });
    }

    pub fn set_resources(
        &mut self,
        side: Side,
        resources: Vec<(ResourceTab, Rc<dyn FileSystemRpc>)>,
    ) {
        let p = self.panel_mut(side);
        let (tabs, providers): (Vec<_>, Vec<_>) = resources.into_iter().unzip();
        let active_resource = if providers.is_empty() { None } else { Some(0) };
        *p = Panel {
            providers,
            state: PanelState {
                resources: tabs,
                active_resource,
                ..PanelState::default()
            },
            ..Panel::default()
        };
        self.emit(side);
    }

    pub async fn select_resource(&mut self, side: Side, index: usize) -> bool {
        if self.panel_ref(side).providers.get(index).is_none() {
            return false;
        }
        {
            let p = self.panel_mut(side);
            p.state.active_resource = Some(index);
            p.back.clear();
            p.forward.clear();
        }
        let ok = self.set_dir(side, "/", false).await;
        self.emit(side);
        ok
    }

    async fn set_dir(&mut self, side: Side, path: &str, keep_cursor: bool) -> bool {
        let Some(provider) = self.provider(side) else {
            self.panel_mut(side).state.last_error = Some("no resource selected".into());
            return false;
        };
        match provider.list_dir(path.to_string()).await {
            Ok(mut list) => {
                list.sort_by(|a, b| {
                    b.is_dir
                        .cmp(&a.is_dir)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
                let crumbs = provider
                    .get_path_segments(path)
                    .into_iter()
                    .map(|s| Crumb {
                        name: s.name,
                        path: s.path,
                    })
                    .collect();
                let p = self.panel_mut(side);
                p.state.entries = list.iter().map(FileEntry::from).collect();
                p.state.cwd = path.to_string();
                p.state.breadcrumb = crumbs;
                p.state.cursor = if keep_cursor {
                    p.state.cursor.min(p.state.entries.len().saturating_sub(1))
                } else {
                    0
                };
                p.state.selection.clear();
                p.state.last_error = None;
                true
            }
            Err(e) => {
                self.panel_mut(side).state.last_error = Some(e.to_string());
                false
            }
        }
    }

    pub async fn navigate(&mut self, side: Side, path: &str) -> bool {
        let old_cwd = self.panel_ref(side).state.cwd.clone();
        let ok = self.set_dir(side, path, false).await;
        if ok && old_cwd != path && !old_cwd.is_empty() {
            let p = self.panel_mut(side);
            p.back.push(old_cwd);
            p.forward.clear();
        }
        self.emit(side);
        ok
    }

    pub async fn enter(&mut self, side: Side) -> bool {
        let st = &self.panel_ref(side).state;
        let Some(entry) = st.entries.get(st.cursor) else {
            return false;
        };
        if !entry.is_dir {
            return false;
        }
        let target = join(&st.cwd, &entry.name);
        self.navigate(side, &target).await
    }

    pub async fn breadcrumb(&mut self, side: Side, index: usize) -> bool {
        let Some(crumb) = self.panel_ref(side).state.breadcrumb.get(index) else {
            return false;
        };
        let path = crumb.path.clone();
        self.navigate(side, &path).await
    }

    pub async fn up(&mut self, side: Side) -> bool {
        let crumbs = &self.panel_ref(side).state.breadcrumb;
        let parent = match crumbs.len() {
            0 => return false,
            1 => "/".to_string(),
            n => crumbs[n - 2].path.clone(),
        };
        self.navigate(side, &parent).await
    }

    pub async fn back(&mut self, side: Side) -> bool {
        let Some(prev) = self.panel_mut(side).back.pop() else {
            return false;
        };
        let cur = self.panel_ref(side).state.cwd.clone();
        let ok = self.set_dir(side, &prev, false).await;
        let p = self.panel_mut(side);
        if ok {
            p.forward.push(cur);
        } else {
            p.back.push(prev);
        }
        self.emit(side);
        ok
    }

    pub async fn forward(&mut self, side: Side) -> bool {
        let Some(next) = self.panel_mut(side).forward.pop() else {
            return false;
        };
        let cur = self.panel_ref(side).state.cwd.clone();
        let ok = self.set_dir(side, &next, false).await;
        let p = self.panel_mut(side);
        if ok {
            p.back.push(cur);
        } else {
            p.forward.push(next);
        }
        self.emit(side);
        ok
    }

    pub async fn refresh(&mut self, side: Side) -> bool {
        let cwd = self.panel_ref(side).state.cwd.clone();
        if cwd.is_empty() {
            return false;
        }
        let ok = self.set_dir(side, &cwd, true).await;
        self.emit(side);
        ok
    }

    pub fn set_cursor(&mut self, side: Side, index: usize) {
        let p = self.panel_mut(side);
        p.state.cursor = index.min(p.state.entries.len().saturating_sub(1));
        self.emit(side);
    }

    pub fn toggle_select(&mut self, side: Side, index: usize) {
        let p = self.panel_mut(side);
        if index >= p.state.entries.len() {
            return;
        }
        match p.state.selection.iter().position(|i| *i == index) {
            Some(pos) => {
                p.state.selection.remove(pos);
            }
            None => {
                p.state.selection.push(index);
                p.state.selection.sort_unstable();
            }
        }
        self.emit(side);
    }

    pub fn activate(&mut self, side: Side) {
        if self.active != side {
            self.active = side;
            self.events.push(Event::ActivePanel { side });
        }
    }

    pub async fn mkdir(&mut self, side: Side, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let Some(provider) = self.provider(side) else {
            return false;
        };
        let cwd = self.panel_ref(side).state.cwd.clone();
        let ok = match provider
            .create_directory(cwd.clone(), name.to_string(), None)
            .await
        {
            Ok(()) => self.set_dir(side, &cwd, true).await,
            Err(e) => {
                self.panel_mut(side).state.last_error = Some(e.to_string());
                false
            }
        };
        self.emit(side);
        ok
    }

    pub async fn delete_selected(&mut self, side: Side) -> bool {
        let Some(provider) = self.provider(side) else {
            return false;
        };
        let st = &self.panel_ref(side).state;
        let indices: Vec<usize> = if st.selection.is_empty() {
            if st.entries.is_empty() {
                return false;
            }
            vec![st.cursor]
        } else {
            st.selection.clone()
        };
        let paths: Vec<String> = indices
            .iter()
            .filter_map(|i| st.entries.get(*i))
            .map(|e| join(&st.cwd, &e.name))
            .collect();
        let cwd = st.cwd.clone();
        let ok = match provider.delete_entries(paths).await {
            Ok(()) => self.set_dir(side, &cwd, true).await,
            Err(e) => {
                self.panel_mut(side).state.last_error = Some(e.to_string());
                false
            }
        };
        self.emit(side);
        ok
    }

    fn marked(&self, side: Side) -> (String, Vec<TransferItem>) {
        let st = &self.panel_ref(side).state;
        let indices: Vec<usize> = if st.selection.is_empty() {
            if st.entries.is_empty() {
                Vec::new()
            } else {
                vec![st.cursor]
            }
        } else {
            st.selection.clone()
        };
        let items = indices
            .iter()
            .filter_map(|i| st.entries.get(*i))
            .filter(|e| e.name != "." && e.name != "..")
            .map(|e| TransferItem {
                name: e.name.clone(),
                is_dir: e.is_dir,
            })
            .collect();
        (st.cwd.clone(), items)
    }

    fn resource_label(&self, side: Side) -> String {
        let st = &self.panel_ref(side).state;
        st.active_resource
            .and_then(|i| st.resources.get(i))
            .map(|r| r.label.clone())
            .unwrap_or_default()
    }

    pub fn set_clipboard(&mut self, side: Side, kind: TransferKind) -> bool {
        let Some(provider) = self.provider(side) else {
            return false;
        };
        let (cwd, items) = self.marked(side);
        if items.is_empty() {
            return false;
        }
        self.clipboard = Some(Clipboard {
            kind,
            provider,
            label: self.resource_label(side),
            side,
            cwd,
            items,
        });
        self.emit_clipboard();
        true
    }

    pub fn clear_clipboard(&mut self) {
        if self.clipboard.take().is_some() {
            self.emit_clipboard();
        }
    }

    pub fn clipboard(&self) -> Option<ClipboardView> {
        self.clipboard.as_ref().map(|c| ClipboardView {
            kind: c.kind,
            count: c.items.len(),
            side: c.side,
            source: if c.label.is_empty() {
                c.cwd.clone()
            } else {
                format!("{}:{}", c.label, c.cwd)
            },
        })
    }

    fn emit_clipboard(&mut self) {
        let clipboard = self.clipboard();
        self.events.push(Event::ClipboardChanged { clipboard });
    }

    fn source(&self, from: TransferFrom, dest: Side) -> Result<Source, String> {
        match from {
            TransferFrom::OtherPane(kind) => {
                let side = Self::other(dest);
                let provider = self
                    .provider(side)
                    .ok_or_else(|| "both panes need a resource".to_string())?;
                let (cwd, items) = self.marked(side);
                Ok(Source {
                    kind,
                    provider,
                    cwd,
                    items,
                    side: Some(side),
                })
            }
            TransferFrom::Clipboard => {
                let c = self
                    .clipboard
                    .as_ref()
                    .ok_or_else(|| "the clipboard is empty".to_string())?;
                Ok(Source {
                    kind: c.kind,
                    provider: c.provider.clone(),
                    cwd: c.cwd.clone(),
                    items: c.items.clone(),
                    side: Some(c.side),
                })
            }
        }
    }

    fn dest_dir(&self, dest: Side, into: Option<&str>) -> String {
        let cwd = self.panel_ref(dest).state.cwd.clone();
        match into {
            Some(name) if !name.is_empty() => join(&cwd, name),
            _ => cwd,
        }
    }

    pub async fn plan_transfer(
        &mut self,
        dest: Side,
        from: TransferFrom,
        into: Option<String>,
    ) -> TransferPlan {
        let to = self.dest_dir(dest, into.as_deref());
        let (kind, items, src_cwd, error) = match self.source(from, dest) {
            Ok(src) => (src.kind, src.items, src.cwd, None),
            Err(e) => (
                match from {
                    TransferFrom::OtherPane(k) => k,
                    TransferFrom::Clipboard => TransferKind::Copy,
                },
                Vec::new(),
                String::new(),
                Some(e),
            ),
        };
        let mut plan = TransferPlan {
            kind,
            items,
            conflicts: Vec::new(),
            from: src_cwd,
            to: to.clone(),
            error,
        };
        if plan.error.is_some() {
            return plan;
        }
        if plan.items.is_empty() {
            plan.error = Some("nothing selected".into());
            return plan;
        }
        let Some(dst_provider) = self.provider(dest) else {
            plan.error = Some("both panes need a resource".into());
            return plan;
        };
        if let Ok(existing) = dst_provider.list_dir(to).await {
            let taken: Vec<&str> = existing.iter().map(|e| e.name.as_str()).collect();
            plan.conflicts = plan
                .items
                .iter()
                .filter(|i| taken.contains(&i.name.as_str()))
                .map(|i| i.name.clone())
                .collect();
        }
        plan
    }

    pub async fn run_transfer(
        &mut self,
        dest: Side,
        from: TransferFrom,
        into: Option<String>,
        resolutions: Vec<(String, OnConflict)>,
    ) -> bool {
        let src = match self.source(from, dest) {
            Ok(src) => src,
            Err(e) => {
                self.panel_mut(dest).state.last_error = Some(e);
                self.emit(dest);
                return false;
            }
        };
        let Some(dst_provider) = self.provider(dest) else {
            self.panel_mut(dest).state.last_error = Some("both panes need a resource".into());
            self.emit(dest);
            return false;
        };
        if src.items.is_empty() {
            self.panel_mut(dest).state.last_error = Some("nothing selected".into());
            self.emit(dest);
            return false;
        }

        let dst_dir = self.dest_dir(dest, into.as_deref());
        let mut taken: Vec<String> = dst_provider
            .list_dir(dst_dir.clone())
            .await
            .map(|e| e.into_iter().map(|e| e.name).collect())
            .unwrap_or_default();

        let same_provider = Rc::ptr_eq(&src.provider, &dst_provider);
        let moving = src.kind == TransferKind::Move;

        let mut ok_all = true;
        let mut last_err: Option<String> = None;
        for item in &src.items {
            let src_path = join(&src.cwd, &item.name);
            let dst_name = match resolutions
                .iter()
                .find(|(n, _)| n == &item.name)
                .map(|(_, r)| *r)
            {
                Some(OnConflict::Skip) => continue,
                Some(OnConflict::KeepBoth) => free_name(&item.name, &taken),
                Some(OnConflict::Replace) | None => item.name.clone(),
            };
            if same_provider && src_path == join(&dst_dir, &dst_name) {
                continue;
            }
            taken.push(dst_name.clone());

            if moving && same_provider {
                if let Err(e) = src
                    .provider
                    .rename_entry(src_path, join(&dst_dir, &dst_name))
                    .await
                {
                    last_err = Some(e.to_string());
                    ok_all = false;
                }
                continue;
            }
            match copy_entry(
                &src.provider,
                &dst_provider,
                &src_path,
                &dst_dir,
                &dst_name,
                item.is_dir,
            )
            .await
            {
                Ok(()) => {
                    if moving {
                        if let Err(e) = src.provider.delete_entries(vec![src_path]).await {
                            last_err = Some(e.to_string());
                            ok_all = false;
                        }
                    }
                }
                Err(e) => {
                    last_err = Some(e.to_string());
                    ok_all = false;
                }
            }
        }

        if from == TransferFrom::Clipboard {
            self.clipboard = None;
            self.emit_clipboard();
        }

        let dest_cwd = self.panel_ref(dest).state.cwd.clone();
        let mut refreshed = self.set_dir(dest, &dest_cwd, true).await;
        if let Some(e) = last_err {
            self.panel_mut(dest).state.last_error = Some(e);
        }
        self.emit(dest);
        if moving {
            if let Some(side) = src.side {
                let src_cwd = self.panel_ref(side).state.cwd.clone();
                refreshed = self.set_dir(side, &src_cwd, true).await && refreshed;
                self.emit(side);
            }
        }
        ok_all && refreshed
    }

    pub async fn copy_to(&mut self, dest: Side) -> bool {
        self.run_transfer(
            dest,
            TransferFrom::OtherPane(TransferKind::Copy),
            None,
            Vec::new(),
        )
        .await
    }

    pub async fn move_to(&mut self, dest: Side) -> bool {
        self.run_transfer(
            dest,
            TransferFrom::OtherPane(TransferKind::Move),
            None,
            Vec::new(),
        )
        .await
    }

    pub async fn duplicate(&mut self, side: Side, src: String, dst: String) -> bool {
        let Some(provider) = self.provider(side) else {
            return false;
        };
        let (cwd, is_dir) = {
            let st = &self.panel_ref(side).state;
            let name = basename(&src);
            let is_dir = st
                .entries
                .iter()
                .find(|e| e.name == name)
                .map(|e| e.is_dir)
                .unwrap_or(false);
            (st.cwd.clone(), is_dir)
        };
        let name = basename(&dst);
        let ok = match copy_entry(&provider, &provider, &src, &cwd, name, is_dir).await {
            Ok(()) => self.set_dir(side, &cwd, true).await,
            Err(e) => {
                self.panel_mut(side).state.last_error = Some(e.to_string());
                false
            }
        };
        self.emit(side);
        ok
    }

    pub async fn upload_local(
        &mut self,
        side: Side,
        dir: String,
        files: Vec<std::path::PathBuf>,
    ) -> bool {
        let Some(provider) = self.provider(side) else {
            return false;
        };
        let mut ok_all = true;
        let mut last_err: Option<String> = None;
        for path in files {
            let (Some(parent), Some(name)) = (
                path.parent().map(|p| p.to_path_buf()),
                path.file_name().map(|n| n.to_string_lossy().to_string()),
            ) else {
                last_err = Some(format!("cannot take {}", path.display()));
                ok_all = false;
                continue;
            };
            let is_dir = path.is_dir();
            let local: Rc<dyn FileSystemRpc> = Rc::new(crate::local_fs::LocalFsRpc::new(parent));
            let src = format!("/{name}");
            if let Err(e) = copy_entry(&local, &provider, &src, &dir, &name, is_dir).await {
                last_err = Some(e.to_string());
                ok_all = false;
            }
        }
        let cwd = self.panel_ref(side).state.cwd.clone();
        let refreshed = self.set_dir(side, &cwd, true).await;
        if let Some(e) = last_err {
            self.panel_mut(side).state.last_error = Some(e);
        }
        self.emit(side);
        ok_all && refreshed
    }

    pub async fn rename_cursor(&mut self, side: Side, new_name: &str) -> bool {
        let new_name = new_name.trim();
        if new_name.is_empty() || new_name.contains('/') {
            return false;
        }
        let Some(provider) = self.provider(side) else {
            return false;
        };
        let st = &self.panel_ref(side).state;
        let Some(entry) = st.entries.get(st.cursor) else {
            return false;
        };
        let old_path = join(&st.cwd, &entry.name);
        let new_path = join(&st.cwd, new_name);
        let cwd = st.cwd.clone();
        let ok = match provider.rename_entry(old_path, new_path).await {
            Ok(()) => self.set_dir(side, &cwd, true).await,
            Err(e) => {
                self.panel_mut(side).state.last_error = Some(e.to_string());
                false
            }
        };
        self.emit(side);
        ok
    }

    pub async fn chmod(&mut self, side: Side, path: &str, mode: u32) -> bool {
        let Some(provider) = self.provider(side) else {
            return false;
        };
        let cwd = self.panel_ref(side).state.cwd.clone();
        let ok = match provider.set_permissions(path.to_string(), mode).await {
            Ok(()) => self.set_dir(side, &cwd, true).await,
            Err(e) => {
                self.panel_mut(side).state.last_error = Some(e.to_string());
                false
            }
        };
        self.emit(side);
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::AppError;
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;

    fn fentry(name: &str, is_dir: bool, size: u64) -> RemoteFileEntry {
        RemoteFileEntry {
            name: name.to_string(),
            is_dir,
            size,
            modified: 0,
            permissions: None,
        }
    }

    #[derive(Default)]
    struct FakeFs {
        dirs: RefCell<BTreeMap<String, Vec<RemoteFileEntry>>>,
        files: RefCell<BTreeMap<String, Vec<u8>>>,
        fail: Cell<bool>,
    }

    impl FakeFs {
        fn sample() -> Self {
            let fs = Self::default();
            fs.dirs.borrow_mut().insert(
                "/".into(),
                vec![
                    fentry("zebra.txt", false, 10),
                    fentry("docs", true, 0),
                    fentry("readme.md", false, 5),
                ],
            );
            fs.dirs
                .borrow_mut()
                .insert("/docs".into(), vec![fentry("notes.md", false, 7)]);
            fs.files
                .borrow_mut()
                .insert("/readme.md".into(), b"readme-content".to_vec());
            fs.files
                .borrow_mut()
                .insert("/zebra.txt".into(), b"zzz".to_vec());
            fs.files
                .borrow_mut()
                .insert("/docs/notes.md".into(), b"notes!!".to_vec());
            fs
        }

        fn split(path: &str) -> (String, String) {
            let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
            let parent = if parent.is_empty() { "/" } else { parent };
            (parent.to_string(), name.to_string())
        }
    }

    #[async_trait::async_trait(?Send)]
    impl FileSystemRpc for FakeFs {
        async fn list_dir(&self, path: String) -> Result<Vec<RemoteFileEntry>, AppError> {
            if self.fail.get() {
                return Err(AppError::Other("io boom".into()));
            }
            self.dirs
                .borrow()
                .get(&path)
                .cloned()
                .ok_or_else(|| AppError::Other(format!("not found: {path}")))
        }

        async fn create_directory(
            &self,
            parent_path: String,
            dir_name: String,
            _permissions: Option<u32>,
        ) -> Result<(), AppError> {
            if self.fail.get() {
                return Err(AppError::Other("io boom".into()));
            }
            let mut dirs = self.dirs.borrow_mut();
            dirs.get_mut(&parent_path)
                .ok_or_else(|| AppError::Other("no parent".into()))?
                .push(fentry(&dir_name, true, 0));
            dirs.insert(join(&parent_path, &dir_name), Vec::new());
            Ok(())
        }

        async fn delete_entries(&self, paths: Vec<String>) -> Result<(), AppError> {
            let mut dirs = self.dirs.borrow_mut();
            let mut files = self.files.borrow_mut();
            for path in paths {
                let (parent, name) = Self::split(&path);
                if let Some(list) = dirs.get_mut(&parent) {
                    list.retain(|e| e.name != name);
                }
                let prefix = format!("{path}/");
                dirs.retain(|k, _| k != &path && !k.starts_with(&prefix));
                files.retain(|k, _| k != &path && !k.starts_with(&prefix));
            }
            Ok(())
        }

        async fn rename_entry(&self, path: String, new_path: String) -> Result<(), AppError> {
            let mut dirs = self.dirs.borrow_mut();
            let (parent, old_name) = Self::split(&path);
            let (new_parent, new_name) = Self::split(&new_path);

            let list = dirs
                .get_mut(&parent)
                .ok_or_else(|| AppError::Other("no parent".into()))?;
            let pos = list
                .iter()
                .position(|e| e.name == old_name)
                .ok_or_else(|| AppError::Other("no entry".into()))?;
            let mut entry = list.remove(pos);
            entry.name = new_name;

            dirs.get_mut(&new_parent)
                .ok_or_else(|| AppError::Other("no destination parent".into()))?
                .push(entry);

            if let Some(children) = dirs.remove(&path) {
                dirs.insert(new_path.clone(), children);
            }
            let mut files = self.files.borrow_mut();
            if let Some(bytes) = files.remove(&path) {
                files.insert(new_path, bytes);
            }
            Ok(())
        }

        async fn read_file(
            &self,
            path: String,
            _cb: Option<Box<dyn Fn(u64) + 'static>>,
        ) -> Result<Vec<u8>, AppError> {
            if self.fail.get() {
                return Err(AppError::Other("io boom".into()));
            }
            self.files
                .borrow()
                .get(&path)
                .cloned()
                .ok_or_else(|| AppError::Other(format!("no file: {path}")))
        }

        async fn write_file(
            &self,
            path: String,
            content: Vec<u8>,
            _permissions: Option<u32>,
            _cb: Option<Box<dyn Fn(u64) + 'static>>,
        ) -> Result<(), AppError> {
            if self.fail.get() {
                return Err(AppError::Other("io boom".into()));
            }
            let (parent, name) = Self::split(&path);
            let size = content.len() as u64;
            self.dirs
                .borrow_mut()
                .entry(parent)
                .or_default()
                .push(fentry(&name, false, size));
            self.files.borrow_mut().insert(path, content);
            Ok(())
        }

        async fn set_permissions(&self, path: String, permissions: u32) -> Result<(), AppError> {
            if self.fail.get() {
                return Err(AppError::Other("io boom".into()));
            }
            let (parent, name) = Self::split(&path);
            let mut dirs = self.dirs.borrow_mut();
            let entry = dirs
                .get_mut(&parent)
                .and_then(|list| list.iter_mut().find(|e| e.name == name))
                .ok_or_else(|| AppError::Other(format!("no entry: {path}")))?;
            entry.permissions = Some(permissions);
            Ok(())
        }
    }

    fn tab(id: &str) -> ResourceTab {
        ResourceTab {
            id: id.to_string(),
            label: id.to_string(),
        }
    }

    async fn fm_ready() -> (FileManager, Rc<FakeFs>) {
        let fs = Rc::new(FakeFs::sample());
        let mut fm = FileManager::new();
        fm.set_resources(Side::Right, vec![(tab("home"), fs.clone())]);
        assert!(fm.select_resource(Side::Right, 0).await);
        fm.take_events();
        (fm, fs)
    }

    fn names(fm: &FileManager, side: Side) -> Vec<String> {
        fm.panel(side)
            .entries
            .iter()
            .map(|e| e.name.clone())
            .collect()
    }

    #[tokio::test]
    async fn select_resource_loads_root_sorted_dirs_first() {
        let (fm, _) = fm_ready().await;
        let p = fm.panel(Side::Right);
        assert_eq!(p.cwd, "/");
        assert_eq!(
            names(&fm, Side::Right),
            vec!["docs", "readme.md", "zebra.txt"]
        );
        assert!(p.breadcrumb.is_empty(), "root has no segments");
        assert_eq!(p.active_resource, Some(0));
        assert!(p.last_error.is_none());
    }

    #[tokio::test]
    async fn set_resources_auto_selects_first_so_mount_has_a_provider() {
        let fs = Rc::new(FakeFs::sample());
        let mut fm = FileManager::new();
        fm.set_resources(Side::Right, vec![(tab("home"), fs.clone())]);
        assert_eq!(
            fm.panel(Side::Right).active_resource,
            Some(0),
            "a panel that has resources must have one active"
        );
        assert!(
            fm.active_provider(Side::Right).is_some(),
            "active_provider must be set so Mount has a disk to serve"
        );
    }

    #[tokio::test]
    async fn select_resource_out_of_range_fails() {
        let (mut fm, _) = fm_ready().await;
        assert!(!fm.select_resource(Side::Right, 5).await);
    }

    #[tokio::test]
    async fn actions_without_resource_are_noops() {
        let mut fm = FileManager::new();
        assert!(!fm.navigate(Side::Left, "/").await);
        assert!(!fm.mkdir(Side::Left, "x").await);
        assert_eq!(
            fm.panel(Side::Left).last_error.as_deref(),
            Some("no resource selected")
        );
    }

    #[tokio::test]
    async fn enter_dir_and_up_flow() {
        let (mut fm, _) = fm_ready().await;
        assert!(fm.enter(Side::Right).await);
        assert_eq!(fm.panel(Side::Right).cwd, "/docs");
        assert_eq!(names(&fm, Side::Right), vec!["notes.md"]);
        assert_eq!(fm.panel(Side::Right).breadcrumb.len(), 1);

        assert!(fm.up(Side::Right).await);
        assert_eq!(fm.panel(Side::Right).cwd, "/");
    }

    #[tokio::test]
    async fn breadcrumb_jumps_to_a_segment() {
        let (mut fm, _) = fm_ready().await;
        fm.enter(Side::Right).await;
        assert_eq!(fm.panel(Side::Right).cwd, "/docs");
        assert!(fm.breadcrumb(Side::Right, 0).await);
        assert_eq!(fm.panel(Side::Right).cwd, "/docs");
        assert!(!fm.breadcrumb(Side::Right, 9).await);
    }

    #[tokio::test]
    async fn enter_on_file_is_noop() {
        let (mut fm, _) = fm_ready().await;
        fm.set_cursor(Side::Right, 1);
        assert!(!fm.enter(Side::Right).await);
        assert_eq!(fm.panel(Side::Right).cwd, "/");
    }

    #[tokio::test]
    async fn up_at_root_is_noop() {
        let (mut fm, _) = fm_ready().await;
        assert!(!fm.up(Side::Right).await);
    }

    #[tokio::test]
    async fn back_and_forward_roundtrip() {
        let (mut fm, _) = fm_ready().await;
        fm.enter(Side::Right).await;
        assert!(fm.panel(Side::Right).can_back);

        assert!(fm.back(Side::Right).await);
        assert_eq!(fm.panel(Side::Right).cwd, "/");
        assert!(fm.panel(Side::Right).can_forward);

        assert!(fm.forward(Side::Right).await);
        assert_eq!(fm.panel(Side::Right).cwd, "/docs");
        assert!(!fm.panel(Side::Right).can_forward);
    }

    #[tokio::test]
    async fn navigating_to_same_path_does_not_push_history() {
        let (mut fm, _) = fm_ready().await;
        fm.navigate(Side::Right, "/").await;
        assert!(!fm.panel(Side::Right).can_back);
    }

    #[tokio::test]
    async fn failed_navigation_keeps_state_and_reports_error() {
        let (mut fm, fs) = fm_ready().await;
        fs.fail.set(true);
        assert!(!fm.navigate(Side::Right, "/docs").await);
        let p = fm.panel(Side::Right);
        assert_eq!(p.cwd, "/", "cwd unchanged");
        assert_eq!(p.entries.len(), 3, "entries unchanged");
        assert_eq!(p.last_error.as_deref(), Some("io boom"));
        assert!(!p.can_back, "no history entry for a failed move");
        assert!(matches!(
            fm.take_events().last(),
            Some(Event::PanelUpdated { .. })
        ));
    }

    #[tokio::test]
    async fn switching_resource_tab_loads_other_tree_and_clears_history() {
        let fs_a = Rc::new(FakeFs::sample());
        let fs_b = Rc::new(FakeFs::default());
        fs_b.dirs
            .borrow_mut()
            .insert("/".into(), vec![fentry("only-b.txt", false, 1)]);

        let mut fm = FileManager::new();
        fm.set_resources(Side::Right, vec![(tab("home"), fs_a), (tab("www"), fs_b)]);
        fm.select_resource(Side::Right, 0).await;
        fm.enter(Side::Right).await;

        fm.select_resource(Side::Right, 1).await;
        let p = fm.panel(Side::Right);
        assert_eq!(names(&fm, Side::Right), vec!["only-b.txt"]);
        assert_eq!(p.active_resource, Some(1));
        assert!(!p.can_back, "history is per-resource visit");
    }

    #[tokio::test]
    async fn cursor_clamps_and_selection_toggles() {
        let (mut fm, _) = fm_ready().await;
        fm.set_cursor(Side::Right, 99);
        assert_eq!(fm.panel(Side::Right).cursor, 2);

        fm.toggle_select(Side::Right, 2);
        fm.toggle_select(Side::Right, 0);
        assert_eq!(fm.panel(Side::Right).selection, vec![0, 2]);
        fm.toggle_select(Side::Right, 2);
        assert_eq!(fm.panel(Side::Right).selection, vec![0]);
        fm.toggle_select(Side::Right, 99);
        assert_eq!(fm.panel(Side::Right).selection, vec![0]);
    }

    #[tokio::test]
    async fn navigation_clears_selection() {
        let (mut fm, _) = fm_ready().await;
        fm.toggle_select(Side::Right, 1);
        fm.enter(Side::Right).await;
        assert!(fm.panel(Side::Right).selection.is_empty());
    }

    #[tokio::test]
    async fn activate_emits_only_on_change() {
        let (mut fm, _) = fm_ready().await;
        fm.activate(Side::Right);
        fm.activate(Side::Right);
        let evs = fm.take_events();
        assert_eq!(
            evs.iter()
                .filter(|e| matches!(e, Event::ActivePanel { .. }))
                .count(),
            1
        );
        assert_eq!(fm.active(), Side::Right);
    }

    #[tokio::test]
    async fn mkdir_shows_up_in_listing() {
        let (mut fm, _) = fm_ready().await;
        assert!(fm.mkdir(Side::Right, "new-folder").await);
        assert_eq!(
            names(&fm, Side::Right),
            vec!["docs", "new-folder", "readme.md", "zebra.txt"]
        );
        assert!(!fm.mkdir(Side::Right, "   ").await, "blank name rejected");
    }

    #[tokio::test]
    async fn delete_selected_removes_entries_and_subtrees() {
        let (mut fm, fs) = fm_ready().await;
        fm.toggle_select(Side::Right, 0);
        fm.toggle_select(Side::Right, 2);
        assert!(fm.delete_selected(Side::Right).await);
        assert_eq!(names(&fm, Side::Right), vec!["readme.md"]);
        assert!(
            !fs.dirs.borrow().contains_key("/docs"),
            "subtree gone from the backing fs"
        );
        assert!(fm.panel(Side::Right).selection.is_empty());
    }

    #[tokio::test]
    async fn delete_without_selection_uses_cursor() {
        let (mut fm, _) = fm_ready().await;
        fm.set_cursor(Side::Right, 2);
        assert!(fm.delete_selected(Side::Right).await);
        assert_eq!(names(&fm, Side::Right), vec!["docs", "readme.md"]);
    }

    #[tokio::test]
    async fn rename_cursor_entry() {
        let (mut fm, _) = fm_ready().await;
        fm.set_cursor(Side::Right, 2);
        assert!(fm.rename_cursor(Side::Right, "alpha.txt").await);
        assert_eq!(
            names(&fm, Side::Right),
            vec!["docs", "alpha.txt", "readme.md"]
        );
        assert!(
            !fm.rename_cursor(Side::Right, "a/b").await,
            "slash rejected"
        );
    }

    #[tokio::test]
    async fn chmod_updates_permissions_and_refreshes() {
        let (mut fm, _) = fm_ready().await;
        assert!(fm.chmod(Side::Right, "/readme.md", 0o600).await);
        let p = fm.panel(Side::Right);
        let e = p.entries.iter().find(|e| e.name == "readme.md").unwrap();
        assert_eq!(
            e.permissions,
            Some(0o600),
            "new mode reflected after refresh"
        );
        assert!(p.last_error.is_none());
    }

    #[tokio::test]
    async fn chmod_error_becomes_last_error() {
        let (mut fm, fs) = fm_ready().await;
        fs.fail.set(true);
        assert!(!fm.chmod(Side::Right, "/readme.md", 0o600).await);
        assert_eq!(fm.panel(Side::Right).last_error.as_deref(), Some("io boom"));
    }

    #[tokio::test]
    async fn cut_remembers_the_marked_entries_and_survives_a_refresh() {
        let (mut fm, _fs) = fm_ready().await;
        fm.toggle_select(Side::Right, 0);
        fm.toggle_select(Side::Right, 2);

        assert!(fm.set_clipboard(Side::Right, TransferKind::Move));
        let view = fm.clipboard().expect("the clipboard holds the marks");
        assert_eq!(view.count, 2);
        assert_eq!(view.kind, TransferKind::Move);
        assert_eq!(
            view.side,
            Side::Right,
            "the badge belongs to the pane acted on"
        );

        fm.refresh(Side::Right).await;
        assert!(fm.panel(Side::Right).selection.is_empty());
        assert_eq!(fm.clipboard().map(|c| c.count), Some(2));
    }

    #[tokio::test]
    async fn the_parent_link_is_never_something_to_transfer() {
        let (mut fm, _fs) = fm_ready().await;
        fm.set_resources(Side::Left, {
            let fs = Rc::new(FakeFs::default());
            fs.dirs.borrow_mut().insert(
                "/".into(),
                vec![fentry("..", true, 0), fentry("real.txt", false, 3)],
            );
            fs.files
                .borrow_mut()
                .insert("/real.txt".into(), b"abc".to_vec());
            vec![(tab("dots"), fs as Rc<dyn FileSystemRpc>)]
        });
        fm.select_resource(Side::Left, 0).await;

        fm.toggle_select(Side::Left, 0);
        fm.toggle_select(Side::Left, 1);
        assert!(fm.set_clipboard(Side::Left, TransferKind::Copy));
        assert_eq!(
            fm.clipboard().map(|c| c.count),
            Some(1),
            "only the real entry may be held",
        );

        fm.clear_clipboard();
        fm.toggle_select(Side::Left, 1);
        assert!(!fm.set_clipboard(Side::Left, TransferKind::Copy));
        assert!(fm.clipboard().is_none());
    }

    #[tokio::test]
    async fn copying_with_nothing_marked_takes_the_cursor_entry() {
        let (mut fm, _fs) = fm_ready().await;
        fm.set_cursor(Side::Right, 1);
        assert!(fm.set_clipboard(Side::Right, TransferKind::Copy));
        assert_eq!(fm.clipboard().map(|c| c.count), Some(1));
    }

    #[tokio::test]
    async fn an_empty_pane_puts_nothing_on_the_clipboard() {
        let mut fm = FileManager::new();
        assert!(!fm.set_clipboard(Side::Right, TransferKind::Copy));
        assert!(fm.clipboard().is_none());
    }

    #[tokio::test]
    async fn clearing_the_clipboard_announces_itself() {
        let (mut fm, _fs) = fm_ready().await;
        fm.set_cursor(Side::Right, 1);
        fm.set_clipboard(Side::Right, TransferKind::Copy);
        fm.take_events();

        fm.clear_clipboard();
        assert!(matches!(
            fm.take_events().last(),
            Some(Event::ClipboardChanged { clipboard: None })
        ));
        fm.clear_clipboard();
        assert!(fm.take_events().is_empty());
    }

    #[tokio::test]
    async fn the_plan_names_what_the_destination_already_holds() {
        let (mut fm, _src) = fm_ready().await;
        let dest = Rc::new(FakeFs::default());
        dest.dirs
            .borrow_mut()
            .insert("/".into(), vec![fentry("readme.md", false, 1)]);
        dest.files
            .borrow_mut()
            .insert("/readme.md".into(), b"older".to_vec());
        fm.set_resources(Side::Left, vec![(tab("dest"), dest.clone())]);
        fm.select_resource(Side::Left, 0).await;

        fm.toggle_select(Side::Right, 1);
        fm.toggle_select(Side::Right, 2);
        fm.set_clipboard(Side::Right, TransferKind::Copy);

        let plan = fm
            .plan_transfer(Side::Left, TransferFrom::Clipboard, None)
            .await;
        assert_eq!(plan.items.len(), 2);
        assert_eq!(plan.conflicts, vec!["readme.md"]);
        assert_eq!(plan.to, "/");
        assert!(plan.error.is_none());
        assert_eq!(
            dest.files.borrow().get("/readme.md").cloned(),
            Some(b"older".to_vec()),
            "planning must not touch a byte",
        );
    }

    #[tokio::test]
    async fn planning_an_empty_clipboard_reports_why() {
        let (mut fm, _fs) = fm_ready().await;
        let plan = fm
            .plan_transfer(Side::Right, TransferFrom::Clipboard, None)
            .await;
        assert_eq!(plan.error.as_deref(), Some("the clipboard is empty"));
        assert!(plan.items.is_empty());
    }

    #[tokio::test]
    async fn skip_leaves_the_existing_file_alone() {
        let (mut fm, _src) = fm_ready().await;
        let dest = Rc::new(FakeFs::default());
        dest.dirs
            .borrow_mut()
            .insert("/".into(), vec![fentry("readme.md", false, 1)]);
        dest.files
            .borrow_mut()
            .insert("/readme.md".into(), b"older".to_vec());
        fm.set_resources(Side::Left, vec![(tab("dest"), dest.clone())]);
        fm.select_resource(Side::Left, 0).await;

        fm.set_cursor(Side::Right, 1);
        fm.set_clipboard(Side::Right, TransferKind::Copy);
        fm.run_transfer(
            Side::Left,
            TransferFrom::Clipboard,
            None,
            vec![("readme.md".into(), OnConflict::Skip)],
        )
        .await;

        assert_eq!(
            dest.files.borrow().get("/readme.md").cloned(),
            Some(b"older".to_vec())
        );
    }

    #[tokio::test]
    async fn replace_overwrites_the_existing_file() {
        let (mut fm, _src) = fm_ready().await;
        let dest = Rc::new(FakeFs::default());
        dest.dirs
            .borrow_mut()
            .insert("/".into(), vec![fentry("readme.md", false, 1)]);
        dest.files
            .borrow_mut()
            .insert("/readme.md".into(), b"older".to_vec());
        fm.set_resources(Side::Left, vec![(tab("dest"), dest.clone())]);
        fm.select_resource(Side::Left, 0).await;

        fm.set_cursor(Side::Right, 1);
        fm.set_clipboard(Side::Right, TransferKind::Copy);
        assert!(
            fm.run_transfer(
                Side::Left,
                TransferFrom::Clipboard,
                None,
                vec![("readme.md".into(), OnConflict::Replace)],
            )
            .await
        );

        assert_eq!(
            dest.files.borrow().get("/readme.md").cloned(),
            Some(b"readme-content".to_vec())
        );
    }

    #[tokio::test]
    async fn keep_both_lands_beside_the_existing_file() {
        let (mut fm, _src) = fm_ready().await;
        let dest = Rc::new(FakeFs::default());
        dest.dirs
            .borrow_mut()
            .insert("/".into(), vec![fentry("readme.md", false, 1)]);
        dest.files
            .borrow_mut()
            .insert("/readme.md".into(), b"older".to_vec());
        fm.set_resources(Side::Left, vec![(tab("dest"), dest.clone())]);
        fm.select_resource(Side::Left, 0).await;

        fm.set_cursor(Side::Right, 1);
        fm.set_clipboard(Side::Right, TransferKind::Copy);
        assert!(
            fm.run_transfer(
                Side::Left,
                TransferFrom::Clipboard,
                None,
                vec![("readme.md".into(), OnConflict::KeepBoth)],
            )
            .await
        );

        assert_eq!(
            dest.files.borrow().get("/readme.md").cloned(),
            Some(b"older".to_vec()),
            "the file that was there stays untouched",
        );
        assert_eq!(
            dest.files.borrow().get("/readme (2).md").cloned(),
            Some(b"readme-content".to_vec()),
        );
    }

    #[tokio::test]
    async fn pasting_a_folder_onto_one_with_the_same_name_merges_instead_of_failing() {
        let (mut fm, _src) = fm_ready().await;
        let dest = Rc::new(FakeFs::default());
        dest.dirs
            .borrow_mut()
            .insert("/".into(), vec![fentry("docs", true, 0)]);
        dest.dirs
            .borrow_mut()
            .insert("/docs".into(), vec![fentry("keepme.txt", false, 1)]);
        dest.files
            .borrow_mut()
            .insert("/docs/keepme.txt".into(), b"mine".to_vec());
        fm.set_resources(Side::Left, vec![(tab("dest"), dest.clone())]);
        fm.select_resource(Side::Left, 0).await;

        fm.set_cursor(Side::Right, 0);
        fm.set_clipboard(Side::Right, TransferKind::Copy);
        assert!(
            fm.run_transfer(
                Side::Left,
                TransferFrom::Clipboard,
                None,
                vec![("docs".into(), OnConflict::Replace)],
            )
            .await,
            "creating a directory that is already there must not fail the transfer",
        );

        assert!(dest.files.borrow().contains_key("/docs/notes.md"));
        assert!(dest.files.borrow().contains_key("/docs/keepme.txt"));
    }

    #[tokio::test]
    async fn a_paste_empties_the_clipboard() {
        let (mut fm, _src) = fm_ready().await;
        let dest = Rc::new(FakeFs::default());
        dest.dirs.borrow_mut().insert("/".into(), Vec::new());
        fm.set_resources(Side::Left, vec![(tab("dest"), dest.clone())]);
        fm.select_resource(Side::Left, 0).await;

        fm.set_cursor(Side::Right, 1);
        fm.set_clipboard(Side::Right, TransferKind::Copy);
        assert!(fm.clipboard().is_some());

        fm.run_transfer(Side::Left, TransferFrom::Clipboard, None, Vec::new())
            .await;
        assert!(fm.clipboard().is_none(), "a paste consumes what it pasted");
    }

    #[tokio::test]
    async fn pasting_into_a_named_folder_lands_inside_it() {
        let (mut fm, src) = fm_ready().await;
        fm.set_resources(Side::Left, vec![(tab("same"), src.clone())]);
        fm.select_resource(Side::Left, 0).await;

        fm.set_cursor(Side::Right, 1);
        fm.set_clipboard(Side::Right, TransferKind::Copy);
        assert!(
            fm.run_transfer(
                Side::Left,
                TransferFrom::Clipboard,
                Some("docs".into()),
                Vec::new(),
            )
            .await
        );

        assert!(src.files.borrow().contains_key("/docs/readme.md"));
        assert!(
            src.files.borrow().contains_key("/readme.md"),
            "a copy leaves the original where it was",
        );
    }

    #[tokio::test]
    async fn cutting_and_pasting_inside_one_pane_moves_without_a_round_trip() {
        let (mut fm, src) = fm_ready().await;
        fm.set_cursor(Side::Right, 1);
        fm.set_clipboard(Side::Right, TransferKind::Move);

        assert!(
            fm.run_transfer(
                Side::Right,
                TransferFrom::Clipboard,
                Some("docs".into()),
                Vec::new(),
            )
            .await
        );

        assert!(src.files.borrow().contains_key("/docs/readme.md"));
        assert!(!src.files.borrow().contains_key("/readme.md"));
        assert!(!names(&fm, Side::Right).contains(&"readme.md".to_string()));
    }

    #[tokio::test]
    async fn pasting_a_cut_into_the_folder_it_came_from_is_a_no_op() {
        let (mut fm, src) = fm_ready().await;
        fm.set_cursor(Side::Right, 1);
        fm.set_clipboard(Side::Right, TransferKind::Move);

        fm.run_transfer(Side::Right, TransferFrom::Clipboard, None, Vec::new())
            .await;
        assert!(
            src.files.borrow().contains_key("/readme.md"),
            "renaming an entry onto itself must not delete it",
        );
    }

    #[test]
    fn free_name_walks_past_what_is_taken() {
        let taken = vec!["report.pdf".to_string(), "report (2).pdf".to_string()];
        assert_eq!(free_name("report.pdf", &taken), "report (3).pdf");
        assert_eq!(free_name("fresh.pdf", &taken), "fresh.pdf");
        assert_eq!(free_name("noext", &["noext".to_string()]), "noext (2)");
        assert_eq!(
            free_name(".hidden", &[".hidden".to_string()]),
            ".hidden (2)"
        );
    }

    #[tokio::test]
    async fn copy_between_panes_reads_source_writes_dest() {
        let (mut fm, _src) = fm_ready().await;
        let dest = Rc::new(FakeFs::default());
        dest.dirs.borrow_mut().insert("/".into(), Vec::new());
        fm.set_resources(Side::Left, vec![(tab("dest"), dest.clone())]);
        fm.select_resource(Side::Left, 0).await;

        fm.set_cursor(Side::Right, 1);
        fm.take_events();

        assert!(fm.copy_to(Side::Left).await);
        assert_eq!(names(&fm, Side::Left), vec!["readme.md"]);
        assert_eq!(
            dest.files.borrow().get("/readme.md").cloned(),
            Some(b"readme-content".to_vec())
        );
        assert!(matches!(
            fm.take_events().last(),
            Some(Event::PanelUpdated {
                side: Side::Left,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn move_across_providers_lands_then_removes_the_original() {
        let (mut fm, src) = fm_ready().await;
        let dest = Rc::new(FakeFs::default());
        dest.dirs.borrow_mut().insert("/".into(), Vec::new());
        fm.set_resources(Side::Left, vec![(tab("dest"), dest.clone())]);
        fm.select_resource(Side::Left, 0).await;

        fm.set_cursor(Side::Right, 1);
        fm.take_events();

        assert!(fm.move_to(Side::Left).await);
        assert_eq!(
            dest.files.borrow().get("/readme.md").cloned(),
            Some(b"readme-content".to_vec()),
            "the bytes must arrive before anything is deleted",
        );
        assert!(
            !src.files.borrow().contains_key("/readme.md"),
            "the original is gone once its copy landed",
        );
        assert_eq!(names(&fm, Side::Left), vec!["readme.md"]);
        assert!(!names(&fm, Side::Right).contains(&"readme.md".to_string()));
    }

    #[tokio::test]
    async fn move_within_one_provider_is_a_rename_not_a_transfer() {
        let (mut fm, src) = fm_ready().await;
        fm.set_resources(Side::Left, vec![(tab("same"), src.clone())]);
        fm.select_resource(Side::Left, 0).await;
        fm.navigate(Side::Left, "/docs").await;
        fm.set_cursor(Side::Right, 1);
        fm.take_events();

        assert!(fm.move_to(Side::Left).await);
        assert!(
            src.files.borrow().contains_key("/docs/readme.md"),
            "the entry moved under /docs; it holds {:?}",
            src.files.borrow().keys().collect::<Vec<_>>(),
        );
        assert!(!src.files.borrow().contains_key("/readme.md"));
    }

    #[tokio::test]
    async fn move_folder_across_providers_takes_the_whole_tree() {
        let (mut fm, src) = fm_ready().await;
        let dest = Rc::new(FakeFs::default());
        dest.dirs.borrow_mut().insert("/".into(), Vec::new());
        fm.set_resources(Side::Left, vec![(tab("dest"), dest.clone())]);
        fm.select_resource(Side::Left, 0).await;

        fm.set_cursor(Side::Right, 0);
        fm.take_events();

        assert!(fm.move_to(Side::Left).await);
        assert!(
            dest.dirs.borrow().contains_key("/docs"),
            "the directory itself must exist on the destination",
        );
        assert!(
            !src.dirs.borrow().contains_key("/docs"),
            "and be gone from the source once it landed",
        );
    }

    #[tokio::test]
    async fn duplicate_copies_beside_the_original() {
        let (mut fm, src) = fm_ready().await;
        fm.take_events();

        assert!(
            fm.duplicate(Side::Right, "/readme.md".into(), "readme copy.md".into())
                .await
        );
        assert_eq!(
            src.files.borrow().get("/readme copy.md").cloned(),
            Some(b"readme-content".to_vec()),
        );
        assert!(
            src.files.borrow().contains_key("/readme.md"),
            "the original stays put",
        );
    }

    #[tokio::test]
    async fn duplicate_takes_a_folder_whole() {
        let (mut fm, src) = fm_ready().await;
        fm.take_events();

        assert!(
            fm.duplicate(Side::Right, "/docs".into(), "docs copy".into())
                .await
        );
        assert_eq!(
            src.files.borrow().get("/docs copy/notes.md").cloned(),
            Some(b"notes!!".to_vec()),
            "a duplicated folder carries what is inside it",
        );
    }

    #[tokio::test]
    async fn upload_local_puts_real_disk_files_on_the_provider() {
        let (mut fm, src) = fm_ready().await;
        let scratch = std::env::temp_dir().join(format!("fm-upload-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("tree/inner")).unwrap();
        std::fs::write(scratch.join("dropped.txt"), b"from the desktop").unwrap();
        std::fs::write(scratch.join("tree/inner/deep.txt"), b"deep").unwrap();
        fm.take_events();

        assert!(
            fm.upload_local(
                Side::Right,
                "/".into(),
                vec![scratch.join("dropped.txt"), scratch.join("tree")],
            )
            .await
        );
        assert_eq!(
            src.files.borrow().get("/dropped.txt").cloned(),
            Some(b"from the desktop".to_vec()),
        );
        assert_eq!(
            src.files.borrow().get("/tree/inner/deep.txt").cloned(),
            Some(b"deep".to_vec()),
            "a dropped folder arrives whole",
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[tokio::test]
    async fn copy_uses_selection_when_present() {
        let (mut fm, _src) = fm_ready().await;
        let dest = Rc::new(FakeFs::default());
        dest.dirs.borrow_mut().insert("/".into(), Vec::new());
        fm.set_resources(Side::Left, vec![(tab("dest"), dest.clone())]);
        fm.select_resource(Side::Left, 0).await;

        fm.toggle_select(Side::Right, 1);
        fm.toggle_select(Side::Right, 2);
        assert!(fm.copy_to(Side::Left).await);
        assert_eq!(names(&fm, Side::Left), vec!["readme.md", "zebra.txt"]);
    }

    #[tokio::test]
    async fn copy_folder_rebuilds_the_whole_tree() {
        let (mut fm, src) = fm_ready().await;
        src.dirs
            .borrow_mut()
            .get_mut("/docs")
            .unwrap()
            .push(fentry("deep", true, 0));
        src.dirs
            .borrow_mut()
            .insert("/docs/deep".into(), vec![fentry("buried.txt", false, 6)]);
        src.files
            .borrow_mut()
            .insert("/docs/deep/buried.txt".into(), b"buried".to_vec());

        let dest = Rc::new(FakeFs::default());
        dest.dirs.borrow_mut().insert("/".into(), Vec::new());
        fm.set_resources(Side::Left, vec![(tab("dest"), dest.clone())]);
        fm.select_resource(Side::Left, 0).await;

        fm.set_cursor(Side::Right, 0);
        assert!(fm.copy_to(Side::Left).await);

        assert!(dest.dirs.borrow().contains_key("/docs"));
        assert!(dest.dirs.borrow().contains_key("/docs/deep"));
        assert_eq!(
            dest.files.borrow().get("/docs/deep/buried.txt").cloned(),
            Some(b"buried".to_vec()),
            "a file two levels down must arrive with its bytes",
        );
        assert!(src.dirs.borrow().contains_key("/docs/deep"));
    }

    #[tokio::test]
    async fn copy_without_a_dest_resource_is_an_error() {
        let (mut fm, _src) = fm_ready().await;
        fm.set_cursor(Side::Right, 1);
        assert!(!fm.copy_to(Side::Left).await);
        assert!(fm
            .panel(Side::Left)
            .last_error
            .as_deref()
            .unwrap()
            .contains("both panes"));
    }

    #[tokio::test]
    async fn set_mounted_records_url_and_exposes_active_provider() {
        let (mut fm, _src) = fm_ready().await;
        assert!(
            fm.active_provider(Side::Right).is_some(),
            "peer provider is active"
        );
        assert!(fm.panel(Side::Right).mounted_as.is_none());

        fm.set_mounted(Side::Right, Some("http://127.0.0.1:9/mnt/right/".into()));
        assert_eq!(
            fm.panel(Side::Right).mounted_as.as_deref(),
            Some("http://127.0.0.1:9/mnt/right/")
        );
        assert!(matches!(
            fm.take_events().last(),
            Some(Event::PanelUpdated {
                side: Side::Right,
                ..
            })
        ));

        fm.set_mounted(Side::Right, None);
        assert!(fm.panel(Side::Right).mounted_as.is_none());
    }

    #[tokio::test]
    async fn mutation_error_becomes_last_error() {
        let (mut fm, fs) = fm_ready().await;
        fs.fail.set(true);
        assert!(!fm.mkdir(Side::Right, "x").await);
        assert_eq!(fm.panel(Side::Right).last_error.as_deref(), Some("io boom"));
        assert_eq!(fm.panel(Side::Right).entries.len(), 3, "listing untouched");
    }

    #[tokio::test]
    async fn panes_are_independent() {
        let (mut fm, _) = fm_ready().await;
        let local = Rc::new(FakeFs::default());
        local
            .dirs
            .borrow_mut()
            .insert("/".into(), vec![fentry("local.bin", false, 1)]);
        fm.set_resources(Side::Left, vec![(tab("local"), local)]);
        fm.select_resource(Side::Left, 0).await;

        assert_eq!(names(&fm, Side::Left), vec!["local.bin"]);
        assert_eq!(
            names(&fm, Side::Right),
            vec!["docs", "readme.md", "zebra.txt"],
            "right panel untouched by left's configuration"
        );
    }

    #[tokio::test]
    async fn panel_updated_serializes_snake_case() {
        let (mut fm, _) = fm_ready().await;
        fm.set_cursor(Side::Right, 1);
        let ev = fm.take_events().pop().unwrap();
        let json = serde_json::to_string(&ev).unwrap();
        assert!(json.contains(r#""event":"panel_updated""#), "{json}");
        assert!(json.contains(r#""side":"right""#), "{json}");
        assert!(json.contains(r#""cwd":"/""#), "{json}");
    }
}
