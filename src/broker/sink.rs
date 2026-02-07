//! Sink delivery — clipboard and file output for captured turns.
//!
//! Each function is called from the broker loop after a `Deliver`
//! handler returns a [`SideEffect::Clipboard`] or [`SideEffect::FileWrite`].
//!
//! Both are best-effort per CONTRACT_REGISTRY.md §328–329. On failure
//! the broker loop replaces the optimistic ok response with an error.
//!
//! CONTRACT_REGISTRY.md §266: every sink receives `(content, metadata)`.

use super::state::SinkMetadata;

/// Write content to the system clipboard via the provided writer.
///
/// Delegates to `clipboard_writer` for the actual clipboard operation.
/// Returns `Err("clipboard_failed")` if the writer fails.
///
/// The writer is a boxed closure that wraps a `ClipboardProvider::write()`
/// call, keeping the broker structurally independent of resolver types.
///
/// `metadata` is accepted per CONTRACT_REGISTRY.md §266 but unused
/// by the clipboard sink.
pub async fn deliver_clipboard(
    content: &[u8],
    _metadata: &SinkMetadata,
    clipboard_writer: &(dyn Fn(&[u8]) -> Result<(), String> + Sync),
) -> Result<(), String> {
    clipboard_writer(content)
}

/// Write content to a file.
///
/// Uses `tokio::fs::write` for async I/O. Returns `Err("file_write_failed")`
/// on any I/O error.
///
/// `metadata` is accepted per CONTRACT_REGISTRY.md §266 but unused
/// by the file sink in v1.
pub async fn deliver_file(
    path: &str,
    content: &[u8],
    _metadata: &SinkMetadata,
) -> Result<(), String> {
    tokio::fs::write(path, content)
        .await
        .map_err(|_| "file_write_failed".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_metadata() -> SinkMetadata {
        SinkMetadata {
            turn_id: "s1:1".into(),
            timestamp: 1000,
            byte_length: 15,
            interrupted: false,
            truncated: false,
        }
    }

    #[tokio::test]
    async fn file_write_success() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("output.txt");
        let content = b"hello from sink";

        deliver_file(path.to_str().unwrap(), content, &dummy_metadata())
            .await
            .unwrap();

        let written = tokio::fs::read(&path).await.unwrap();
        assert_eq!(written, content);
    }

    #[tokio::test]
    async fn file_write_bad_path() {
        let result = deliver_file("/nonexistent/dir/file.txt", b"data", &dummy_metadata()).await;
        assert_eq!(result, Err("file_write_failed".to_string()));
    }
}
