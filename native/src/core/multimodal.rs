//! Validated multimodal metadata and rich response blocks.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::{Path, PathBuf}};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind { Image, Document, Audio, Video }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AttachmentMetadata {
    pub id: String,
    pub file_name: String,
    pub mime_type: String,
    pub kind: AttachmentKind,
    pub byte_length: u64,
    pub sha256: String,
}

impl AttachmentMetadata {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.id.is_empty() || self.id.len() > 80 || !self.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err("Attachment identifier is invalid");
        }
        if self.file_name.is_empty() || self.file_name.len() > 240 || self.file_name == "." || self.file_name == ".."
            || self.file_name.contains('/') || self.file_name.contains('\\')
            || self.file_name.chars().any(char::is_control) {
            return Err("Attachment filename is invalid");
        }
        let mime = self.mime_type.to_ascii_lowercase();
        let (allowed, max_bytes): (&[&str], u64) = match self.kind {
            AttachmentKind::Image => (&["image/png", "image/jpeg", "image/webp", "image/gif", "image/avif", "image/heic"], 20 * 1024 * 1024),
            AttachmentKind::Document => (&["application/pdf", "application/msword", "application/rtf",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                "application/vnd.ms-excel", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                "application/vnd.ms-powerpoint", "application/vnd.openxmlformats-officedocument.presentationml.presentation",
                "text/plain", "text/markdown", "text/csv"], 50 * 1024 * 1024),
            AttachmentKind::Audio => (&["audio/mpeg", "audio/wav", "audio/x-wav", "audio/mp4", "audio/aac",
                "audio/ogg", "audio/flac", "audio/webm"], 50 * 1024 * 1024),
            AttachmentKind::Video => (&["video/mp4", "video/quicktime", "video/webm", "video/x-matroska"], 100 * 1024 * 1024),
        };
        if !allowed.contains(&mime.as_str()) { return Err("Attachment MIME type is not supported for its kind"); }
        if self.byte_length == 0 || self.byte_length > max_bytes { return Err("Attachment size is outside the supported limit"); }
        if self.sha256.len() != 64 || !self.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("Attachment checksum must be a SHA-256 hex digest");
        }
        Ok(())
    }

    /// Validates bytes against declared size, SHA-256 and file signatures.
    pub fn validate_content(&self, bytes: &[u8]) -> Result<(), &'static str> {
        self.validate()?;
        if bytes.len() as u64 != self.byte_length { return Err("Attachment byte length does not match metadata"); }
        let digest = Sha256::digest(bytes);
        let actual = digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        if !actual.eq_ignore_ascii_case(&self.sha256) { return Err("Attachment checksum does not match its content"); }
        let valid_signature = match self.mime_type.to_ascii_lowercase().as_str() {
            "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "image/jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
            "image/gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
            "image/webp" => bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
            "image/avif" | "image/heic" | "video/mp4" | "video/quicktime" | "audio/mp4" =>
                bytes.len() >= 12 && &bytes[4..8] == b"ftyp",
            "application/pdf" => bytes.starts_with(b"%PDF-"),
            "application/rtf" => bytes.starts_with(b"{\\rtf"),
            "application/msword" | "application/vnd.ms-excel" | "application/vnd.ms-powerpoint"
            | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            | "application/vnd.openxmlformats-officedocument.presentationml.presentation" =>
                bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1"),
            "audio/wav" | "audio/x-wav" => bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE",
            "audio/mpeg" => bytes.starts_with(b"ID3") || bytes.get(0).is_some_and(|byte| *byte == 0xff)
                && bytes.get(1).is_some_and(|byte| byte & 0xe0 == 0xe0),
            "audio/aac" => bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] & 0xf6 == 0xf0,
            "audio/ogg" => bytes.starts_with(b"OggS"),
            "audio/flac" => bytes.starts_with(b"fLaC"),
            "audio/webm" | "video/webm" | "video/x-matroska" => bytes.starts_with(b"\x1a\x45\xdf\xa3"),
            "text/plain" | "text/markdown" | "text/csv" => std::str::from_utf8(bytes).is_ok(),
            _ => false,
        };
        if !valid_signature { return Err("Attachment content does not match its declared MIME type"); }
        Ok(())
    }
}

/// Account-scoped, atomic storage for validated attachment bytes.
pub struct AttachmentStore {
    root: PathBuf,
    user_id: String,
}

impl AttachmentStore {
    pub fn new(root: impl AsRef<Path>, user_id: &str) -> Result<Self, &'static str> {
        if !valid_account_uuid(user_id) { return Err("Attachment storage requires a canonical account UUID"); }
        Ok(Self { root: root.as_ref().to_path_buf(), user_id: user_id.to_owned() })
    }

    fn path_for(&self, attachment_id: &str) -> Result<PathBuf, &'static str> {
        if attachment_id.is_empty() || attachment_id.len() > 80
            || !attachment_id.chars().all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_') {
            return Err("Attachment identifier is invalid");
        }
        Ok(self.root.join(&self.user_id).join("attachments").join(format!("{attachment_id}.bin")))
    }

    pub fn save(&self, metadata: &AttachmentMetadata, bytes: &[u8]) -> Result<PathBuf, String> {
        metadata.validate_content(bytes).map_err(str::to_owned)?;
        let path = self.path_for(&metadata.id).map_err(str::to_owned)?;
        let parent = path.parent().ok_or("Attachment storage path is invalid")?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).map_err(|error| error.to_string())?;
        }
        let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temporary)
            .map_err(|error| error.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600)).map_err(|error| error.to_string())?;
        }
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        fs::rename(&temporary, &path).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            error.to_string()
        })?;
        Ok(path)
    }

    pub fn load(&self, metadata: &AttachmentMetadata) -> Result<Vec<u8>, String> {
        let path = self.path_for(&metadata.id).map_err(str::to_owned)?;
        let bytes = fs::read(path).map_err(|error| error.to_string())?;
        metadata.validate_content(&bytes).map_err(str::to_owned)?;
        Ok(bytes)
    }

    pub fn delete(&self, attachment_id: &str) -> Result<(), String> {
        let path = self.path_for(attachment_id).map_err(str::to_owned)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

fn valid_account_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
        && bytes.iter().enumerate().all(|(index, byte)| {
            [8, 13, 18, 23].contains(&index) || byte.is_ascii_hexdigit()
        })
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Citation {
    pub title: String,
    pub url: String,
    pub publisher: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RichBlock {
    Markdown { text: String },
    Code { language: String, source: String },
    Table { headers: Vec<String>, rows: Vec<Vec<String>> },
    Image { attachment_id: String, alt: String },
    Audio { attachment_id: String, transcript: Option<String> },
    Video { attachment_id: String, summary: Option<String> },
    Citation { citation: Citation },
}

impl RichBlock {
    pub fn validate(&self) -> Result<(), &'static str> {
        let valid_attachment_id = |id: &str| !id.is_empty() && id.len() <= 80
            && id.chars().all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_');
        match self {
            Self::Markdown { text } if text.len() > 200_000 => Err("Markdown block is too large"),
            Self::Code { language, source } if language.len() > 64 || source.len() > 200_000
                || language.chars().any(char::is_control) => Err("Code block exceeds supported limits"),
            Self::Table { headers, rows } if headers.len() > 32 || rows.len() > 500
                || rows.iter().any(|row| row.len() > 32)
                || headers.iter().chain(rows.iter().flatten()).any(|cell| cell.len() > 20_000) =>
                Err("Table exceeds supported dimensions or cell limits"),
            Self::Image { attachment_id, alt } if !valid_attachment_id(attachment_id) || alt.len() > 1000 =>
                Err("Image block has an invalid attachment reference or alt text"),
            Self::Audio { attachment_id, transcript } if !valid_attachment_id(attachment_id)
                || transcript.as_ref().is_some_and(|value| value.len() > 50_000) =>
                Err("Audio block has an invalid attachment reference or transcript"),
            Self::Video { attachment_id, summary } if !valid_attachment_id(attachment_id)
                || summary.as_ref().is_some_and(|value| value.len() > 50_000) =>
                Err("Video block has an invalid attachment reference or summary"),
            Self::Citation { citation } if !citation.url.starts_with("https://")
                || citation.url.len() > 2048 || citation.title.len() > 512
                || citation.publisher.as_ref().is_some_and(|value| value.len() > 256)
                || citation.url.chars().any(char::is_control) =>
                Err("Citation metadata is invalid or URL is not HTTPS"),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_metadata_enforces_type_size_and_checksum() {
        let image = AttachmentMetadata {
            id: "image_1".into(), file_name: "photo.png".into(), mime_type: "image/png".into(),
            kind: AttachmentKind::Image, byte_length: 1024, sha256: "a".repeat(64),
        };
        assert!(image.validate().is_ok());
        let mut invalid = image;
        invalid.mime_type = "application/octet-stream".into();
        assert_eq!(invalid.validate(), Err("Attachment MIME type does not match its kind"));
    }

    #[test]
    fn attachment_content_checks_signature_length_and_sha256() {
        let bytes = b"\x89PNG\r\n\x1a\nvalid-image-payload";
        let digest = Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        let mut metadata = AttachmentMetadata {
            id: "image_secure".into(),
            file_name: "photo.png".into(),
            mime_type: "image/png".into(),
            kind: AttachmentKind::Image,
            byte_length: bytes.len() as u64,
            sha256: digest.clone(),
        };
        assert!(metadata.validate_content(bytes).is_ok());
        metadata.sha256 = "0".repeat(64);
        assert_eq!(metadata.validate_content(bytes), Err("Attachment checksum does not match its content"));
        metadata.sha256 = digest;
        metadata.kind = AttachmentKind::Document;
        metadata.mime_type = "application/pdf".into();
        assert_eq!(metadata.validate_content(bytes), Err("Attachment content does not match its declared MIME type"));
    }

    #[test]
    fn attachment_store_is_atomic_account_scoped_and_revalidates_content() {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("cenai-attachments-{}-{unique}", std::process::id()));
        let owner = "550e8400-e29b-41d4-a716-446655440010";
        let other = "550e8400-e29b-41d4-a716-446655440011";
        let bytes = b"\\x89PNG\\r\\n\\x1a\\nattachment-payload";
        let digest = Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        let metadata = AttachmentMetadata {
            id: "image_1".into(), file_name: "photo.png".into(), mime_type: "image/png".into(),
            kind: AttachmentKind::Image, byte_length: bytes.len() as u64, sha256: digest,
        };
        let store = AttachmentStore::new(&root, owner).unwrap();
        store.save(&metadata, bytes).unwrap();
        assert_eq!(store.load(&metadata).unwrap(), bytes);
        let other_store = AttachmentStore::new(&root, other).unwrap();
        assert!(other_store.load(&metadata).is_err());
        store.delete(&metadata.id).unwrap();
        assert!(store.load(&metadata).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rich_content_rejects_insecure_citations_and_oversized_tables() {
        let citation = RichBlock::Citation { citation: Citation { title: "Source".into(), url: "http://example.com".into(), publisher: None } };
        assert_eq!(citation.validate(), Err("Citation URL must use HTTPS"));
        let table = RichBlock::Table { headers: vec!["a".into()], rows: vec![vec!["x".into(); 40]] };
        assert_eq!(table.validate(), Err("Table exceeds supported dimensions"));
    }
}
