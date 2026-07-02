//! MangaPill provider — HTML scraper (`https://mangapill.com`).
//!
//! Ported from manga-tui. English-only; no volumes or chapter titles. Uses the
//! generic `HtmlParser` trait via `Scraper` (not direct scraper calls). The
//! `Referer` header is required for image requests to bypass Cloudflare.
use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, CACHE_CONTROL, CONNECTION, HOST, REFERER};
use reqwest::{Client, StatusCode, Url};
use regex::Regex;

use crate::manga::models::{
    Chapter, ChapterFilters, ChapterOrderBy, ChapterPageUrl, ChapterReader, ChapterToRead, DecodeBytesToImage, Genres,
    GetChapterPages, GetChaptersResponse, GetMangasResponse, GoToReadChapter, HomePageMangaProvider, ImageQuality,
    Languages, ListOfChapters, Manga, MangaPageProvider, MangaProvider, MangaProviders, MangaSearchFilters, MangaStatus,
    Pagination, PopularManga, ProviderIdentity, Rating, ReaderPageProvider, RecentlyAddedManga, SearchChapterById,
    SearchMangaById, SearchMangaPanel, SearchPageProvider, SearchManga as SearchMangaModel, SearchTerm, SortedChapters,
    SortedVolumes, Volumes,
};
use crate::manga::cache::{CacheDuration, Cacher, InsertEntry};
use crate::manga::html_parser::{HtmlElement, HtmlParser};
use crate::manga::scraper_impl::Scraper;

pub const MANGA_PILL_URL: &str = "https://mangapill.com";

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0";

#[derive(Clone, Debug)]
pub struct MangaPillProvider {
    client: Client,
    base_url: Url,
    cache_provider: Arc<dyn Cacher>,
}

impl MangaPillProvider {
    const CHAPTER_PAGE_CACHE: CacheDuration = CacheDuration::Long;
    const HOME_PAGE_CACHE: CacheDuration = CacheDuration::Short;
    const MANGA_PAGE_CACHE: CacheDuration = CacheDuration::LongLong;
    const SEARCH_PAGE_CACHE: CacheDuration = CacheDuration::VeryShort;

    pub fn new(cache_provider: Arc<dyn Cacher>) -> Self {
        Self::with_url(MANGA_PILL_URL.parse().unwrap(), cache_provider)
    }

    pub fn with_url(base_url: Url, cache_provider: Arc<dyn Cacher>) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(REFERER, HeaderValue::from_static(MANGA_PILL_URL));
        headers.insert(HOST, HeaderValue::from_static("mangapill.com"));
        headers.insert(ACCEPT, HeaderValue::from_static("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8,application/json"));
        headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip, deflate"));
        headers.insert(CACHE_CONTROL, HeaderValue::from_static("max-age=604800"));
        headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.5"));
        headers.insert(CONNECTION, HeaderValue::from_static("keep-alive"));
        headers.insert("DNT", HeaderValue::from_static("1"));
        headers.insert("sec-fetch-site", HeaderValue::from_static("none"));
        headers.insert("sec-fetch-mode", HeaderValue::from_static("navigate"));
        headers.insert("sec-fetch-user", HeaderValue::from_static("?1"));
        headers.insert("sec-fetch-dest", HeaderValue::from_static("document"));

        let client = Client::builder()
            .cookie_store(true)
            .timeout(Duration::from_secs(30))
            .default_headers(headers)
            .user_agent(USER_AGENT)
            .build()
            .unwrap();

        Self { client, base_url, cache_provider }
    }

    fn map_chapter_to_read(&self, doc: String, chapter_id: &str) -> Result<ChapterToRead, crate::manga::html_parser::ParseErr> {
        let parser = Scraper::new(HtmlElement::new(doc));
        let chapter = ChapterPageDataParser::new(parser).scrape_page()?;
        Ok(ChapterToRead {
            id: chapter_id.to_string(),
            title: chapter.manga_title,
            number: chapter.number,
            volume_number: None,
            num_page_bookmarked: None,
            language: Languages::English,
            pages_url: chapter.pages_url,
        })
    }

    fn map_manga(&self, doc: String, manga_id: &str) -> Result<Manga, crate::manga::html_parser::ParseErr> {
        let parser = Scraper::new(HtmlElement::new(doc));
        let m = MangaPageDataParser::new(parser).scrape_manga_page()?;
        let id_safe = manga_id.split('/').nth(1).ok_or(format!("bad manga id: {manga_id}"))?.to_string();
        Ok(Manga {
            id: manga_id.to_string(),
            title: m.title,
            description: m.description.unwrap_or_else(|| "No description".to_string()),
            status: m.status.into(),
            cover_img_url: m.cover_url,
            languages: vec![Languages::English],
            genres: m.tags.into_iter().map(Genres::from).collect(),
            id_safe_for_download: id_safe,
            ..Default::default()
        })
    }

    fn map_popular(&self, doc: String) -> Result<Vec<PopularManga>, crate::manga::html_parser::ParseErr> {
        let parser = Scraper::new(HtmlElement::new(doc));
        Ok(PopularMangasMangaPill::new(parser).scrape_popular_mangas()?.into_iter().map(|m| PopularManga {
            id: m.page_url,
            title: m.title,
            genres: vec![],
            description: m.latest_chapter.map(|l| format!("Latest chapter: {l}")).unwrap_or_default(),
            status: None,
            cover_img_url: m.cover_url,
        }).collect())
    }

    fn map_latest(&self, doc: String) -> Result<Vec<RecentlyAddedManga>, crate::manga::html_parser::ParseErr> {
        let parser = Scraper::new(HtmlElement::new(doc));
        Ok(LatestMangas::new(parser).scrape_latest_mangas()?.into_iter().map(|m| RecentlyAddedManga {
            id: m.page_url,
            title: m.title,
            description: m.latest_chapter.map(|l| format!("Latest chapter: {l}")).unwrap_or_default(),
            cover_img_url: m.cover_url,
        }).collect())
    }

    fn map_chapters(&self, doc: String, manga_id: &str, filters: ChapterFilters, pagination: Pagination) -> Result<GetChaptersResponse, crate::manga::html_parser::ParseErr> {
        let parser = Scraper::new(HtmlElement::new(doc));
        let pages = MangaPillChaptersParser::new(parser).scrape_list_of_chapters()?;
        let total = pages.chapters.len();
        let mut chapters: Vec<Chapter> = pages.chapters.into_iter().map(|c| Chapter {
            id_safe_for_download: c.full_url.split('/').nth(1).unwrap_or("").to_string(),
            id: c.full_url,
            manga_id: manga_id.to_string(),
            title: String::new(),
            chapter_number: c.number,
            ..Default::default()
        }).collect();
        if filters.order == ChapterOrderBy::Ascending {
            chapters.reverse();
        }
        Ok(GetChaptersResponse {
            chapters: chapters.into_iter()
                .skip((pagination.items_per_page * (pagination.current_page - 1)) as usize)
                .take(pagination.items_per_page as usize)
                .collect(),
            total_chapters: total as u32,
        })
    }

    fn map_search(&self, doc: String) -> Result<GetMangasResponse, crate::manga::html_parser::ParseErr> {
        let parser = Scraper::new(HtmlElement::new(doc));
        let res = SearchPageMangasParser::new(parser).scrape_search_page()?;
        let mut total = res.mangas.len();
        let mangas: Vec<SearchMangaModel> = res.mangas.into_iter().map(|m| SearchMangaModel {
            id: m.page_url,
            cover_img_url: m.cover_url,
            title: m.title,
            status: Some(m.status.into()),
            languages: vec![Languages::English],
            ..Default::default()
        }).collect();
        if res.next_page.is_some() || res.previous_page.is_some() {
            total = 10000;
        }
        Ok(GetMangasResponse { mangas, total_mangas: total as u32, next_page: res.next_page.is_some() })
    }

    async fn get_chapter(&self, chapter_id: &str) -> Result<ChapterToRead, Box<dyn Error + Send + Sync>> {
        let url = format!("{}{}", self.base_url, chapter_id);
        let doc = fetch_html(&self.client, &self.cache_provider, &url, Self::MANGA_PAGE_CACHE).await?;
        Ok(self.map_chapter_to_read(doc, chapter_id)?)
    }

    async fn get_list_of_chapters(&self, manga_id: &str) -> Result<ListOfChapters, Box<dyn Error + Send + Sync>> {
        let url = format!("{}{}", self.base_url, manga_id);
        let doc = fetch_html(&self.client, &self.cache_provider, &url, Self::MANGA_PAGE_CACHE).await?;
        let parser = Scraper::new(HtmlElement::new(doc));
        let chapters = MangaPillChaptersParser::new(parser).scrape_list_of_chapters()?;
        Ok(ListOfChapters::from(chapters))
    }
}

async fn fetch_html(client: &Client, cache: &Arc<dyn Cacher>, url: &str, duration: CacheDuration) -> Result<String, Box<dyn Error + Send + Sync>> {
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

impl crate::manga::models::GetRawImage for MangaPillProvider {
    async fn get_raw_image(&self, url: &str) -> Result<bytes::Bytes, Box<dyn Error + Send + Sync>> {
        if let Some(cached) = self.cache_provider.get(url)? {
            return Ok(cached.data.into());
        }
        let response = self.client.get(url).send().await?;
        if response.status() != StatusCode::OK {
            return Err(format!("Could not get image on MangaPill with url: {url}").into());
        }
        let bytes = response.bytes().await?;
        self.cache_provider.cache(InsertEntry { id: url, data: &bytes, duration: CacheDuration::Long }).ok();
        Ok(bytes)
    }
}

impl DecodeBytesToImage for MangaPillProvider {}
impl SearchMangaPanel for MangaPillProvider {}

impl ProviderIdentity for MangaPillProvider {
    fn name(&self) -> MangaProviders {
        MangaProviders::Mangapill
    }
}

impl SearchMangaById for MangaPillProvider {
    async fn get_manga_by_id(&self, manga_id: &str) -> Result<Manga, Box<dyn Error + Send + Sync>> {
        let url = format!("{}{}", self.base_url, manga_id);
        let doc = fetch_html(&self.client, &self.cache_provider, &url, Self::MANGA_PAGE_CACHE).await?;
        Ok(self.map_manga(doc, manga_id)?)
    }
}

impl HomePageMangaProvider for MangaPillProvider {
    async fn get_popular_mangas(&self) -> Result<Vec<PopularManga>, Box<dyn Error + Send + Sync>> {
        let doc = fetch_html(&self.client, &self.cache_provider, self.base_url.as_str(), Self::HOME_PAGE_CACHE).await?;
        Ok(self.map_popular(doc)?)
    }

    async fn get_recently_added_mangas(&self) -> Result<Vec<RecentlyAddedManga>, Box<dyn Error + Send + Sync>> {
        let doc = fetch_html(&self.client, &self.cache_provider, self.base_url.as_str(), Self::HOME_PAGE_CACHE).await?;
        Ok(self.map_latest(doc)?)
    }
}

impl SearchChapterById for MangaPillProvider {
    async fn search_chapter(&self, chapter_id: &str, _manga_id: &str) -> Result<ChapterToRead, Box<dyn Error + Send + Sync>> {
        self.get_chapter(chapter_id).await
    }
}

impl GoToReadChapter for MangaPillProvider {
    async fn read_chapter(&self, chapter_id: &str, manga_id: &str) -> Result<(ChapterToRead, ListOfChapters), Box<dyn Error + Send + Sync>> {
        let chapter = self.get_chapter(chapter_id).await?;
        let list = self.get_list_of_chapters(manga_id).await?;
        Ok((chapter, list))
    }
}

impl GetChapterPages for MangaPillProvider {
    async fn get_chapter_pages_url_with_extension(&self, chapter_id: &str, _manga_id: &str, _image_quality: ImageQuality) -> Result<Vec<ChapterPageUrl>, Box<dyn Error + Send + Sync>> {
        let url = format!("{}{}", self.base_url, chapter_id);
        let doc = fetch_html(&self.client, &self.cache_provider, &url, Self::CHAPTER_PAGE_CACHE).await?;
        let parser = Scraper::new(HtmlElement::new(doc));
        Ok(ChapterPagesScraper::new(parser).scrape_pages_from_chapter()?)
    }
}

impl MangaPageProvider for MangaPillProvider {
    async fn get_chapters(&self, manga_id: &str, filters: ChapterFilters, pagination: Pagination) -> Result<GetChaptersResponse, Box<dyn Error + Send + Sync>> {
        let url = format!("{}{}", self.base_url, manga_id);
        let doc = fetch_html(&self.client, &self.cache_provider, &url, Self::MANGA_PAGE_CACHE).await?;
        Ok(self.map_chapters(doc, manga_id, filters, pagination)?)
    }

    async fn get_all_chapters(&self, manga_id: &str, _language: Languages) -> Result<Vec<Chapter>, Box<dyn Error + Send + Sync>> {
        let res = self.get_chapters(manga_id, ChapterFilters::default(), Pagination::new(1, 10000000, 10000000)).await?;
        Ok(res.chapters)
    }
}

impl ReaderPageProvider for MangaPillProvider {}

impl SearchPageProvider for MangaPillProvider {
    async fn search_mangas(&self, search_term: Option<SearchTerm>, _filters: MangaSearchFilters, pagination: Pagination) -> Result<GetMangasResponse, Box<dyn Error + Send + Sync>> {
        let current_page = pagination.current_page;
        let search = search_term.map(|s| format!("q={s}")).unwrap_or_default();
        let endpoint = format!("{}search?{}&page={current_page}", self.base_url, search);
        let doc = fetch_html(&self.client, &self.cache_provider, &endpoint, Self::SEARCH_PAGE_CACHE).await?;
        Ok(self.map_search(doc)?)
    }
}

impl MangaProvider for MangaPillProvider {}

// ── response parsers (ported from manga-tui mangapill/response.rs) ───────────

#[derive(Debug, Default, PartialEq, Eq)]
struct PopularMangaItem {
    id: String,
    page_url: String,
    cover_url: String,
    title: String,
    latest_chapter: Option<String>,
}

struct PopularMangasMangaPill<T: HtmlParser> { parser: T }

impl<T: HtmlParser> PopularMangasMangaPill<T> {
    fn new(parser: T) -> Self { Self { parser } }

    fn parse_one(&self, div: &HtmlElement) -> Result<PopularMangaItem, crate::manga::html_parser::ParseErr> {
        let a_tags = self.parser.get_matching_elements_from(div, "a");
        let img = a_tags.first()
            .and_then(|a| self.parser.get_element_from(a, "img"))
            .and_then(|img| self.parser.get_element_attr(&img, "data-src"))
            .ok_or(crate::manga::html_parser::ParseErr("img source not found".to_string()))?;
        let chapter_number = a_tags.get(1).ok_or(crate::manga::html_parser::ParseErr("chapter number not found".to_string()))?;
        let chapter_number = self.parser.get_element_from(chapter_number, "div").ok_or(crate::manga::html_parser::ParseErr("element not found".to_string()))?;
        let chapter_number = self.parser.get_inner_text(&chapter_number);
        let title_and_id = a_tags.last().ok_or(crate::manga::html_parser::ParseErr("title not found".to_string()))?;
        let title = self.parser.get_element_from(title_and_id, "div").ok_or(crate::manga::html_parser::ParseErr("title not found".to_string()))?;
        let title = self.parser.get_inner_text(&title);
        let page_url = self.parser.get_element_attr(title_and_id, "href").ok_or(crate::manga::html_parser::ParseErr("a with id not found".to_string()))?;
        let id = page_url.split('/').nth(2).ok_or(crate::manga::html_parser::ParseErr("id not extracted".to_string()))?;
        Ok(PopularMangaItem {
            id: id.to_string(),
            page_url: page_url.replacen('/', "", 1),
            title,
            cover_url: img,
            latest_chapter: Some(chapter_number),
        })
    }

    fn scrape_popular_mangas(self) -> Result<Vec<PopularMangaItem>, crate::manga::html_parser::ParseErr> {
        let mut res = vec![];
        for el in self.parser.get_matching_elements(".featured-grid > div").iter() {
            if let Ok(p) = self.parse_one(el) { res.push(p); }
        }
        Ok(res)
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct LatestMangItem {
    id: String,
    page_url: String,
    cover_url: String,
    title: String,
    latest_chapter: Option<String>,
}

struct LatestMangas<T: HtmlParser> { parser: T }

impl<T: HtmlParser> LatestMangas<T> {
    fn new(parser: T) -> Self { Self { parser } }

    fn parse_one(&self, div: &HtmlElement) -> Result<LatestMangItem, crate::manga::html_parser::ParseErr> {
        let latest_chapter = self.parser.get_element_from(div, "a > div:nth-of-type(1)").map(|el| self.parser.get_inner_text(&el));
        let img = self.parser.get_element_from(div, "img")
            .and_then(|i| self.parser.get_element_attr(&i, "data-src")).ok_or(crate::manga::html_parser::ParseErr("img src not found".to_string()))?;
        let a_title = self.parser.get_element_from(div, ".px-1 > a:nth-of-type(2)").ok_or(crate::manga::html_parser::ParseErr("title not found".to_string()))?;
        let title = self.parser.get_element_from(&a_title, "div").ok_or(crate::manga::html_parser::ParseErr("no title".to_string()))?;
        let title = self.parser.get_inner_text(&title);
        let page_url = self.parser.get_element_attr(&a_title, "href").ok_or(crate::manga::html_parser::ParseErr("no id".to_string()))?;
        let id = page_url.split('/').nth(2).ok_or(crate::manga::html_parser::ParseErr("failed to parse id".to_string()))?;
        Ok(LatestMangItem { id: id.to_string(), page_url: page_url.replacen('/', "", 1), title, cover_url: img, latest_chapter })
    }

    fn scrape_latest_mangas(self) -> Result<Vec<LatestMangItem>, crate::manga::html_parser::ParseErr> {
        let mut res = vec![];
        for el in self.parser.get_matching_elements("div.col-span-4:nth-child(1) > div:nth-child(2) > div").iter().take(5) {
            if let Ok(p) = self.parse_one(el) { res.push(p); }
        }
        Ok(res)
    }
}

#[derive(Debug, PartialEq, Eq)]
struct MangaPillStatus { name: String }

impl Default for MangaPillStatus {
    fn default() -> Self { Self { name: "publishing".to_string() } }
}

impl From<MangaPillStatus> for MangaStatus {
    fn from(v: MangaPillStatus) -> Self {
        match v.name.to_lowercase().as_str() {
            "ongoing" | "publishing" => MangaStatus::Ongoing,
            "finished" => MangaStatus::Completed,
            "on hiatus" => MangaStatus::Hiatus,
            "discontinued" => MangaStatus::Cancelled,
            _ => MangaStatus::default(),
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct MangaPillTag { name: String }

impl From<MangaPillTag> for Genres {
    fn from(v: MangaPillTag) -> Self {
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
    cover_url: String,
    title: String,
    description: Option<String>,
    tags: Vec<MangaPillTag>,
    status: MangaPillStatus,
}

struct MangaPageDataParser<T: HtmlParser> { scraper: T }

impl<T: HtmlParser> MangaPageDataParser<T> {
    fn new(scraper: T) -> Self { Self { scraper } }

    fn scrape_manga_page(self) -> Result<MangaPageData, crate::manga::html_parser::ParseErr> {
        let main = self.scraper.get_element(".sm\\:flex-row").ok_or(crate::manga::html_parser::ParseErr("main content not found".to_string()))?;
        let cover_url = self.scraper.get_element_from(&main, "img")
            .and_then(|i| self.scraper.get_element_attr(&i, "data-src")).ok_or(crate::manga::html_parser::ParseErr("img src not found".to_string()))?;
        let title = self.scraper.get_element_from(&main, "h1").ok_or(crate::manga::html_parser::ParseErr("no title".to_string()))?;
        let title = self.scraper.get_inner_text(&title);
        let description = self.scraper.get_element_from(&main, "p").map(|el| self.scraper.get_inner_text(&el));
        let status = self.scraper.get_element_from(&main, "div.grid:nth-child(3) > div:nth-child(2) > div:nth-child(2)").ok_or(crate::manga::html_parser::ParseErr("no status".to_string()))?;
        let status = self.scraper.get_inner_text(&status);
        let tags: Vec<MangaPillTag> = self.scraper.get_matching_elements_from(&main, "a.text-sm").iter()
            .map(|a| MangaPillTag { name: self.scraper.get_inner_text(a) }).collect();
        Ok(MangaPageData { cover_url, title, description, status: MangaPillStatus { name: status }, tags })
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct MangaPillChapterListItem { id: String, full_url: String, number: String }

#[derive(Debug, Default, PartialEq, Eq)]
struct MangaPillChapters { chapters: Vec<MangaPillChapterListItem> }

struct MangaPillChaptersParser<T: HtmlParser> { scraper: T }

impl<T: HtmlParser> MangaPillChaptersParser<T> {
    fn new(scraper: T) -> Self { Self { scraper } }

    fn scrape_list_of_chapters(self) -> Result<MangaPillChapters, crate::manga::html_parser::ParseErr> {
        let scrape_a = |a: &HtmlElement| -> Option<MangaPillChapterListItem> {
            let url = self.scraper.get_element_attr(a, "href")?;
            let id = url.split('/').nth(2)?.split('-').last()?;
            Some(MangaPillChapterListItem {
                id: id.to_string(),
                full_url: url.replacen('/', "", 1),
                number: self.scraper.get_inner_text(a).split(' ').nth(1)?.to_string(),
            })
        };
        let chapters: Vec<MangaPillChapterListItem> = self.scraper.get_matching_elements("a.border").iter().filter_map(scrape_a).collect();
        Ok(MangaPillChapters { chapters })
    }
}

impl From<MangaPillChapters> for ListOfChapters {
    fn from(v: MangaPillChapters) -> Self {
        let chapters: Vec<ChapterReader> = v.chapters.into_iter().map(|c| ChapterReader { id: c.full_url, number: c.number, volume: "none".to_string() }).collect();
        Self { volumes: SortedVolumes::new(vec![Volumes { volume: "none".to_string(), chapters: SortedChapters::new(chapters) }]) }
    }
}

#[derive(Debug, Default)]
struct ChapterPageData {
    manga_title: String,
    number: f64,
    pages_url: Vec<Url>,
}

struct ChapterPageDataParser<T: HtmlParser> { scraper: T }

impl<T: HtmlParser> ChapterPageDataParser<T> {
    fn new(scraper: T) -> Self { Self { scraper } }

    fn scrape_page(self) -> Result<ChapterPageData, crate::manga::html_parser::ParseErr> {
        let title = self.scraper.get_element("h1").map(|h1| self.scraper.get_inner_text(&h1)).unwrap_or_else(|| "No title".to_string());
        let re = Regex::new(r"\d+(\.\d+)?").unwrap();
        let number = re.find(&title).and_then(|m| m.as_str().parse::<f64>().ok()).unwrap_or_default();
        let pages_url: Vec<Url> = self.scraper.get_matching_elements(".js-page").iter()
            .filter_map(|img| self.scraper.get_element_attr(img, "data-src").and_then(|u| Url::parse(&u).ok())).collect();
        Ok(ChapterPageData { manga_title: title, number, pages_url })
    }
}

struct ChapterPagesScraper<T: HtmlParser> { scraper: T }

impl<T: HtmlParser> ChapterPagesScraper<T> {
    fn new(scraper: T) -> Self { Self { scraper } }

    fn scrape_pages_from_chapter(self) -> Result<Vec<ChapterPageUrl>, crate::manga::html_parser::ParseErr> {
        let pages: Vec<ChapterPageUrl> = self.scraper.get_matching_elements(".js-page").iter().filter_map(|img| {
            let url = self.scraper.get_element_attr(img, "data-src")?;
            let parsed = Url::parse(&url).ok()?;
            let extension = Path::new(parsed.path()).extension().and_then(|e| e.to_str())?.to_string();
            Some(ChapterPageUrl { url: parsed, extension })
        }).collect();
        Ok(pages)
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct SearchPageItem {
    page_url: String,
    cover_url: String,
    title: String,
    status: MangaPillStatus,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ButtonSearchPagination { _url: String }

#[derive(Debug, Default, PartialEq, Eq)]
struct SearchPageMangas {
    mangas: Vec<SearchPageItem>,
    next_page: Option<ButtonSearchPagination>,
    previous_page: Option<ButtonSearchPagination>,
}

struct SearchPageMangasParser<T: HtmlParser> { scraper: T }

impl<T: HtmlParser> SearchPageMangasParser<T> {
    fn new(scraper: T) -> Self { Self { scraper } }

    fn scrape_search_item(&self, div: &HtmlElement) -> Result<SearchPageItem, crate::manga::html_parser::ParseErr> {
        let cover_url = self.scraper.get_element_from(div, "img")
            .and_then(|i| self.scraper.get_element_attr(&i, "data-src")).ok_or(crate::manga::html_parser::ParseErr("no img".to_string()))?;
        let a = self.scraper.get_element_from(div, "a.mb-2").ok_or(crate::manga::html_parser::ParseErr("a not found".to_string()))?;
        let page_url = self.scraper.get_element_attr(&a, "href").map(|s| s.replacen('/', "", 1)).ok_or(crate::manga::html_parser::ParseErr("no page url".to_string()))?;
        let title = self.scraper.get_element_from(&a, "div").map(|el| self.scraper.get_inner_text(&el)).ok_or(crate::manga::html_parser::ParseErr("no title".to_string()))?;
        let status = self.scraper.get_element_from(div, "div.text-xs.leading-5:last-of-type")
            .map(|d| MangaPillStatus { name: self.scraper.get_inner_text(&d) }).unwrap_or_default();
        Ok(SearchPageItem { page_url, cover_url, title, status })
    }

    fn scrape_search_page(self) -> Result<SearchPageMangas, crate::manga::html_parser::ParseErr> {
        let mut mangas = vec![];
        for div in self.scraper.get_matching_elements("div.my-3:nth-child(3) > div").iter() {
            if let Ok(m) = self.scrape_search_item(div) { mangas.push(m); }
        }
        let buttons = self.scraper.get_matching_elements("a.btn");
        let next_page = buttons.iter().find(|el| self.scraper.get_inner_text(el).to_lowercase() == "next")
            .and_then(|b| self.scraper.get_element_attr(b, "href").map(|u| ButtonSearchPagination { _url: u }));
        let previous_page = buttons.iter().find(|el| self.scraper.get_inner_text(el).to_lowercase() == "previous")
            .and_then(|b| self.scraper.get_element_attr(b, "href").map(|u| ButtonSearchPagination { _url: u }));
        Ok(SearchPageMangas { mangas, next_page, previous_page })
    }
}
