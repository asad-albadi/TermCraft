use super::*;
pub(super) enum UploadSource {
    File {
        file: tokio::fs::File,
        before: std::fs::Metadata,
    },
    Memory(std::io::Cursor<Vec<u8>>),
}
impl UploadSource {
    pub fn len(&self) -> u64 {
        match self {
            Self::File { before, .. } => before.len(),
            Self::Memory(cursor) => cursor.get_ref().len() as u64,
        }
    }
    pub async fn read_exact(&mut self, bytes: &mut [u8]) -> Result<(), String> {
        match self {
            Self::File { file, .. } => {
                file.read_exact(bytes).await.map_err(error)?;
            }
            Self::Memory(cursor) => std::io::Read::read_exact(cursor, bytes).map_err(error)?,
        }
        Ok(())
    }
    pub async fn read_extra(&mut self) -> Result<usize, String> {
        let mut byte = [0];
        match self {
            Self::File { file, .. } => file.read(&mut byte).await.map_err(error),
            Self::Memory(cursor) => std::io::Read::read(cursor, &mut byte).map_err(error),
        }
    }
    pub async fn unchanged(&self) -> Result<bool, String> {
        match self {
            Self::File { file, before } => {
                let after = file.metadata().await.map_err(error)?;
                Ok(before.len() == after.len() && before.modified().ok() == after.modified().ok())
            }
            Self::Memory(_) => Ok(true),
        }
    }
}
