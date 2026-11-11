//! Borrowed GTFS-Realtime protobuf traversal: never materialise the national feed.
use crate::Result;
#[derive(Clone, Copy)]
pub enum Value<'a> {
    Number(u64),
    Bytes(&'a [u8]),
    Other,
}
impl<'a> Value<'a> {
    pub fn number(self) -> Result<u64> {
        if let Self::Number(value) = self {
            Ok(value)
        } else {
            Err("unexpected protobuf wire type")
        }
    }
    pub fn bytes(self) -> Result<&'a [u8]> {
        if let Self::Bytes(value) = self {
            Ok(value)
        } else {
            Err("unexpected protobuf wire type")
        }
    }
    pub fn text(self) -> Result<&'a str> {
        let bytes = self.bytes()?;
        if bytes.len() > 256 {
            return Err("protobuf string exceeds budget");
        }
        std::str::from_utf8(bytes).map_err(|_| "invalid protobuf text")
    }
}
pub struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
    fn varint(&mut self) -> Result<u64> {
        let mut value = 0;
        for shift in (0..70).step_by(7) {
            let byte = *self
                .bytes
                .get(self.at)
                .ok_or("truncated protobuf integer")?;
            self.at += 1;
            if shift == 63 && byte > 1 {
                return Err("protobuf integer overflow");
            }
            value |= u64::from(byte & 127) << shift;
            if byte & 128 == 0 {
                return Ok(value);
            }
        }
        Err("invalid protobuf integer")
    }
    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(len).ok_or("protobuf length overflow")?;
        let bytes = self
            .bytes
            .get(self.at..end)
            .ok_or("truncated protobuf data")?;
        self.at = end;
        Ok(bytes)
    }
    pub fn next(&mut self) -> Result<Option<(u32, Value<'a>)>> {
        if self.at == self.bytes.len() {
            return Ok(None);
        }
        let tag = self.varint()?;
        let field = u32::try_from(tag >> 3).map_err(|_| "invalid protobuf field")?;
        if field == 0 || field > 536_870_911 {
            return Err("invalid protobuf field");
        }
        let value = match tag & 7 {
            0 => Value::Number(self.varint()?),
            1 => {
                self.take(8)?;
                Value::Other
            }
            2 => {
                let len = usize::try_from(self.varint()?).map_err(|_| "invalid protobuf length")?;
                Value::Bytes(self.take(len)?)
            }
            5 => {
                self.take(4)?;
                Value::Other
            }
            _ => return Err("unsupported protobuf wire type"),
        };
        Ok(Some((field, value)))
    }
}
pub struct Header<'a> {
    pub timestamp: u64,
    pub version: Option<&'a str>,
}
pub fn header(bytes: &[u8]) -> Result<Header<'_>> {
    let mut root = Reader::new(bytes);
    let mut header = None;
    let mut count = 0;
    while let Some((field, value)) = root.next()? {
        match field {
            1 => {
                if header.is_some() {
                    return Err("duplicate feed header");
                }
                header = Some(value.bytes()?);
            }
            2 => {
                value.bytes()?;
                count += 1;
                if count > 20000 {
                    return Err("too many feed entities");
                }
            }
            _ => {}
        }
    }
    let mut reader = Reader::new(header.ok_or("missing feed header")?);
    let mut at = None;
    let mut version = None;
    let mut format = None;
    let mut mode = 0;
    while let Some((field, value)) = reader.next()? {
        match field {
            1 => format = Some(value.text()?),
            2 => mode = value.number()?,
            3 => at = Some(value.number()?),
            4 => version = Some(value.text()?),
            _ => {}
        }
    }
    if mode != 0 || !matches!(format, Some("1.0" | "2.0")) {
        return Err("unsupported GTFS feed version or differential data");
    }
    let timestamp = at
        .filter(|at| *at > 0 && *at <= i64::MAX as u64)
        .ok_or("invalid feed timestamp")?;
    Ok(Header { timestamp, version })
}
pub struct Trip<'a> {
    pub id: &'a str,
    pub date: &'a str,
    pub route: &'a str,
    pub relationship: u64,
    pub observed: Option<u64>,
}
pub fn trip(bytes: &[u8]) -> Result<Trip<'_>> {
    let mut update = Reader::new(bytes);
    let mut descriptor = None;
    let mut observed = None;
    while let Some((field, value)) = update.next()? {
        match field {
            1 => descriptor = Some(value.bytes()?),
            4 => observed = Some(value.number()?),
            _ => {}
        }
    }
    let mut reader = Reader::new(descriptor.ok_or("missing trip descriptor")?);
    let mut id = None;
    let mut route = "";
    let mut date = "";
    let mut relationship = 0;
    while let Some((field, value)) = reader.next()? {
        match field {
            1 => id = Some(value.text()?),
            3 => date = value.text()?,
            4 => relationship = value.number()?,
            5 => route = value.text()?,
            _ => {}
        }
    }
    let id = id.unwrap_or("");
    Ok(Trip {
        id,
        date,
        route,
        relationship,
        observed: observed.filter(|at| *at > 0 && *at <= i64::MAX as u64),
    })
}
pub struct StopTime<'a> {
    pub stop: &'a str,
    pub time: Option<u64>,
    pub relationship: u64,
    pub sequence: Option<u64>,
    pub delay: Option<i32>,
    pub departure: bool,
}
#[derive(Default)]
struct Event {
    time: Option<u64>,
    delay: Option<i32>,
}
fn event(bytes: &[u8]) -> Result<Event> {
    let mut reader = Reader::new(bytes);
    let mut event = Event::default();
    while let Some((field, value)) = reader.next()? {
        if field == 1 {
            event.delay = Some(value.number()? as i32);
        } else if field == 2 {
            let value = value.number()?;
            event.time = (value > 0 && value <= i64::MAX as u64).then_some(value);
        }
    }
    Ok(event)
}
pub fn stop_time(bytes: &[u8]) -> Result<StopTime<'_>> {
    let mut reader = Reader::new(bytes);
    let mut stop = "";
    let mut arrival = Event::default();
    let mut departure = Event::default();
    let mut sequence = None;
    let mut relationship = 0;
    while let Some((field, value)) = reader.next()? {
        match field {
            1 => sequence = Some(value.number()?),
            2 => arrival = event(value.bytes()?)?,
            3 => departure = event(value.bytes()?)?,
            4 => stop = value.text()?,
            5 => relationship = value.number()?,
            _ => {}
        }
    }
    Ok(StopTime {
        stop,
        time: departure.time.or(arrival.time),
        delay: departure.delay.or(arrival.delay),
        departure: departure.delay.is_some(),
        sequence,
        relationship,
    })
}
