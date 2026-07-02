//! WeebCentral provider — HTML scraper (`https://weebcentral.com/`).
//!
//! Ported from manga-tui. The `Referer` header (`https://weebcentral.com/`) is
//! required on image requests to bypass Cloudflare. English-only; no volumes
//! or chapter titles.
use std::error::Error;
use std::fmt::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveDate;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, CACHE_CONTROL, CONNECTION, HOST, REFERER};
use reqwest::{Client, StatusCode, Url};
use scraper::{html, ElementRef, Selector};

use crate::manga::models::{
    Author, Chapter, ChapterFilters, ChapterOrderBy, ChapterPageUrl, ChapterReader, ChapterToRead, DecodeBytesToImage,
    Genres, GetChapterPages, GetChaptersResponse, GetMangasResponse, GoToReadChapter, HomePageMangaProvider, ImageQuality,
    Languages, ListOfChapters, Manga, MangaPageProvider, MangaProvider, MangaProviders, MangaSearchFilters, MangaStatus,
    Pagination, PopularManga, ProviderIdentity, Rating, ReaderPageProvider, RecentlyAddedManga, SearchChapterById,
    SearchMangaById, SearchMangaPanel, SearchPageProvider, SearchTerm, SortedChapters, SortedVolumes, Volumes,
};
use crate::manga::cache::{CacheDuration, Cacher, InsertEntry};
use crate::manga::html_parser::{HtmlElement, ParseHtml};

pub const WEEBCENTRAL_BASE_URL: &str = "https://weebcentral.com/";

#[derive(Clone, Debug)]
pub struct WeebcentralProvider {
    client: Client,
    base_url: Url,
    chapter_pages_header: HeaderMap,
    cache_provider: Arc<dyn Cacher>,
}

const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:133.0) Gecko/20100101 Firefox/133.0",
    "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_5) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15",
];

fn get_random_user_agent() -> &'static str {
    USER_AGENTS[0]
}

impl WeebcentralProvider {
    const CHAPTER_PAGE_CACHE: CacheDuration = CacheDuration::Long;
    const HOME_PAGE_CACHE: CacheDuration = CacheDuration::Short;
    const MANGA_PAGE_CACHE: CacheDuration = CacheDuration::LongLong;
    const SEARCH_PAGE_CACHE: CacheDuration = CacheDuration::VeryShort;

    pub fn new(cache_provider: Arc<dyn Cacher>) -> Self {
        Self::with_url(WEEBCENTRAL_BASE_URL.parse().unwrap(), cache_provider)
    }

    pub fn with_url(base_url: Url, cache_provider: Arc<dyn Cacher>) -> Self {
        let mut default_headers = HeaderMap::new();
        default_headers.insert(REFERER, HeaderValue::from_static("https://google.com"));
        default_headers.insert(HOST, HeaderValue::from_static("weebcentral.com"));
        default_headers.insert(ACCEPT, HeaderValue::from_static("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8,application/json"));
        default_headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip, deflate"));
        default_headers.insert(CACHE_CONTROL, HeaderValue::from_static("max-age=604800"));
        default_headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.5"));
        default_headers.insert(CONNECTION, HeaderValue::from_static("keep-alive"));
        default_headers.insert("DNT", HeaderValue::from_static("1"));
        default_headers.insert("sec-fetch-site", HeaderValue::from_static("none"));
        default_headers.insert("sec-fetch-mode", HeaderValue::from_static("navigate"));
        default_headers.insert("sec-fetch-user", HeaderValue::from_static("?1"));
        default_headers.insert("sec-fetch-dest", HeaderValue::from_static("document"));

        let mut chapter_pages_header = HeaderMap::new();
        chapter_pages_header.insert(CACHE_CONTROL, HeaderValue::from_static("max-age=604800"));
        chapter_pages_header.insert(REFERER, HeaderValue::from_static(WEEBCENTRAL_BASE_URL));
        chapter_pages_header.insert(CONNECTION, HeaderValue::from_static("keep-alive"));
        chapter_pages_header.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip, deflate"));
        chapter_pages_header.insert(ACCEPT, HeaderValue::from_static("image/avif,image/webp,image/png,image/svg+xml,image/*;q=0.8,*/*;q=0.5"));

        let client = Client::builder()
            .cookie_store(true)
            .timeout(Duration::from_secs(30))
            .default_headers(default_headers)
            .user_agent(get_random_user_agent())
            .build()
            .unwrap();

        Self { client, base_url, chapter_pages_header, cache_provider }
    }

    fn make_chapter_pages_url(&self, chapter_id: &str) -> String {
        format!("{}chapters/{chapter_id}/images?is_prev=False&current_page=1&reading_style=long_strip", self.base_url)
    }
    fn make_full_chapter_list_url(&self, manga_id: &str) -> String {
        format!("{}series/{manga_id}/full-chapter-list", self.base_url)
    }
    fn make_chapter_page_url(&self, chapter_id: &str) -> String {
        format!("{}chapters/{chapter_id}", self.base_url)
    }
    fn make_manga_page_url(&self, manga_id: &str) -> String {
        format!("{}series/{manga_id}", self.base_url)
    }

    fn map_chapter_to_read(&self, chapter_id: &str, data: ChapterPageData, pages: Vec<ChapterPageUrl>) -> ChapterToRead {
        ChapterToRead {
            id: chapter_id.to_string(),
            title: String::new(),
            number: data.number.parse().unwrap_or_default(),
            volume_number: None,
            num_page_bookmarked: None,
            language: Languages::English,
            pages_url: pages.into_iter().map(|p| p.url).collect(),
        }
    }

    fn map_chapters(&self, chapters: WeebcentralChapters, manga_id: &str) -> Vec<Chapter> {
        chapters.chapters.into_iter().map(|c| Chapter {
            id: c.id.clone(),
            id_safe_for_download: c.id,
            manga_id: manga_id.to_string(),
            title: String::new(),
            language: Languages::English,
            chapter_number: c.number,
            volume_number: None,
            scanlator: Some("Weeb central".to_string()),
            publication_date: Some(c.datetime),
        }).collect()
    }

    fn map_chapters_with_filters(&self, chapters: WeebcentralChapters, manga_id: &str, filters: ChapterFilters, pagination: Pagination) -> GetChaptersResponse {
        let total = chapters.chapters.len();
        let mut chapters: Vec<Chapter> = self.map_chapters(chapters, manga_id);
        if filters.order == ChapterOrderBy::Ascending {
            chapters.reverse();
        }
        let from = pagination.index_to_slice_from();
        let to = pagination.to_index(total);
        let chapters = chapters.as_slice().get(from..to).unwrap_or(&[]).to_vec();
        GetChaptersResponse { total_chapters: total as u32, chapters }
    }

    async fn get_chapter(&self, chapter_id: &str) -> Result<ChapterToRead, Box<dyn Error + Send + Sync>> {
        let pages = self.get_chapter_pages_url_with_extension(chapter_id, "", ImageQuality::default()).await?;
        let chapter_page_url = self.make_chapter_page_url(chapter_id);
        let doc = fetch_cached_html(&self.client, &self.cache_provider, &chapter_page_url, Self::CHAPTER_PAGE_CACHE).await?;
        let data = ChapterPageData::parse_html(HtmlElement::new(doc))?;
        Ok(self.map_chapter_to_read(chapter_id, data, pages))
    }

    async fn get_list_of_chapters(&self, manga_id: &str) -> Result<ListOfChapters, Box<dyn Error + Send + Sync>> {
        let url = self.make_full_chapter_list_url(manga_id);
        let doc = fetch_cached_html(&self.client, &self.cache_provider, &url, Self::CHAPTER_PAGE_CACHE).await?;
        let chapters = WeebcentralChapters::parse_html(HtmlElement::new(doc))?;
        Ok(ListOfChapters::from(chapters))
    }
}

/// Shared helper: GET an HTML doc, consulting the cache first and storing on miss.
async fn fetch_cached_html(client: &Client, cache: &Arc<dyn Cacher>, url: &str, duration: CacheDuration) -> Result<String, Box<dyn Error + Send + Sync>> {
    if let Some(cached) = cache.get(url)? {
        return Ok(String::from_utf8(cached.data)?);
    }
    let response = client.get(url).send().await?;
    if response.status() != StatusCode::OK {
        return Err(format!("request to {url} failed: {}", response.status()).into());
    }
    let doc = response.text().await?;
    cache.cache(InsertEntry { id: url, data: doc.as_bytes(), duration }).ok();
    Ok(doc)
}

impl crate::manga::models::GetRawImage for WeebcentralProvider {
    async fn get_raw_image(&self, url: &str) -> Result<bytes::Bytes, Box<dyn Error + Send + Sync>> {
        if let Some(cached) = self.cache_provider.get(url)? {
            return Ok(cached.data.into());
        }
        let response = self.client.get(url).headers(self.chapter_pages_header.clone()).send().await?;
        if response.status() != StatusCode::OK {
            return Err(format!("Could not get image on weebcentral with url: {url}").into());
        }
        let bytes = response.bytes().await?;
        self.cache_provider.cache(InsertEntry { id: url, data: &bytes, duration: CacheDuration::Long }).ok();
        Ok(bytes)
    }
}

impl DecodeBytesToImage for WeebcentralProvider {}
impl SearchMangaPanel for WeebcentralProvider {}

impl HomePageMangaProvider for WeebcentralProvider {
    async fn get_popular_mangas(&self) -> Result<Vec<PopularManga>, Box<dyn Error + Send + Sync>> {
        let doc = fetch_cached_html(&self.client, &self.cache_provider, self.base_url.as_str(), Self::HOME_PAGE_CACHE).await?;
        let resp = PopularMangasWeebCentral::parse_html(HtmlElement::new(doc))?;
        Ok(resp.mangas.into_iter().map(PopularManga::from).collect())
    }

    async fn get_recently_added_mangas(&self) -> Result<Vec<RecentlyAddedManga>, Box<dyn Error + Send + Sync>> {
        let doc = fetch_cached_html(&self.client, &self.cache_provider, self.base_url.as_str(), Self::HOME_PAGE_CACHE).await?;
        let resp = LatestMangas::parse_html(HtmlElement::new(doc))?;
        Ok(resp.mangas.into_iter().map(RecentlyAddedManga::from).collect())
    }
}

impl SearchMangaById for WeebcentralProvider {
    async fn get_manga_by_id(&self, manga_id: &str) -> Result<Manga, Box<dyn Error + Send + Sync>> {
        let url = self.make_manga_page_url(manga_id);
        let doc = fetch_cached_html(&self.client, &self.cache_provider, &url, Self::MANGA_PAGE_CACHE).await?;
        let manga = MangaPageData::parse_html(HtmlElement::new(doc))?;
        Ok(Manga::from(manga))
    }
}

impl GoToReadChapter for WeebcentralProvider {
    async fn read_chapter(&self, chapter_id: &str, manga_id: &str) -> Result<(ChapterToRead, ListOfChapters), Box<dyn Error + Send + Sync>> {
        let chapter = self.get_chapter(chapter_id).await?;
        let list = self.get_list_of_chapters(manga_id).await?;
        Ok((chapter, list))
    }
}

impl GetChapterPages for WeebcentralProvider {
    async fn get_chapter_pages_url_with_extension(&self, chapter_id: &str, _manga_id: &str, _image_quality: ImageQuality) -> Result<Vec<ChapterPageUrl>, Box<dyn Error + Send + Sync>> {
        let url = self.make_chapter_pages_url(chapter_id);
        let doc = fetch_cached_html(&self.client, &self.cache_provider, &url, Self::CHAPTER_PAGE_CACHE).await?;
        let pages = ChapterPagesLinks::parse_html(HtmlElement::new(doc))?;
        Ok(pages.pages.into_iter().map(ChapterPageUrl::from).collect())
    }
}

impl MangaPageProvider for WeebcentralProvider {
    async fn get_chapters(&self, manga_id: &str, filters: ChapterFilters, pagination: Pagination) -> Result<GetChaptersResponse, Box<dyn Error + Send + Sync>> {
        let url = self.make_full_chapter_list_url(manga_id);
        let doc = fetch_cached_html(&self.client, &self.cache_provider, &url, Self::CHAPTER_PAGE_CACHE).await?;
        let chapters = WeebcentralChapters::parse_html(HtmlElement::new(doc))?;
        Ok(self.map_chapters_with_filters(chapters, manga_id, filters, pagination))
    }

    async fn get_all_chapters(&self, manga_id: &str, _language: Languages) -> Result<Vec<Chapter>, Box<dyn Error + Send + Sync>> {
        let url = self.make_full_chapter_list_url(manga_id);
        let doc = fetch_cached_html(&self.client, &self.cache_provider, &url, Self::CHAPTER_PAGE_CACHE).await?;
        let chapters = WeebcentralChapters::parse_html(HtmlElement::new(doc))?;
        Ok(self.map_chapters(chapters, manga_id))
    }
}

impl SearchChapterById for WeebcentralProvider {
    async fn search_chapter(&self, chapter_id: &str, _manga_id: &str) -> Result<ChapterToRead, Box<dyn Error + Send + Sync>> {
        self.get_chapter(chapter_id).await
    }
}

impl ReaderPageProvider for WeebcentralProvider {}

impl SearchPageProvider for WeebcentralProvider {
    async fn search_mangas(&self, search_term: Option<SearchTerm>, _filters: MangaSearchFilters, pagination: Pagination) -> Result<GetMangasResponse, Box<dyn Error + Send + Sync>> {
        let limit = 24;
        let offset = if pagination.current_page == 1 { 0 } else { limit * pagination.current_page };
        let search = match search_term { Some(t) => format!("text={t}"), None => String::new() };
        let url = format!(
            "{}search/data?{search}&limit={limit}&offset={offset}&sort=Best+Match&order=Descending&official=Any&anime=Any&adult=Any&display_mode=Full+Display",
            self.base_url
        );
        let doc = fetch_cached_html(&self.client, &self.cache_provider, &url, Self::SEARCH_PAGE_CACHE).await?;
        let mangas = SearchPageMangas::parse_html(HtmlElement::new(doc))?;
        Ok(GetMangasResponse::from(mangas))
    }
}

impl ProviderIdentity for WeebcentralProvider {
    fn name(&self) -> MangaProviders {
        MangaProviders::Weebcentral
    }
}

impl MangaProvider for WeebcentralProvider {}

// ── response parsers (ported from manga-tui weebcentral/response.rs) ─────────

fn sel(s: &str) -> Selector {
    Selector::parse(s).unwrap()
}

/// `https://weebcentral.com/series/01J.../Title` → `01J...`
fn extract_manga_id_from_url(url: &str) -> String {
    let mut parts: Vec<&str> = url.split('/').collect();
    parts.reverse();
    parts.get(1).map(|id| id.to_string()).unwrap_or_default()
}

#[derive(Debug, Default, PartialEq, Eq)]
struct PopularMangaItem {
    id: String,
    cover_url: String,
    title: String,
    latest_chapter: Option<String>,
}

impl From<PopularMangaItem> for PopularManga {
    fn from(m: PopularMangaItem) -> Self {
        Self {
            id: m.id,
            title: m.title,
            genres: vec![],
            description: format!("Latest chapter: {}", m.latest_chapter.unwrap_or_default()),
            status: None,
            cover_img_url: m.cover_url,
        }
    }
}

struct PopularMangasWeebCentral { mangas: Vec<PopularMangaItem> }

impl ParseHtml for PopularMangasWeebCentral {
    type ParseError = crate::manga::html_parser::ParseErr;
    fn parse_html(html: HtmlElement) -> Result<Self, Self::ParseError> {
        let doc = html::Html::parse_document(html.as_str());
        let section = doc.select(&sel(".md\\:grid-cols-3")).next().ok_or(crate::manga::html_parser::ParseErr("popular section not found".to_string()))?;
        let mut mangas = vec![];
        for div in section.select(&sel("article.md\\:hidden")) {
            mangas.push(parse_popular_or_latest_item(&div, false)?);
        }
        Ok(Self { mangas })
    }
}

impl From<PopularMangaItem> for RecentlyAddedManga {
    fn from(m: PopularMangaItem) -> Self {
        Self { id: m.id, title: m.title, description: m.latest_chapter.unwrap_or_default(), cover_img_url: m.cover_url }
    }
}

struct LatestMangas { mangas: Vec<PopularMangaItem> }

impl ParseHtml for LatestMangas {
    type ParseError = crate::manga::html_parser::ParseErr;
    fn parse_html(html: HtmlElement) -> Result<Self, Self::ParseError> {
        let doc = html::Html::parse_document(html.as_str());
        let section = doc.select(&sel(".cols-span-1 > section:nth-child(2)")).next().ok_or(crate::manga::html_parser::ParseErr("latest section not found".to_string()))?;
        let mut mangas = vec![];
        for div in section.select(&sel("article")) {
            mangas.push(parse_popular_or_latest_item(&div, true)?);
        }
        Ok(Self { mangas })
    }
}

fn parse_popular_or_latest_item(div: &ElementRef, is_latest: bool) -> Result<PopularMangaItem, crate::manga::html_parser::ParseErr> {
    let img = div.select(&sel("img")).next().ok_or(crate::manga::html_parser::ParseErr("no image".to_string()))?;
    let cover = img.attr("src").ok_or(crate::manga::html_parser::ParseErr("no src".to_string()))?.to_string();
    let article = div.select(&sel("article")).next().ok_or(crate::manga::html_parser::ParseErr("no article".to_string()))?;
    let a = article.select(&sel("a")).next().ok_or(crate::manga::html_parser::ParseErr("no link".to_string()))?;
    let href = a.attr("href").ok_or(crate::manga::html_parser::ParseErr("no href".to_string()))?;
    let title = article.select(&sel(".truncate")).next().ok_or(crate::manga::html_parser::ParseErr("no title".to_string()))?.inner_html();
    let latest_chapter = if is_latest {
        article.select(&sel("span")).next().map(|t| t.inner_html())
    } else {
        article.select(&sel("span")).last().map(|t| t.inner_html())
    };
    Ok(PopularMangaItem { id: extract_manga_id_from_url(href), cover_url: cover, title, latest_chapter })
}

#[derive(Debug, Default, PartialEq, Eq)]
struct WeebcentralStatus { name: String }

impl From<WeebcentralStatus> for MangaStatus {
    fn from(v: WeebcentralStatus) -> Self {
        match v.name.to_lowercase().as_str() {
            "ongoing" => MangaStatus::Ongoing,
            _ => MangaStatus::default(),
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct WeebcentralTag { name: String }

impl From<WeebcentralTag> for Genres {
    fn from(v: WeebcentralTag) -> Self {
        let rating = match v.name.to_lowercase().as_str() {
            "ecchi" => Rating::Moderate,
            "adult" => Rating::Nsfw,
            _ => Rating::Normal,
        };
        Genres::new(v.name, rating)
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct MangaPageData {
    id: String,
    cover_url: String,
    title: String,
    description: Option<String>,
    authors: Vec<String>,
    tags: Vec<WeebcentralTag>,
    status: WeebcentralStatus,
}

impl From<MangaPageData> for Manga {
    fn from(m: MangaPageData) -> Self {
        let author = if m.authors.is_empty() {
            None
        } else {
            Some(Author {
                id: String::new(),
                name: m.authors.into_iter().fold(String::new(), |mut a, n| { let _ = write!(a, "{n},"); a }),
            })
        };
        Self {
            id: m.id.clone(),
            id_safe_for_download: m.id,
            title: m.title,
            genres: m.tags.into_iter().map(Genres::from).collect(),
            description: m.description.unwrap_or_else(|| "No description".to_string()),
            status: m.status.into(),
            cover_img_url: m.cover_url,
            languages: vec![Languages::English],
            rating: String::new(),
            artist: None,
            author,
        }
    }
}

impl ParseHtml for MangaPageData {
    type ParseError = crate::manga::html_parser::ParseErr;
    fn parse_html(html: HtmlElement) -> Result<Self, Self::ParseError> {
        let doc = html::Html::parse_document(html.as_str());
        let page_url = doc.select(&sel(r#"link[rel="canonical"]"#)).next().ok_or(crate::manga::html_parser::ParseErr("no canonical link".to_string()))?.attr("href").ok_or(crate::manga::html_parser::ParseErr("no href".to_string()))?.to_string();
        let cover_url = doc.select(&sel("picture > source")).next().ok_or(crate::manga::html_parser::ParseErr("no cover".to_string()))?.attr("srcset").ok_or(crate::manga::html_parser::ParseErr("no srcset".to_string()))?.to_string();
        let title = doc.select(&sel("h1")).next().ok_or(crate::manga::html_parser::ParseErr("no title".to_string()))?.inner_html();
        let description = doc.select(&sel("p.whitespace-pre-wrap.break-words")).next().map(|e| e.inner_html());

        let more_data = doc.select(&sel("ul.flex.flex-col.gap-4")).next().ok_or(crate::manga::html_parser::ParseErr("no data list".to_string()))?;
        let mut authors = vec![];
        let mut tags = vec![];
        let mut status = WeebcentralStatus::default();
        for li in more_data.select(&sel("li")) {
            let which = li.select(&sel("strong")).next().map(|e| e.inner_html()).unwrap_or_default();
            let which = which.trim();
            match which {
                "Author(s):" => li.select(&sel("span > a")).for_each(|a| authors.push(a.inner_html())),
                "Status:" => {
                    status = WeebcentralStatus { name: li.select(&sel("a")).next().ok_or(crate::manga::html_parser::ParseErr("no status".to_string()))?.inner_html() };
                }
                "Tags(s):" => li.select(&sel("span > a")).for_each(|a| tags.push(WeebcentralTag { name: a.inner_html() })),
                _ => {}
            }
        }
        Ok(Self { id: extract_manga_id_from_url(&page_url), cover_url, title, description, authors, status, tags })
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct WeebcentralChapter { id: String, number: String, datetime: NaiveDate }

#[derive(Debug, Default, PartialEq, Eq)]
struct WeebcentralChapters { chapters: Vec<WeebcentralChapter> }

fn parse_chapter_tag(a: &ElementRef) -> Result<WeebcentralChapter, crate::manga::html_parser::ParseErr> {
    let href = a.attr("href").ok_or(crate::manga::html_parser::ParseErr("no href".to_string()))?;
    let id = href.split('/').last().ok_or(crate::manga::html_parser::ParseErr("no id".to_string()))?.to_string();
    let span = a.select(&sel("span.grow.flex.items-center.gap-2")).next().ok_or(crate::manga::html_parser::ParseErr("no chap span".to_string()))?;
    let chap = span.select(&sel("span")).next().ok_or(crate::manga::html_parser::ParseErr("no chap title".to_string()))?;
    let number = chap.inner_html().split(' ').last().ok_or(crate::manga::html_parser::ParseErr("no number".to_string()))?.to_string();
    let dt = a.select(&sel("time")).next().ok_or(crate::manga::html_parser::ParseErr("no time".to_string()))?;
    let dt = dt.attr("datetime").ok_or(crate::manga::html_parser::ParseErr("no datetime".to_string()))?.to_string();
    Ok(WeebcentralChapter { id, number, datetime: chrono::DateTime::parse_from_rfc3339(&dt).unwrap_or_default().date_naive() })
}

impl ParseHtml for WeebcentralChapters {
    type ParseError = crate::manga::html_parser::ParseErr;
    fn parse_html(html: HtmlElement) -> Result<Self, Self::ParseError> {
        let doc = html::Html::parse_document(html.as_str());
        let mut chapters = vec![];
        for a in doc.select(&sel("div > a")) {
            if let Ok(c) = parse_chapter_tag(&a) { chapters.push(c); }
        }
        Ok(Self { chapters })
    }
}

impl From<WeebcentralChapters> for ListOfChapters {
    fn from(v: WeebcentralChapters) -> Self {
        let chapters: Vec<ChapterReader> = v.chapters.into_iter().map(|c| ChapterReader { id: c.id, number: c.number, volume: "none".to_string() }).collect();
        Self { volumes: SortedVolumes::new(vec![Volumes { volume: "none".to_string(), chapters: SortedChapters::new(chapters) }]) }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct WeebcentralPage { url: String, extension: String }

#[derive(Debug, Default, PartialEq, Eq)]
struct ChapterPagesLinks { pages: Vec<WeebcentralPage> }

impl From<WeebcentralPage> for ChapterPageUrl {
    fn from(v: WeebcentralPage) -> Self {
        Self { url: v.url.parse().unwrap(), extension: v.extension }
    }
}

impl ParseHtml for ChapterPagesLinks {
    type ParseError = crate::manga::html_parser::ParseErr;
    fn parse_html(html: HtmlElement) -> Result<Self, Self::ParseError> {
        let frag = html::Html::parse_fragment(html.as_str());
        let mut pages = vec![];
        for img in frag.select(&sel("img")) {
            if let Some(src) = img.attr("src").map(|s| s.to_string()) {
                let extension = Path::new(&src).extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
                pages.push(WeebcentralPage { url: src, extension });
            }
        }
        Ok(Self { pages })
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ChapterPageData { number: String }

impl ParseHtml for ChapterPageData {
    type ParseError = crate::manga::html_parser::ParseErr;
    fn parse_html(html: HtmlElement) -> Result<Self, Self::ParseError> {
        let doc = html::Html::parse_document(html.as_str());
        let btn = doc.select(&sel("#nav-top > div:nth-child(1) > div:nth-child(1) > button:nth-child(2)")).next().ok_or(crate::manga::html_parser::ParseErr("no chap number btn".to_string()))?;
        let number = btn.select(&sel("span")).next().ok_or(crate::manga::html_parser::ParseErr("no span".to_string()))?.inner_html();
        let number = number.split(' ').last().ok_or(crate::manga::html_parser::ParseErr("no number".to_string()))?.to_string();
        Ok(Self { number })
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct SearchPageItem {
    id: String,
    title: String,
    cover_url: String,
    status: WeebcentralStatus,
    authors: Vec<String>,
    tags: Vec<WeebcentralTag>,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct SearchPageMangas { mangas: Vec<SearchPageItem>, more_result: bool }

impl ParseHtml for SearchPageItem {
    type ParseError = crate::manga::html_parser::ParseErr;
    fn parse_html(html: HtmlElement) -> Result<Self, Self::ParseError> {
        let frag = html::Html::parse_fragment(html.as_str());
        let href = frag.select(&sel("span.tooltip.tooltip-bottom a")).next().ok_or(crate::manga::html_parser::ParseErr("no a".to_string()))?.attr("href").ok_or(crate::manga::html_parser::ParseErr("no href".to_string()))?;
        let cover_url = frag.select(&sel("source")).next().ok_or(crate::manga::html_parser::ParseErr("no cover".to_string()))?.attr("srcset").ok_or(crate::manga::html_parser::ParseErr("no srcset".to_string()))?.to_string();
        let title = frag.select(&sel(".line-clamp-1")).next().ok_or(crate::manga::html_parser::ParseErr("no title".to_string()))?.inner_html();
        let authors: Vec<String> = frag.select(&sel("article.bg-base-300:nth-child(1) > section:nth-child(2) > div:nth-child(5) > span > a")).map(|a| a.inner_html().trim().to_string()).collect();
        let status = WeebcentralStatus { name: frag.select(&sel("article.bg-base-300:nth-child(1) > section:nth-child(2) > div:nth-child(3) > span:nth-child(2)")).next().ok_or(crate::manga::html_parser::ParseErr("no status".to_string()))?.inner_html() };
        let tags: Vec<WeebcentralTag> = frag.select(&sel("article.bg-base-300:nth-child(1) > section:nth-child(2) > div:nth-child(6) > span")).map(|t| WeebcentralTag { name: t.inner_html().replace(',', "") }).collect();
        Ok(Self { cover_url, id: extract_manga_id_from_url(href), title, authors, status, tags })
    }
}

impl ParseHtml for SearchPageMangas {
    type ParseError = crate::manga::html_parser::ParseErr;
    fn parse_html(html: HtmlElement) -> Result<Self, Self::ParseError> {
        let frag = html::Html::parse_fragment(html.as_str());
        let mut mangas = vec![];
        for article in frag.select(&sel("article.bg-base-300")) {
            if let Ok(m) = SearchPageItem::parse_html(HtmlElement::new(article.html())) { mangas.push(m); }
        }
        let more_result = frag.select(&sel("button.col-span-2")).next().is_some();
        Ok(Self { mangas: mangas.into_iter().take(24).collect(), more_result })
    }
}

impl From<SearchPageItem> for crate::manga::models::SearchManga {
    fn from(m: SearchPageItem) -> Self {
        Self {
            id: m.id,
            title: m.title,
            genres: m.tags.into_iter().map(Genres::from).collect(),
            description: None,
            status: Some(m.status.into()),
            cover_img_url: m.cover_url,
            languages: vec![Languages::English],
            artist: None,
            author: None,
        }
    }
}

impl From<SearchPageMangas> for GetMangasResponse {
    fn from(v: SearchPageMangas) -> Self {
        let count = v.mangas.len() as u32;
        Self {
            mangas: v.mangas.into_iter().map(crate::manga::models::SearchManga::from).collect(),
            total_mangas: if v.more_result { 1000000 } else { count },
            next_page: v.more_result,
        }
    }
}

// (end)
