use std::{
    convert::Infallible,
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

    pub fn slot_transfer_dir(&self, slot: &SlotInfos, sync_token: &str) -> PathBuf {
        self.slot_root_dir(slot)
            .join(format!("open-sync-{sync_token}"))
    }

    pub fn slot_completion_dir(&self, slot: &SlotInfos, sync_token: &str) -> PathBuf {
        self.slot_transfer_dir(slot, sync_token).join("complete")
    }

    pub fn slot_pending_dir(&self, slot: &SlotInfos, sync_token: &str) -> PathBuf {
        self.slot_transfer_dir(slot, sync_token).join("pending")
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
    pub fn parse(input: &str) -> Self {
        match input.find(':') {
            Some(sep) => Self {
                name: input[0..sep].to_owned(),
                linked: Some(PathBuf::from(&input[sep + 1..])),
            },

            None => Self {
                name: input.to_owned(),
                linked: None,
            },
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
    type Err = Infallible;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Ok(Self::parse(input))
    }
}
