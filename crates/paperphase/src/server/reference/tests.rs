use super::*;
use std::{
    net::TcpListener,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    thread,
};
const ZIP: &[u8] = include_bytes!("../../../../../plugins/dublin-bus/fixtures/reference.zip");
struct Reply {
    body: Vec<u8>,
    etag: String,
    ignore_ranges: bool,
}
struct Server {
    url: String,
    reply: Arc<Mutex<Reply>>,
    ranges: Arc<AtomicUsize>,
    credentials: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/reference.zip", listener.local_addr().unwrap());
        let reply = Arc::new(Mutex::new(Reply {
            body: ZIP.to_vec(),
            etag: "\"fixture1\"".into(),
            ignore_ranges: false,
        }));
        let shared = Arc::clone(&reply);
        let ranges = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&ranges);
        let credentials = Arc::new(AtomicBool::new(false));
        let leaked = Arc::clone(&credentials);
        let done = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&done);
        let worker = thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        stream
                            .set_write_timeout(Some(Duration::from_secs(1)))
                            .unwrap();
                        let mut buf = [0; 8192];
                        let n = stream.read(&mut buf).unwrap();
                        let request = String::from_utf8_lossy(&buf[..n]).to_lowercase();
                        if request.contains("x-api-key:") {
                            leaked.store(true, Ordering::SeqCst);
                        }
                        let reply = shared.lock().unwrap();
                        let size = reply.body.len();
                        let (status, body, extra) = if request.starts_with("head") {
                            if request.contains(&format!("if-none-match: {}", reply.etag)) {
                                ("304 Not Modified", Vec::new(), String::new())
                            } else {
                                ("200 OK", Vec::new(), String::new())
                            }
                        } else if !reply.ignore_ranges {
                            count.fetch_add(1, Ordering::SeqCst);
                            let range = request
                                .lines()
                                .find_map(|line| line.strip_prefix("range: bytes="))
                                .unwrap();
                            let (first, last) = range.split_once('-').unwrap();
                            let first = first.parse::<usize>().unwrap();
                            let last = last.parse::<usize>().unwrap();
                            (
                                "206 Partial Content",
                                reply.body[first..=last].to_vec(),
                                format!("Content-Range: bytes {first}-{last}/{size}\r\n"),
                            )
                        } else {
                            ("200 OK", reply.body.clone(), String::new())
                        };
                        let length = if request.starts_with("head") {
                            size
                        } else {
                            body.len()
                        };
                        let header = format!(
                            "HTTP/1.1 {status}\r\nContent-Length: {length}\r\nETag: {}\r\n{extra}Connection: close\r\n\r\n",
                            reply.etag
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(&body);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1))
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            url,
            reply,
            ranges,
            credentials,
            done,
            worker: Some(worker),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
        self.worker.take().unwrap().join().unwrap();
    }
}
fn spec(url: &str) -> Archive {
    serde_json::from_value(serde_json::json!({"url":url,"cache":"reference.json","refresh_secs":3600,"tables":[
 {"id":"stops","file":"stops.txt","key":"stop_code","fields":["stop_id","stop_name"],"filter":{"column":"stop_id","contains":"DB"}},
 {"id":"routes","file":"routes.txt","key":"route_id","fields":["route_short_name"]},
 {"id":"trips","file":"trips.txt","key":"trip_id","fields":["route_id","trip_headsign"]},
 {"id":"feed","file":"feed_info.txt","key":"feed_version","fields":["feed_version"]}
 ]})).unwrap()
}
fn client() -> Client {
    paperphase_runtime::PluginRuntime::new(Default::default())
        .unwrap()
        .reference_client()
}
#[test]
fn range_import_filters_csv_and_reuses_conditional_and_durable_cache() {
    let server = Server::new();
    let dir = tempfile::tempdir().unwrap();
    let source = Source::new(spec(&server.url), dir.path()).unwrap();
    assert!(source.snapshot().is_none());
    source.update(&client()).unwrap();
    let data = source.snapshot().unwrap();
    assert_eq!(
        data.get("stops", "2813"),
        Some("[\"8220DB002813\",\"Ranelagh Road\"]")
    );
    assert!(data.get("stops", "999238").is_none());
    assert!(data.get("stops", "9999").is_some());
    assert!(
        data.get("trips", "trip-b")
            .unwrap()
            .contains("Dún Laoghaire")
    );
    let ranges = server.ranges.load(Ordering::SeqCst);
    assert_eq!(ranges, 10);
    source.update(&client()).unwrap();
    assert_eq!(server.ranges.load(Ordering::SeqCst), ranges);
    assert!(!server.credentials.load(Ordering::SeqCst));
    let restored = Source::new(spec(&server.url), dir.path()).unwrap();
    assert_eq!(restored.snapshot().unwrap().rows(), data.rows());
}
#[test]
fn failed_refresh_retains_reference_snapshot_and_disk_cache() {
    let server = Server::new();
    let dir = tempfile::tempdir().unwrap();
    let source = Source::new(spec(&server.url), dir.path()).unwrap();
    source.update(&client()).unwrap();
    let old = source.snapshot().unwrap();
    let disk = std::fs::read(dir.path().join("reference.json")).unwrap();
    {
        let mut reply = server.reply.lock().unwrap();
        reply.etag = "\"fixture2\"".into();
        reply.ignore_ranges = true;
    }
    assert!(source.update(&client()).is_err());
    assert!(Arc::ptr_eq(&old, &source.snapshot().unwrap()));
    assert_eq!(
        std::fs::read(dir.path().join("reference.json")).unwrap(),
        disk
    );
    {
        let mut reply = server.reply.lock().unwrap();
        reply.ignore_ranges = false;
        reply.body[40] ^= 0x40;
    }
    assert!(source.update(&client()).is_err());
    assert!(Arc::ptr_eq(&old, &source.snapshot().unwrap()));
}
#[test]
fn malformed_config_and_archive_directory_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mut value = spec("https://example.org/data.zip");
    value.refresh_secs = 1;
    assert!(Source::new(value, dir.path()).is_err());
    assert!(directory_entries(b"bad", 1).is_err());
    let mut broken = ZIP.to_vec();
    let at = broken.windows(4).position(|p| p == b"PK\x01\x02").unwrap();
    broken[at + 34] = 1;
    let end = broken.windows(4).rposition(|p| p == b"PK\x05\x06").unwrap();
    let size = u32_at(&broken, end + 12).unwrap() as usize;
    let start = u32_at(&broken, end + 16).unwrap() as usize;
    assert!(
        directory_entries(
            &broken[start..start + size],
            u16_at(&broken, end + 10).unwrap()
        )
        .is_err()
    );
}

#[test]
fn combined_reference_memory_admission_preserves_the_previous_reservation() {
    let budget = Budget::default();
    budget.replace(0, 40 * 1024 * 1024).unwrap();
    assert!(budget.replace(0, 30 * 1024 * 1024).is_err());
    budget.replace(40 * 1024 * 1024, 32 * 1024 * 1024).unwrap();
    budget.replace(0, 32 * 1024 * 1024).unwrap();
    assert!(budget.replace(0, 1).is_err());
    budget.replace(32 * 1024 * 1024, 0).unwrap();
    budget.replace(0, 1).unwrap();
}

#[test]
fn selected_stop_schedules_stream_and_reimport_for_changed_stops_with_unchanged_etag() {
    let server = Server::new();
    let dir = tempfile::tempdir().unwrap();
    let mut spec = spec(&server.url);
    spec.selection_setting = Some("stop_numbers".into());
    spec.tables.push(serde_json::from_value(serde_json::json!({"id":"times","file":"stop_times.txt","key":"stop_id","page_size":1,"group":true,"fields":["trip_id","stop_sequence","arrival_time","departure_time","pickup_type"],"streaming":true,"select":{"column":"stop_id","table":"stops","value_index":0}})).unwrap());
    let source = Source::new(spec.clone(), dir.path()).unwrap();
    let settings =
        |stops: &str| serde_json::from_value(serde_json::json!({"stop_numbers":stops})).unwrap();
    source.select(source.selections(&settings("002813")));
    source.update(&client()).unwrap();
    assert!(source.ready_for(&settings("2813")));
    assert!(!source.ready_for(&settings("2")));
    assert!(
        source
            .snapshot()
            .unwrap()
            .get("times", "8220DB002813|0")
            .unwrap()
            .contains("trip-delay")
    );
    assert!(
        source
            .snapshot()
            .unwrap()
            .get("times", "8220DB000002|0")
            .is_none()
    );
    assert!(
        source
            .snapshot()
            .unwrap()
            .get("times", "8220DB002813|1")
            .unwrap()
            .contains("trip-scheduled")
    );
    let ranges = server.ranges.load(Ordering::SeqCst);
    source.select(source.selections(&settings("2813,2")));
    assert!(source.ready_for(&settings("2813")));
    source.update(&client()).unwrap();
    assert!(server.ranges.load(Ordering::SeqCst) > ranges);
    assert!(source.ready_for(&settings("2")));
    assert!(
        source
            .snapshot()
            .unwrap()
            .get("times", "8220DB000002|0")
            .is_some()
    );
    let restored = Source::new(spec, dir.path()).unwrap();
    assert!(restored.ready_for(&settings("2813,2")));
    assert!(!server.credentials.load(Ordering::SeqCst));
}
