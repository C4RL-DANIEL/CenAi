//! Validated multimodal metadata and rich response blocks.
use crate::core::{network::SessionVault, workspace::WORKSPACE_CIPHER_MAGIC};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, io::{Cursor, Read, Write}, path::{Path, PathBuf}, sync::Arc};
use quick_xml::{events::Event, Reader};
use zip::ZipArchive;

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
    pub fn from_bytes(id: String, file_name: String, mime_type: String, kind: AttachmentKind, bytes: &[u8]) -> Result<Self, String> {
        let digest = Sha256::digest(bytes);
        let metadata = Self {
            id,
            file_name,
            mime_type,
            kind,
            byte_length: bytes.len() as u64,
            sha256: digest.iter().map(|byte| format!("{byte:02x}")).collect(),
        };
        metadata.validate_content(bytes).map_err(str::to_owned)?;
        Ok(metadata)
    }

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
            AttachmentKind::Document => (&["application/pdf", "application/zip", "application/msword", "application/rtf",
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
            "application/zip" => bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06"),
            "application/rtf" => bytes.starts_with(b"{\\rtf"),
            "application/msword" | "application/vnd.ms-excel" | "application/vnd.ms-powerpoint"
            | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            | "application/vnd.openxmlformats-officedocument.presentationml.presentation" =>
                bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1"),
            "audio/wav" | "audio/x-wav" => bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE",
            "audio/mpeg" => bytes.starts_with(b"ID3") || bytes.first().is_some_and(|byte| *byte == 0xff)
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
#[derive(Clone)]
pub struct AttachmentStore {
    root: PathBuf,
    user_id: String,
    vault: Option<Arc<dyn SessionVault>>,
}

impl AttachmentStore {
    pub fn new(root: impl AsRef<Path>, user_id: &str) -> Result<Self, &'static str> {
        if !valid_account_uuid(user_id) { return Err("Attachment storage requires a canonical account UUID"); }
        Ok(Self { root: root.as_ref().to_path_buf(), user_id: user_id.to_owned(), vault: None })
    }

    pub fn new_encrypted(root: impl AsRef<Path>, user_id: &str, vault: Arc<dyn SessionVault>) -> Result<Self, String> {
        if !valid_account_uuid(user_id) { return Err("Attachment storage requires a canonical account UUID".into()); }
        if vault.account_id()? != user_id { return Err("Attachment encryption vault does not match the authenticated account".into()); }
        let store = Self { root: root.as_ref().to_path_buf(), user_id: user_id.to_owned(), vault: Some(vault) };
        store.migrate_plaintext_files()?;
        Ok(store)
    }

    fn relative_key(&self, attachment_id: &str, extension: &str) -> String {
        format!("attachments/{attachment_id}.{extension}")
    }

    fn encode(&self, relative: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
        match &self.vault { Some(vault) => vault.encrypt_workspace_bytes(&self.user_id, relative, bytes), None => Ok(bytes.to_vec()) }
    }

    fn decode(&self, relative: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
        match &self.vault { Some(vault) => vault.decrypt_workspace_bytes(&self.user_id, relative, bytes), None => Ok(bytes.to_vec()) }
    }

    fn read_storage(&self, path: &Path, relative: &str) -> Result<Vec<u8>, String> {
        let bytes = fs::read(path).map_err(|error| error.to_string())?;
        self.decode(relative, &bytes)
    }

    fn write_atomic(&self, path: &Path, relative: &str, bytes: &[u8]) -> Result<(), String> {
        let stored = self.encode(relative, bytes)?;
        let parent = path.parent().ok_or("Attachment storage path is invalid")?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).map_err(|error| error.to_string())?; }
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);
        let temporary = path.with_extension(format!("tmp-{}-{}", std::process::id(), NEXT_TEMP.fetch_add(1, Ordering::Relaxed)));
        let result = (|| -> Result<(), String> {
            let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temporary).map_err(|error| error.to_string())?;
            #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; file.set_permissions(fs::Permissions::from_mode(0o600)).map_err(|error| error.to_string())?; }
            file.write_all(&stored).map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            drop(file);
            fs::rename(&temporary, path).map_err(|error| error.to_string())?;
            Ok(())
        })();
        if result.is_err() { let _ = fs::remove_file(&temporary); }
        result
    }

    fn migrate_plaintext_files(&self) -> Result<(), String> {
        let directory = self.root.join(&self.user_id).join("attachments");
        if !directory.exists() { return Ok(()); }
        for entry in fs::read_dir(&directory).map_err(|error| error.to_string())? {
            let path = entry.map_err(|error| error.to_string())?.path();
            if !path.is_file() { continue; }
            let extension = path.extension().and_then(|value| value.to_str()).unwrap_or("");
            if extension != "bin" && extension != "json" { continue; }
            let id = path.file_stem().and_then(|value| value.to_str()).ok_or("Attachment filename is invalid")?;
            let relative = self.relative_key(id, extension);
            // Only inspect the authenticated-envelope marker for existing files;
            // decrypting every binary attachment during startup wastes memory and CPU.
            let mut prefix = vec![0u8; WORKSPACE_CIPHER_MAGIC.len()];
            let mut file = fs::File::open(&path).map_err(|error| error.to_string())?;
            let prefix_len = file.read(&mut prefix).map_err(|error| error.to_string())?;
            if prefix_len == WORKSPACE_CIPHER_MAGIC.len() && prefix == WORKSPACE_CIPHER_MAGIC {
                continue;
            }
            drop(file);
            let plaintext = fs::read(&path).map_err(|error| error.to_string())?;
            self.write_atomic(&path, &relative, &plaintext)?;
        }
        Ok(())
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
        let metadata_path = path.with_extension("json");
        let metadata_bytes = serde_json::to_vec(metadata).map_err(|error| error.to_string())?;
        self.write_atomic(&path, &self.relative_key(&metadata.id, "bin"), bytes)?;
        if let Err(error) = self.write_atomic(&metadata_path, &self.relative_key(&metadata.id, "json"), &metadata_bytes) {
            let _ = fs::remove_file(&path);
            return Err(error);
        }
        Ok(path)
    }
    pub fn list(&self) -> Result<Vec<AttachmentMetadata>, String> {
        let directory = self.root.join(&self.user_id).join("attachments");
        if !directory.exists() { return Ok(Vec::new()); }
        let mut items = Vec::new();
        for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let id = path.file_stem().and_then(|value| value.to_str()).ok_or("Attachment filename is invalid")?;
            let bytes = self.read_storage(&path, &self.relative_key(id, "json"))?;
            let metadata: AttachmentMetadata = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
            metadata.validate().map_err(str::to_owned)?;
            if metadata.id != id { return Err("Attachment metadata identifier does not match its filename".into()); }
            let stored = self.path_for(&metadata.id).map_err(str::to_owned)?;
            if !stored.is_file() { continue; }
            items.push(metadata);
        }
        items.sort_by(|left, right| left.file_name.cmp(&right.file_name));
        Ok(items)
    }

    pub fn delete(&self, attachment_id: &str) -> Result<bool, String> {
        let bytes_path = self.path_for(attachment_id).map_err(str::to_owned)?;
        let metadata_path = bytes_path.with_extension("json");
        let existed = bytes_path.exists() || metadata_path.exists();
        if !existed { return Ok(false); }
        if bytes_path.exists() { fs::remove_file(bytes_path).map_err(|error| error.to_string())?; }
        if metadata_path.exists() { fs::remove_file(metadata_path).map_err(|error| error.to_string())?; }
        Ok(true)
    }

    pub fn load(&self, metadata: &AttachmentMetadata) -> Result<Vec<u8>, String> {
        metadata.validate().map_err(str::to_owned)?;
        let path = self.path_for(&metadata.id).map_err(str::to_owned)?;
        let bytes = self.read_storage(&path, &self.relative_key(&metadata.id, "bin"))?;
        metadata.validate_content(&bytes).map_err(str::to_owned)?;
        Ok(bytes)
    }

}


const MAX_EXTRACTED_TEXT_BYTES: usize = 2 * 1024 * 1024;
const MAX_ARCHIVE_ENTRY_BYTES: usize = 2 * 1024 * 1024;
const MAX_ARCHIVE_TOTAL_BYTES: usize = 10 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 2_000;

fn push_bounded_text(output: &mut String, value: &str) -> Result<(), String> {
    if output.len().saturating_add(value.len()) > MAX_EXTRACTED_TEXT_BYTES {
        return Err("Extracted document text exceeds the 2 MB analysis limit".into());
    }
    output.push_str(value);
    Ok(())
}

fn xml_tag_is(name: &[u8], local_name: &[u8]) -> bool {
    name == local_name || name.rsplit(|byte| *byte == b':').next() == Some(local_name)
}

fn append_xml_text(output: &mut String, value: &quick_xml::events::BytesText<'_>) -> Result<(), String> {
    let decoded = value.decode().map_err(|error| format!("Office XML text could not be decoded: {error}"))?;
    let unescaped = quick_xml::escape::unescape(decoded.as_ref())
        .map_err(|error| format!("Office XML entity was invalid: {error}"))?;
    push_bounded_text(output, unescaped.as_ref())
}

fn decode_xml_reference(reference: &quick_xml::events::BytesRef<'_>) -> Result<String, String> {
    if let Some(character) = reference.resolve_char_ref()
        .map_err(|error| format!("Office XML character reference is invalid: {error}"))? {
        return Ok(character.to_string());
    }
    let name = reference.decode()
        .map_err(|error| format!("Office XML entity could not be decoded: {error}"))?;
    quick_xml::escape::resolve_predefined_entity(name.as_ref())
        .map(str::to_owned)
        .ok_or_else(|| format!("Office XML uses an unsupported entity: &{};", name))
}

fn xml_document_text(xml: &[u8]) -> Result<String, String> {
    if xml.len() > MAX_EXTRACTED_TEXT_BYTES * 2 {
        return Err("Office XML document exceeds the extraction limit".into());
    }
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut output = String::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(element)) if xml_tag_is(element.name().as_ref(), b"p") => {
                if !output.is_empty() && !output.ends_with('\n') { output.push('\n'); }
            }
            Ok(Event::Empty(element)) if xml_tag_is(element.name().as_ref(), b"tab") => output.push('\t'),
            Ok(Event::Empty(element)) if xml_tag_is(element.name().as_ref(), b"br") => output.push('\n'),
            Ok(Event::Text(value)) => append_xml_text(&mut output, &value)?,
            Ok(Event::GeneralRef(reference)) => {
                let decoded = decode_xml_reference(&reference)?;
                push_bounded_text(&mut output, &decoded)?;
            }
            Ok(Event::CData(value)) => {
                let decoded = value.decode().map_err(|error| format!("Office XML CDATA could not be decoded: {error}"))?;
                push_bounded_text(&mut output, decoded.as_ref())?;
            }
            Ok(Event::End(element)) if xml_tag_is(element.name().as_ref(), b"p") => output.push('\n'),
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(format!("Office XML document is malformed: {error}")),
        }
        buffer.clear();
    }
    Ok(output.trim().to_owned())
}

fn read_zip_entry<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, String> {
    let file = match archive.by_name(name) {
        Ok(file) => file,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(format!("Document archive could not be read: {error}")),
    };
    if file.size() > max_bytes as u64 {
        return Err(format!("Archive entry {name} exceeds the extraction size limit"));
    }
    let mut bytes = Vec::with_capacity(file.size() as usize);
    file.take(max_bytes as u64 + 1).read_to_end(&mut bytes)
        .map_err(|error| format!("Archive entry {name} could not be read: {error}"))?;
    if bytes.len() > max_bytes {
        return Err(format!("Archive entry {name} exceeds the extraction size limit"));
    }
    Ok(Some(bytes))
}

fn archive_names<R: Read + std::io::Seek>(archive: &mut ZipArchive<R>) -> Result<Vec<(String, u64)>, String> {
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err("Document archive contains too many entries".into());
    }
    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(|error| format!("Document archive entry is invalid: {error}"))?;
        entries.push((file.name().to_owned(), file.size()));
    }
    Ok(entries)
}

fn extract_docx(bytes: &[u8]) -> Result<String, String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("Word document archive is invalid: {error}"))?;
    let xml = read_zip_entry(&mut archive, "word/document.xml", MAX_EXTRACTED_TEXT_BYTES * 2)?
        .ok_or("Word document does not contain its main document body")?;
    xml_document_text(&xml)
}

fn extract_pptx(bytes: &[u8]) -> Result<String, String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("PowerPoint document archive is invalid: {error}"))?;
    let mut slides = archive_names(&mut archive)?.into_iter()
        .filter(|(name, _)| name.starts_with("ppt/slides/slide") && name.ends_with(".xml"))
        .collect::<Vec<_>>();
    slides.sort_by_key(|(name, _)| {
        name.rsplit("slide").next().and_then(|value| value.strip_suffix(".xml"))
            .and_then(|number| number.parse::<u32>().ok()).unwrap_or(u32::MAX)
    });
    if slides.is_empty() { return Err("PowerPoint document contains no readable slides".into()); }
    let mut output = String::new();
    for (name, _) in slides {
        let xml = read_zip_entry(&mut archive, &name, MAX_EXTRACTED_TEXT_BYTES * 2)?
            .ok_or("PowerPoint slide disappeared while reading")?;
        let text = xml_document_text(&xml)?;
        if !text.is_empty() {
            push_bounded_text(&mut output, &format!("{name}:\n{text}\n\n"))?;
        }
    }
    if output.trim().is_empty() { return Err("PowerPoint document contains no extractable slide text".into()); }
    Ok(output.trim().to_owned())
}

fn parse_shared_strings(xml: &[u8]) -> Result<Vec<String>, String> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut strings = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(element)) if xml_tag_is(element.name().as_ref(), b"si") => {
                current.clear();
                in_string = true;
            }
            Ok(Event::Text(value)) if in_string => append_xml_text(&mut current, &value)?,
            Ok(Event::GeneralRef(reference)) if in_string => {
                let decoded = decode_xml_reference(&reference)?;
                push_bounded_text(&mut current, &decoded)?;
            }
            Ok(Event::End(element)) if xml_tag_is(element.name().as_ref(), b"si") => {
                strings.push(std::mem::take(&mut current));
                in_string = false;
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(format!("Spreadsheet shared strings are malformed: {error}")),
        }
        buffer.clear();
    }
    Ok(strings)
}

fn parse_worksheet_cells(xml: &[u8], shared_strings: &[String]) -> Result<String, String> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut output = String::new();
    let mut cell_type = String::new();
    let mut cell_value = String::new();
    let mut in_cell = false;
    let mut in_value = false;
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(element)) if xml_tag_is(element.name().as_ref(), b"c") => {
                in_cell = true;
                cell_value.clear();
                cell_type.clear();
                for attribute in element.attributes().with_checks(false) {
                    let attribute = attribute.map_err(|error| format!("Spreadsheet cell attribute is invalid: {error}"))?;
                    if attribute.key.as_ref() == b"t" {
                        cell_type = attribute.unescape_value().map_err(|error| error.to_string())?.into_owned();
                    }
                }
            }
            Ok(Event::Start(element)) if in_cell
                && (xml_tag_is(element.name().as_ref(), b"v") || xml_tag_is(element.name().as_ref(), b"t")) => in_value = true,
            Ok(Event::Text(value)) if in_cell && in_value => append_xml_text(&mut cell_value, &value)?,
            Ok(Event::GeneralRef(reference)) if in_cell && in_value => {
                let decoded = decode_xml_reference(&reference)?;
                push_bounded_text(&mut cell_value, &decoded)?;
            }
            Ok(Event::End(element)) if xml_tag_is(element.name().as_ref(), b"v")
                || xml_tag_is(element.name().as_ref(), b"t") => in_value = false,
            Ok(Event::End(element)) if xml_tag_is(element.name().as_ref(), b"c") && in_cell => {
                let value = if cell_type == "s" {
                    cell_value.trim().parse::<usize>().ok()
                        .and_then(|index| shared_strings.get(index)).cloned().unwrap_or_default()
                } else {
                    cell_value.trim().to_owned()
                };
                if !value.is_empty() {
                    push_bounded_text(&mut output, &value)?;
                    output.push('\t');
                }
                in_cell = false;
                in_value = false;
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(format!("Spreadsheet worksheet is malformed: {error}")),
        }
        buffer.clear();
    }
    Ok(output)
}

fn extract_xlsx(bytes: &[u8]) -> Result<String, String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("Spreadsheet archive is invalid: {error}"))?;
    let shared_strings = read_zip_entry(&mut archive, "xl/sharedStrings.xml", MAX_EXTRACTED_TEXT_BYTES * 2)?
        .map(|xml| parse_shared_strings(&xml)).transpose()?.unwrap_or_default();
    let mut sheets = archive_names(&mut archive)?.into_iter()
        .filter(|(name, _)| name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml"))
        .collect::<Vec<_>>();
    sheets.sort_by_key(|(name, _)| {
        name.rsplit("sheet").next().and_then(|value| value.strip_suffix(".xml"))
            .and_then(|number| number.parse::<u32>().ok()).unwrap_or(u32::MAX)
    });
    if sheets.is_empty() { return Err("Spreadsheet contains no readable worksheets".into()); }
    let mut output = String::new();
    for (index, (name, _)) in sheets.into_iter().enumerate() {
        let xml = read_zip_entry(&mut archive, &name, MAX_EXTRACTED_TEXT_BYTES * 2)?
            .ok_or("Spreadsheet worksheet disappeared while reading")?;
        let values = parse_worksheet_cells(&xml, &shared_strings)?;
        if !values.trim().is_empty() {
            push_bounded_text(&mut output, &format!("Sheet {}:\n{}\n", index + 1, values))?;
        }
    }
    if output.trim().is_empty() { return Err("Spreadsheet contains no extractable cell text".into()); }
    Ok(output.trim().to_owned())
}

fn safe_archive_entry(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && !name.chars().any(char::is_control)
        && name.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
}

fn is_supported_text_extension(name: &str) -> bool {
    matches!(name.rsplit('.').next().unwrap_or_default().to_ascii_lowercase().as_str(),
        "txt" | "md" | "markdown" | "csv" | "json" | "xml" | "html" | "htm"
        | "rs" | "py" | "js" | "ts" | "kt" | "java" | "toml" | "yaml" | "yml"
        | "sql" | "sh" | "css" | "c" | "h" | "cpp" | "go" | "rb")
}

fn extract_zip_text(bytes: &[u8]) -> Result<String, String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("ZIP archive is invalid: {error}"))?;
    let entries = archive_names(&mut archive)?;
    let mut output = String::new();
    let mut total_bytes = 0usize;
    for (name, size) in entries {
        if !safe_archive_entry(&name) || !is_supported_text_extension(&name) || size == 0
            || size > MAX_ARCHIVE_ENTRY_BYTES as u64 {
            continue;
        }
        if total_bytes.saturating_add(size as usize) > MAX_ARCHIVE_TOTAL_BYTES { break; }
        let Some(content) = read_zip_entry(&mut archive, &name, MAX_ARCHIVE_ENTRY_BYTES)? else { continue; };
        total_bytes = total_bytes.saturating_add(content.len());
        let Ok(text) = std::str::from_utf8(&content) else { continue; };
        push_bounded_text(&mut output, &format!("File: {name}\n{text}\n\n"))?;
    }
    if output.trim().is_empty() {
        return Err("ZIP archive contains no supported UTF-8 text files under the extraction limits".into());
    }
    Ok(output.trim().to_owned())
}

/// Extracts safe text from modern Office documents and text-only ZIP archives.
/// Extraction is read-only, in-memory, size-bounded and rejects archive path traversal.
pub fn extract_document_text(mime_type: &str, bytes: &[u8]) -> Result<String, String> {
    match mime_type.to_ascii_lowercase().as_str() {
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => extract_docx(bytes),
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => extract_pptx(bytes),
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => extract_xlsx(bytes),
        "application/zip" => extract_zip_text(bytes),
        "application/msword" | "application/vnd.ms-excel" | "application/vnd.ms-powerpoint" =>
            Err("Legacy binary Office documents are not supported; save as DOCX, XLSX or PPTX and try again.".into()),
        _ => Err("This document format does not have a local text extractor.".into()),
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
        assert_eq!(invalid.validate(), Err("Attachment MIME type is not supported for its kind"));
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
        let bytes = b"\x89PNG\r\n\x1a\nattachment-payload";
        let digest = Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        let metadata = AttachmentMetadata {
            id: "image_1".into(), file_name: "photo.png".into(), mime_type: "image/png".into(),
            kind: AttachmentKind::Image, byte_length: bytes.len() as u64, sha256: digest,
        };
        let store = AttachmentStore::new(&root, owner).unwrap();
        store.save(&metadata, bytes).unwrap();
        assert_eq!(store.list().unwrap(), vec![metadata.clone()]);
        assert_eq!(store.load(&metadata).unwrap(), bytes);
        let other_store = AttachmentStore::new(&root, other).unwrap();
        assert!(other_store.load(&metadata).is_err());
        assert!(store.delete(&metadata.id).unwrap());
        assert!(store.list().unwrap().is_empty());
        assert!(store.load(&metadata).is_err());
        let _ = fs::remove_dir_all(root);
    }

    fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn docx_text_extraction_decodes_xml_entities_and_paragraphs() {
        let xml = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>Hello &amp; welcome</w:t></w:r></w:p><w:p><w:r><w:t>Second line</w:t></w:r></w:p></w:body></w:document>"#;
        let bytes = make_zip(&[("word/document.xml", xml)]);
        let text = extract_document_text("application/vnd.openxmlformats-officedocument.wordprocessingml.document", &bytes).unwrap();
        assert!(text.contains("Hello & welcome"));
        assert!(text.contains("Second line"));
    }

    #[test]
    fn xlsx_text_extraction_resolves_shared_string_cells() {
        let shared = br#"<sst><si><t>Product</t></si><si><t>Widget</t></si></sst>"#;
        let sheet = br#"<worksheet><sheetData><row><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row></sheetData></worksheet>"#;
        let bytes = make_zip(&[
            ("xl/sharedStrings.xml", shared),
            ("xl/worksheets/sheet1.xml", sheet),
        ]);
        let text = extract_document_text("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", &bytes).unwrap();
        assert!(text.contains("Product"));
        assert!(text.contains("Widget"));
    }

    #[test]
    fn pptx_text_extraction_reads_slide_text() {
        let slide = br#"<p:sld xmlns:p="urn:p" xmlns:a="urn:a"><p:cSld><a:p><a:r><a:t>Slide content</a:t></a:r></a:p></p:cSld></p:sld>"#;
        let bytes = make_zip(&[("ppt/slides/slide1.xml", slide)]);
        let text = extract_document_text("application/vnd.openxmlformats-officedocument.presentationml.presentation", &bytes).unwrap();
        assert!(text.contains("Slide content"));
    }

    #[test]
    fn zip_extraction_only_reads_bounded_utf8_text_and_rejects_traversal() {
        let bytes = make_zip(&[
            ("docs/readme.md", b"safe project notes"),
            ("../secret.txt", b"must not be exposed"),
            ("assets/image.bin", b"not text"),
        ]);
        let metadata = AttachmentMetadata::from_bytes(
            "archive_1".into(), "project.zip".into(), "application/zip".into(),
            AttachmentKind::Document, &bytes,
        ).unwrap();
        assert!(metadata.validate_content(&bytes).is_ok());
        let text = extract_document_text("application/zip", &bytes).unwrap();
        assert!(text.contains("safe project notes"));
        assert!(!text.contains("must not be exposed"));
    }

    #[test]
    fn rich_content_rejects_insecure_citations_and_oversized_tables() {
        let citation = RichBlock::Citation { citation: Citation { title: "Source".into(), url: "http://example.com".into(), publisher: None } };
        assert_eq!(citation.validate(), Err("Citation metadata is invalid or URL is not HTTPS"));
        let table = RichBlock::Table { headers: vec!["a".into()], rows: vec![vec!["x".into(); 40]] };
        assert_eq!(table.validate(), Err("Table exceeds supported dimensions or cell limits"));
    }
}
