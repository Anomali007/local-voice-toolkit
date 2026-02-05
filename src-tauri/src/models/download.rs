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
