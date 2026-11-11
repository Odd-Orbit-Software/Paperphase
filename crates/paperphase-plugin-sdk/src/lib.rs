//! Experimental guest bindings for the foundation harness, not a stable SDK.

pub use paperphase_core::SettingsValues;
pub use paperphase_core::{Dataset, Field, FieldKind, Publication};

#[derive(Debug)]
pub enum Error {
    UnsupportedTarget,
    InvalidResponse,
    InvalidDataset(paperphase_core::ValidationError),
}

/// Serialize the same typed dataset contract the host and renderer validate.
pub fn publish_dataset(dataset: &Dataset) -> Result<(), Error> {
    dataset.validate().map_err(Error::InvalidDataset)?;
    let bytes = serde_json::to_vec(dataset).map_err(|_| Error::InvalidResponse)?;
    publish(&bytes)
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "paperphase_v0")]
unsafe extern "C" {
    #[link_name = "source_url"]
    fn host_source_url(ptr: i32, capacity: i32) -> i32;
    #[link_name = "settings_json"]
    fn host_settings_json(ptr: i32, capacity: i32) -> i32;
    #[link_name = "reference_lookup"]
    fn host_reference_lookup(
        table_ptr: i32,
        table_len: i32,
        key_ptr: i32,
        key_len: i32,
        out_ptr: i32,
        capacity: i32,
    ) -> i32;
    #[link_name = "http_get"]
    fn host_http_get(url_ptr: i32, url_len: i32, out_ptr: i32, capacity: i32) -> i32;
    #[link_name = "publish"]
    fn host_publish(ptr: i32, len: i32) -> i32;
    #[link_name = "publish_with_metadata"]
    fn host_publish_with_metadata(ptr: i32, len: i32) -> i32;
}

pub fn settings() -> Result<SettingsValues, Error> {
    #[cfg(target_arch = "wasm32")]
    {
        let mut bytes = vec![0; 16 * 1024];
        // SAFETY: The host receives an allocated writable buffer of this size.
        let length = unsafe { host_settings_json(bytes.as_mut_ptr() as i32, bytes.len() as i32) };
        let length = usize::try_from(length).map_err(|_| Error::InvalidResponse)?;
        if length > bytes.len() {
            return Err(Error::InvalidResponse);
        }
        serde_json::from_slice(&bytes[..length]).map_err(|_| Error::InvalidResponse)
    }
    #[cfg(not(target_arch = "wasm32"))]
    Err(Error::UnsupportedTarget)
}

pub fn source_url() -> Result<String, Error> {
    #[cfg(target_arch = "wasm32")]
    {
        let mut bytes = vec![0; 8192];
        // SAFETY: The host receives an allocated writable buffer of this size.
        let length = unsafe { host_source_url(bytes.as_mut_ptr() as i32, bytes.len() as i32) };
        let length = usize::try_from(length).map_err(|_| Error::InvalidResponse)?;
        if length > bytes.len() {
            return Err(Error::InvalidResponse);
        }
        bytes.truncate(length);
        String::from_utf8(bytes).map_err(|_| Error::InvalidResponse)
    }
    #[cfg(not(target_arch = "wasm32"))]
    Err(Error::UnsupportedTarget)
}

pub fn http_get(url: &str) -> Result<Vec<u8>, Error> {
    http_get_bounded(url, 64 * 1024)
}

/// Larger responses require a matching host registration; default remains 64 KiB.
pub fn http_get_bounded(url: &str, capacity: usize) -> Result<Vec<u8>, Error> {
    if !(1..=8 * 1024 * 1024).contains(&capacity) {
        return Err(Error::InvalidResponse);
    }
    #[cfg(target_arch = "wasm32")]
    {
        let length = i32::try_from(url.len()).map_err(|_| Error::InvalidResponse)?;
        let mut bytes = vec![0; capacity];
        // SAFETY: Both buffers remain allocated for the duration of the host call.
        let received = unsafe {
            host_http_get(
                url.as_ptr() as i32,
                length,
                bytes.as_mut_ptr() as i32,
                bytes.len() as i32,
            )
        };
        let received = usize::try_from(received).map_err(|_| Error::InvalidResponse)?;
        if received > bytes.len() {
            return Err(Error::InvalidResponse);
        }
        bytes.truncate(received);
        Ok(bytes)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = url;
        Err(Error::UnsupportedTarget)
    }
}

pub fn publish(json: &[u8]) -> Result<(), Error> {
    #[cfg(target_arch = "wasm32")]
    {
        let length = i32::try_from(json.len()).map_err(|_| Error::InvalidResponse)?;
        // SAFETY: The byte slice remains valid for the complete host call.
        let status = unsafe { host_publish(json.as_ptr() as i32, length) };
        if status == 0 {
            Ok(())
        } else {
            Err(Error::InvalidResponse)
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = json;
        Err(Error::UnsupportedTarget)
    }
}

/// Publish up to eight datasets atomically, with observation and polling hints.
pub fn publish_with_metadata(publication: &Publication) -> Result<(), Error> {
    publication.validate().map_err(Error::InvalidDataset)?;
    #[cfg(target_arch = "wasm32")]
    {
        let bytes = serde_json::to_vec(publication).map_err(|_| Error::InvalidResponse)?;
        let length = i32::try_from(bytes.len()).map_err(|_| Error::InvalidResponse)?;
        // SAFETY: The serialized buffer remains allocated throughout the host call.
        let status = unsafe { host_publish_with_metadata(bytes.as_ptr() as i32, length) };
        if status == 0 {
            Ok(())
        } else {
            Err(Error::InvalidResponse)
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    Err(Error::UnsupportedTarget)
}

/// Shared clock formatting for location-local forecast labels.
pub mod clock {
    pub use paperphase_core::{ClockContent, ClockSpec, DateFormat, HourCycle};
}

/// Read a named, host-granted immutable table. No guest paths or SQL are accepted.
pub fn reference_lookup(table: &str, key: &str) -> Result<Option<Vec<String>>, Error> {
    #[cfg(target_arch = "wasm32")]
    {
        if table.len() > 64 || key.len() > 192 {
            return Err(Error::InvalidResponse);
        }
        let mut bytes = vec![0; 4096];
        // SAFETY: Input slices and the allocated output buffer live through the call.
        let len = unsafe {
            host_reference_lookup(
                table.as_ptr() as i32,
                table.len() as i32,
                key.as_ptr() as i32,
                key.len() as i32,
                bytes.as_mut_ptr() as i32,
                bytes.len() as i32,
            )
        };
        let len = usize::try_from(len).map_err(|_| Error::InvalidResponse)?;
        if len == 0 {
            return Ok(None);
        }
        if len > bytes.len() {
            return Err(Error::InvalidResponse);
        }
        serde_json::from_slice(&bytes[..len])
            .map(Some)
            .map_err(|_| Error::InvalidResponse)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (table, key);
        Err(Error::UnsupportedTarget)
    }
}
