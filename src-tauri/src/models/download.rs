#![allow(dead_code)]

use anyhow::Result;
use futures_util::StreamExt;
use std::path::Path;

pub struct ModelDownloader {
    client: reqwest::Client,
}

impl ModelDownloader {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    pub async fn download<F>(
        &self,
        url: &str,
        dest: &Path,
        progress_callback: F,
    ) -> Result<()>
    where
        F: Fn(DownloadProgress) + Send + 'static,
    {
        tracing::info!("Downloading from: {}", url);

        // A non-2xx response (e.g. a moved file's 404 page) must never be
        // saved as the model.
        let response = self.client.get(url).send().await?.error_for_status()?;
        let total_size = response.content_length().unwrap_or(0);

        tracing::info!("Download size: {} bytes", total_size);

        // Write to a .part file and rename only once complete, so an
        // interrupted download never looks like a finished model.
        let part = part_path(dest);
        let mut file = tokio::fs::File::create(&part).await?;
        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();

        let streamed: Result<()> = async {
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await?;

                downloaded += chunk.len() as u64;

                let progress = DownloadProgress {
                    downloaded,
                    total: total_size,
                    percentage: if total_size > 0 {
                        (downloaded as f64 / total_size as f64 * 100.0) as u8
                    } else {
                        0
                    },
                };

                progress_callback(progress);
            }
            tokio::io::AsyncWriteExt::flush(&mut file).await?;
            Ok(())
        }
        .await;
        drop(file);

        let result = streamed.and_then(|()| check_complete(downloaded, total_size));
        if let Err(e) = result {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(e);
        }

        tokio::fs::rename(&part, dest).await?;
        tracing::info!("Download complete: {:?}", dest);
        Ok(())
    }

    pub async fn download_with_retry<F>(
        &self,
        url: &str,
        dest: &Path,
        progress_callback: F,
        max_retries: u32,
    ) -> Result<()>
    where
        F: Fn(DownloadProgress) + Send + Clone + 'static,
    {
        let mut last_error = None;

        for attempt in 0..max_retries {
            if attempt > 0 {
                tracing::info!("Retry attempt {} for {}", attempt, url);
                tokio::time::sleep(tokio::time::Duration::from_secs(2_u64.pow(attempt))).await;
            }

            match self.download(url, dest, progress_callback.clone()).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    tracing::warn!("Download attempt {} failed: {}", attempt + 1, e);
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap())
    }
}

impl Default for ModelDownloader {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
    pub percentage: u8,
}

/// Temporary path a download is written to before it is complete.
pub fn part_path(dest: &Path) -> std::path::PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest.with_file_name(name)
}

/// A download is complete when it received the advertised length (or, when
/// no length was sent, at least something).
fn check_complete(downloaded: u64, expected: u64) -> Result<()> {
    if (expected > 0 && downloaded != expected) || downloaded == 0 {
        anyhow::bail!("Download incomplete: received {} of {} bytes", downloaded, expected);
    }
    Ok(())
}

/// Extract a zip file to a directory
/// For CoreML models, the zip contains a .mlmodelc directory structure
pub fn extract_zip(zip_path: &Path, dest_dir: &Path) -> Result<()> {
    tracing::info!("Extracting zip: {:?} to {:?}", zip_path, dest_dir);

    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    // Create destination directory
    std::fs::create_dir_all(dest_dir)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = match file.enclosed_name() {
            Some(path) => {
                // The zip contains paths like "ggml-base.en-encoder.mlmodelc/..."
                // We want to extract to dest_dir directly, stripping the top-level dir
                let components: Vec<_> = path.components().collect();
                if components.len() > 1 {
                    // Skip the first component (the .mlmodelc dir name in the zip)
                    let relative: std::path::PathBuf = components[1..].iter().collect();
                    dest_dir.join(relative)
                } else {
                    // This is the top-level directory entry, skip it
                    continue;
                }
            }
            None => continue,
        };

        if file.name().ends_with('/') {
            // It's a directory
            std::fs::create_dir_all(&outpath)?;
        } else {
            // It's a file
            if let Some(parent) = outpath.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            let mut outfile = std::fs::File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
        }

        // Set permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = file.unix_mode() {
                std::fs::set_permissions(&outpath, std::fs::Permissions::from_mode(mode))?;
            }
        }
    }

    tracing::info!("Extraction complete: {:?}", dest_dir);
    Ok(())
}

/// Check if a model ID represents a CoreML model
pub fn is_coreml_model(model_id: &str) -> bool {
    model_id.ends_with(".mlmodelc")
}

/// Check if a URL points to a zip file
pub fn is_zip_url(url: &str) -> bool {
    url.ends_with(".zip")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_is_coreml_model() {
        assert!(is_coreml_model("ggml-base.en-encoder.mlmodelc"));
        assert!(is_coreml_model("ggml-tiny.en-encoder.mlmodelc"));
        assert!(!is_coreml_model("ggml-base.en.bin"));
        assert!(!is_coreml_model("kokoro-v1.0.onnx"));
    }

    #[test]
    fn test_is_zip_url() {
        assert!(is_zip_url("https://example.com/model.mlmodelc.zip"));
        assert!(is_zip_url("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en-encoder.mlmodelc.zip"));
        assert!(!is_zip_url("https://example.com/model.bin"));
        assert!(!is_zip_url("https://example.com/model.onnx"));
    }

    #[test]
    fn test_extract_zip_creates_destination() {
        let temp_dir = TempDir::new().unwrap();
        let zip_path = temp_dir.path().join("test.zip");
        let dest_dir = temp_dir.path().join("extracted");

        // Create a simple zip file with a nested structure
        {
            let file = std::fs::File::create(&zip_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);

            // Add a directory entry (simulating top-level .mlmodelc dir)
            let options =
                zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

            zip.add_directory("test.mlmodelc/", options).unwrap();

            // Add a file inside the directory
            zip.start_file("test.mlmodelc/model.json", options).unwrap();
            zip.write_all(b"{\"test\": true}").unwrap();

            // Add a subdirectory with a file
            zip.add_directory("test.mlmodelc/subdir/", options).unwrap();
            zip.start_file("test.mlmodelc/subdir/data.bin", options)
                .unwrap();
            zip.write_all(b"binary data").unwrap();

            zip.finish().unwrap();
        }

        // Extract the zip
        extract_zip(&zip_path, &dest_dir).unwrap();

        // Verify the destination was created
        assert!(dest_dir.exists());

        // Verify the files were extracted (top-level dir stripped)
        assert!(dest_dir.join("model.json").exists());
        assert!(dest_dir.join("subdir").exists());
        assert!(dest_dir.join("subdir/data.bin").exists());

        // Verify file contents
        let content = std::fs::read_to_string(dest_dir.join("model.json")).unwrap();
        assert_eq!(content, "{\"test\": true}");
    }

    #[test]
    fn test_extract_zip_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let result = extract_zip(
            &temp_dir.path().join("nonexistent.zip"),
            &temp_dir.path().join("dest"),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_part_path() {
        let p = part_path(Path::new("/tmp/models/kokoro-v1.0.onnx"));
        assert_eq!(p, Path::new("/tmp/models/kokoro-v1.0.onnx.part"));
    }

    #[test]
    fn test_check_complete() {
        assert!(check_complete(100, 100).is_ok());
        assert!(check_complete(42, 0).is_ok());
        assert!(check_complete(99, 100).is_err(), "truncated");
        assert!(check_complete(0, 0).is_err(), "empty body");
    }

    /// Serve one canned HTTP response on a local port.
    async fn serve_once(status: &str, body: &'static [u8], content_length: Option<usize>) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let status = status.to_string();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf).await;
            let len = content_length.unwrap_or(body.len());
            let head = format!("HTTP/1.1 {status}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n");
            let _ = sock.write_all(head.as_bytes()).await;
            let _ = sock.write_all(body).await;
        });
        format!("http://{addr}/model.bin")
    }

    #[tokio::test]
    async fn test_download_404_leaves_no_file() {
        let url = serve_once("404 Not Found", b"Entry not found", None).await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("model.bin");
        let result = ModelDownloader::new().download(&url, &dest, |_| {}).await;
        assert!(result.is_err());
        assert!(!dest.exists(), "error page must not be saved as the model");
        assert!(!part_path(&dest).exists());
    }

    #[tokio::test]
    async fn test_download_truncated_leaves_no_file() {
        let url = serve_once("200 OK", b"only part", Some(1000)).await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("model.bin");
        let result = ModelDownloader::new().download(&url, &dest, |_| {}).await;
        assert!(result.is_err());
        assert!(!dest.exists());
        assert!(!part_path(&dest).exists());
    }

    #[tokio::test]
    async fn test_download_success() {
        let url = serve_once("200 OK", b"model bytes", None).await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("model.bin");
        ModelDownloader::new().download(&url, &dest, |_| {}).await.unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"model bytes");
        assert!(!part_path(&dest).exists());
    }

    #[test]
    fn test_download_progress_percentage() {
        let progress = DownloadProgress {
            downloaded: 50,
            total: 100,
            percentage: 50,
        };
        assert_eq!(progress.percentage, 50);

        let progress_complete = DownloadProgress {
            downloaded: 100,
            total: 100,
            percentage: 100,
        };
        assert_eq!(progress_complete.percentage, 100);
    }
}
