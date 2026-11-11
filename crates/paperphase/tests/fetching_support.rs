use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

pub struct Observation {
    pub url: String,
    pub reply: Arc<Mutex<(u16, Option<u64>, Value)>>,
    pub hits: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Observation {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/observation", listener.local_addr().unwrap());
        let reply = Arc::new(Mutex::new((200, None, json!({"temperature_c":18.5}))));
        let hits = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let response = Arc::clone(&reply);
        let count = Arc::clone(&hits);
        let done = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
                        let mut request = [0; 8192];
                        let _ = stream.read(&mut request);
                        let (status, retry, body) = response.lock().unwrap().clone();
                        count.fetch_add(1, Ordering::SeqCst);
                        let body = serde_json::to_vec(&body).unwrap();
                        let retry = retry.map_or(String::new(), |seconds| {
                            format!("Retry-After: {seconds}\r\n")
                        });
                        let _ = write!(
                            stream,
                            "HTTP/1.1 {status} Reply\r\n{retry}Content-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
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
            hits,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Observation {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.worker.take().unwrap().join();
    }
}

pub fn registration(dir: &std::path::Path, url: &str) -> std::path::PathBuf {
    let module = std::env::var("PAPERPHASE_TEST_PLUGIN")
        .expect("build the compiled guest using scripts/check.sh");
    let file = dir.join("bindings.json");
    std::fs::write(&file,serde_json::to_vec(&json!({"schema_version":1,"bindings":[{"connector_id":"example.temperature","wasm":std::path::absolute(module).unwrap(),"source_url":url,"stale_after_secs":60}]})).unwrap()).unwrap();
    file
}
