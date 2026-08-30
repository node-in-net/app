use common::AppError;
use fm_core::rpc::{FileSystemRpc, RemoteFileEntry};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub struct LocalFsRpc {
    base: PathBuf,
}

impl LocalFsRpc {
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    fn resolve(&self, scoped: &str) -> Result<PathBuf, AppError> {
        let rel = scoped.trim_start_matches('/');
        if rel.split(['/', '\\']).any(|seg| seg == "..") {
            return Err(AppError::Other("path escapes the resource".into()));
        }
        Ok(if rel.is_empty() {
            self.base.clone()
        } else {
            self.base.join(rel)
        })
    }
}

fn entry_of(path: &Path, name: String) -> Option<RemoteFileEntry> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    #[cfg(unix)]
    let permissions = {
        use std::os::unix::fs::PermissionsExt;
        Some(meta.permissions().mode() & 0o7777)
    };
    #[cfg(not(unix))]
    let permissions = None;
    Some(RemoteFileEntry {
        name,
        is_dir: meta.is_dir(),
        size: meta.len(),
        modified,
        permissions,
    })
}

#[async_trait::async_trait(?Send)]
impl FileSystemRpc for LocalFsRpc {
    async fn list_dir(&self, path: String) -> Result<Vec<RemoteFileEntry>, AppError> {
        let dir = self.resolve(&path)?;
        let rd = std::fs::read_dir(&dir).map_err(|e| AppError::Other(e.to_string()))?;
        let mut out = Vec::new();
        for item in rd.flatten() {
            let name = item.file_name().to_string_lossy().into_owned();
            if let Some(e) = entry_of(&item.path(), name) {
                out.push(e);
            }
        }
        Ok(out)
    }

    async fn create_directory(
        &self,
        parent_path: String,
        dir_name: String,
        _permissions: Option<u32>,
    ) -> Result<(), AppError> {
        if dir_name.contains(['/', '\\']) {
            return Err(AppError::Other("invalid directory name".into()));
        }
        let dir = self.resolve(&parent_path)?.join(dir_name);
        std::fs::create_dir(&dir).map_err(|e| AppError::Other(e.to_string()))
    }

    async fn delete_entries(&self, paths: Vec<String>) -> Result<(), AppError> {
        for scoped in paths {
            let p = self.resolve(&scoped)?;
            let meta = std::fs::symlink_metadata(&p).map_err(|e| AppError::Other(e.to_string()))?;
            let res = if meta.is_dir() {
                std::fs::remove_dir_all(&p)
            } else {
                std::fs::remove_file(&p)
            };
            res.map_err(|e| AppError::Other(e.to_string()))?;
        }
        Ok(())
    }

    async fn rename_entry(&self, path: String, new_path: String) -> Result<(), AppError> {
        let from = self.resolve(&path)?;
        let to = self.resolve(&new_path)?;
        std::fs::rename(from, to).map_err(|e| AppError::Other(e.to_string()))
    }

    async fn read_file(
        &self,
        path: String,
        _progress_callback: Option<Box<dyn Fn(u64) + 'static>>,
    ) -> Result<Vec<u8>, AppError> {
        let p = self.resolve(&path)?;
        std::fs::read(&p).map_err(|e| AppError::Other(e.to_string()))
    }

    async fn write_file(
        &self,
        path: String,
        content: Vec<u8>,
        permissions: Option<u32>,
        _progress_callback: Option<Box<dyn Fn(u64) + 'static>>,
    ) -> Result<(), AppError> {
        let p = self.resolve(&path)?;
        std::fs::write(&p, content).map_err(|e| AppError::Other(e.to_string()))?;
        #[cfg(unix)]
        if let Some(mode) = permissions {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode));
        }
        #[cfg(not(unix))]
        let _ = permissions;
        Ok(())
    }

    async fn set_permissions(&self, path: String, permissions: u32) -> Result<(), AppError> {
        let p = self.resolve(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(permissions))
                .map_err(|e| AppError::Other(e.to_string()))?;
        }
        #[cfg(not(unix))]
        let _ = (p, permissions);
        Ok(())
    }

    fn is_local(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fm::{FileManager, ResourceTab, Side};
    use std::rc::Rc;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("appcore-localfs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("docs")).unwrap();
        std::fs::write(dir.join("readme.md"), b"hello").unwrap();
        std::fs::write(dir.join("docs/notes.md"), b"notes!").unwrap();
        dir
    }

    fn fm_on(dir: &Path) -> FileManager {
        let mut fm = FileManager::new();
        fm.set_resources(
            Side::Left,
            vec![(
                ResourceTab {
                    id: "local".into(),
                    label: "Local".into(),
                },
                Rc::new(LocalFsRpc::new(dir)) as Rc<dyn FileSystemRpc>,
            )],
        );
        fm
    }

    #[tokio::test]
    async fn real_fs_end_to_end_through_the_file_manager() {
        let dir = scratch("e2e");
        let mut fm = fm_on(&dir);
        assert!(fm.select_resource(Side::Left, 0).await);

        let names: Vec<_> = fm
            .panel(Side::Left)
            .entries
            .iter()
            .map(|e| e.name.clone())
            .collect();
        assert_eq!(names, vec!["docs", "readme.md"]);
        assert!(fm.panel(Side::Left).entries[1].size == 5);

        assert!(fm.enter(Side::Left).await);
        assert_eq!(fm.panel(Side::Left).cwd, "/docs");

        assert!(fm.mkdir(Side::Left, "sub").await);
        assert!(dir.join("docs/sub").is_dir());

        fm.set_cursor(Side::Left, 1);
        assert!(fm.rename_cursor(Side::Left, "renamed.md").await);
        assert!(dir.join("docs/renamed.md").is_file());

        fm.toggle_select(Side::Left, 0);
        fm.toggle_select(Side::Left, 1);
        assert!(fm.delete_selected(Side::Left).await);
        assert!(!dir.join("docs/sub").exists());
        assert!(!dir.join("docs/renamed.md").exists());
        assert!(fm.panel(Side::Left).entries.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn path_traversal_is_rejected() {
        let dir = scratch("traversal");
        let rpc = LocalFsRpc::new(&dir);
        assert!(rpc.list_dir("/../..".into()).await.is_err());
        assert!(rpc
            .create_directory("/".into(), "a/b".into(), None)
            .await
            .is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn set_permissions_changes_the_real_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("chmod");
        let rpc = LocalFsRpc::new(&dir);
        rpc.set_permissions("/readme.md".into(), 0o600)
            .await
            .unwrap();
        let mode = std::fs::metadata(dir.join("readme.md"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "the real on-disk mode changed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn listing_missing_dir_is_an_error_not_a_panic() {
        let dir = scratch("missing");
        let rpc = LocalFsRpc::new(&dir);
        assert!(rpc.list_dir("/nope".into()).await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
