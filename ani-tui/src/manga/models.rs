//! Manga domain models and provider traits.
//!
//! Ported from manga-tui's `backend::manga_provider`, pruned to the subset this
//! app needs: home, search, manga detail, and reader. The download subsystem,
//! the Feed page, and the provider-specific filter-widget machinery are not
//! ported (see `docs/superpowers/specs/2026-06-27-anime-manga-merge-design.md`).
//!
//! Traits keep `impl Future` return types (not dyn-safe) because the app holds
//! providers behind a concrete enum (`MangaProviderInstance`) rather than a
//! trait object — see `providers::mod`.
use std::cmp::Ordering;
use std::error::Error;
use std::fmt::Display;
use std::future::Future;
use std::io::Cursor;

use bytes::Bytes;
use chrono::NaiveDate;
use image::{DynamicImage, GenericImageView};
use image::io::Reader as ImageReader;
use reqwest::Url;
use serde::{Deserialize, Serialize};

/// Validated, trimmed, lowercased search string. `None` when empty.
#[derive(Debug, Clone)]
pub struct SearchTerm(String);

impl SearchTerm {
    pub fn trimmed_lowercased(search_term: &str) -> Option<Self> {
        let s = search_term.trim();
        if s.is_empty() { None } else { Some(Self(s.to_lowercase())) }
    }
}

impl Display for SearchTerm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A `Vec` guaranteed sorted + deduped.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SortedVec<T: std::fmt::Debug>(Vec<T>);

impl<T: std::fmt::Debug + PartialEq> SortedVec<T> {
    pub fn sorted_by<F>(mut vec: Vec<T>, by: F) -> Self
    where
        F: FnMut(&T, &T) -> Ordering,
    {
        vec.sort_by(by);
        vec.dedup();
        Self(vec)
    }

    pub fn as_slice(&self) -> &[T] {
        &self.0
    }
}

// ── enums ──────────────────────────────────────────────────────────────────

#[derive(Debug, Default, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum Rating {
    #[default]
    Normal,
    Doujinshi,
    Moderate,
    Nsfw,
}

#[derive(Debug, Default, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Genres {
    pub title: String,
    pub rating: Rating,
}

impl Genres {
    pub fn new(title: String, rating: Rating) -> Self {
        Self { title, rating }
    }
}

#[derive(Debug, Default, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum MangaStatus {
    #[default]
    Ongoing,
    Cancelled,
    Completed,
    Hiatus,
}

impl MangaStatus {
    pub fn as_label(self) -> &'static str {
        match self {
            MangaStatus::Ongoing => "🟢 ongoing",
            MangaStatus::Completed => "🔵 completed",
            MangaStatus::Hiatus => "🟡 hiatus",
            MangaStatus::Cancelled => "🟠 cancelled",
        }
    }
}

/// Languages mangadex translates into. Scrapers (weebcentral/mangapill) are
/// English-only, so they always produce `English`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Languages {
    French,
    #[default]
    English,
    Spanish,
    #[allow(non_camel_case_types)]
    SpanishLa,
    Italian,
    Japanese,
    Korean,
    BrazilianPortuguese,
    Portuguese,
    TraditionalChinese,
    SimplifiedChinese,
    Russian,
    German,
    Vietnamese,
    Unknown,
}

impl Languages {
    pub fn as_iso_code(self) -> &'static str {
        match self {
            Languages::French => "fr",
            Languages::English => "en",
            Languages::Spanish => "es",
            Languages::SpanishLa => "es-la",
            Languages::Italian => "it",
            Languages::Japanese => "ja",
            Languages::Korean => "ko",
            Languages::BrazilianPortuguese => "pt-br",
            Languages::Portuguese => "pt",
            Languages::TraditionalChinese => "zh-hk",
            Languages::SimplifiedChinese => "zh",
            Languages::Russian => "ru",
            Languages::German => "de",
            Languages::Vietnamese => "vi",
            Languages::Unknown => "",
        }
    }

    pub fn try_from_iso_code(code: &str) -> Option<Self> {
        Some(match code {
            "fr" => Languages::French,
            "en" => Languages::English,
            "es" => Languages::Spanish,
            "es-la" => Languages::SpanishLa,
            "it" => Languages::Italian,
            "ja" => Languages::Japanese,
            "ko" => Languages::Korean,
            "pt-br" => Languages::BrazilianPortuguese,
            "pt" => Languages::Portuguese,
            "zh-hk" => Languages::TraditionalChinese,
            "zh" => Languages::SimplifiedChinese,
            "ru" => Languages::Russian,
            "de" => Languages::German,
            "vi" => Languages::Vietnamese,
            _ => return None,
        })
    }
}

/// Image quality only matters for mangadex (`.256.jpg` vs `.512.jpg`, data vs
/// data-saver). Scrapers ignore it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageQuality {
    Low,
    #[default]
    High,
}

/// Which manga provider is active.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MangaProviders {
    #[default]
    Mangadex,
    Weebcentral,
    Mangapill,
}

impl MangaProviders {
    pub fn as_str(self) -> &'static str {
        match self {
            MangaProviders::Mangadex => "mangadex",
            MangaProviders::Weebcentral => "weebcentral",
            MangaProviders::Mangapill => "mangapill",
        }
    }

    pub fn try_from_str(s: &str) -> Option<Self> {
        Some(match s {
            "mangadex" => MangaProviders::Mangadex,
            "weebcentral" => MangaProviders::Weebcentral,
            "mangapill" => MangaProviders::Mangapill,
            _ => return None,
        })
    }
}

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Author {
    pub id: String,
    pub name: String,
}

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Artist {
    pub id: String,
    pub name: String,
}

// ── manga models ───────────────────────────────────────────────────────────

#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
pub struct Manga {
    pub id: String,
    pub id_safe_for_download: String,
    pub title: String,
    pub genres: Vec<Genres>,
    pub description: String,
    pub status: MangaStatus,
    pub cover_img_url: String,
    pub languages: Vec<Languages>,
    pub rating: String,
    pub artist: Option<Artist>,
    pub author: Option<Author>,
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct SearchManga {
    pub id: String,
    pub title: String,
    pub genres: Vec<Genres>,
    pub description: Option<String>,
    pub status: Option<MangaStatus>,
    pub cover_img_url: String,
    pub languages: Vec<Languages>,
    pub artist: Option<Author>,
    pub author: Option<Author>,
}

#[derive(Debug, Default, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct PopularManga {
    pub id: String,
    pub title: String,
    pub genres: Vec<Genres>,
    pub description: String,
    pub status: Option<MangaStatus>,
    pub cover_img_url: String,
}

#[derive(Debug, Default, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct RecentlyAddedManga {
    pub id: String,
    pub title: String,
    pub description: String,
    pub cover_img_url: String,
}

#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub id: String,
    pub id_safe_for_download: String,
    pub manga_id: String,
    pub title: String,
    pub language: Languages,
    pub chapter_number: String,
    pub volume_number: Option<String>,
    pub scanlator: Option<String>,
    pub publication_date: Option<NaiveDate>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct ChapterToRead {
    pub id: String,
    pub title: String,
    pub number: f64,
    pub volume_number: Option<String>,
    pub num_page_bookmarked: Option<u32>,
    pub language: Languages,
    pub pages_url: Vec<Url>,
}

impl Default for ChapterOrderBy {
    fn default() -> Self {
        ChapterOrderBy::Descending
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ChapterOrderBy {
    Ascending,
    Descending,
}

impl ChapterOrderBy {
    pub fn toggle(self) -> Self {
        match self {
            ChapterOrderBy::Ascending => ChapterOrderBy::Descending,
            ChapterOrderBy::Descending => ChapterOrderBy::Ascending,
        }
    }
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct ChapterFilters {
    pub order: ChapterOrderBy,
    pub language: Languages,
}

/// Simplified search filters (v1: language + sort only). Replaces the
/// provider-specific `FiltersHandler`/`FiltersWidget` assoc-type machinery
/// from manga-tui.
#[derive(Debug, Default, Clone)]
pub struct MangaSearchFilters {
    pub language: Languages,
    pub order: ChapterOrderBy,
}

// ── chapter volume tree (for next/prev chapter navigation) ─────────────────

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ChapterReader {
    pub id: String,
    pub number: String,
    pub volume: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SortedChapters(SortedVec<ChapterReader>);

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Volumes {
    pub volume: String,
    pub chapters: SortedChapters,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SortedVolumes(SortedVec<Volumes>);

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ListOfChapters {
    pub volumes: SortedVolumes,
}

impl Default for ChapterToRead {
    fn default() -> Self {
        Self {
            id: String::default(),
            number: 1.0,
            title: String::default(),
            volume_number: Some("1".to_string()),
            pages_url: vec![],
            language: Languages::default(),
            num_page_bookmarked: None,
        }
    }
}

impl SortedVolumes {
    pub fn new(volumes: Vec<Volumes>) -> Self {
        Self(SortedVec::sorted_by(volumes, |a, b| {
            if a.volume == "none" && b.volume.parse::<u32>().is_ok() {
                Ordering::Greater
            } else if a.volume.parse::<u32>().is_ok() && b.volume == "none" {
                Ordering::Less
            } else {
                a.volume.parse::<u32>().unwrap_or(0).cmp(&b.volume.parse().unwrap_or(0))
            }
        }))
    }

    pub fn search_next_volume(&self, volume: &str) -> Option<Volumes> {
        let v = self.as_slice();
        v.iter().position(|vol| vol.volume == volume).and_then(|i| v.get(i + 1).cloned())
    }

    pub fn search_previous_volume(&self, volume: &str) -> Option<Volumes> {
        let v = self.as_slice();
        v.iter().position(|vol| vol.volume == volume).and_then(|i| v.get(i.saturating_sub(1)).cloned())
    }

    pub fn as_slice(&self) -> &[Volumes] {
        self.0.as_slice()
    }
}

impl SortedChapters {
    pub fn new(chapters: Vec<ChapterReader>) -> Self {
        Self(SortedVec::sorted_by(chapters, |a, b| {
            a.number.parse::<f64>().unwrap_or(0.0).total_cmp(&b.number.parse::<f64>().unwrap_or(0.0))
        }))
    }

    pub fn search_next_chapter(&self, current: &str) -> Option<ChapterReader> {
        let c = self.as_slice();
        match c.iter().position(|chap| chap.number == current) {
            Some(i) => c.get(i + 1).cloned(),
            None => c.iter().next().cloned(),
        }
    }

    pub fn as_slice(&self) -> &[ChapterReader] {
        self.0.as_slice()
    }
}

impl ListOfChapters {
    pub fn get_next_chapter(&self, volume: Option<&str>, chapter_number: f64) -> Option<ChapterReader> {
        let vol_key = volume.unwrap_or("none");
        let volume = self.volumes.as_slice().iter().find(|v| v.volume == vol_key)?;
        match volume.chapters.search_next_chapter(&chapter_number.to_string()) {
            Some(c) => Some(c),
            None => {
                let next_volume = self.volumes.search_next_volume(vol_key)?;
                next_volume.chapters.search_next_chapter(&chapter_number.to_string())
            }
        }
    }

    pub fn get_previous_chapter_in_previous_volume(&self, volume: &str, chapter_number: f64) -> Option<ChapterReader> {
        let prev = self.volumes.search_previous_volume(volume).filter(|v| v.volume != volume)?;
        prev.chapters.as_slice().last().cloned().filter(|c| c.number != chapter_number.to_string())
    }

    pub fn get_previous_chapter(&self, volume: Option<&str>, chapter_number: f64) -> Option<ChapterReader> {
        let vol_key = volume.unwrap_or("none");
        let volumes = self.volumes.as_slice().iter().find(|v| v.volume == vol_key)?;
        let chapters = volumes.chapters.as_slice();
        let cur = chapters.iter().position(|c| c.number == chapter_number.to_string());
        match cur {
            Some(i) => {
                let prev = chapters.get(i.saturating_sub(1)).cloned().filter(|c| c.number != chapter_number.to_string());
                prev.or_else(|| self.get_previous_chapter_in_previous_volume(vol_key, chapter_number))
            }
            None => self.get_previous_chapter_in_previous_volume(vol_key, chapter_number),
        }
    }
}

// ── reader / page models ────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Clone, Default)]
pub struct MangaPanel {
    pub image_decoded: DynamicImage,
    pub dimensions: (u32, u32),
}

#[derive(Debug, PartialEq, Clone, Default)]
pub struct ChapterPage {
    pub bytes: Bytes,
    pub extension: String,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct ChapterPageUrl {
    pub url: Url,
    pub extension: String,
}

// ── pagination ──────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Pagination {
    pub current_page: u32,
    pub items_per_page: u32,
    pub total_items: u32,
}

impl Default for Pagination {
    fn default() -> Self {
        Self { current_page: 1, items_per_page: 16, total_items: 100 }
    }
}

impl Pagination {
    pub fn new(current_page: u32, total_items: u32, items_per_page: u32) -> Self {
        Self { current_page, items_per_page, total_items }
    }

    pub fn from_first_page(items_per_page: u32) -> Self {
        Self { current_page: 1, items_per_page, total_items: 300 }
    }

    pub fn go_next_page(&mut self) {
        if self.current_page * self.items_per_page < self.total_items {
            self.current_page += 1;
        }
    }

    pub fn go_previous_page(&mut self) {
        if self.current_page != 1 {
            self.current_page -= 1;
        }
    }

    pub fn can_go_next_page(&self) -> bool {
        self.current_page * self.items_per_page < self.total_items
    }

    pub fn can_go_previous_page(&self) -> bool {
        self.current_page != 1
    }

    /// Client-side pagination slice start index.
    pub fn index_to_slice_from(&self) -> usize {
        ((self.current_page - 1) * self.items_per_page) as usize
    }

    /// Client-side pagination slice end index (clamped to `total`).
    pub fn to_index(&self, total: usize) -> usize {
        ((self.current_page * self.items_per_page) as usize).min(total)
    }
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct GetChaptersResponse {
    pub chapters: Vec<Chapter>,
    pub total_chapters: u32,
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct GetMangasResponse {
    pub mangas: Vec<SearchManga>,
    pub total_mangas: u32,
    pub next_page: bool,
}

// ── provider traits ──────────────────────────────────────────────────────────

pub trait ProviderIdentity {
    fn name(&self) -> MangaProviders;
}

pub trait GetRawImage {
    fn get_raw_image(&self, url: &str) -> impl Future<Output = Result<Bytes, Box<dyn Error + Send + Sync>>> + Send;
}

pub trait DecodeBytesToImage: GetRawImage + Clone + Send + Sync {
    fn get_image(&self, cover_img_url: &str) -> impl Future<Output = Result<DynamicImage, Box<dyn Error + Send + Sync>>> + Send {
        Box::pin(async {
            let bytes = self.get_raw_image(cover_img_url).await?;
            let image = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?.decode()?;
            Ok(image)
        })
    }
}

pub trait SearchMangaPanel: DecodeBytesToImage + Send + Clone {
    fn search_manga_panel(&self, endpoint: Url) -> impl Future<Output = Result<MangaPanel, Box<dyn Error + Send + Sync>>> + Send {
        Box::pin(async move {
            let image_decoded = self.get_image(endpoint.as_str()).await?;
            let dimensions = image_decoded.dimensions();
            Ok(MangaPanel { image_decoded, dimensions })
        })
    }
}

pub trait SearchMangaById: Clone + Send + Sync {
    fn get_manga_by_id(&self, manga_id: &str) -> impl Future<Output = Result<Manga, Box<dyn Error + Send + Sync>>> + Send;
}

pub trait SearchChapterById: Send + Clone {
    fn search_chapter(&self, chapter_id: &str, manga_id: &str) -> impl Future<Output = Result<ChapterToRead, Box<dyn Error + Send + Sync>>> + Send;
}

pub trait GetChapterPages: GetRawImage + Send + Sync {
    fn get_chapter_pages_url_with_extension(
        &self,
        chapter_id: &str,
        manga_id: &str,
        image_quality: ImageQuality,
    ) -> impl Future<Output = Result<Vec<ChapterPageUrl>, Box<dyn Error + Send + Sync>>> + Send;
}

pub trait GoToReadChapter: Send + Clone + 'static + Sync {
    fn read_chapter(&self, chapter_id: &str, manga_id: &str)
        -> impl Future<Output = Result<(ChapterToRead, ListOfChapters), Box<dyn Error + Send + Sync>>> + Send;
}

/// Popular + recently-added mangas (home screen).
pub trait HomePageMangaProvider: ProviderIdentity + DecodeBytesToImage + SearchMangaById + Clone + Send + Sync + 'static {
    fn get_popular_mangas(&self) -> impl Future<Output = Result<Vec<PopularManga>, Box<dyn Error + Send + Sync>>> + Send;
    fn get_recently_added_mangas(&self) -> impl Future<Output = Result<Vec<RecentlyAddedManga>, Box<dyn Error + Send + Sync>>> + Send;
}

/// Paginated chapter list (manga detail screen).
pub trait MangaPageProvider: DecodeBytesToImage + GoToReadChapter + ProviderIdentity + GetChapterPages + Clone + Send + Sync {
    fn get_chapters(
        &self,
        manga_id: &str,
        filters: ChapterFilters,
        pagination: Pagination,
    ) -> impl Future<Output = Result<GetChaptersResponse, Box<dyn Error + Send + Sync>>> + Send;

    fn get_all_chapters(
        &self,
        manga_id: &str,
        language: Languages,
    ) -> impl Future<Output = Result<Vec<Chapter>, Box<dyn Error + Send + Sync>>> + Send;
}

/// Search with the simplified filter struct (no assoc types).
pub trait SearchPageProvider: DecodeBytesToImage + SearchMangaById + ProviderIdentity + Clone + Send + Sync + 'static {
    fn search_mangas(
        &self,
        search_term: Option<SearchTerm>,
        filters: MangaSearchFilters,
        pagination: Pagination,
    ) -> impl Future<Output = Result<GetMangasResponse, Box<dyn Error + Send + Sync>>> + Send;
}

/// Reader screen (fetch a single page image).
pub trait ReaderPageProvider: SearchMangaPanel + SearchChapterById + ProviderIdentity + Send + Sync + 'static {}

/// Compound marker. Note: `FeedPageProvider` is NOT included (out of scope).
pub trait MangaProvider: HomePageMangaProvider + MangaPageProvider + SearchPageProvider + ReaderPageProvider + Send + Sync {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pagination_next_page() {
        let mut p = Pagination::new(1, 10, 5);
        p.go_next_page();
        assert_eq!(p.current_page, 2);
        p.go_next_page(); // 2*5 == 10, not < 10
        assert_eq!(p.current_page, 2);
    }

    #[test]
    fn list_of_chapters_next_and_prev() {
        let list = ListOfChapters {
            volumes: SortedVolumes::new(vec![
                Volumes {
                    volume: "1".to_string(),
                    chapters: SortedChapters::new(vec![
                        ChapterReader { number: "1".to_string(), ..Default::default() },
                        ChapterReader { number: "2".to_string(), ..Default::default() },
                    ]),
                },
                Volumes {
                    volume: "2".to_string(),
                    chapters: SortedChapters::new(vec![ChapterReader {
                        number: "3".to_string(),
                        ..Default::default()
                    }]),
                },
            ]),
        };

        let next = list.get_next_chapter(Some("1"), 2.0).unwrap();
        assert_eq!(next.number, "3");

        let prev = list.get_previous_chapter(Some("2"), 3.0).unwrap();
        assert_eq!(prev.number, "2");

        assert!(list.get_next_chapter(Some("2"), 3.0).is_none());
        assert!(list.get_previous_chapter(Some("1"), 1.0).is_none());
    }
}
