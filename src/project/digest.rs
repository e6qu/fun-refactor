use anyhow::Result;
use sha2::{Digest, Sha256};

#[derive(Clone, Default)]
pub(super) struct RevisionDigest {
    pub(super) digest: Sha256,
    pub(super) buffer: Vec<u8>,
}

impl RevisionDigest {
    pub(super) fn update(&mut self, value: impl serde::Serialize) -> Result<()> {
        let start = self.buffer.len();
        if let Err(error) = serde_json::to_writer(&mut self.buffer, &value) {
            self.buffer.truncate(start);
            return Err(error.into());
        }
        if self.buffer.len() >= 65536 {
            self.flush();
        }
        Ok(())
    }

    pub(super) fn flush(&mut self) {
        self.digest.update(&self.buffer);
        self.buffer.clear();
    }

    pub(super) fn finish(mut self) -> String {
        self.flush();
        format!("{:x}", self.digest.finalize())
    }
}
