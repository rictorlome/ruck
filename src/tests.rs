use crate::crypto::new_cipher;
use crate::file::FileHandle;

use std::path::PathBuf;

#[tokio::test]
async fn test_file_handle_nonexistent_file() {
    let pb = PathBuf::new();
    let fh = FileHandle::new(pb).await;
    assert!(fh.is_err());
}
