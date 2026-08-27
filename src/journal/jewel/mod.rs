use crate::journal::file::FileId;
use crate::journal::{JournalId, JournalResult, JournalService};

impl JournalService {
    #[expect(unused)]
    pub async fn stat_jewel_db(&self, journal_id: JournalId, file_id: FileId) -> JournalResult<()> {
        todo!()
    }
}
