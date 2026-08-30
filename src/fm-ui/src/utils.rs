use adw::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

pub fn build_path_string(parts: &[String]) -> String {
    if parts.is_empty() {
        return "/".to_string();
    }
    let mut path = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i == 0 {
            if p.ends_with(':') {
                path = format!("{}/", p);
            } else {
                path = format!("/{}", p);
            }
        } else {
            if !path.ends_with('/') && !path.ends_with('\\') {
                path.push('/');
            }
            path.push_str(p);
        }
    }
    path
}

pub fn format_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if size >= GB {
        format!("{:.2} GB", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.2} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.2} KB", size as f64 / KB as f64)
    } else {
        format!("{} B", size)
    }
}

pub fn update_status_bar_text(
    status_label: &gtk::Label,
    selection_model: &gtk::MultiSelection,
    _list_store: &gtk::gio::ListStore,
    cached_entries: &Rc<RefCell<Vec<(String, bool, u64, String, Option<u32>)>>>,
) {
    let bitset = selection_model.selection();
    let status_text = if bitset.is_empty() {
        let entries = cached_entries.borrow();
        let folders = entries
            .iter()
            .filter(|(_, is_dir, _, _, _)| *is_dir)
            .count();
        let files = entries
            .iter()
            .filter(|(_, is_dir, _, _, _)| !*is_dir)
            .count();
        if folders == 0 && files == 0 {
            crate::i18n::tr("fm.status.empty_folder").to_string()
        } else {
            let folders_str = if folders == 1 {
                crate::i18n::tr("fm.status.one_folder").to_string()
            } else {
                crate::i18n::trf(
                    "fm.status.many_folders",
                    &[("count", &*(folders).to_string())],
                )
                .to_string()
            };
            let files_str = if files == 1 {
                crate::i18n::tr("fm.status.one_file").to_string()
            } else {
                crate::i18n::trf("fm.status.many_files", &[("count", &*(files).to_string())])
                    .to_string()
            };
            format!("{}, {}", folders_str, files_str)
        }
    } else if bitset.size() == 1 {
        let pos = bitset.nth(0);
        if let Some(obj) = selection_model.item(pos) {
            if let Ok(entry) = obj.downcast::<crate::file_entry::FileEntry>() {
                let mut name = entry.name();
                if name.chars().count() > 50 {
                    let mut name_trunc: String = name.chars().take(47).collect();
                    name_trunc.push_str("...");
                    name = name_trunc;
                }
                let date = entry.date();
                let date_part = if date.is_empty() {
                    "".to_string()
                } else {
                    format!(" | {}", date)
                };
                if entry.is_dir() {
                    crate::i18n::trf(
                        "fm.status.selected_folder",
                        &[
                            ("name", &*(name).to_string()),
                            ("date", &*(date_part).to_string()),
                        ],
                    )
                    .to_string()
                } else {
                    crate::i18n::trf(
                        "fm.status.selected_file",
                        &[
                            ("name", &*(name).to_string()),
                            ("size", &*(format_size(entry.size())).to_string()),
                            ("date", &*(date_part).to_string()),
                        ],
                    )
                    .to_string()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    } else {
        let mut sel_folders = 0;
        let mut sel_files = 0;
        for i in 0..bitset.size() {
            let pos = bitset.nth(i as u32);
            if let Some(obj) = selection_model.item(pos) {
                if let Ok(entry) = obj.downcast::<crate::file_entry::FileEntry>() {
                    if entry.is_dir() {
                        sel_folders += 1;
                    } else {
                        sel_files += 1;
                    }
                }
            }
        }
        let folders_str = if sel_folders == 1 {
            crate::i18n::tr("fm.status.one_folder").to_string()
        } else {
            crate::i18n::trf(
                "fm.status.many_folders",
                &[("count", &*(sel_folders).to_string())],
            )
            .to_string()
        };
        let files_str = if sel_files == 1 {
            crate::i18n::tr("fm.status.one_file").to_string()
        } else {
            crate::i18n::trf(
                "fm.status.many_files",
                &[("count", &*(sel_files).to_string())],
            )
            .to_string()
        };
        crate::i18n::trf(
            "fm.status.selected_multiple",
            &[
                ("folders", &*(folders_str).to_string()),
                ("files", &*(files_str).to_string()),
            ],
        )
        .to_string()
    };
    status_label.set_text(&status_text);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileIcon {
    Resource(String),
    IconName(String),
    GeneratedSvg(String),
}

pub fn get_file_icon(name: &str, is_dir: bool, size: u32, permissions: Option<u32>) -> FileIcon {
    let resource_size = size;

    if is_dir {
        return FileIcon::Resource("/com/fm-ui/gtk/folder.svg".to_string());
    }

    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    let is_tar_gz = name.to_lowercase().ends_with(".tar.gz");

    let mapped_type = if is_tar_gz {
        Some((crate::icon_generator::FileType::Archive, "TAR".to_string()))
    } else {
        match ext.as_str() {
            "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" => {
                let text = if ext == "7z" {
                    "7Z".to_string()
                } else {
                    ext.to_uppercase()
                };
                Some((crate::icon_generator::FileType::Archive, text))
            }
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" => {
                let text = if ext.len() > 4 {
                    ext[..4].to_uppercase()
                } else {
                    ext.to_uppercase()
                };
                Some((crate::icon_generator::FileType::Document, text))
            }
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" => {
                let text = if ext == "jpeg" {
                    "JPG".to_string()
                } else {
                    ext.to_uppercase()
                };
                Some((crate::icon_generator::FileType::Photo, text))
            }
            "nef" | "cr2" | "cr3" | "arw" | "dng" | "raf" | "orf" | "rw2" | "pef" => {
                Some((crate::icon_generator::FileType::Photo, "RAW".to_string()))
            }
            "rs" | "js" | "ts" | "html" | "css" | "go" | "py" | "cpp" | "c" | "h" | "cs"
            | "java" => {
                let text = if ext.len() > 4 {
                    ext[..4].to_uppercase()
                } else {
                    ext.to_uppercase()
                };
                Some((crate::icon_generator::FileType::Developer, text))
            }
            "mp3" | "wav" | "ogg" | "flac" | "mp4" | "mkv" | "avi" | "mov" => {
                let text = if ext.len() > 4 {
                    ext[..4].to_uppercase()
                } else {
                    ext.to_uppercase()
                };
                Some((crate::icon_generator::FileType::Media, text))
            }
            "txt" | "ini" | "conf" | "toml" | "yaml" | "yml" | "json" => {
                let text = if ext.len() > 4 {
                    ext[..4].to_uppercase()
                } else {
                    ext.to_uppercase()
                };
                Some((crate::icon_generator::FileType::ConfigText, text))
            }
            "apk" => Some((
                crate::icon_generator::FileType::Executable,
                "APK".to_string(),
            )),
            "exe" => Some((
                crate::icon_generator::FileType::Executable,
                "EXE".to_string(),
            )),
            "dll" => Some((
                crate::icon_generator::FileType::Developer,
                "DLL".to_string(),
            )),
            _ => {
                if let Some(mode) = permissions {
                    if (mode & 0o111) != 0 {
                        Some((
                            crate::icon_generator::FileType::Executable,
                            "BIN".to_string(),
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        }
    };

    if let Some((file_type, text)) = mapped_type {
        let svg = crate::icon_generator::generate_svg_icon(&text, file_type, size);
        FileIcon::GeneratedSvg(svg)
    } else {
        if resource_size == 30 {
            let icon = match ext.as_str() {
                "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "nef" | "cr2" | "cr3" | "arw"
                | "dng" | "raf" | "orf" | "rw2" | "pef" => "image-x-generic",
                "mp4" | "mkv" | "avi" | "webm" | "mov" => "video-x-generic",
                "mp3" | "wav" | "ogg" | "flac" => "audio-x-generic",
                "rs" | "js" | "ts" | "html" | "css" | "json" | "toml" | "yml" | "txt" | "ini" => {
                    "text-x-script"
                }
                "pdf" => "application-pdf",
                "exe" | "sh" | "bash" | "bin" => "application-x-executable",
                _ => "text-x-generic",
            };
            if icon == "text-x-generic" || icon == "text-x-script" {
                FileIcon::Resource("/com/fm-ui/gtk/file.svg".to_string())
            } else {
                FileIcon::IconName(icon.to_string())
            }
        } else {
            FileIcon::Resource("/com/fm-ui/gtk/file.svg".to_string())
        }
    }
}
