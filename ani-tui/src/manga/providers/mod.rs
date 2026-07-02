//! Concrete manga providers + a dispatching enum.
//!
//! The app holds `MangaProviderInstance` instead of `Box<dyn MangaProvider>`
//! because the provider traits use `impl Future` returns (not dyn-safe). The
//! enum matches and forwards to the concrete impl for every provider.
pub mod mangadex;
pub mod mangapill;
pub mod weebcentral;

use std::error::Error;
use std::sync::Arc;

use mangadex::MangadexClient;
use mangapill::MangaPillProvider;
use weebcentral::WeebcentralProvider;

use super::cache::{Cacher, InMemoryCache};
use super::models::*;

/// All three providers behind one enum, dispatched manually.
#[derive(Clone)]
pub enum MangaProviderInstance {
    Mangadex(MangadexClient),
    Weebcentral(WeebcentralProvider),
    Mangapill(MangaPillProvider),
}

impl MangaProviderInstance {
    /// Build the default instance for a provider kind, sharing one cache.
    pub fn new(provider: MangaProviders, cache: Arc<dyn Cacher>, image_quality: ImageQuality) -> Self {
        match provider {
            MangaProviders::Mangadex => Self::Mangadex(MangadexClient::new(cache).with_image_quality(image_quality)),
            MangaProviders::Weebcentral => Self::Weebcentral(WeebcentralProvider::new(cache)),
            MangaProviders::Mangapill => Self::Mangapill(MangaPillProvider::new(cache)),
        }
    }

    pub fn name(&self) -> MangaProviders {
        match self {
            Self::Mangadex(p) => p.name(),
            Self::Weebcentral(p) => p.name(),
            Self::Mangapill(p) => p.name(),
        }
    }

    pub async fn get_popular_mangas(&self) -> Result<Vec<PopularManga>, Box<dyn Error + Send + Sync>> {
        Ok(match self {
            Self::Mangadex(p) => p.get_popular_mangas().await?,
            Self::Weebcentral(p) => p.get_popular_mangas().await?,
            Self::Mangapill(p) => p.get_popular_mangas().await?,
        })
    }

    pub async fn get_recently_added_mangas(&self) -> Result<Vec<RecentlyAddedManga>, Box<dyn Error + Send + Sync>> {
        Ok(match self {
            Self::Mangadex(p) => p.get_recently_added_mangas().await?,
            Self::Weebcentral(p) => p.get_recently_added_mangas().await?,
            Self::Mangapill(p) => p.get_recently_added_mangas().await?,
        })
    }

    pub async fn get_manga_by_id(&self, manga_id: &str) -> Result<Manga, Box<dyn Error + Send + Sync>> {
        Ok(match self {
            Self::Mangadex(p) => p.get_manga_by_id(manga_id).await?,
            Self::Weebcentral(p) => p.get_manga_by_id(manga_id).await?,
            Self::Mangapill(p) => p.get_manga_by_id(manga_id).await?,
        })
    }

    pub async fn search_mangas(
        &self,
        search_term: Option<SearchTerm>,
        filters: MangaSearchFilters,
        pagination: Pagination,
    ) -> Result<GetMangasResponse, Box<dyn Error + Send + Sync>> {
        Ok(match self {
            Self::Mangadex(p) => p.search_mangas(search_term, filters, pagination).await?,
            Self::Weebcentral(p) => p.search_mangas(search_term, filters, pagination).await?,
            Self::Mangapill(p) => p.search_mangas(search_term, filters, pagination).await?,
        })
    }

    pub async fn get_chapters(
        &self,
        manga_id: &str,
        filters: ChapterFilters,
        pagination: Pagination,
    ) -> Result<GetChaptersResponse, Box<dyn Error + Send + Sync>> {
        Ok(match self {
            Self::Mangadex(p) => p.get_chapters(manga_id, filters, pagination).await?,
            Self::Weebcentral(p) => p.get_chapters(manga_id, filters, pagination).await?,
            Self::Mangapill(p) => p.get_chapters(manga_id, filters, pagination).await?,
        })
    }

    pub async fn read_chapter(
        &self,
        chapter_id: &str,
        manga_id: &str,
    ) -> Result<(ChapterToRead, ListOfChapters), Box<dyn Error + Send + Sync>> {
        Ok(match self {
            Self::Mangadex(p) => p.read_chapter(chapter_id, manga_id).await?,
            Self::Weebcentral(p) => p.read_chapter(chapter_id, manga_id).await?,
            Self::Mangapill(p) => p.read_chapter(chapter_id, manga_id).await?,
        })
    }

    /// Fetch a single reader page image (decoded). Used by the manga reader
    /// screen; reuses ani-tui's image decode path.
    pub async fn get_page_image(&self, url: &str) -> Result<image::DynamicImage, Box<dyn Error + Send + Sync>> {
        let img = match self {
            Self::Mangadex(p) => p.get_image(url).await?,
            Self::Weebcentral(p) => p.get_image(url).await?,
            Self::Mangapill(p) => p.get_image(url).await?,
        };
        Ok(img)
    }

    /// Fetch a page image and encode it as PNG bytes, ready for the image
    /// renderer. PNG is chosen because ani-tui's renderer supports it across all
    /// protocols and the source images are already decoded.
    pub async fn get_page_image_bytes(&self, url: &str) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
        let img = self.get_page_image(url).await?;
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png)?;
        Ok(buf.into_inner())
    }

    pub async fn search_chapter(
        &self,
        chapter_id: &str,
        manga_id: &str,
    ) -> Result<ChapterToRead, Box<dyn Error + Send + Sync>> {
        Ok(match self {
            Self::Mangadex(p) => p.search_chapter(chapter_id, manga_id).await?,
            Self::Weebcentral(p) => p.search_chapter(chapter_id, manga_id).await?,
            Self::Mangapill(p) => p.search_chapter(chapter_id, manga_id).await?,
        })
    }
}

/// Shared default cache for all providers (spawned cleanup thread, capacity 500).
pub fn default_cache() -> Arc<dyn Cacher> {
    InMemoryCache::init(500)
}
