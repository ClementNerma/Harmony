use anyhow::{bail, Result};

use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

pub struct Paths {
    data_dir: PathBuf,
}

impl Paths {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    // pub fn data_dir(&self) -> &Path {
    //     &self.data_dir
    // }

    pub fn app_data_file(&self) -> PathBuf {
        self.data_dir.join("state.json")
    }

    pub fn slot_root_dir(&self, slot: &SlotInfos) -> PathBuf {
        self.data_dir.join("slots").join(slot.name())
    }

    pub fn slot_content_dir(&self, slot: &SlotInfos) -> PathBuf {
        slot.linked()
            .map(Path::to_owned)
            .unwrap_or_else(|| self.slot_root_dir(slot).join("content"))
    }

    pub fn slot_transfer_dir(&self, slot: &SlotInfos, SyncId(sync_id): SyncId) -> PathBuf {
        self.slot_root_dir(slot)
            .join(format!("open-sync-{sync_id:x}"))
    }

    pub fn slot_completion_dir(&self, slot: &SlotInfos, sync_id: SyncId) -> PathBuf {
        self.slot_transfer_dir(slot, sync_id).join("complete")
    }

    pub fn slot_pending_dir(&self, slot: &SlotInfos, sync_id: SyncId) -> PathBuf {
        self.slot_transfer_dir(slot, sync_id).join("pending")
    }
}

pub fn is_relative_linear_path(path: &Path) -> bool {
    path.has_root() || path.iter().any(|c| c == "." || c == "..")
}

#[derive(Clone)]
pub struct SlotInfos {
    name: String,
    linked: Option<PathBuf>,
}

impl SlotInfos {
    pub fn new(name: String, linked: Option<PathBuf>) -> Result<Self> {
        if name.trim().is_empty() {
            bail!("Slot name cannot be empty");
        }

        for fc in FORBIDDEN_CHARS {
            if name.contains(*fc) {
                bail!("Character {fc:?} is forbidden");
            }
        }

        if let Some(ref linked) = linked {
            if !linked.has_root() {
                bail!("Path linking requires a root path");
            }

            if linked.iter().any(|c| c == ".") {
                bail!("Current dir components '.' are forbidden in linked paths");
            }

            if linked.iter().any(|c| c == "..") {
                bail!("Parent dir components '..' are forbidden in linked paths");
            }
        }

        Ok(Self { name, linked })
    }

    pub fn parse(input: &str) -> Result<Self> {
        match input.find(':') {
            Some(sep) => Self::new(
                input[0..sep].to_owned(),
                Some(PathBuf::from(&input[sep + 1..])),
            ),

            None => Self::new(input.to_owned(), None),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn linked(&self) -> Option<&Path> {
        self.linked.as_deref()
    }
}

impl FromStr for SlotInfos {
    type Err = anyhow::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::parse(input)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SyncId(pub u64);

static FORBIDDEN_CHARS: &[char] = &[
    '/', '\\', '<', '>', ':', '"', '|', '?', '*', '\r', '\n', '\x00',
];
