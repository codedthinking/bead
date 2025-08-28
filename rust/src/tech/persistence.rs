use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use serde::{Deserialize, Serialize};
use crate::error::Result;

/// Load JSON from a file
pub fn load_json<T: for<'de> Deserialize<'de>>(path: impl AsRef<Path>) -> Result<T> {
    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let data = serde_json::from_str(&contents)?;
    Ok(data)
}

/// Save JSON to a file with pretty formatting
pub fn save_json<T: Serialize>(data: &T, path: impl AsRef<Path>) -> Result<()> {
    let json = serde_json::to_string_pretty(data)?;
    let mut file = File::create(path)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}

/// Load JSON from a ZIP file entry
pub fn load_json_from_zip<T: for<'de> Deserialize<'de>>(
    archive: &mut zip::ZipArchive<File>,
    path: &str,
) -> Result<T> {
    let mut file = archive.by_name(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let data = serde_json::from_str(&contents)?;
    Ok(data)
}

/// Save JSON to a ZIP file entry
pub fn save_json_to_zip<T: Serialize, W: Write + std::io::Seek>(
    writer: &mut zip::ZipWriter<W>,
    data: &T,
    path: &str,
) -> Result<()> {
    let json = serde_json::to_string_pretty(data)?;
    writer.start_file(path, zip::write::FileOptions::default())?;
    writer.write_all(json.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::collections::HashMap;

    #[test]
    fn test_json_roundtrip() {
        let mut data = HashMap::new();
        data.insert("key1".to_string(), "value1".to_string());
        data.insert("key2".to_string(), "value2".to_string());

        let temp_file = NamedTempFile::new().unwrap();
        save_json(&data, temp_file.path()).unwrap();

        let loaded: HashMap<String, String> = load_json(temp_file.path()).unwrap();
        assert_eq!(loaded, data);
    }
}