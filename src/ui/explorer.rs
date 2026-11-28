use super::*;
use harbor_workspace::remote_files::{EntryKind, RemoteEntry};

#[derive(Clone, Copy)]
pub(super) enum FileType {
    Folder,
    Link,
    Code,
    Config,
    Text,
    Document,
    Pdf,
    Image,
    Audio,
    Video,
    Archive,
    Script,
    Executable,
    File,
}

impl FileType {
    pub(super) fn from_name(name: &str) -> Self {
        let name = name.to_ascii_lowercase();
        let suffix = name
            .rsplit_once('.')
            .map(|(_, extension)| extension)
            .unwrap_or("");
        match name.as_str() {
            "readme" | "license" | "copying" | "authors" | "changelog" => return Self::Text,
            ".env" | ".editorconfig" | ".gitignore" | ".gitattributes" | ".npmrc"
            | ".gitconfig" | "cargo.lock" => return Self::Config,
            "makefile" | "dockerfile" => return Self::Script,
            _ if name.starts_with(".env.") => return Self::Config,
            _ => {}
        }
        match suffix {
            "rs" | "c" | "h" | "cpp" | "hpp" | "go" | "java" | "kt" | "cs" | "swift" | "ts"
            | "tsx" | "js" | "jsx" | "css" | "html" | "sql" => Self::Code,
            "toml" | "yaml" | "yml" | "ini" | "conf" | "cfg" | "json" | "xml" | "properties" => {
                Self::Config
            }
            "txt" | "md" | "rst" | "log" | "csv" => Self::Text,
            "doc" | "docx" | "odt" | "rtf" | "xls" | "xlsx" | "ods" | "ppt" | "pptx" => {
                Self::Document
            }
            "pdf" => Self::Pdf,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" | "tiff" | "heic"
            | "avif" => Self::Image,
            "wav" | "mp3" | "aac" | "ogg" | "flac" | "m4a" | "opus" => Self::Audio,
            "mp4" | "mkv" | "webm" | "mov" | "avi" | "m4v" => Self::Video,
            "zip" | "7z" | "rar" | "tar" | "gz" | "tgz" | "xz" | "bz2" | "zst" | "txz" | "jar" => {
                Self::Archive
            }
            "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" | "cmd" | "py" | "rb" | "pl" => {
                Self::Script
            }
            "exe" | "msi" | "appimage" | "com" => Self::Executable,
            _ => Self::File,
        }
    }

    fn descriptor(self) -> (&'static str, &'static str) {
        match self {
            Self::Folder => ("folder", "Folder"),
            Self::Link => ("link", "Symbolic link"),
            Self::Code => ("file-code", "Source code"),
            Self::Config => ("settings", "Configuration"),
            Self::Text => ("files", "Text document"),
            Self::Document => ("file-document", "Document"),
            Self::Pdf => ("file-pdf", "PDF document"),
            Self::Image => ("file-image", "Image"),
            Self::Audio => ("file-audio", "Audio"),
            Self::Video => ("file-video", "Video"),
            Self::Archive => ("file-archive", "Archive"),
            Self::Script => ("terminal", "Script"),
            Self::Executable => ("file-program", "Executable"),
            Self::File => ("file-blank", "File"),
        }
    }
}

pub(super) struct ExplorerRow<'a> {
    pub name: &'a str,
    pub kind: FileType,
    pub path: String,
    pub detail: String,
    pub bytes: Option<u64>,
}

impl<'a> ExplorerRow<'a> {
    pub fn remote(entry: &'a RemoteEntry) -> Self {
        let kind = match entry.kind {
            EntryKind::Directory => FileType::Folder,
            EntryKind::Symlink => FileType::Link,
            EntryKind::File => FileType::from_name(&entry.display_name),
            EntryKind::Other => FileType::File,
        };
        let detail = match entry.kind {
            EntryKind::Directory => "Folder".into(),
            EntryKind::Symlink => "Link".into(),
            _ => entry
                .size
                .map(|bytes| format!("{bytes} B"))
                .unwrap_or_default(),
        };
        Self {
            name: &entry.display_name,
            kind,
            path: entry.path.display(),
            detail,
            bytes: entry.size,
        }
    }

    pub fn show(&self, ui: &mut Ui, id: Id, selected: bool, height: f32, sense: Sense) -> Response {
        let (_, rect) = ui.allocate_space(vec2(ui.available_width(), height));
        let response = ui.interact(rect, id, sense);
        if response.clicked() {
            response.request_focus();
        }
        if response.gained_focus() {
            response.scroll_to_me(None);
        }
        let foreground = fluent::paint_control(ui, &response, rect, selected, ControlKind::Flat);
        let (glyph, description) = self.kind.descriptor();
        if ui.is_rect_visible(rect) {
            draw_icon(
                ui.painter(),
                Rect::from_center_size(rect.left_center() + vec2(16., 0.), Vec2::splat(19.)),
                glyph,
                if matches!(self.kind, FileType::Folder) {
                    accent(ui)
                } else {
                    muted(ui)
                },
            );
            let metadata_width = (rect.width() * 0.32).min(86.);
            paint_ellipsized(
                ui,
                rect.left_center() + vec2(36., 0.),
                rect.width() - 36. - metadata_width,
                self.name,
                foreground,
            );
            let metadata_rect = Rect::from_min_max(
                pos2(rect.right() - metadata_width, rect.top()),
                rect.right_bottom(),
            );
            ui.painter().with_clip_rect(metadata_rect).text(
                rect.right_center() - vec2(8., 0.),
                Align2::RIGHT_CENTER,
                &self.detail,
                FontId::proportional(12.),
                muted(ui),
            );
        }
        response.widget_info(|| {
            WidgetInfo::selected(
                WidgetType::SelectableLabel,
                response.enabled(),
                selected,
                format!("{} · {description}", self.name),
            )
        });
        let mut hint = format!("{}\n{description}", self.path);
        if let Some(bytes) = self.bytes {
            hint.push_str(&format!(" · {bytes} bytes"));
        }
        if response.enabled() {
            response.fluent_hint(hint)
        } else {
            response.disabled_hint(hint)
        }
    }
}
