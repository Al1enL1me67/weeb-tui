//! Mangadex provider — REST API client (`https://api.mangadex.org`).
//!
//! Ported from manga-tui's `backend::manga_provider::mangadex`, keeping the
//! endpoint logic and JSON response types. The provider-specific filter widget
//! subsystem is dropped: search uses only language + sort (see design doc §4.3).
//! Adapted to reqwest 0.11 (ani-tui's pinned version).
use std::error::Error;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use bytes::Bytes;
use chrono::Months;
use reqwest::{Client, Response, StatusCode, Url};
use serde::{Deserialize, Serialize};

use crate::manga::models::{
    Artist, Author, Chapter, ChapterFilters, ChapterOrderBy, ChapterPageUrl, ChapterReader, ChapterToRead,
    DecodeBytesToImage, Genres, GetChapterPages, GetChaptersResponse, GetMangasResponse,
    GoToReadChapter, HomePageMangaProvider, ImageQuality, Languages, ListOfChapters, Manga, MangaPageProvider,
    MangaProvider, MangaProviders, MangaSearchFilters, MangaStatus, PopularManga, ProviderIdentity, Rating,
    ReaderPageProvider, RecentlyAddedManga, SearchChapterById, SearchMangaById, SearchMangaPanel, SearchPageProvider,
    SearchTerm, SortedChapters, SortedVolumes, Volumes,
};
use crate::manga::cache::{CacheDuration, Cacher, InsertEntry};

pub const API_URL_BASE: &str = "https://api.mangadex.org";
pub const COVER_IMG_URL_BASE: &str = "https://uploads.mangadex.org/covers";

const APP_USER_AGENT: &str = concat!("ani-tui/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Debug)]
pub struct MangadexClient {
    client: Client,
    cache_provider: Arc<dyn Cacher>,
    api_url_base: Url,
    cover_img_url_base: Url,
    image_quality: ImageQuality,
}

impl MangadexClient {
    fn make_cover_img_url_lower_quality(&self, manga_id: &str, file_name: &str) -> String {
        format!("{}/{manga_id}/{file_name}.256.jpg", self.cover_img_url_base)
    }

    fn make_cover_img_url(&self, manga_id: &str, file_name: &str) -> String {
        format!("{}/{manga_id}/{file_name}.512.jpg", self.cover_img_url_base)
    }

    pub fn new(cache_provider: Arc<dyn Cacher>) -> Self {
        Self::with_urls(API_URL_BASE.parse().unwrap(), COVER_IMG_URL_BASE.parse().unwrap(), cache_provider)
    }

    pub fn with_urls(api_url_base: Url, cover_img_url_base: Url, cache_provider: Arc<dyn Cacher>) -> Self {
        let client = Client::builder()
            .timeout(StdDuration::from_secs(10))
            .user_agent(APP_USER_AGENT)
            .build()
            .unwrap();
        Self { client, cache_provider, api_url_base, cover_img_url_base, image_quality: ImageQuality::default() }
    }

    pub fn with_image_quality(mut self, image_quality: ImageQuality) -> Self {
        self.image_quality = image_quality;
        self
    }

    pub async fn search_list_of_chapters(
        &self,
        manga_id: &str,
        language: Languages,
    ) -> Result<AggregateChapterResponse, Box<dyn Error + Send + Sync>> {
        let endpoint =
            format!("{}/manga/{}/aggregate?translatedLanguage[]={}", self.api_url_base, manga_id, language.as_iso_code());
        if let Some(cached) = self.cache_provider.get(&endpoint)? {
            return Ok(serde_json::from_slice(&cached.data)?);
        }
        let response: AggregateChapterResponse = self.client.get(&endpoint).send().await?.json().await?;
        self.cache_provider
            .cache(InsertEntry { id: &endpoint, data: &serde_json::to_vec(&response)?, duration: CacheDuration::Long })
            .ok();
        Ok(response)
    }

    async fn get_manga_statistics(&self, id_manga: &str) -> Result<MangaStatisticsResponse, reqwest::Error> {
        let endpoint = format!("{}/statistics/manga/{id_manga}", self.api_url_base);
        self.client.get(endpoint).timeout(StdDuration::from_secs(1)).send().await?.json().await
    }

    pub async fn check_status(&self) -> Result<Response, reqwest::Error> {
        self.client.get(format!("{}/ping", self.api_url_base)).send().await
    }

    fn map_popular_mangas(&self) -> impl Fn(Data) -> PopularManga + '_ {
        move |manga| {
            let mut cover_img_url = String::new();
            for rel in &manga.relationships {
                if let Some(attrs) = &rel.attributes {
                    if rel.type_field.as_str() == "cover_art" {
                        let file = attrs.file_name.as_ref().unwrap().to_string();
                        cover_img_url = self.make_cover_img_url(&manga.id, &file);
                    }
                }
            }
            let status = status_from_str(&manga.attributes.status);
            let mut genres: Vec<Genres> = manga.attributes.tags.into_iter().map(Genres::from).collect();
            let rating = content_rating(&manga.attributes.content_rating);
            genres.push(Genres::new(manga.attributes.content_rating, rating));
            if let Some(pb) = manga.attributes.publication_demographic {
                genres.push(Genres::new(pb, Rating::Normal));
            }
            PopularManga {
                id: manga.id,
                description: manga.attributes.description.and_then(|d| d.en).unwrap_or_default(),
                genres,
                title: manga.attributes.title.into(),
                status: Some(status),
                cover_img_url,
            }
        }
    }

    async fn map_manga_found_by_id(&self, manga: GetMangaByIdResponse) -> Manga {
        let id = manga.data.id.clone();
        let title: String = manga.data.attributes.title.into();
        let mut genres: Vec<Genres> = manga.data.attributes.tags.into_iter().map(Genres::from).collect();
        let rating = content_rating(&manga.data.attributes.content_rating);
        genres.push(Genres::new(manga.data.attributes.content_rating.clone(), rating));
        if let Some(pb) = manga.data.attributes.publication_demograpchic {
            genres.push(Genres::new(pb, Rating::Normal));
        }
        let description = manga.data.attributes.description.and_then(|d| d.en).unwrap_or_default();
        let status = status_from_str(&manga.data.attributes.status);

        let mut cover_img_url = String::new();
        let mut author = None;
        let mut artist = None;
        for rel in &manga.data.relationships {
            if let Some(attrs) = &rel.attributes {
                match rel.type_field.as_str() {
                    "cover_art" => {
                        let file = attrs.file_name.as_ref().unwrap().to_string();
                        cover_img_url = self.make_cover_img_url_lower_quality(&id, &file);
                    }
                    "author" => {
                        let name = attrs.name.clone().unwrap_or_default();
                        author = Some(Author { id: rel.id.clone(), name });
                    }
                    "artist" => {
                        let name = attrs.name.clone().unwrap_or_default();
                        artist = Some(Artist { id: rel.id.clone(), name });
                    }
                    _ => {}
                }
            }
        }

        let languages: Vec<Languages> = manga
            .data
            .attributes
            .available_translated_languages
            .into_iter()
            .flatten()
            .flat_map(|l| Languages::try_from_iso_code(&l))
            .collect();

        let avg = self.get_manga_statistics(&id).await.ok().and_then(|r| {
            r.statistics.get(&id).and_then(|s| s.rating.average.map(|v| v.ceil()))
        }).map(|v| format!("{v} out of 10")).unwrap_or_default();

        Manga {
            rating: avg,
            id: id.clone(),
            id_safe_for_download: id,
            title,
            genres,
            description,
            status,
            cover_img_url,
            languages,
            artist,
            author,
        }
    }

    fn map_response_to_pages_url(&self, response: ChapterPagesResponse, image_quality: ImageQuality) -> Vec<ChapterPageUrl> {
        let base = response.get_image_url_endpoint(image_quality);
        let files = match image_quality {
            ImageQuality::Low => response.chapter.data_saver,
            ImageQuality::High => response.chapter.data,
        };
        files
            .into_iter()
            .map(|f| {
                let url = Url::parse(&format!("{base}/{f}")).unwrap();
                let extension = std::path::Path::new(&f).extension().unwrap().to_str().unwrap().to_string();
                ChapterPageUrl { url, extension }
            })
            .collect()
    }

    fn map_chapter_response(&self, manga_id: String) -> impl Fn(ChapterData) -> Chapter + '_ {
        move |chapter| {
            let id = chapter.id.clone();
            let scanlator = chapter
                .relationships
                .iter()
                .find(|rel| rel.type_field == "scanlation_group")
                .and_then(|rel| rel.attributes.as_ref().map(|a| a.name.clone()));
            Chapter {
                id: id.clone(),
                id_safe_for_download: id,
                manga_id: manga_id.clone(),
                title: chapter.attributes.title.unwrap_or("No title".to_string()),
                language: Languages::try_from_iso_code(&chapter.attributes.translated_language).unwrap_or_default(),
                chapter_number: chapter.attributes.chapter.unwrap_or("0".to_string()),
                volume_number: chapter.attributes.volume,
                scanlator,
                publication_date: chrono::DateTime::parse_from_rfc3339(&chapter.attributes.readable_at)
                    .ok()
                    .map(|d| d.date_naive()),
            }
        }
    }
}

fn status_from_str(s: &str) -> MangaStatus {
    match s {
        "ongoing" => MangaStatus::Ongoing,
        "hiatus" => MangaStatus::Hiatus,
        "completed" => MangaStatus::Completed,
        "cancelled" => MangaStatus::Cancelled,
        _ => MangaStatus::default(),
    }
}

fn content_rating(s: &str) -> Rating {
    match s {
        "suggestive" => Rating::Moderate,
        "pornographic" | "erotica" => Rating::Nsfw,
        _ => Rating::Normal,
    }
}

impl crate::manga::models::GetRawImage for MangadexClient {
    async fn get_raw_image(&self, url: &str) -> Result<Bytes, Box<dyn Error + Send + Sync>> {
        if let Some(cached) = self.cache_provider.get(url)? {
            return Ok(cached.data.into());
        }
        let response = self.client.get(url).timeout(StdDuration::from_secs(10)).send().await?;
        if response.status() != StatusCode::OK {
            return Err(format!("Could not get image on mangadex with url: {url}").into());
        }
        let bytes = response.bytes().await?;
        self.cache_provider
            .cache(InsertEntry { id: url, data: &bytes, duration: CacheDuration::LongLong })
            .ok();
        Ok(bytes)
    }
}

impl DecodeBytesToImage for MangadexClient {}
impl SearchMangaPanel for MangadexClient {}

impl HomePageMangaProvider for MangadexClient {
    async fn get_recently_added_mangas(&self) -> Result<Vec<RecentlyAddedManga>, Box<dyn Error + Send + Sync>> {
        let lang = Languages::English.as_iso_code();
        let endpoint = format!("{}/manga", self.api_url_base);
        let response: SearchMangaResponse = self
            .client
            .get(endpoint)
            .query(&[
                ("limit", "5"),
                ("contentRating[]", "safe"),
                ("order[createdAt]", "desc"),
                ("includes[]", "cover_art"),
                ("includes[]", "artist"),
                ("includes[]", "author"),
                ("hasAvailableChapters", "true"),
                ("availableTranslatedLanguage[]", lang),
            ])
            .send()
            .await?
            .json()
            .await?;

        Ok(response
            .data
            .into_iter()
            .map(|manga| {
                let mut cover = String::new();
                for rel in &manga.relationships {
                    if let Some(attrs) = &rel.attributes {
                        if rel.type_field.as_str() == "cover_art" {
                            let file = attrs.file_name.as_ref().unwrap().to_string();
                            cover = self.make_cover_img_url_lower_quality(&manga.id, &file);
                        }
                    }
                }
                RecentlyAddedManga {
                    id: manga.id,
                    title: manga.attributes.title.into(),
                    description: manga
                        .attributes
                        .description
                        .and_then(|d| d.en)
                        .unwrap_or_else(|| "No description".to_string()),
                    cover_img_url: cover,
                }
            })
            .collect())
    }

    async fn get_popular_mangas(&self) -> Result<Vec<PopularManga>, Box<dyn Error + Send + Sync>> {
        let month_ago = chrono::offset::Local::now().date_naive().checked_sub_months(Months::new(1)).unwrap();
        let lang = Languages::English.as_iso_code();
        let endpoint = format!("{}/manga", self.api_url_base);
        let from_date = format!("{month_ago}T00:00:00");
        let cache_id = format!("{endpoint}{from_date}-popular_manga");

        if let Some(cached) = self.cache_provider.get(&cache_id)? {
            let resp: SearchMangaResponse = serde_json::from_slice(&cached.data)?;
            return Ok(resp.data.into_iter().map(self.map_popular_mangas()).collect());
        }

        let response: SearchMangaResponse = self
            .client
            .get(endpoint)
            .query(&[
                ("includes[]", "cover_art"),
                ("includes[]", "artist"),
                ("includes[]", "author"),
                ("order[followedCount]", "desc"),
                ("contentRating[]", "safe"),
                ("hasAvailableChapters", "true"),
                ("availableTranslatedLanguage[]", lang),
                ("createdAtSince", from_date.as_str()),
            ])
            .send()
            .await?
            .json()
            .await?;

        let popular: Vec<PopularManga> = response.data.into_iter().map(self.map_popular_mangas()).collect();
        self.cache_provider
            .cache(InsertEntry { id: &cache_id, data: &serde_json::to_vec(&popular)?, duration: CacheDuration::Short })
            .ok();
        Ok(popular)
    }
}

impl SearchMangaById for MangadexClient {
    async fn get_manga_by_id(&self, manga_id: &str) -> Result<Manga, Box<dyn Error + Send + Sync>> {
        let endpoint = format!("{}/manga/{manga_id}", self.api_url_base);
        let cache_id = format!("{endpoint}-manga-by-id");
        if let Some(cached) = self.cache_provider.get(&cache_id)? {
            return Ok(serde_json::from_slice(&cached.data)?);
        }
        let resp: GetMangaByIdResponse = self
            .client
            .get(endpoint)
            .query(&[("includes[]", "cover_art"), ("includes[]", "author"), ("includes[]", "artist")])
            .send()
            .await?
            .json()
            .await?;
        let manga = self.map_manga_found_by_id(resp).await;
        self.cache_provider
            .cache(InsertEntry { id: &cache_id, data: &serde_json::to_vec(&manga)?, duration: CacheDuration::LongLong })
            .ok();
        Ok(manga)
    }
}

impl GetChapterPages for MangadexClient {
    async fn get_chapter_pages_url_with_extension(
        &self,
        chapter_id: &str,
        _manga_id: &str,
        _image_quality: ImageQuality,
    ) -> Result<Vec<ChapterPageUrl>, Box<dyn Error + Send + Sync>> {
        let endpoint = format!("{}/at-home/server/{chapter_id}", self.api_url_base);
        if let Some(cached) = self.cache_provider.get(&endpoint)? {
            let resp: ChapterPagesResponse = serde_json::from_slice(&cached.data)?;
            return Ok(self.map_response_to_pages_url(resp, self.image_quality));
        }
        let resp: ChapterPagesResponse = self.client.get(&endpoint).send().await?.json().await?;
        self.cache_provider
            .cache(InsertEntry { id: &endpoint, data: &serde_json::to_vec(&resp)?, duration: CacheDuration::Long })
            .ok();
        Ok(self.map_response_to_pages_url(resp, self.image_quality))
    }
}

impl GoToReadChapter for MangadexClient {
    async fn read_chapter(&self, chapter_id: &str, manga_id: &str) -> Result<(ChapterToRead, ListOfChapters), Box<dyn Error + Send + Sync>> {
        let chapter = self.search_chapter(chapter_id, manga_id).await?;
        let list = self.search_list_of_chapters(manga_id, chapter.language).await?;
        Ok((chapter, list.into()))
    }
}

impl SearchChapterById for MangadexClient {
    async fn search_chapter(&self, chapter_id: &str, _manga_id: &str) -> Result<ChapterToRead, Box<dyn Error + Send + Sync>> {
        let endpoint = format!("{}/chapter/{chapter_id}", self.api_url_base);
        let cached_path = endpoint.clone();
        let response: OneChapterResponse = if let Some(cached) = self.cache_provider.get(&cached_path)? {
            serde_json::from_slice(&cached.data)?
        } else {
            let r = self.client.get(&endpoint).send().await?;
            if r.status() != StatusCode::OK {
                return Err(format!("Could not get chapter of id {chapter_id} on mangadex").into());
            }
            let resp: OneChapterResponse = r.json().await?;
            self.cache_provider
                .cache(InsertEntry { id: &cached_path, data: &serde_json::to_vec(&resp)?, duration: CacheDuration::Long })
                .ok();
            resp
        };

        let pages_url: Vec<Url> = self
            .get_chapter_pages_url_with_extension(chapter_id, "", self.image_quality)
            .await?
            .into_iter()
            .map(|p| p.url)
            .collect();

        let language = Languages::try_from_iso_code(&response.data.attributes.translated_language).unwrap_or_default();
        Ok(ChapterToRead {
            id: response.data.id,
            title: response.data.attributes.title.unwrap_or("No title".to_string()),
            number: response.data.attributes.chapter.and_then(|n| n.parse().ok()).unwrap_or_default(),
            volume_number: response.data.attributes.volume,
            num_page_bookmarked: None,
            language,
            pages_url,
        })
    }
}

impl MangaPageProvider for MangadexClient {
    async fn get_chapters(
        &self,
        manga_id: &str,
        filters: ChapterFilters,
        pagination: crate::manga::models::Pagination,
    ) -> Result<GetChaptersResponse, Box<dyn Error + Send + Sync>> {
        let endpoint = format!("{}/manga/{manga_id}/feed", self.api_url_base);
        let offset = (pagination.current_page.saturating_sub(1)) * pagination.items_per_page;
        let order = match filters.order {
            ChapterOrderBy::Ascending => "asc",
            ChapterOrderBy::Descending => "desc",
        };
        let cache_id = format!("{endpoint}{offset}{order}get-chapters-cache");

        let build_response = |resp: ChapterResponse| {
            let total = resp.total;
            let chapters: Vec<Chapter> = resp.data.into_iter().map(self.map_chapter_response(manga_id.to_string())).collect();
            GetChaptersResponse { chapters, total_chapters: total as u32 }
        };

        if let Some(cached) = self.cache_provider.get(&cache_id)? {
            let resp: ChapterResponse = serde_json::from_slice(&cached.data)?;
            return Ok(build_response(resp));
        }

        let resp: ChapterResponse = self
            .client
            .get(endpoint)
            .query(&[
                ("includes[]", "scanlation_group"),
                ("limit", &pagination.items_per_page.to_string()),
                ("offset", &offset.to_string()),
                ("order", order),
                ("contentRating[]", "safe"),
                ("contentRating[]", "suggestive"),
                ("contentRating[]", "erotica"),
                ("contentRating[]", "pornographic"),
                ("order[volume]", order),
                ("order[chapter]", order),
                ("includeExternalUrl", "0"),
                ("translatedLanguage[]", filters.language.as_iso_code()),
            ])
            .send()
            .await?
            .json()
            .await?;

        self.cache_provider
            .cache(InsertEntry { id: &cache_id, data: &serde_json::to_vec(&resp)?, duration: CacheDuration::Long })
            .ok();
        Ok(build_response(resp))
    }

    async fn get_all_chapters(&self, manga_id: &str, language: Languages) -> Result<Vec<Chapter>, Box<dyn Error + Send + Sync>> {
        let lang = language.as_iso_code();
        let endpoint = format!("{}/manga/{manga_id}/feed", self.api_url_base);
        let cache_id = format!("{endpoint}get-all-chapters");
        if let Some(cached) = self.cache_provider.get(&cache_id)? {
            let resp: ChapterResponse = serde_json::from_slice(&cached.data)?;
            return Ok(resp.data.into_iter().map(self.map_chapter_response(manga_id.to_string())).collect());
        }
        let resp: ChapterResponse = self
            .client
            .get(endpoint)
            .query(&[
                ("limit", "500"),
                ("offset", "0"),
                ("order[volume]", "asc"),
                ("order[chapter]", "asc"),
                ("translatedLanguage[]", lang),
                ("includes[]", "scanlation_group"),
                ("includeExternalUrl", "0"),
                ("contentRating[]", "safe"),
                ("contentRating[]", "suggestive"),
                ("contentRating[]", "erotica"),
                ("contentRating[]", "pornographic"),
            ])
            .timeout(StdDuration::from_secs(10))
            .send()
            .await?
            .json()
            .await?;
        self.cache_provider
            .cache(InsertEntry { id: &cache_id, data: &serde_json::to_vec(&resp)?, duration: CacheDuration::Long })
            .ok();
        Ok(resp.data.into_iter().map(self.map_chapter_response(manga_id.to_string())).collect())
    }
}

impl ReaderPageProvider for MangadexClient {}

impl SearchPageProvider for MangadexClient {
    async fn search_mangas(
        &self,
        search_term: Option<SearchTerm>,
        filters: MangaSearchFilters,
        pagination: crate::manga::models::Pagination,
    ) -> Result<GetMangasResponse, Box<dyn Error + Send + Sync>> {
        let offset = (pagination.current_page - 1) * pagination.items_per_page;
        let search_by_title = match search_term {
            Some(s) => format!("title={s}"),
            None => String::new(),
        };
        let order = match filters.order {
            ChapterOrderBy::Ascending => "asc",
            ChapterOrderBy::Descending => "desc",
        };
        let items_per_page = pagination.items_per_page;
        let url = format!(
            "{}/manga?{search_by_title}&includes[]=cover_art&includes[]=author&includes[]=artist&limit={items_per_page}&offset={offset}&includedTagsMode=AND&excludedTagsMode=OR&hasAvailableChapters=true&order[followedCount]={order}&availableTranslatedLanguage[]={}",
            self.api_url_base,
            filters.language.as_iso_code()
        );
        let response: SearchMangaResponse = self.client.get(url).send().await?.json().await?;
        let total = response.total;
        let mangas: Vec<crate::manga::models::SearchManga> = response
            .data
            .into_iter()
            .map(|manga| {
                let id = manga.id.clone();
                let title: String = manga.attributes.title.into();
                let mut genres: Vec<Genres> = manga.attributes.tags.into_iter().map(Genres::from).collect();
                let rating = content_rating(&manga.attributes.content_rating);
                genres.push(Genres::new(manga.attributes.content_rating, rating));
                let description = manga.attributes.description.and_then(|d| d.en).unwrap_or_default();
                let status = status_from_str(&manga.attributes.status);

                let mut cover = String::new();
                let mut author = None;
                let mut artist = None;
                for rel in &manga.relationships {
                    if let Some(attrs) = &rel.attributes {
                        match rel.type_field.as_str() {
                            "cover_art" => {
                                let file = attrs.file_name.as_ref().unwrap().to_string();
                                cover = self.make_cover_img_url_lower_quality(&id, &file);
                            }
                            "author" => author = Some(Author { id: rel.id.clone(), name: attrs.name.clone().unwrap_or_default() }),
                            "artist" => artist = Some(Author { id: rel.id.clone(), name: attrs.name.clone().unwrap_or_default() }),
                            _ => {}
                        }
                    }
                }
                let languages = manga
                    .attributes
                    .available_translated_languages
                    .into_iter()
                    .flatten()
                    .flat_map(|l| Languages::try_from_iso_code(&l))
                    .collect();
                crate::manga::models::SearchManga {
                    id,
                    title,
                    genres,
                    description: Some(description),
                    artist,
                    author,
                    cover_img_url: cover,
                    languages,
                    status: Some(status),
                }
            })
            .collect();

        Ok(GetMangasResponse {
            next_page: !mangas.is_empty(),
            mangas,
            total_mangas: total,
        })
    }
}

impl ProviderIdentity for MangadexClient {
    fn name(&self) -> MangaProviders {
        MangaProviders::Mangadex
    }
}

impl MangaProvider for MangadexClient {}

// ── JSON response types (ported from manga-tui api_responses.rs) ─────────────

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchMangaResponse {
    pub result: String,
    pub response: String,
    pub data: Vec<Data>,
    pub limit: i32,
    pub offset: u32,
    pub total: u32,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Data {
    pub id: String,
    pub attributes: Attributes,
    pub relationships: Vec<MangaSearchRelationship>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attributes {
    pub title: Title,
    pub description: Option<Description>,
    pub status: String,
    pub tags: Vec<Tag>,
    pub content_rating: String,
    pub state: String,
    pub created_at: String,
    pub publication_demographic: Option<String>,
    pub available_translated_languages: Vec<Option<String>>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Title {
    pub en: Option<String>,
    pub ja: Option<String>,
    #[serde(rename = "ja-ro")]
    pub ja_ro: Option<String>,
    pub jp: Option<String>,
    pub es: Option<String>,
    #[serde(rename = "es-la")]
    pub es_la: Option<String>,
    pub zh: Option<String>,
    pub pt: Option<String>,
    #[serde(rename = "pt-br")]
    pub pt_br: Option<String>,
    pub ko: Option<String>,
    #[serde(rename = "zh-ro")]
    pub zh_ro: Option<String>,
    #[serde(rename = "ko-ro")]
    pub ko_ro: Option<String>,
}

impl From<Title> for String {
    fn from(v: Title) -> Self {
        v.en.or(v.ja_ro).or(v.ja).or(v.jp).or(v.ko_ro).or(v.ko).or(v.zh_ro).or(v.zh).or(v.pt_br).or(v.pt).or(v.es).or(v.es_la)
            .unwrap_or_else(|| "No title".to_string())
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Description {
    pub en: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MangaSearchRelationship {
    pub id: String,
    #[serde(rename = "type")]
    pub type_field: String,
    pub attributes: Option<MangaSearchAttributes>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MangaSearchAttributes {
    #[serde(rename = "fileName")]
    pub file_name: Option<String>,
    pub name: Option<String>,
    pub locale: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: String,
    pub attributes: TagAttributes,
}

impl From<Tag> for Genres {
    fn from(value: Tag) -> Self {
        let rating = match value.attributes.name.en.to_lowercase().as_str() {
            "sexual violence" | "gore" => Rating::Nsfw,
            "doujinshi" => Rating::Doujinshi,
            _ => Rating::Normal,
        };
        Genres::new(value.attributes.name.en, rating)
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagAttributes {
    pub name: TagName,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagName {
    pub en: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterResponse {
    pub data: Vec<ChapterData>,
    pub limit: i64,
    pub offset: i64,
    pub total: i64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterData {
    pub id: String,
    pub attributes: ChapterAttribute,
    pub relationships: Vec<Relationship>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterAttribute {
    pub volume: Option<String>,
    pub chapter: Option<String>,
    pub title: Option<String>,
    pub translated_language: String,
    pub readable_at: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relationship {
    pub id: String,
    #[serde(rename = "type")]
    pub type_field: String,
    pub attributes: Option<ChapterRelationshipAttribute>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterRelationshipAttribute {
    pub name: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterPagesResponse {
    pub result: String,
    pub base_url: String,
    pub chapter: ChapterPages,
}

impl ChapterPagesResponse {
    pub fn get_image_url_endpoint(&self, quality: ImageQuality) -> String {
        let q = match quality {
            ImageQuality::Low => "data-saver",
            ImageQuality::High => "data",
        };
        format!("{}/{}/{}", self.base_url, q, self.chapter.hash)
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterPages {
    pub hash: String,
    pub data: Vec<String>,
    pub data_saver: Vec<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MangaStatisticsResponse {
    pub result: String,
    pub statistics: std::collections::HashMap<String, Statistics>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Statistics {
    pub rating: StatsRating,
    pub follows: Option<u64>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsRating {
    pub average: Option<f64>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateChapterResponse {
    pub result: String,
    pub volumes: std::collections::HashMap<String, VolumesMangadex>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumesMangadex {
    pub volume: String,
    pub count: i32,
    #[serde(deserialize_with = "deserialize_aggregate_chapters")]
    pub chapters: std::collections::HashMap<String, Chapters>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chapters {
    pub chapter: String,
    pub id: String,
    pub others: Vec<String>,
    pub count: i32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum VecOrHashMap {
    Hash(std::collections::HashMap<String, Chapters>),
    Vec(Vec<Chapters>),
}

/// Volume 0 may serialize `chapters` as an array instead of a map.
fn deserialize_aggregate_chapters<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<std::collections::HashMap<String, Chapters>, D::Error> {
    use serde::Deserialize;
    let deserialized = VecOrHashMap::deserialize(deserializer)?;
    let mut chapters = std::collections::HashMap::new();
    match deserialized {
        VecOrHashMap::Vec(list) => {
            for (i, c) in list.into_iter().enumerate() {
                chapters.insert(i.to_string(), c);
            }
        }
        VecOrHashMap::Hash(h) => chapters = h,
    }
    Ok(chapters)
}

impl From<AggregateChapterResponse> for ListOfChapters {
    fn from(value: AggregateChapterResponse) -> Self {
        let mut volumes: Vec<Volumes> = vec![];
        for (vol_key, vol) in value.volumes {
            let chapters: Vec<ChapterReader> = vol
                .chapters
                .into_iter()
                .map(|(number, chap)| ChapterReader {
                    id: chap.others.first().cloned().unwrap_or(chap.id),
                    number,
                    volume: vol_key.clone(),
                })
                .collect();
            volumes.push(Volumes { chapters: SortedChapters::new(chapters), volume: vol_key });
        }
        ListOfChapters { volumes: SortedVolumes::new(volumes) }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OneChapterResponse {
    pub data: OneChapterData,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OneChapterData {
    pub id: String,
    pub attributes: ChapterAttribute,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMangaByIdResponse {
    pub data: GetMangaByIdData,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMangaByIdData {
    pub id: String,
    pub attributes: GetMangaByIdAttributes,
    pub relationships: Vec<MangaRelationship>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMangaByIdAttributes {
    pub title: Title,
    pub description: Option<Description>,
    pub publication_demograpchic: Option<String>,
    pub tags: Vec<Tag>,
    pub content_rating: String,
    pub status: String,
    pub available_translated_languages: Vec<Option<String>>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MangaRelationship {
    pub id: String,
    #[serde(rename = "type")]
    pub type_field: String,
    pub attributes: Option<MangaRelationshipAttributes>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MangaRelationshipAttributes {
    #[serde(rename = "fileName")]
    pub file_name: Option<String>,
    pub name: Option<String>,
}
