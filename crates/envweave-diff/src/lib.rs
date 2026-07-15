#![forbid(unsafe_code)]

//! Document sessions and optimistic-concurrency file saving. Monaco remains a UI adapter.

use std::{
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

pub const MAX_EDIT_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextDocument {
    pub path: PathBuf,
    pub content: String,
    pub revision: String,
    pub line_ending: LineEnding,
    pub read_only: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

#[derive(Debug, Error)]
pub enum DiffError {
    #[error("file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("binary files cannot be edited")]
    Binary,
    #[error("file is larger than the editable limit")]
    TooLarge,
    #[error("file is not valid UTF-8")]
    UnsupportedEncoding,
    #[error("file changed outside EnvWeave")]
    ExternalModification,
}

pub fn open_text(path: &Path) -> Result<TextDocument, DiffError> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_EDIT_BYTES {
        return Err(DiffError::TooLarge);
    }
    let bytes = fs::read(path)?;
    document_from_bytes(path.to_path_buf(), bytes, false)
}

pub fn document_from_bytes(
    path: PathBuf,
    bytes: Vec<u8>,
    read_only: bool,
) -> Result<TextDocument, DiffError> {
    if bytes.len() as u64 > MAX_EDIT_BYTES {
        return Err(DiffError::TooLarge);
    }
    if bytes.contains(&0) {
        return Err(DiffError::Binary);
    }
    let content = String::from_utf8(bytes.clone()).map_err(|_| DiffError::UnsupportedEncoding)?;
    let line_ending = if content.contains("\r\n") {
        LineEnding::CrLf
    } else {
        LineEnding::Lf
    };
    Ok(TextDocument {
        path,
        content,
        revision: revision(&bytes),
        line_ending,
        read_only,
    })
}

pub fn save_text(
    document: &TextDocument,
    expected_revision: &str,
    content: &str,
) -> Result<TextDocument, DiffError> {
    let current = fs::read(&document.path)?;
    if revision(&current) != expected_revision {
        return Err(DiffError::ExternalModification);
    }
    let permissions = fs::metadata(&document.path)?.permissions();
    let normalized = match document.line_ending {
        LineEnding::Lf => content.replace("\r\n", "\n"),
        LineEnding::CrLf => content.replace("\r\n", "\n").replace('\n', "\r\n"),
    };
    let temporary = document.path.with_extension("envweave.tmp");
    fs::write(&temporary, normalized.as_bytes())?;
    fs::set_permissions(&temporary, permissions)?;
    fs::rename(temporary, &document.path)?;
    open_text(&document.path)
}

fn revision(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prevents_overwrite_after_external_change() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x");
        fs::write(&p, "a\r\n").unwrap();
        let doc = open_text(&p).unwrap();
        assert_eq!(doc.line_ending, LineEnding::CrLf);
        fs::write(&p, "external").unwrap();
        assert!(matches!(
            save_text(&doc, &doc.revision, "mine"),
            Err(DiffError::ExternalModification)
        ));
    }

    #[test]
    fn builds_a_read_only_document_from_history_bytes() {
        let doc =
            document_from_bytes(PathBuf::from("files/demo"), b"old\r\n".to_vec(), true).unwrap();
        assert!(doc.read_only);
        assert_eq!(doc.line_ending, LineEnding::CrLf);
    }
}
