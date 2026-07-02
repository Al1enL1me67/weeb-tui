use anyhow::{Context, Result};
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

const MAX_CONCURRENT_DOWNLOADS: usize = 10;
const MAX_MEMORY_CACHE: usize = 50;

#[derive(Debug, Clone)]
pub struct CachedImage {
    pub id: String,
    pub url: String,
    pub data: Vec<u8>,
    pub accessed_at: std::time::Instant,
}

#[derive(Clone)]
pub struct ImagePipeline {
    #[allow(dead_code)]
    client: reqwest::Client,
    memory_cache: Arc<RwLock<LruCache<String, CachedImage>>>,
    db: Arc<crate::db::Database>,
    download_tx: mpsc::Sender<DownloadRequest>,
}

#[derive(Debug)]
struct DownloadRequest {
    id: String,
    url: String,
    result_tx: mpsc::Sender<Result<Vec<u8>>>,
}

impl ImagePipeline {
    pub fn new(db: Arc<crate::db::Database>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        let memory_cache = Arc::new(RwLock::new(
            LruCache::new(NonZeroUsize::new(MAX_MEMORY_CACHE).unwrap()),
        ));
        let (download_tx, mut download_rx) = mpsc::channel::<DownloadRequest>(100);

        let client_clone = client.clone();
        let cache_clone = memory_cache.clone();
        let db_clone = db.clone();

        // Spawn background download worker
        tokio::spawn(async move {
            let semaphore = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_DOWNLOADS));

            while let Some(request) = download_rx.recv().await {
                let permit = semaphore.clone().acquire_owned().await.unwrap();
                let client = client_clone.clone();
                let cache = cache_clone.clone();
                let db = db_clone.clone();
                let result_tx = request.result_tx.clone();

                tokio::spawn(async move {
                    let _permit = permit;
                    let result =
                        Self::download_and_cache(client, cache, db, &request.id, &request.url)
                            .await;

                    let _ = result_tx.send(result).await;
                });
            }
        });

        Self {
            client,
            memory_cache,
            db,
            download_tx,
        }
    }

    async fn download_and_cache(
        client: reqwest::Client,
        cache: Arc<RwLock<LruCache<String, CachedImage>>>,
        db: Arc<crate::db::Database>,
        id: &str,
        url: &str,
    ) -> Result<Vec<u8>> {
        // Check disk cache first
        if let Some(cached) = db.get_cached_image(id).await? {
            tracing::debug!("Image {} loaded from disk cache", id);

            // Add to memory cache
            let image = CachedImage {
                id: id.to_string(),
                url: url.to_string(),
                data: cached.data.clone(),
                accessed_at: std::time::Instant::now(),
            };

            let mut cache_guard = cache.write().await;
            cache_guard.put(id.to_string(), image);
            drop(cache_guard);

            return Ok(cached.data);
        }

        // Download from URL
        tracing::debug!("Downloading image {} from {}", id, url);
        let response = client.get(url).send().await?;
        let data = response.bytes().await?.to_vec();

        // Save to disk cache
        let _ = db.cache_image(id, url, &data).await;

        // Add to memory cache
        let image = CachedImage {
            id: id.to_string(),
            url: url.to_string(),
            data: data.clone(),
            accessed_at: std::time::Instant::now(),
        };

        let mut cache_guard = cache.write().await;
        cache_guard.put(id.to_string(), image);

        Ok(data)
    }

    pub async fn get_image(&self, id: &str) -> Option<CachedImage> {
        // Check memory cache first
        let mut cache_guard = self.memory_cache.write().await;
        if let Some(image) = cache_guard.get(id) {
            return Some(image.clone());
        }

        None
    }

    pub async fn request_download(&self, id: String, url: String) -> Result<Vec<u8>> {
        // Check memory cache first
        {
            let mut cache_guard = self.memory_cache.write().await;
            if let Some(image) = cache_guard.get(&id) {
                return Ok(image.data.clone());
            }
        }

        // Check disk cache
        if let Some(cached) = self.db.get_cached_image(&id).await? {
            // Add to memory cache
            let image = CachedImage {
                id: id.clone(),
                url: url.clone(),
                data: cached.data.clone(),
                accessed_at: std::time::Instant::now(),
            };

            let mut cache_guard = self.memory_cache.write().await;
            cache_guard.put(id.clone(), image);
            drop(cache_guard);

            return Ok(cached.data);
        }

        // Send download request
        let (result_tx, mut result_rx) = mpsc::channel(1);
        let request = DownloadRequest { id, url, result_tx };

        let _ = self.download_tx.send(request).await;

        // Wait for result
        result_rx
            .recv()
            .await
            .context("Download worker closed")?
            .context("Download failed")
    }

    pub async fn preload_images(&self, images: Vec<(String, String)>) {
        for (id, url) in images {
            // Check if already in cache
            {
                let cache_guard = self.memory_cache.read().await;
                if cache_guard.contains(&id) {
                    continue;
                }
            }

            // Send download request without waiting
            let (result_tx, _result_rx) = mpsc::channel(1);
            let request = DownloadRequest { id, url, result_tx };
            let _ = self.download_tx.send(request).await;
        }
    }
}
