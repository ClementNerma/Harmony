use std::path::PathBuf;

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

    pub fn slot_root_dir(&self, slot_name: &str) -> PathBuf {
        self.data_dir.join("slots").join(slot_name)
    }

    pub fn slot_content_dir(&self, slot_name: &str) -> PathBuf {
        self.slot_root_dir(slot_name).join("content")
    }

    pub fn slot_transfer_dir(&self, slot_name: &str, sync_token: &str) -> PathBuf {
        self.slot_root_dir(slot_name)
            .join(format!("opened-sync-{sync_token}"))
    }

    pub fn slot_completion_dir(&self, slot_name: &str, sync_token: &str) -> PathBuf {
        self.slot_transfer_dir(slot_name, sync_token)
            .join("complete")
    }

    pub fn slot_pending_dir(&self, slot_name: &str, sync_token: &str) -> PathBuf {
        self.slot_transfer_dir(slot_name, sync_token)
            .join("pending")
    }
}
