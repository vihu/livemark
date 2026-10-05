//! Reading and writing markdown files: UTF-8 only, byte for byte (PLAN-001
//! contract 1), saved through a temporary file so a crash or a full disk
//! never leaves half a note.
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// The file's text, or why it could not be read (not UTF-8 included: a
/// lossy conversion would change bytes on save).
pub fn load(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// When the file at `path` was last modified, if it can be read.
pub fn modified(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Writes `text` to `path` through a temporary file in the same directory,
/// then renames it over the original, so the file holds either the old or
/// the new text. A symlink is followed (its target is replaced, not the
/// link), and an existing file's permissions are kept.
pub fn save(path: &Path, text: &str) -> std::io::Result<()> {
    let target = match std::fs::canonicalize(path) {
        Ok(target) => target,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => path.to_owned(),
        Err(e) => return Err(e),
    };
    let name = target
        .file_name()
        .ok_or_else(|| std::io::Error::other("the path names no file"))?;
    let temp = target.with_file_name(format!(".{}.livemark-save", name.to_string_lossy()));
    let written = (|| {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        if let Ok(metadata) = std::fs::metadata(&target) {
            std::fs::set_permissions(&temp, metadata.permissions())?;
        }
        std::fs::rename(&temp, &target)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

/// The bytes of the picture an image in the note at `note` points to:
/// a path relative to the note's folder (`%20` and the like decoded) or an
/// absolute one, at most 32 MB. Nothing with a scheme (`https:`, `data:`):
/// the app makes no network requests.
pub fn image_bytes(note: &Path, url: &str) -> Option<Vec<u8>> {
    if url.contains(':') || url.starts_with('#') || url.is_empty() {
        return None;
    }
    let path = note.parent()?.join(
        percent_encoding::percent_decode_str(url)
            .decode_utf8_lossy()
            .as_ref(),
    );
    let size = std::fs::metadata(&path).ok()?.len();
    (size <= PICTURE_MAX)
        .then(|| std::fs::read(&path).ok())
        .flatten()
}

/// The largest picture read, in bytes.
pub const PICTURE_MAX: u64 = 32 << 20;

/// Keeps a picture (its bytes, a file `extension` such as `png`) for the
/// note at `note`, in the `assets` folder next to it as `<note>-<n>.<ext>`
/// (the first free `n`, the user's pick in PLAN-002; never over a file),
/// and gives where its markdown points.
pub fn save_picture(note: &Path, bytes: &[u8], extension: &str) -> Result<String, String> {
    let folder = note
        .parent()
        .ok_or("the note has no folder")?
        .join("assets");
    std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let stem = note
        .file_stem()
        .map_or("note".into(), |s| s.to_string_lossy());
    for n in 1.. {
        let name = format!("{stem}-{n}.{extension}");
        let path = folder.join(&name);
        match std::fs::File::create_new(&path) {
            Ok(mut file) => {
                file.write_all(bytes)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                return Ok(format!("assets/{}", name.replace(' ', "%20")));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
    }
    unreachable!("some name is free")
}

/// Where an image's markdown in the note at `note` points to `file`: its
/// path from the note's folder when it is inside it, else the whole path;
/// spaces as `%20`.
pub fn link_to(note: &Path, file: &Path) -> String {
    let inside = note
        .parent()
        .and_then(|folder| file.strip_prefix(folder).ok());
    inside.unwrap_or(file).to_string_lossy().replace(' ', "%20")
}

/// Whether `file` is a picture the editor draws (PNG, JPEG, GIF, WebP).
pub fn is_picture(file: &Path) -> bool {
    file.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp"
        )
    })
}

/// The markdown file the user picks, if any.
pub async fn pick() -> Option<PathBuf> {
    let file = rfd::AsyncFileDialog::new()
        .add_filter("Markdown", &["md", "markdown", "txt"])
        .pick_file()
        .await?;
    Some(file.path().to_owned())
}

/// The folder the user picks, if any: a vault.
pub async fn pick_folder() -> Option<PathBuf> {
    let folder = rfd::AsyncFileDialog::new().pick_folder().await?;
    Some(folder.path().to_owned())
}

/// Saves to `path`, or asks where first when there is none.
pub async fn save_as(path: Option<PathBuf>, text: String) -> Result<Option<PathBuf>, String> {
    let path = match path {
        Some(path) => path,
        None => {
            let dialog = rfd::AsyncFileDialog::new()
                .add_filter("Markdown", &["md"])
                .set_file_name("untitled.md");
            let Some(file) = dialog.save_file().await else {
                return Ok(None);
            };
            file.path().to_owned()
        }
    };
    save(&path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use livemark::widget::Editor;

    use super::{load, save};

    /// A fresh directory under the system temp directory for one test.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("livemark-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn an_unedited_file_saves_byte_for_byte() {
        let fixtures = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/docs");
        let dir = scratch("unedited");
        for name in ["coverage.md", "line-endings.md"] {
            let original = std::fs::read(format!("{fixtures}/{name}")).unwrap();
            let editor = Editor::new(load(format!("{fixtures}/{name}").as_ref()).unwrap());
            let out = dir.join(name);
            save(&out, editor.text()).unwrap();
            assert_eq!(std::fs::read(&out).unwrap(), original, "{name}");
            std::fs::remove_file(out).unwrap();
        }
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn saving_replaces_the_text_and_leaves_no_temporary_file() {
        let dir = scratch("replace");
        let path = dir.join("note.md");
        std::fs::write(&path, "old\n").unwrap();
        save(&path, "new\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new\n");
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(names, ["note.md"]);
        assert!(save(&dir.join("missing/note.md"), "x").is_err());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn saving_through_a_symlink_keeps_the_link_and_the_permissions() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = scratch("symlink");
        let real = dir.join("real.md");
        let link = dir.join("link.md");
        std::fs::write(&real, "old\n").unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o640)).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        save(&link, "new\n").unwrap();
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "new\n");
        let mode = std::fs::metadata(&real).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);
        for path in [&link, &real] {
            std::fs::remove_file(path).unwrap();
        }
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn a_file_that_is_not_utf8_does_not_open() {
        let dir = scratch("latin1");
        let path = dir.join("latin1.md");
        std::fs::write(&path, b"caf\xe9\n").unwrap();
        assert!(load(&path).unwrap_err().contains("UTF-8"));
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn pictures_are_kept_in_assets_and_linked_from_the_note() {
        let dir = std::env::temp_dir().join(format!("livemark-paste-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let note = dir.join("my note.md");
        assert_eq!(
            super::save_picture(&note, b"one", "png").unwrap(),
            "assets/my%20note-1.png"
        );
        assert_eq!(
            super::save_picture(&note, b"two", "png").unwrap(),
            "assets/my%20note-2.png"
        );
        assert_eq!(
            std::fs::read(dir.join("assets/my note-2.png")).unwrap(),
            b"two"
        );
        // Read back through any escape, as other tools write them.
        assert_eq!(
            super::image_bytes(&note, "assets/my%20note%2D2.png").unwrap(),
            b"two"
        );
        assert_eq!(
            super::link_to(&note, &dir.join("pics/a b.png")),
            "pics/a%20b.png"
        );
        assert_eq!(
            super::link_to(&note, std::path::Path::new("/elsewhere/c.png")),
            "/elsewhere/c.png"
        );
        assert!(super::is_picture(std::path::Path::new("x.JPG")));
        assert!(!super::is_picture(std::path::Path::new("x.md")));
        for name in ["my note-1.png", "my note-2.png"] {
            std::fs::remove_file(dir.join("assets").join(name)).unwrap();
        }
        std::fs::remove_dir(dir.join("assets")).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }
}
