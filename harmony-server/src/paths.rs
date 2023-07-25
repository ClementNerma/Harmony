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

    pub fn slot_dir(&self, slot_name: &str) -> PathBuf {
        self.data_dir.join("slots").join(slot_name)
    }

    pub fn slot_files_dir(&self, slot_name: &str) -> PathBuf {
        self.slot_dir(slot_name).join("content")
    }

    pub fn slot_open_sync_dir(&self, slot_name: &str, sync_token: &str) -> PathBuf {
        self.slot_dir(slot_name)
            .join(format!("opened-sync-{sync_token}"))
    }

    pub fn slot_opened_sync_complete_dir(&self, slot_name: &str, sync_token: &str) -> PathBuf {
        self.slot_open_sync_dir(slot_name, sync_token)
            .join("complete")
    }

    pub fn slot_opened_sync_pending_dir(&self, slot_name: &str, sync_token: &str) -> PathBuf {
        self.slot_open_sync_dir(slot_name, sync_token)
            .join("pending")
    }
}
