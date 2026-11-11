//! Bounded UTF-8 RSS 2.0 / Atom headline extraction. No article or image requests.
use paperphase_plugin_sdk::{Dataset, Field, FieldKind, Publication, SettingsValues};
use quick_xml::{NsReader, events::Event, name::ResolveResult};
use serde::Deserialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use url::Url;

type Result<T> = std::result::Result<T, &'static str>;
const ATOM: &str = "http://www.w3.org/2005/Atom";

#[derive(Deserialize)]
struct Settings {
    label: String,
    feed_url: String,
    max_items: usize,
}
#[derive(Default)]
struct Entry {
    title: String,
    link: Option<String>,
    published: Option<String>,
    id: String,
}
#[derive(Clone, Copy, PartialEq)]
enum Format {
    Rss,
    Atom,
}
#[derive(Clone, Copy)]
enum Target {
    FeedTitle,
    Title,
    Link,
    Published,
    Updated,
    Id,
    Ttl,
}
struct Capture {
    depth: usize,
    target: Target,
    html: bool,
    value: String,
    base: Url,
}
struct Node {
    name: String,
    base: Url,
    relevant: bool,
}

pub fn convert(bytes: &[u8], values: &SettingsValues) -> Result<Publication> {
    let settings: Settings =
        serde_json::from_value(json!(values)).map_err(|_| "invalid feed settings")?;
    if settings.label.trim().is_empty()
        || settings.label.len() > 128
        || !(1..=5).contains(&settings.max_items)
        || bytes.len() > 65536
    {
        return Err("invalid feed settings or size");
    }
    let source = Url::parse(&settings.feed_url).map_err(|_| "invalid feed URL")?;
    if !matches!(source.scheme(), "http" | "https")
        || !source.username().is_empty()
        || source.password().is_some()
    {
        return Err("invalid feed URL");
    }
    let document = std::str::from_utf8(bytes).map_err(|_| "feed must be UTF-8")?;
    let mut reader = NsReader::from_str(document.trim_start_matches('\u{feff}'));
    let mut stack: Vec<Node> = Vec::new();
    let mut format = None;
    let mut closed = false;
    let mut channel_seen = false;
    let mut entry: Option<Entry> = None;
    let mut capture: Option<Capture> = None;
    let mut feed_title = String::new();
    let mut ttl = 900;
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    let mut item_count = 0;
    let mut events = 0;
    loop {
        events += 1;
        if events > 20000 {
            return Err("feed event budget exceeded");
        }
        let (namespace, event) = reader
            .read_resolved_event()
            .map_err(|_| "invalid feed XML")?;
        match event {
            Event::Start(ref tag) | Event::Empty(ref tag) => {
                if stack.len() >= 32 || closed {
                    return Err("invalid feed depth or trailing root");
                }
                let empty = matches!(event, Event::Empty(_));
                let name = tag.local_name().as_ref().to_owned();
                let atom = matches!(namespace, ResolveResult::Bound(ns) if ns.as_ref() == ATOM);
                let plain = matches!(namespace, ResolveResult::Unbound);
                let mut attributes = BTreeMap::new();
                for attribute in tag.attributes() {
                    let attribute = attribute.map_err(|_| "invalid feed attribute")?;
                    let value = attribute
                        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                        .map_err(|_| "invalid feed attribute")?
                        .into_owned();
                    attributes.insert(attribute.key.as_ref().to_owned(), value);
                }
                let base = stack.last().map_or(&source, |node| &node.base);
                let base = if let Some(value) = attributes.get("xml:base") {
                    base.join(value).map_err(|_| "invalid XML base")?
                } else {
                    base.clone()
                };
                if stack.is_empty() {
                    format = Some(
                        if name == "rss"
                            && plain
                            && attributes.get("version").map(String::as_str) == Some("2.0")
                        {
                            Format::Rss
                        } else if name == "feed" && atom {
                            Format::Atom
                        } else {
                            return Err("expected RSS 2.0 or Atom feed");
                        },
                    );
                }
                let relevant = match format {
                    Some(Format::Rss) => plain,
                    Some(Format::Atom) => atom,
                    None => false,
                };
                let rss = format == Some(Format::Rss);
                if relevant && rss && stack.len() == 1 && name == "channel" {
                    if channel_seen {
                        return Err("multiple RSS channels");
                    }
                    channel_seen = true;
                }
                let container = if rss {
                    stack.len() == 2 && stack[1].name == "channel" && stack[1].relevant
                } else {
                    stack.len() == 1
                };
                if relevant && container && name == if rss { "item" } else { "entry" } {
                    item_count += 1;
                    if item_count > 512 {
                        return Err("feed item budget exceeded");
                    }
                    entry = Some(Entry::default());
                }
                let entry_depth = if rss { 3 } else { 2 };
                let in_entry = entry.is_some() && stack.len() == entry_depth;
                let in_feed = if rss {
                    stack.len() == 2 && stack[1].name == "channel" && stack[1].relevant
                } else {
                    stack.len() == 1
                };
                if relevant && capture.is_none() {
                    let target = match name.as_str() {
                        "title" if in_entry => Some(Target::Title),
                        "title" if in_feed => Some(Target::FeedTitle),
                        "link" if in_entry && rss => Some(Target::Link),
                        "pubDate" if in_entry && rss => Some(Target::Published),
                        "published" if in_entry && !rss => Some(Target::Published),
                        "updated" if in_entry && !rss => Some(Target::Updated),
                        "guid" if in_entry && rss => Some(Target::Id),
                        "id" if in_entry && !rss => Some(Target::Id),
                        "ttl" if in_feed && rss => Some(Target::Ttl),
                        _ => None,
                    };
                    if let Some(target) = target {
                        capture = Some(Capture {
                            depth: stack.len() + 1,
                            target,
                            html: rss || attributes.get("type").is_some_and(|kind| kind == "html"),
                            value: String::new(),
                            base: base.clone(),
                        });
                    }
                }
                if !rss
                    && relevant
                    && in_entry
                    && name == "link"
                    && attributes.get("rel").is_none_or(|rel| rel == "alternate")
                    && let Some(href) = attributes.get("href")
                {
                    let item = entry.as_mut().unwrap();
                    if item.link.is_none() {
                        item.link = link(&base, href);
                    }
                }
                stack.push(Node {
                    name,
                    base,
                    relevant,
                });
                if empty {
                    finish(
                        &mut stack,
                        &mut capture,
                        &mut entry,
                        &mut feed_title,
                        &mut ttl,
                        &mut entries,
                        &mut seen,
                        settings.max_items,
                        format.unwrap(),
                    )?;
                    if stack.is_empty() {
                        closed = true;
                    }
                }
            }
            Event::End(_) => {
                finish(
                    &mut stack,
                    &mut capture,
                    &mut entry,
                    &mut feed_title,
                    &mut ttl,
                    &mut entries,
                    &mut seen,
                    settings.max_items,
                    format.ok_or("missing root")?,
                )?;
                if stack.is_empty() {
                    closed = true;
                }
            }
            Event::Text(text) => {
                let text = text.xml10_content();
                if stack.is_empty() && !text.trim().is_empty() {
                    return Err("text outside feed");
                }
                append(&mut capture, &text)?;
            }
            Event::CData(text) => {
                if stack.is_empty() {
                    return Err("CDATA outside root");
                }
                append(&mut capture, &text.xml10_content())?;
            }
            Event::GeneralRef(reference) => {
                let text = reference.xml10_content();
                let value = if let Some(ch) = reference
                    .resolve_char_ref()
                    .map_err(|_| "invalid XML reference")?
                {
                    ch.to_string()
                } else {
                    match text.as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => return Err("unknown XML entity"),
                    }
                    .into()
                };
                if stack.is_empty() {
                    return Err("reference outside feed");
                }
                append(&mut capture, &value)?;
            }
            Event::DocType(_) => return Err("DTD is not supported"),
            Event::Decl(declaration) => {
                if format.is_some()
                    || !declaration
                        .version()
                        .is_ok_and(|version| version.as_ref() == "1.0")
                {
                    return Err("unsupported XML declaration");
                }
                if declaration.encoding().is_some_and(|value| {
                    !value.is_ok_and(|encoding| {
                        encoding.eq_ignore_ascii_case("utf-8")
                            || encoding.eq_ignore_ascii_case("utf8")
                    })
                }) {
                    return Err("feed must declare UTF-8");
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !closed
        || !stack.is_empty()
        || format.is_none()
        || (format == Some(Format::Rss) && !channel_seen)
        || (item_count > 0 && entries.is_empty())
    {
        return Err("feed structure or headlines unavailable");
    }
    let feed_title = if feed_title.is_empty() {
        settings.label
    } else {
        feed_title
    };
    let headline_list = entries
        .iter()
        .map(|item: &Entry| format!("• {}", item.title))
        .collect::<Vec<_>>()
        .join("\n");
    let headlines = format!("{feed_title}\n{headline_list}");
    let rows = entries
        .into_iter()
        .map(|item| {
            BTreeMap::from([
                ("title".into(), json!(item.title)),
                ("link".into(), json!(item.link)),
                ("published".into(), json!(item.published)),
                ("feed-title".into(), json!(feed_title)),
                ("headlines".into(), json!(headlines)),
                ("source".into(), json!(settings.feed_url)),
            ])
        })
        .collect();
    let dataset = Dataset {
        schema_version: 1,
        id: "headlines".into(),
        fields: [
            "title",
            "link",
            "published",
            "feed-title",
            "headlines",
            "source",
        ]
        .into_iter()
        .map(|id| Field {
            id: id.into(),
            kind: FieldKind::Text,
            unit: None,
        })
        .collect(),
        rows,
    };
    let publication = Publication {
        additional_observed_at: Default::default(),
        dataset,
        observed_at: None,
        min_poll_secs: Some(ttl),
        additional_datasets: vec![],
    };
    publication
        .validate()
        .map_err(|_| "feed output exceeded data limits")?;
    Ok(publication)
}
fn append(capture: &mut Option<Capture>, text: &str) -> Result<()> {
    if let Some(capture) = capture {
        if capture.value.len() + text.len() > 16384 {
            return Err("feed text budget exceeded");
        }
        capture.value.push_str(text);
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn finish(
    stack: &mut Vec<Node>,
    capture: &mut Option<Capture>,
    entry: &mut Option<Entry>,
    feed_title: &mut String,
    ttl: &mut u32,
    entries: &mut Vec<Entry>,
    seen: &mut BTreeSet<String>,
    limit: usize,
    format: Format,
) -> Result<()> {
    if capture
        .as_ref()
        .is_some_and(|capture| capture.depth == stack.len())
    {
        let capture = capture.take().unwrap();
        let value = clean(&capture.value, capture.html, 512);
        match capture.target {
            Target::FeedTitle => *feed_title = truncate(&value, 128),
            Target::Title => entry.as_mut().ok_or("title outside entry")?.title = value,
            Target::Link => {
                entry.as_mut().ok_or("link outside entry")?.link =
                    link(&capture.base, capture.value.trim())
            }
            Target::Id => {
                entry.as_mut().ok_or("ID outside entry")?.id = truncate(capture.value.trim(), 1024)
            }
            Target::Published => {
                entry.as_mut().ok_or("date outside entry")?.published =
                    Some(truncate(capture.value.trim(), 128))
            }
            Target::Updated => {
                let entry = entry.as_mut().ok_or("date outside entry")?;
                if entry.published.is_none() {
                    entry.published = Some(truncate(capture.value.trim(), 128));
                }
            }
            Target::Ttl => {
                let minutes: u32 = capture
                    .value
                    .trim()
                    .parse()
                    .map_err(|_| "invalid feed TTL")?;
                *ttl = minutes
                    .checked_mul(60)
                    .filter(|seconds| *seconds <= 604800)
                    .ok_or("feed TTL exceeds supported interval")?
                    .max(900);
            }
        }
    }
    let node = stack.pop().ok_or("unbalanced feed")?;
    let item_depth = if format == Format::Rss { 2 } else { 1 };
    if node.relevant && stack.len() == item_depth && (node.name == "item" || node.name == "entry") {
        let item = entry.take().ok_or("invalid entry")?;
        if !item.title.is_empty() {
            let identity = if !item.id.is_empty() {
                item.id.clone()
            } else {
                item.link.clone().unwrap_or_else(|| item.title.clone())
            };
            if seen.insert(identity) && entries.len() < limit {
                entries.push(item);
            }
        }
    }
    Ok(())
}
fn link(base: &Url, value: &str) -> Option<String> {
    let url = base.join(value).ok()?;
    (matches!(url.scheme(), "http" | "https")
        && url.username().is_empty()
        && url.password().is_none()
        && url.as_str().len() <= 2048)
        .then(|| url.into())
}
fn truncate(value: &str, max: usize) -> String {
    let mut end = value.len().min(max);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].into()
}
fn clean(value: &str, html: bool, max: usize) -> String {
    let text = if html {
        let mut output = String::new();
        let mut chars = value.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '<'
                && chars
                    .peek()
                    .is_some_and(|ch| ch.is_ascii_alphabetic() || matches!(ch, '/' | '!'))
            {
                let mut quote = None;
                for ch in chars.by_ref() {
                    if quote == Some(ch) {
                        quote = None;
                    } else if quote.is_none() && matches!(ch, '\'' | '"') {
                        quote = Some(ch);
                    } else if ch == '>' && quote.is_none() {
                        break;
                    }
                }
            } else {
                output.push(ch);
            }
        }
        html_escape::decode_html_entities(&output).into_owned()
    } else {
        value.to_owned()
    };
    truncate(&text.split_whitespace().collect::<Vec<_>>().join(" "), max)
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn run() -> i32 {
    fn fetch() -> std::result::Result<(), ()> {
        let settings = paperphase_plugin_sdk::settings().map_err(|_| ())?;
        let source = paperphase_plugin_sdk::source_url().map_err(|_| ())?;
        let bytes = paperphase_plugin_sdk::http_get(&source).map_err(|_| ())?;
        let publication = convert(&bytes, &settings).map_err(|_| ())?;
        paperphase_plugin_sdk::publish_with_metadata(&publication).map_err(|_| ())
    }
    if fetch().is_ok() { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn settings() -> SettingsValues {
        serde_json::from_value(
            json!({"label":"My feed","feed_url":"https://example.org/feed.xml","max_items":3}),
        )
        .unwrap()
    }
    #[test]
    fn rss_titles_relative_links_plain_text_and_ttl_are_preserved() {
        let publication =
            convert(include_bytes!("../fixtures/headlines.xml"), &settings()).unwrap();
        assert_eq!(publication.min_poll_secs, Some(3600));
        assert_eq!(publication.dataset.rows.len(), 3);
        assert_eq!(
            publication.dataset.rows[0]["link"],
            "https://example.org/stories/trail"
        );
        assert_eq!(
            publication.dataset.rows[1]["title"],
            "Community garden welcomes new volunteers"
        );
        assert!(
            publication.dataset.rows[0]["headlines"]
                .as_str()
                .unwrap()
                .contains("• Local library")
        );
    }
    #[test]
    fn atom_namespaces_html_xhtml_xml_base_and_alternate_links_work() {
        let publication = convert(include_bytes!("../fixtures/atom.xml"), &settings()).unwrap();
        assert_eq!(publication.dataset.rows[0]["title"], "Tea & cake");
        assert_eq!(
            publication.dataset.rows[0]["link"],
            "https://example.org/blog/one"
        );
        assert_eq!(
            publication.dataset.rows[0]["published"],
            "2023-11-14T19:00:00Z"
        );
        assert_eq!(publication.dataset.rows[1]["title"], "A quiet afternoon");
        assert!(publication.dataset.rows[1]["link"].is_null());
    }
    #[test]
    fn duplicate_items_and_limits_preserve_feed_order_and_empty_is_successful() {
        let xml = r#"<rss version="2.0"><channel><title>Empty</title></channel></rss>"#;
        assert!(
            convert(xml.as_bytes(), &settings())
                .unwrap()
                .dataset
                .rows
                .is_empty()
        );
        let xml = r#"<rss version="2.0"><channel><item><title>First</title><guid>1</guid></item><item><title>Duplicate</title><guid>1</guid></item><item><title>Second</title><guid>2</guid></item></channel></rss>"#;
        let mut values = settings();
        values.insert("max_items".into(), json!(1));
        let dataset = convert(xml.as_bytes(), &values).unwrap().dataset;
        assert_eq!(dataset.rows.len(), 1);
        assert_eq!(dataset.rows[0]["title"], "First");
    }
    #[test]
    fn malformed_oversized_deep_and_entity_defining_feeds_fail() {
        let deep = format!(
            "<rss version=\"2.0\"><channel>{}{}</channel></rss>",
            "<a>".repeat(40),
            "</a>".repeat(40)
        );
        for xml in [
            "<html>Not a feed</html>",
            "<rss version=\"2.0\"><channel>",
            "<!DOCTYPE rss [<!ENTITY secret SYSTEM 'file:///etc/passwd'>]><rss version=\"2.0\"><channel/></rss>",
            "<rss version=\"2.0\"><channel><item><title>&unknown;</title></item></channel></rss>",
            &deep,
        ] {
            assert!(convert(xml.as_bytes(), &settings()).is_err(), "{xml}");
        }
        assert!(convert(&vec![b' '; 65537], &settings()).is_err());
        assert!(convert(b"\xff", &settings()).is_err());
    }
}
