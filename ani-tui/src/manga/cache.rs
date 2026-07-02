//! In-memory manga cache ported from manga-tui's `backend::cache`.
//!
//! Reduces repeated scraping requests (and the chance of being rate-limited /
//! blocked). Entries are TTL-based with a background cleanup thread that only
//! evicts once capacity is exceeded.
use std::collections::HashMap;
use std::error::Error;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Clone)]
pub enum CacheDuration {
    LongLong,
    Long,
    Medium,
    Short,
    VeryShort,
}

#[derive(Debug, PartialEq, Clone)]
pub struct InsertEntry<'a> {
    pub id: &'a str,
    pub data: &'a [u8],
    pub duration: CacheDuration,
}

#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct Entry {
    pub data: Vec<u8>,
}

pub trait Cacher: Send + Sync + Debug {
    fn cache(&self, entry: InsertEntry) -> Result<(), Box<dyn Error + Send + Sync>>;
    fn get(&self, id: &str) -> Result<Option<Entry>, Box<dyn Error + Send + Sync>>;
}

#[derive(Debug)]
struct EntryDuration(Duration);

impl From<CacheDuration> for EntryDuration {
    fn from(value: CacheDuration) -> Self {
        Self(match value {
            CacheDuration::LongLong => Duration::from_secs(120),
            CacheDuration::Long => Duration::from_secs(60),
            CacheDuration::Medium => Duration::from_secs(20),
            CacheDuration::Short => Duration::from_secs(10),
            CacheDuration::VeryShort => Duration::from_secs(5),
        })
    }
}

#[derive(Debug)]
struct MemoryEntry {
    data: Vec<u8>,
    time_since_creation: Instant,
    time_to_live: EntryDuration,
}

impl MemoryEntry {
    fn new(data: Vec<u8>, ttl: EntryDuration) -> Self {
        Self { data, time_since_creation: Instant::now(), time_to_live: ttl }
    }

    fn is_expired(&self) -> bool {
        self.time_since_creation.elapsed() > self.time_to_live.0
    }
}

#[derive(Debug)]
pub struct InMemoryCache {
    entries: Mutex<HashMap<String, MemoryEntry>>,
    capacity: usize,
}

impl InMemoryCache {
    pub fn init(capacity: usize) -> Arc<Self> {
        let cache = Arc::new(Self { entries: Mutex::new(HashMap::new()), capacity });
        Self::start_cleanup_thread(Arc::clone(&cache));
        cache
    }

    fn delete_expired_entries(&self) {
        let mut entries = self.entries.lock().unwrap();
        if entries.len() > self.capacity {
            entries.retain(|_, e| !e.is_expired());
        }
    }

    fn start_cleanup_thread(cache: Arc<Self>) -> thread::JoinHandle<()> {
        thread::spawn(move || loop {
            cache.delete_expired_entries();
            thread::sleep(Duration::from_millis(500));
        })
    }
}

impl Cacher for InMemoryCache {
    fn get(&self, id: &str) -> Result<Option<Entry>, Box<dyn Error + Send + Sync>> {
        let mut entries = self.entries.lock().unwrap();
        Ok(entries.get_mut(id).map(|e| {
            e.time_since_creation = Instant::now(); // renew on access
            Entry { data: e.data.clone() }
        }))
    }

    fn cache(&self, entry: InsertEntry) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut entries = self.entries.lock().unwrap();
        entries.insert(entry.id.to_string(), MemoryEntry::new(entry.data.to_vec(), entry.duration.into()));
        Ok(())
    }
}

/// No-op cache (useful for tests + offline scenarios).
#[derive(Debug)]
pub struct EmptyCache;

impl EmptyCache {
    pub fn new_arc() -> Arc<Self> {
        Arc::new(EmptyCache)
    }
}

impl Cacher for EmptyCache {
    fn get(&self, _id: &str) -> Result<Option<Entry>, Box<dyn Error + Send + Sync>> {
        Ok(None)
    }
    fn cache(&self, _entry: InsertEntry) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }
}
