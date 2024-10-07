use std::{error::Error, fmt, fs::File, io::BufReader};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ModpackConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ModpackMetadata>,
    pub mods: Vec<Mod>,
}

#[derive(Serialize, Deserialize)]
pub struct ModpackMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Mod {
    // json config fields
    pub name: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_dependencies: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patches: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rimraf: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_change: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_pak_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steam_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_priority: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    calculated_priority: Option<i32>,
    #[serde(skip_serializing)]
    pub metadata: Option<Metadata>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    // unneeded in launcher; when codebases are combined this will return
}

impl ModpackConfig {
    pub fn modpack_version(&self) -> Option<String> {
        return self.metadata.as_ref()?.version.clone();
    }
}

#[derive(Debug, Clone)]
struct UnsupportedVersionError;
impl fmt::Display for UnsupportedVersionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Unsupported modpack metadata version!  Please upgrade the launcher"
        )
    }
}
impl Error for UnsupportedVersionError {}

pub fn read_mods(path: &str) -> Result<ModpackConfig, Box<dyn Error>> {
    // Open the file in read-only mode with buffer.
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let config: ModpackConfig = serde_json::from_reader(reader)?;
    println!(
        "Read {} mods from configuration at {}",
        config.mods.len(),
        path
    );

    if config.format.unwrap_or_default() > 3 {
        // hardcoded value supported by this version of the launcher
        return Err(Box::new(UnsupportedVersionError));
    }

    Ok(config)
}
