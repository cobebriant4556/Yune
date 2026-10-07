use std::{collections::HashMap, fs, path::{Component, Path, PathBuf}};

#[derive(Default)]
pub struct AssetPaths {
    root: Option<PathBuf>,
    registered: HashMap<String, PathBuf>,
}

pub fn asset_key(id: &str) -> String {
    let id = id.trim();
    if let Some(number) = id.strip_prefix("rbxassetid://") {
        if let Ok(number) = number.parse::<u64>() { return format!("rbxassetid://{number}"); }
    }
    if id.bytes().all(|byte| byte.is_ascii_digit()) && !id.is_empty() {
        if let Ok(number) = id.parse::<u64>() { return format!("rbxassetid://{number}"); }
    }
    for prefix in ["http://www.roblox.com/asset?", "https://www.roblox.com/asset?", "http://www.roblox.com/asset/?", "https://www.roblox.com/asset/?"] {
        if let Some(query) = id.strip_prefix(prefix) {
            for field in query.split('&') {
                if let Some(number) = field.strip_prefix("id=").and_then(|value| value.split('#').next()) {
                    if let Ok(number) = number.parse::<u64>() { return format!("rbxassetid://{number}"); }
                }
            }
        }
    }
    id.to_string()
}

impl AssetPaths {
    pub fn set_root(&mut self, path: &Path) -> Result<(), String> {
        let root = fs::canonicalize(path).map_err(|error| format!("asset root {}: {error}", path.display()))?;
        if !root.is_dir() { return Err("asset root must be a directory".to_string()); }
        self.root = Some(root);
        Ok(())
    }

    pub fn register(&mut self, id: &str, path: &Path) -> Result<(), String> {
        if id.trim().is_empty() { return Err("asset ID must not be empty".to_string()); }
        let path = fs::canonicalize(path).map_err(|error| error.to_string())?;
        if !path.is_file() { return Err("registered asset must be a file".to_string()); }
        self.registered.insert(asset_key(id), path);
        Ok(())
    }

    pub fn resolve(&self, id: &str) -> Result<PathBuf, String> {
        let key = asset_key(id);
        if let Some(path) = self.registered.get(&key) { return Ok(path.clone()); }
        let root = self.root.as_ref().ok_or_else(|| format!("no local mapping for {id}; use setAssetRoot or registerAsset"))?;
        let names = if let Some(number) = key.strip_prefix("rbxassetid://") {
            if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err("invalid numeric asset ID".to_string());
            }
            let mut names = Vec::new();
            for directory in ["", "meshes/", "images/", "assets/"] {
                for extension in ["", ".mesh", ".meshdata", ".png", ".jpg", ".jpeg", ".dds", ".bin"] {
                    names.push(format!("{directory}{number}{extension}"));
                }
            }
            names
        } else {
            let relative = key.strip_prefix("rbxasset://").unwrap_or(&key).replace('\\', "/");
            if relative.contains(':') || !safe_relative(&relative) {
                return Err(format!("unsupported or unsafe local asset path: {id}"));
            }
            vec![relative.clone(), format!("content/{relative}")]
        };
        for name in names {
            if let Ok(path) = fs::canonicalize(root.join(name)) {
                if path.starts_with(root) && path.is_file() { return Ok(path); }
            }
        }
        Err(format!("asset {id} was not found under {}", root.display()))
    }
}

fn safe_relative(path: &str) -> bool {
    !path.is_empty() && Path::new(path).components().all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

pub fn read_limited(path: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    const LIMIT: u64 = 128 * 1024 * 1024;
    let file = fs::File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut bytes).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > LIMIT { return Err("asset exceeds Yune's 128 MiB input limit".to_string()); }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn id_aliases_share_a_key() {
        assert_eq!(asset_key("123"), asset_key("rbxassetid://00123"));
        assert_eq!(asset_key("http://www.roblox.com/asset/?id=123&v=2"), "rbxassetid://123");
        assert_eq!(asset_key("yune://triangle"), "yune://triangle");
    }
    #[test]
    fn traversal_is_rejected() {
        assert!(!safe_relative("../secrets"));
        assert!(!safe_relative("/absolute"));
        assert!(safe_relative("textures/brick.png"));
    }
    #[test]
    fn root_and_registration_resolve_local_files() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("yune-asset-test-{}-{stamp}", std::process::id()));
        fs::create_dir_all(root.join("content/textures")).unwrap();
        fs::write(root.join("321.mesh"), b"mesh").unwrap();
        fs::write(root.join("content/textures/test.png"), b"image").unwrap();
        let mut paths = AssetPaths::default();
        paths.set_root(&root).unwrap();
        assert_eq!(read_limited(&paths.resolve("rbxassetid://321").unwrap()).unwrap(), b"mesh");
        assert!(paths.resolve("rbxasset://textures/test.png").is_ok());
        assert!(paths.resolve("rbxasset://../escape").is_err());
        paths.register("yune://explicit", &root.join("321.mesh")).unwrap();
        assert!(paths.resolve("yune://explicit").is_ok());
        fs::remove_dir_all(root).unwrap();
    }
}
