use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// What a site actually serves.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SiteMode {
    /// Serve a folder of static files ourselves.
    Static { folder: String },
    /// Point at something already running locally (e.g. a dev server).
    Existing { local_port: u16 },
}

/// One onion site the user has created, whether it's currently published or not.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub id: String,
    pub name: String,
    pub mode: SiteMode,
    /// Filled in the first time the site is published; stays stable after that
    /// because we reuse the same private key.
    pub onion_address: Option<String>,
    /// The Tor "ED25519-V3:<base64>" private key blob. This is the only thing
    /// that lets the identity be recreated elsewhere - treat it like a password.
    pub private_key: Option<String>,
    pub created_at: String,
    /// Whether the site should come back up automatically when Velum starts.
    pub enabled: bool,
}

/// The subset of a Site that's safe to send to the frontend by default
/// (never includes the private key).
#[derive(Debug, Clone, Serialize)]
pub struct SiteView {
    pub id: String,
    pub name: String,
    pub mode: SiteMode,
    pub onion_address: Option<String>,
    pub created_at: String,
    pub enabled: bool,
    pub running: bool,
}

pub struct SiteStore {
    path: PathBuf,
    sites: Vec<Site>,
}

impl SiteStore {
    pub fn load(data_dir: &Path) -> anyhow::Result<Self> {
        fs::create_dir_all(data_dir)?;
        let path = data_dir.join("sites.json");
        let sites = if path.exists() {
            let raw = fs::read_to_string(&path)?;
            serde_json::from_str(&raw).unwrap_or_default()
        } else {
            Vec::new()
        };
        Ok(Self { path, sites })
    }

    fn save(&self) -> anyhow::Result<()> {
        let raw = serde_json::to_string_pretty(&self.sites)?;
        fs::write(&self.path, raw)?;
        Ok(())
    }

    pub fn all(&self) -> &[Site] {
        &self.sites
    }

    pub fn get(&self, id: &str) -> Option<&Site> {
        self.sites.iter().find(|s| s.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Site> {
        self.sites.iter_mut().find(|s| s.id == id)
    }

    pub fn insert(&mut self, site: Site) -> anyhow::Result<()> {
        self.sites.push(site);
        self.save()
    }

    pub fn remove(&mut self, id: &str) -> anyhow::Result<Option<Site>> {
        let removed = if let Some(pos) = self.sites.iter().position(|s| s.id == id) {
            Some(self.sites.remove(pos))
        } else {
            None
        };
        self.save()?;
        Ok(removed)
    }

    pub fn persist(&self) -> anyhow::Result<()> {
        self.save()
    }
}
