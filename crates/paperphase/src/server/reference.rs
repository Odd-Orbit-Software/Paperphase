//! Bounded, conditional ZIP/CSV reference imports using the shared HTTP pool.
#[cfg(test)]
mod tests;
use anyhow::{Context, Result, ensure};
use paperphase_runtime::ReferenceData;
use reqwest::{blocking::Client, header};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Default)]
pub(super) struct Budget(std::sync::atomic::AtomicUsize);
impl Budget {
    fn replace(&self, old: usize, new: usize) -> Result<()> {
        use std::sync::atomic::Ordering;
        self.0
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |used| {
                used.checked_sub(old)?
                    .checked_add(new)
                    .filter(|next| *next <= 64 * 1024 * 1024)
            })
            .map_err(|_| anyhow::anyhow!("combined reference memory budget exceeded"))?;
        Ok(())
    }
}
const CACHE_BYTES: u64 = 32 * 1024 * 1024;
const MEMBER_BYTES: u64 = 32 * 1024 * 1024;
const COMPRESSED_BYTES: u64 = 4 * 1024 * 1024;
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Archive {
    pub url: String,
    pub cache: PathBuf,
    pub refresh_secs: u64,
    pub tables: Vec<Table>,
    #[serde(default)]
    pub selection_setting: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Table {
    pub id: String,
    pub file: String,
    pub key: String,
    #[serde(default)]
    pub key_suffix: Vec<String>,
    #[serde(default)]
    pub select: Option<Select>,
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub group: bool,
    #[serde(default)]
    pub page_size: Option<usize>,
    pub fields: Vec<String>,
    pub filter: Option<Filter>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Select {
    pub column: String,
    pub table: String,
    pub value_index: usize,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Filter {
    pub column: String,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub contains: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cache {
    schema_version: u32,
    spec: Archive,
    etag: Option<String>,
    modified: Option<String>,
    data: ReferenceData,
    #[serde(default)]
    selected: BTreeSet<String>,
}
#[derive(Default)]
struct State {
    data: Option<Arc<ReferenceData>>,
    etag: Option<String>,
    modified: Option<String>,
    checked: u64,
    retry_at: u64,
    selected: BTreeSet<String>,
    wanted: BTreeSet<String>,
}
pub(super) struct Source {
    budget: Arc<Budget>,
    spec: Archive,
    state: RwLock<State>,
    refresh: Mutex<()>,
}
impl Source {
    #[cfg(test)]
    pub fn new(spec: Archive, root: &Path) -> Result<Self> {
        Self::with_budget(spec, root, Arc::new(Budget::default()))
    }
    pub fn with_budget(mut spec: Archive, root: &Path, budget: Arc<Budget>) -> Result<Self> {
        let url = reqwest::Url::parse(&spec.url)?;
        ensure!(
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.fragment().is_none(),
            "invalid reference archive URL"
        );
        ensure!(
            (3600..=604800).contains(&spec.refresh_secs)
                && !spec.tables.is_empty()
                && spec.tables.len() <= 8
                && spec
                    .selection_setting
                    .as_ref()
                    .is_none_or(|s| paperphase_core::valid_id(s)),
            "invalid reference archive configuration"
        );
        let mut ids = BTreeSet::new();
        for table in &spec.tables {
            ensure!(
                paperphase_core::valid_id(&table.id)
                    && ids.insert(&table.id)
                    && table.file.len() <= 128
                    && !table.file.is_empty()
                    && !table.key.is_empty()
                    && table.key.len() <= 64
                    && (!table.streaming || table.select.is_some())
                    && table
                        .select
                        .as_ref()
                        .is_none_or(|s| s.table != table.id && ids.contains(&s.table))
                    && table
                        .page_size
                        .is_none_or(|n| table.group && (1..=32).contains(&n))
                    && table.key_suffix.len() <= 3
                    && table
                        .key_suffix
                        .iter()
                        .all(|s| !s.is_empty() && s.len() <= 64)
                    && table.select.as_ref().is_none_or(|s| !s.column.is_empty()
                        && s.column.len() <= 64
                        && paperphase_core::valid_id(&s.table)
                        && s.value_index < 8
                        && spec.selection_setting.is_some())
                    && !table.fields.is_empty()
                    && table.fields.len() <= 16
                    && table.fields.iter().all(|f| !f.is_empty() && f.len() <= 64)
                    && table.filter.as_ref().is_none_or(|f| !f.column.is_empty()
                        && f.column.len() <= 64
                        && f.prefix.len() <= 64
                        && f.contains.as_ref().is_none_or(|s| s.len() <= 64)),
                "invalid reference table definition"
            );
        }
        spec.cache = root.join(&spec.cache);
        let mut state = State::default();
        if let Ok(file) = File::open(&spec.cache) {
            let mut bytes = Vec::new();
            file.take(CACHE_BYTES + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 <= CACHE_BYTES
                && let Ok(mut cache) = serde_json::from_slice::<Cache>(&bytes)
                && cache.schema_version == 1
                && serde_json::to_value(&cache.spec)? == serde_json::to_value(&spec)?
                && cache.selected.len() <= 256
                && cache.data.validate().is_ok()
            {
                budget.replace(0, cache.data.memory_cost())?;
                state.selected = cache.selected.clone();
                state.wanted = cache.selected;
                state.data = Some(Arc::new(cache.data));
                state.etag = cache.etag;
                state.modified = cache.modified;
            }
        }
        Ok(Self {
            budget,
            spec,
            state: RwLock::new(state),
            refresh: Mutex::new(()),
        })
    }
    pub fn snapshot(&self) -> Option<Arc<ReferenceData>> {
        self.state.read().ok()?.data.clone()
    }
    pub fn selections(&self, settings: &paperphase_core::SettingsValues) -> BTreeSet<String> {
        self.spec
            .selection_setting
            .as_ref()
            .and_then(|key| settings.get(key))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .split(|ch: char| ch == ',' || ch.is_whitespace())
            .filter(|s| !s.is_empty())
            .take(256)
            .map(|s| {
                s.parse::<u32>()
                    .map_or_else(|_| s.to_owned(), |n| n.to_string())
            })
            .collect()
    }
    pub fn select(&self, wanted: BTreeSet<String>) {
        if wanted.len() <= 256
            && let Ok(mut state) = self.state.write()
        {
            state.wanted = wanted;
        }
    }
    pub fn ready_for(&self, settings: &paperphase_core::SettingsValues) -> bool {
        let needed = self.selections(settings);
        self.state
            .read()
            .is_ok_and(|s| s.data.is_some() && needed.is_subset(&s.selected))
    }
    pub fn due(&self, now: u64) -> bool {
        self.state.read().is_ok_and(|s| {
            now >= s.retry_at
                && (s.selected != s.wanted
                    || now >= s.checked.saturating_add(self.spec.refresh_secs))
        })
    }
    pub fn update(&self, client: &Client) -> Result<()> {
        let result = self.update_inner(client);
        if result.is_err()
            && let Ok(mut state) = self.state.write()
        {
            state.retry_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |at| at.as_secs())
                .saturating_add(60);
        }
        result
    }
    fn update_inner(&self, client: &Client) -> Result<()> {
        let _guard = self
            .refresh
            .lock()
            .map_err(|_| anyhow::anyhow!("reference refresh lock failed"))?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let (etag, modified, ready, selected) = {
            let mut s = self
                .state
                .write()
                .map_err(|_| anyhow::anyhow!("reference state lock failed"))?;
            s.checked = now;
            (
                s.etag.clone(),
                s.modified.clone(),
                s.data.is_some() && s.selected == s.wanted,
                s.wanted.clone(),
            )
        };
        let mut request = client.head(&self.spec.url).timeout(Duration::from_secs(15));
        if ready {
            if let Some(etag) = &etag {
                request = request.header(header::IF_NONE_MATCH, etag);
            } else if let Some(date) = &modified {
                request = request.header(header::IF_MODIFIED_SINCE, date);
            }
        }
        let response = request.send().context("reference archive HEAD failed")?;
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            ensure!(ready, "reference cache unavailable");
            return Ok(());
        }
        ensure!(
            response.status().is_success(),
            "reference archive HEAD failed"
        );
        let size = response
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|s| s.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .context("reference archive needs a content length")?;
        ensure!(
            (22..=1024 * 1024 * 1024).contains(&size),
            "reference ZIP size exceeds budget"
        );
        let new_etag = response
            .headers()
            .get(header::ETAG)
            .and_then(|s| s.to_str().ok())
            .map(str::to_owned);
        let new_modified = response
            .headers()
            .get(header::LAST_MODIFIED)
            .and_then(|s| s.to_str().ok())
            .map(str::to_owned);
        if ready
            && ((new_etag.is_some() && new_etag == etag)
                || (new_etag.is_none() && new_modified.is_some() && new_modified == modified))
        {
            return Ok(());
        }
        let fetch = RangeSource {
            client,
            url: &self.spec.url,
            size,
            etag: new_etag.as_deref(),
            modified: new_modified.as_deref(),
        };
        let start = size.saturating_sub(65557);
        let tail = fetch.read(start, size - 1)?;
        let eocd = tail
            .windows(4)
            .rposition(|p| p == b"PK\x05\x06")
            .context("ZIP end record missing")?;
        ensure!(
            tail.len() >= eocd + 22
                && u16_at(&tail, eocd + 4)? == 0
                && u16_at(&tail, eocd + 6)? == 0,
            "unsupported multipart ZIP"
        );
        let count = u16_at(&tail, eocd + 10)?;
        let directory_size = u64::from(u32_at(&tail, eocd + 12)?);
        let directory_at = u64::from(u32_at(&tail, eocd + 16)?);
        ensure!(
            count <= 256
                && count > 0
                && directory_size > 0
                && directory_size <= 128 * 1024
                && directory_at
                    .checked_add(directory_size)
                    .is_some_and(|end| end <= start + eocd as u64),
            "ZIP directory exceeds budget"
        );
        let directory = fetch.read(directory_at, directory_at + directory_size - 1)?;
        let entries = directory_entries(&directory, count)?;
        let mut data = ReferenceData::default();
        let mut compressed_total = 0;
        for table in &self.spec.tables {
            let entry = entries
                .get(&table.file)
                .context("reference table missing from ZIP")?;
            compressed_total += entry.compressed;
            ensure!(
                compressed_total <= 72 * 1024 * 1024
                    && entry.compressed > 0
                    && entry.compressed
                        <= if table.streaming {
                            64 * 1024 * 1024
                        } else {
                            COMPRESSED_BYTES
                        }
                    && entry.expanded
                        <= if table.streaming {
                            512 * 1024 * 1024
                        } else {
                            MEMBER_BYTES
                        }
                    && entry.flags & 1 == 0
                    && matches!(entry.method, 0 | 8),
                "ZIP member exceeds import budget"
            );
            let local = fetch.read(entry.offset, entry.offset + 29)?;
            ensure!(
                &local[..4] == b"PK\x03\x04" && u16_at(&local, 8)? == entry.method,
                "invalid ZIP local header"
            );
            let name_len = u64::from(u16_at(&local, 26)?);
            let extra = u64::from(u16_at(&local, 28)?);
            ensure!(name_len <= 1024 && extra <= 65535, "invalid ZIP local name");
            let at = entry
                .offset
                .checked_add(30 + name_len + extra)
                .context("ZIP offset overflow")?;
            let compressed: Box<dyn Read> = if table.streaming {
                Box::new(
                    fetch
                        .open(at, at + entry.compressed - 1, 64 * 1024 * 1024)?
                        .take(entry.compressed),
                )
            } else {
                Box::new(Cursor::new(fetch.read(at, at + entry.compressed - 1)?))
            };
            let stream: Box<dyn Read> = if entry.method == 8 {
                Box::new(flate2::read::DeflateDecoder::new(compressed))
            } else {
                compressed
            };
            let checked = CheckedReader {
                inner: stream.take(entry.expanded + 1),
                crc: crc32fast::Hasher::new(),
                count: 0,
            };
            let allowed = table.select.as_ref().map(|selection| {
                selected
                    .iter()
                    .filter_map(|key| data.get(&selection.table, key))
                    .filter_map(|value| serde_json::from_str::<Vec<String>>(value).ok())
                    .filter_map(|row| row.get(selection.value_index).cloned())
                    .collect::<BTreeSet<_>>()
            });
            import_table(checked, table, entry, allowed.as_ref(), &mut data)?;
        }
        let cache = Cache {
            schema_version: 1,
            spec: self.spec.clone(),
            etag: new_etag.clone(),
            modified: new_modified.clone(),
            data,
            selected: selected.clone(),
        };
        let bytes = serde_json::to_vec(&cache)?;
        ensure!(
            bytes.len() as u64 <= CACHE_BYTES,
            "reference cache exceeds file budget"
        );
        let old_cost = self.snapshot().map_or(0, |data| data.memory_cost());
        let new_cost = cache.data.memory_cost();
        self.budget.replace(old_cost, new_cost)?;
        let persisted = (|| -> Result<()> {
            let parent = self
                .spec
                .cache
                .parent()
                .context("invalid reference cache path")?;
            std::fs::create_dir_all(parent)?;
            let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
            tmp.write_all(&bytes)?;
            tmp.as_file().sync_all()?;
            tmp.persist(&self.spec.cache).map_err(|e| e.error)?;
            File::open(parent)?.sync_all()?;
            Ok(())
        })();
        if let Err(error) = persisted {
            self.budget.replace(new_cost, old_cost)?;
            return Err(error);
        }
        let mut state = self
            .state
            .write()
            .map_err(|_| anyhow::anyhow!("reference state lock failed"))?;
        state.data = Some(Arc::new(cache.data));
        state.etag = new_etag;
        state.modified = new_modified;
        state.selected = selected;
        state.retry_at = 0;
        Ok(())
    }
}
struct RangeSource<'a> {
    client: &'a Client,
    url: &'a str,
    size: u64,
    etag: Option<&'a str>,
    modified: Option<&'a str>,
}
impl RangeSource<'_> {
    fn read(&self, first: u64, last: u64) -> Result<Vec<u8>> {
        let response = self.open(first, last, COMPRESSED_BYTES)?;
        let mut bytes = Vec::new();
        response.take(last - first + 2).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == last - first + 1,
            "invalid reference range length"
        );
        Ok(bytes)
    }
    fn open(&self, first: u64, last: u64, maximum: u64) -> Result<reqwest::blocking::Response> {
        ensure!(
            first <= last && last < self.size && last - first < maximum,
            "invalid archive byte range"
        );
        let mut request = self
            .client
            .get(self.url)
            .timeout(Duration::from_secs(120))
            .header(header::RANGE, format!("bytes={first}-{last}"));
        if let Some(etag) = self.etag.filter(|e| !e.starts_with("W/")) {
            request = request.header(header::IF_MATCH, etag);
        } else if let Some(date) = self.modified {
            request = request.header(header::IF_UNMODIFIED_SINCE, date);
        }
        let response = request
            .send()
            .context("reference byte-range request failed")?;
        ensure!(
            response.status() == reqwest::StatusCode::PARTIAL_CONTENT,
            "reference server must honour byte ranges"
        );
        let expected = format!("bytes {first}-{last}/{}", self.size);
        ensure!(
            response
                .headers()
                .get(header::CONTENT_RANGE)
                .and_then(|s| s.to_str().ok())
                == Some(expected.as_str()),
            "reference range does not match request"
        );
        if let Some(etag) = self.etag {
            ensure!(
                response
                    .headers()
                    .get(header::ETAG)
                    .and_then(|s| s.to_str().ok())
                    == Some(etag),
                "reference archive changed during import"
            );
        }
        if let Some(date) = self.modified {
            ensure!(
                response
                    .headers()
                    .get(header::LAST_MODIFIED)
                    .and_then(|s| s.to_str().ok())
                    == Some(date),
                "reference archive changed during import"
            );
        }
        ensure!(
            response.content_length() == Some(last - first + 1),
            "invalid reference range length"
        );
        Ok(response)
    }
}
struct Entry {
    flags: u16,
    method: u16,
    crc: u32,
    compressed: u64,
    expanded: u64,
    offset: u64,
}
fn u16_at(bytes: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        bytes
            .get(at..at + 2)
            .context("truncated ZIP record")?
            .try_into()?,
    ))
}
fn u32_at(bytes: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .context("truncated ZIP record")?
            .try_into()?,
    ))
}
fn directory_entries(bytes: &[u8], count: u16) -> Result<BTreeMap<String, Entry>> {
    let mut entries = BTreeMap::new();
    let mut at = 0;
    for _ in 0..count {
        ensure!(
            bytes.get(at..at + 4) == Some(b"PK\x01\x02"),
            "invalid ZIP directory entry"
        );
        let len = usize::from(u16_at(bytes, at + 28)?);
        let extra = usize::from(u16_at(bytes, at + 30)?);
        let comment = usize::from(u16_at(bytes, at + 32)?);
        let name = std::str::from_utf8(
            bytes
                .get(at + 46..at + 46 + len)
                .context("invalid ZIP filename")?,
        )?
        .to_owned();
        ensure!(
            u16_at(bytes, at + 34)? == 0,
            "multipart ZIP member unsupported"
        );
        let entry = Entry {
            flags: u16_at(bytes, at + 8)?,
            method: u16_at(bytes, at + 10)?,
            crc: u32_at(bytes, at + 16)?,
            compressed: u64::from(u32_at(bytes, at + 20)?),
            expanded: u64::from(u32_at(bytes, at + 24)?),
            offset: u64::from(u32_at(bytes, at + 42)?),
        };
        ensure!(
            entries.insert(name, entry).is_none(),
            "duplicate ZIP member"
        );
        at += 46 + len + extra + comment;
    }
    ensure!(at == bytes.len(), "unexpected ZIP directory tail");
    Ok(entries)
}
struct CheckedReader {
    inner: std::io::Take<Box<dyn Read>>,
    crc: crc32fast::Hasher,
    count: u64,
}
impl Read for CheckedReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(out)?;
        self.crc.update(&out[..n]);
        self.count += n as u64;
        Ok(n)
    }
}
fn import_table(
    input: CheckedReader,
    table: &Table,
    entry: &Entry,
    allowed: Option<&BTreeSet<String>>,
    data: &mut ReferenceData,
) -> Result<()> {
    let mut reader = csv::Reader::from_reader(input);
    let headers = reader.headers()?.clone();
    ensure!(headers.len() <= 64, "too many CSV columns");
    let column = |name: &str| {
        headers
            .iter()
            .position(|header| header == name)
            .context("missing CSV column")
    };
    let key = column(&table.key)?;
    let suffix = table
        .key_suffix
        .iter()
        .map(|name| column(name))
        .collect::<Result<Vec<_>>>()?;
    let selected_column = table
        .select
        .as_ref()
        .map(|s| column(&s.column))
        .transpose()?;
    let fields = table
        .fields
        .iter()
        .map(|f| column(f))
        .collect::<Result<Vec<_>>>()?;
    let filter = table
        .filter
        .as_ref()
        .map(|f| column(&f.column).map(|at| (at, &f.prefix, &f.contains)))
        .transpose()?;
    let mut record = csv::StringRecord::new();
    let mut count = 0;
    let mut grouped_counts = BTreeMap::<String, usize>::new();
    while reader.read_record(&mut record)? {
        count += 1;
        ensure!(
            count <= if table.streaming { 8_000_000 } else { 200_000 }
                && record.as_slice().len() <= 4096,
            "CSV row budget exceeded"
        );
        if filter.is_some_and(|(at, prefix, contains)| {
            let value = record.get(at).unwrap_or("");
            !value.starts_with(prefix)
                || contains
                    .as_ref()
                    .is_some_and(|needle| !value.contains(needle))
        }) {
            continue;
        }
        if selected_column
            .zip(allowed)
            .is_some_and(|(at, values)| !values.contains(record.get(at).unwrap_or("")))
        {
            continue;
        }
        let mut parts = vec![record.get(key).unwrap_or("")];
        parts.extend(suffix.iter().map(|at| record.get(*at).unwrap_or("")));
        ensure!(
            parts.iter().all(|part| !part.contains('|')),
            "ambiguous composite reference key"
        );
        let mut key = parts.join("|");
        if let Some(size) = table.page_size {
            let count = grouped_counts.entry(key.clone()).or_default();
            key = format!("{key}|{}", *count / size);
            *count += 1;
        }
        let values = fields
            .iter()
            .map(|at| record.get(*at).unwrap_or("").to_owned())
            .collect::<Vec<_>>();
        if table.group {
            data.append(&table.id, &key, &values)
        } else {
            data.insert(&table.id, &key, &values)
        }
        .map_err(|e| anyhow::anyhow!("{e:#}"))?;
    }
    let input = reader.into_inner();
    ensure!(
        input.count == entry.expanded && input.crc.finalize() == entry.crc,
        "CSV size or checksum mismatch"
    );
    Ok(())
}

impl Drop for Source {
    fn drop(&mut self) {
        let old = self
            .state
            .get_mut()
            .ok()
            .and_then(|state| state.data.as_ref())
            .map_or(0, |data| data.memory_cost());
        let _ = self.budget.replace(old, 0);
    }
}
