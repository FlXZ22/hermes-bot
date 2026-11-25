use rusty_leveldb::{in_memory, LdbIterator, Options, DB};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::SystemTime;

const STORAGE_PREFIX: &[u8] = b"_hermes-desktop://app\0";
const MAX_SNAPSHOT: u64 = 32 * 1024 * 1024;
const MAX_IMPORT: usize = 8 * 1024 * 1024;

type Stamp = (u64, SystemTime);

pub fn from_electron() -> Result<BTreeMap<String, String>, String> {
    let config = dirs::config_dir().ok_or("Could not locate the desktop configuration folder")?;
    let source = config.join("hermes-grok-bot-chromium/Local Storage/leveldb");
    let files = match snapshot_files(&source) {
        Ok(files) => files,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(format!("Could not read the previous renderer profile: {error}")),
    };
    if !files.contains_key("CURRENT") {
        return Ok(BTreeMap::new());
    }
    // Work only on a bounded, in-memory copy. The active Electron session
    // remains untouched, including its journal and exclusive LevelDB lock.
    let path = Path::new("legacy");
    let mut options = in_memory();
    options.create_if_missing = false;
    options.paranoid_checks = true;
    let mut remaining = MAX_SNAPSHOT;
    for (name, (size, _)) in &files {
        if *size > remaining {
            return Err("The previous renderer profile is too large to migrate automatically.".into());
        }
        remaining -= *size;
        let mut source_file = OpenOptions::new().read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(source.join(name))
            .map_err(|_| "The previous renderer profile changed while reading it.")?;
        let mut target = options.env.open_writable_file(&path.join(name)).map_err(|error| error.to_string())?;
        let copied = io::copy(&mut source_file.by_ref().take(*size + 1), &mut target).map_err(|error| error.to_string())?;
        if copied != *size {
            return Err("The previous renderer profile is still being written. Close Electron and relaunch Tauri.".into());
        }
    }
    if snapshot_files(&source).ok().as_ref() != Some(&files) {
        return Err("The previous renderer profile is still being written. Close Electron and relaunch Tauri.".into());
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read_entries(path, options)))
        .map_err(|_| "The previous renderer storage could not be decoded.".to_string())?
}

fn snapshot_files(path: &Path) -> io::Result<BTreeMap<String, Stamp>> {
    let mut files = BTreeMap::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else { continue; };
        let allowed = name == "CURRENT"
            || name.strip_prefix("MANIFEST-").is_some_and(decimal)
            || name.split_once('.').is_some_and(|(stem, ext)| decimal(stem) && matches!(ext, "ldb" | "sst" | "log"));
        if allowed && entry.file_type()?.is_file() {
            let metadata = entry.metadata()?;
            files.insert(name, (metadata.len(), metadata.modified()?));
        }
    }
    Ok(files)
}

fn decimal(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

fn read_entries(path: &Path, options: Options) -> Result<BTreeMap<String, String>, String> {
    let mut db = DB::open(path, options).map_err(|_| "Could not open the previous renderer storage.")?;
    let mut iter = db.new_iter().map_err(|_| "Could not read the previous renderer storage.")?;
    let mut entries = BTreeMap::new();
    let mut length = 0;
    iter.seek(STORAGE_PREFIX);
    while let Some((key, value)) = iter.current() {
        let Some(encoded) = key.strip_prefix(STORAGE_PREFIX) else { break; };
        if let Some(key) = decode(encoded).filter(|key| allowed_key(key)) {
            if let Some(value) = decode(&value) {
                length += key.len() + value.len();
                if length > MAX_IMPORT {
                    return Err("The previous renderer data exceeds the import limit.".into());
                }
                entries.insert(key, value);
            }
        }
        if !iter.advance() { break; }
    }
    Ok(entries)
}

fn allowed_key(key: &str) -> bool {
    key.strip_prefix("hermes-grok-bot:").is_some_and(|name| {
        matches!(name, "theme" | "side-width" | "drafts" | "work-drafts" | "work-sessions" | "boards")
            || name.starts_with("boards:")
            || name.starts_with("terminals:")
    })
}

fn decode(encoded: &[u8]) -> Option<String> {
    let (marker, text) = encoded.split_first()?;
    match marker {
        1 => Some(text.iter().map(|&byte| char::from(byte)).collect()),
        0 if text.len() % 2 == 0 => {
            let units: Vec<_> = text.chunks_exact(2).map(|x| u16::from_le_bytes([x[0], x[1]])).collect();
            Some(String::from_utf16_lossy(&units))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_only_eligible_values_from_the_electron_app_origin() {
        let path = Path::new("legacy");
        let mut options = in_memory();
        options.compressor = 1; // legacy Chrome tables use Snappy
        let mut legacy = DB::open(path, options.clone()).unwrap();
        let key = |suffix: &str| [STORAGE_PREFIX, b"\x01hermes-grok-bot:", suffix.as_bytes()].concat();
        let mut unicode = vec![0];
        for unit in "caffè 🔧".encode_utf16() {
            unicode.extend(unit.to_le_bytes());
        }
        legacy.put(&key("theme"), b"\x01sand-dark").unwrap();
        legacy.put(&key("work-drafts"), &unicode).unwrap();
        legacy.put(&key("connection"), b"\x01private-token").unwrap();
        legacy.put(&key("terminals:fixture"), b"\x01fixture terminal").unwrap();
        legacy.put(&key("removed"), b"\x01old").unwrap();
        legacy.delete(&key("removed")).unwrap();
        legacy.put(b"_https://unrelated.test\0\x01hermes-grok-bot:theme", b"\x01unrelated").unwrap();
        legacy.close().unwrap();
        let migrated = read_entries(path, options).unwrap();
        assert_eq!(migrated, BTreeMap::from([
            ("hermes-grok-bot:theme".to_string(), "sand-dark".to_string()),
            ("hermes-grok-bot:work-drafts".to_string(), "caffè 🔧".to_string()),
            ("hermes-grok-bot:terminals:fixture".to_string(), "fixture terminal".to_string()),
        ]));
    }
}
