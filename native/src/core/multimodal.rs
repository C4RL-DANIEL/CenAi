//! Validated multimodal metadata and rich response blocks.
use serde::{Deserialize, Serialize};

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
        if self.file_name.is_empty() || self.file_name.len() > 240 || self.file_name.contains('/') || self.file_name.contains('\\') {
            return Err("Attachment filename is invalid");
        }
        let (prefix, max_bytes) = match self.kind {
            AttachmentKind::Image => ("image/", 20 * 1024 * 1024),
            AttachmentKind::Document => ("application/", 50 * 1024 * 1024),
            AttachmentKind::Audio => ("audio/", 50 * 1024 * 1024),
            AttachmentKind::Video => ("video/", 100 * 1024 * 1024),
        };
        if !self.mime_type.starts_with(prefix) { return Err("Attachment MIME type does not match its kind"); }
        if self.byte_length == 0 || self.byte_length > max_bytes { return Err("Attachment size is outside the supported limit"); }
        if self.sha256.len() != 64 || !self.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("Attachment checksum must be a SHA-256 hex digest");
        }
        Ok(())
    }
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
        match self {
            Self::Markdown { text } if text.len() > 200_000 => Err("Markdown block is too large"),
            Self::Code { source, .. } if source.len() > 200_000 => Err("Code block is too large"),
            Self::Table { headers, rows } if headers.len() > 32 || rows.len() > 500
                || rows.iter().any(|row| row.len() > 32) => Err("Table exceeds supported dimensions"),
            Self::Citation { citation } if !citation.url.starts_with("https://") => Err("Citation URL must use HTTPS"),
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
    fn rich_content_rejects_insecure_citations_and_oversized_tables() {
        let citation = RichBlock::Citation { citation: Citation { title: "Source".into(), url: "http://example.com".into(), publisher: None } };
        assert_eq!(citation.validate(), Err("Citation URL must use HTTPS"));
        let table = RichBlock::Table { headers: vec!["a".into()], rows: vec![vec!["x".into(); 40]] };
        assert_eq!(table.validate(), Err("Table exceeds supported dimensions"));
    }
}
