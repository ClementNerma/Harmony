use std::{path::Path, time::SystemTime};

use anyhow::{bail, Context, Result};
use rand::{rngs::OsRng, Rng};
use serde::{Deserialize, Serialize};
use tokio::fs;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppData {
    access_tokens: Vec<AccessToken>,
}

impl AppData {
    pub fn empty() -> Self {
        Self {
            access_tokens: vec![],
        }
    }

    pub async fn load(path: &Path) -> Result<Self> {
        if !path.is_file() {
            bail!("Provided file path was not found");
        }

        let json = fs::read_to_string(path)
            .await
            .context("Failed to read app data file")?;

        serde_json::from_str(&json).context("Failed to parse app data file")
    }

    pub async fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string(self).context("Failed to serialize app data")?;
        fs::write(path, json)
            .await
            .context("Failed to write app data to file")
    }

    pub fn create_access_token(&mut self, device_name: String) -> &AccessToken {
        self.access_tokens.push(AccessToken::new(device_name));
        self.access_tokens.last().unwrap()
    }

    pub fn get_access_token(&mut self, token: &str) -> Option<&AccessToken> {
        let access_token = self.access_tokens.iter_mut().find(|c| c.token == token)?;
        access_token.last_use = SystemTime::now();
        Some(access_token)
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessToken {
    device_name: String,
    token: String,
    created_at: SystemTime,
    last_use: SystemTime,
}

impl AccessToken {
    pub fn new(device_name: String) -> Self {
        let now = SystemTime::now();

        Self {
            device_name,
            token: generate_id(),
            created_at: now,
            last_use: now,
        }
    }

    // pub fn device_name(&self) -> &str {
    //     &self.device_name
    // }

    pub fn token(&self) -> &str {
        &self.token
    }

    // pub fn created_at(&self) -> &SystemTime {
    //     &self.created_at
    // }
}

const ACCESS_TOKEN_CHARSET: &[u8] =
    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

pub fn generate_id() -> String {
    let one_char = || ACCESS_TOKEN_CHARSET[OsRng.gen_range(0..ACCESS_TOKEN_CHARSET.len())] as char;
    (0..32).map(|_| one_char()).collect()
}
