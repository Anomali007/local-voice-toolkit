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

        let response = self.client.get(url).send().await?;
        let total_size = response.content_length().unwrap_or(0);

        tracing::info!("Download size: {} bytes", total_size);

        let mut file = tokio::fs::File::create(dest).await?;
        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();

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
