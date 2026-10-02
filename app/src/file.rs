//! Reading and writing markdown files: UTF-8 only, byte for byte (PLAN-001
//! contract 1), saved through a temporary file so a crash or a full disk
//! never leaves half a note.
use std::io::Write as _;
use std::path::Path;

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
}
