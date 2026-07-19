//! Minimal HTTP/1.1 + WebSocket transport for the Chrome DevTools inspector.
//!
//! The DevTools surface myco exposes is small and fixed: three JSON GETs and a
//! single WebSocket upgrade carrying CDP messages. That does not justify a web
//! framework, so it is implemented directly on tokio. Only the parts of
//! RFC 6455 that a DevTools client actually exercises are implemented.

use std::io;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use sha1::{Digest, Sha1};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;

/// Cap on the request head, so a client cannot make us buffer without bound
/// while we wait for the terminating CRLFCRLF.
const MAX_HEAD_BYTES: usize = 16 * 1024;

/// Cap on a single WebSocket message. CDP payloads (script sources, heap
/// snapshot chunks) can be large, so this is generous, but still bounded.
const MAX_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;

/// The magic value from RFC 6455 §4.2.2 used to derive `Sec-WebSocket-Accept`.
const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

const CORS_HEADERS: &str = "Access-Control-Allow-Origin: *\r\n\
     Access-Control-Allow-Headers: content-type\r\n\
     Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n";

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

pub struct Request {
    pub method: String,
    pub path: String,
    headers: Vec<(String, String)>,
}

impl Request {
    /// Case-insensitive header lookup; HTTP field names are not case sensitive.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// True when this is a well-formed WebSocket upgrade request.
    pub fn is_websocket_upgrade(&self) -> bool {
        let connection_upgrade = self
            .header("connection")
            .map(|v| v.to_ascii_lowercase().contains("upgrade"))
            .unwrap_or(false);
        let upgrade_websocket = self
            .header("upgrade")
            .map(|v| v.eq_ignore_ascii_case("websocket"))
            .unwrap_or(false);
        connection_upgrade && upgrade_websocket && self.header("sec-websocket-key").is_some()
    }
}

/// Reads and parses a request head. Returns the request along with any bytes
/// already read past the head — for a WebSocket upgrade those are the first
/// frame bytes, and dropping them would corrupt the stream.
pub async fn read_request(stream: &mut TcpStream) -> io::Result<Option<(Request, Vec<u8>)>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];

    loop {
        if let Some(end) = find_head_end(&buf) {
            let head = String::from_utf8_lossy(&buf[..end]).into_owned();
            let leftover = buf[end + 4..].to_vec();
            return Ok(parse_head(&head).map(|req| (req, leftover)));
        }
        if buf.len() > MAX_HEAD_BYTES {
            return Ok(None);
        }
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            // Peer closed before sending a complete head.
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

fn parse_head(head: &str) -> Option<Request> {
    let mut lines = head.split("\r\n");

    let mut request_line = lines.next()?.split_whitespace();
    let method = request_line.next()?.to_string();
    let path = request_line.next()?.to_string();

    let headers = lines
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.trim().to_string(), value.trim().to_string()))
        })
        .collect();

    Some(Request {
        method,
        path,
        headers,
    })
}

pub fn json_response(body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         {CORS_HEADERS}\
         Connection: close\r\n\
         \r\n\
         {body}",
        body.len()
    )
    .into_bytes()
}

pub fn empty_response(status: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status}\r\n\
         Content-Length: 0\r\n\
         {CORS_HEADERS}\
         Connection: close\r\n\
         \r\n"
    )
    .into_bytes()
}

// ---------------------------------------------------------------------------
// WebSocket handshake
// ---------------------------------------------------------------------------

/// Derives `Sec-WebSocket-Accept` per RFC 6455 §4.2.2: base64(SHA1(key + GUID)).
fn accept_key(client_key: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(client_key.as_bytes());
    hasher.update(WS_GUID.as_bytes());
    STANDARD.encode(hasher.finalize())
}

pub async fn write_handshake(stream: &mut TcpStream, client_key: &str) -> io::Result<()> {
    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {}\r\n\
         \r\n",
        accept_key(client_key)
    );
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await
}

// ---------------------------------------------------------------------------
// WebSocket framing
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum Frame {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close,
}

const OP_CONTINUATION: u8 = 0x0;
const OP_TEXT: u8 = 0x1;
const OP_BINARY: u8 = 0x2;
const OP_CLOSE: u8 = 0x8;
const OP_PING: u8 = 0x9;
const OP_PONG: u8 = 0xA;

fn protocol_error(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

pub struct FrameReader {
    inner: OwnedReadHalf,
    buf: Vec<u8>,
    pos: usize,
    /// Accumulated payload of an in-progress fragmented message, with the
    /// opcode of its first frame. Kept on the struct rather than as a local so
    /// that a control frame arriving mid-fragmentation does not discard it.
    fragment: Option<(u8, Vec<u8>)>,
}

impl FrameReader {
    pub fn new(inner: OwnedReadHalf, initial: Vec<u8>) -> Self {
        Self {
            inner,
            buf: initial,
            pos: 0,
            fragment: None,
        }
    }

    async fn fill(&mut self, n: usize) -> io::Result<()> {
        let mut chunk = [0u8; 8192];
        while self.buf.len() - self.pos < n {
            let read = self.inner.read(&mut chunk).await?;
            if read == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "connection closed mid-frame",
                ));
            }
            self.buf.extend_from_slice(&chunk[..read]);
        }
        Ok(())
    }

    async fn take(&mut self, n: usize) -> io::Result<Vec<u8>> {
        self.fill(n).await?;
        let out = self.buf[self.pos..self.pos + n].to_vec();
        self.pos += n;
        // Reclaim consumed bytes periodically so the buffer does not grow
        // without bound across a long-lived session.
        if self.pos >= 64 * 1024 {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
        Ok(out)
    }

    /// Reads one raw frame, unmasking the payload if required.
    async fn read_frame(&mut self) -> io::Result<(bool, u8, Vec<u8>)> {
        let header = self.take(2).await?;
        let fin = header[0] & 0x80 != 0;
        let opcode = header[0] & 0x0F;
        let masked = header[1] & 0x80 != 0;

        let len = match header[1] & 0x7F {
            126 => {
                let ext = self.take(2).await?;
                u16::from_be_bytes([ext[0], ext[1]]) as usize
            }
            127 => {
                let ext = self.take(8).await?;
                let len = u64::from_be_bytes(ext.try_into().expect("8 bytes"));
                usize::try_from(len)
                    .map_err(|_| protocol_error("payload length overflows usize"))?
            }
            short => short as usize,
        };

        if len > MAX_PAYLOAD_BYTES {
            return Err(protocol_error("payload exceeds maximum size"));
        }

        // Control frames must be unfragmented and carry at most 125 bytes.
        if opcode & 0x08 != 0 && (!fin || len > 125) {
            return Err(protocol_error("malformed control frame"));
        }

        let mask = if masked {
            Some(self.take(4).await?)
        } else {
            None
        };

        let mut payload = self.take(len).await?;
        if let Some(mask) = mask {
            for (i, byte) in payload.iter_mut().enumerate() {
                *byte ^= mask[i % 4];
            }
        }

        Ok((fin, opcode, payload))
    }

    /// Returns the next complete message, reassembling fragments. Control
    /// frames are surfaced as soon as they arrive, even mid-fragmentation.
    pub async fn next_message(&mut self) -> io::Result<Frame> {
        loop {
            let (fin, opcode, payload) = self.read_frame().await?;

            match opcode {
                OP_CLOSE => return Ok(Frame::Close),
                OP_PING => return Ok(Frame::Ping(payload)),
                OP_PONG => return Ok(Frame::Pong(payload)),

                OP_CONTINUATION => {
                    let Some((_, acc)) = self.fragment.as_mut() else {
                        return Err(protocol_error("continuation without an open message"));
                    };
                    if acc.len() + payload.len() > MAX_PAYLOAD_BYTES {
                        return Err(protocol_error("fragmented message exceeds maximum size"));
                    }
                    acc.extend_from_slice(&payload);
                    if fin {
                        let (opcode, acc) = self.fragment.take().expect("checked above");
                        return finish(opcode, acc);
                    }
                }

                OP_TEXT | OP_BINARY => {
                    if self.fragment.is_some() {
                        return Err(protocol_error("new data frame while a message is open"));
                    }
                    if fin {
                        return finish(opcode, payload);
                    }
                    self.fragment = Some((opcode, payload));
                }

                _ => return Err(protocol_error("unknown opcode")),
            }
        }
    }
}

fn finish(opcode: u8, payload: Vec<u8>) -> io::Result<Frame> {
    match opcode {
        OP_TEXT => String::from_utf8(payload)
            .map(Frame::Text)
            .map_err(|_| protocol_error("text frame is not valid UTF-8")),
        _ => Ok(Frame::Binary(payload)),
    }
}

pub struct FrameWriter {
    inner: OwnedWriteHalf,
}

impl FrameWriter {
    pub fn new(inner: OwnedWriteHalf) -> Self {
        Self { inner }
    }

    pub async fn write(&mut self, frame: &Frame) -> io::Result<()> {
        let (opcode, payload): (u8, &[u8]) = match frame {
            Frame::Text(text) => (OP_TEXT, text.as_bytes()),
            Frame::Binary(data) => (OP_BINARY, data),
            Frame::Ping(data) => (OP_PING, data),
            Frame::Pong(data) => (OP_PONG, data),
            Frame::Close => (OP_CLOSE, &[]),
        };

        // FIN is always set: we never fragment outbound messages. Server frames
        // are never masked (RFC 6455 §5.1).
        let mut head = Vec::with_capacity(10);
        head.push(0x80 | opcode);
        match payload.len() {
            len if len < 126 => head.push(len as u8),
            len if len <= u16::MAX as usize => {
                head.push(126);
                head.extend_from_slice(&(len as u16).to_be_bytes());
            }
            len => {
                head.push(127);
                head.extend_from_slice(&(len as u64).to_be_bytes());
            }
        }

        self.inner.write_all(&head).await?;
        self.inner.write_all(payload).await?;
        self.inner.flush().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_key_matches_rfc_6455_example() {
        // The worked example from RFC 6455 §1.3.
        assert_eq!(
            accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn parses_request_line_and_headers() {
        let req =
            parse_head("GET /json/list HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket").unwrap();
        assert_eq!(req.method, "GET");
        assert_eq!(req.path, "/json/list");
        assert_eq!(req.header("host"), Some("localhost"));
        // Header lookup must be case-insensitive.
        assert_eq!(req.header("UPGRADE"), Some("websocket"));
        assert_eq!(req.header("missing"), None);
    }

    #[test]
    fn detects_websocket_upgrade() {
        let head = "GET /ws/abc HTTP/1.1\r\n\
                    Connection: keep-alive, Upgrade\r\n\
                    Upgrade: websocket\r\n\
                    Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==";
        assert!(parse_head(head).unwrap().is_websocket_upgrade());

        // Missing the key means we cannot complete the handshake.
        let head = "GET /ws/abc HTTP/1.1\r\nConnection: Upgrade\r\nUpgrade: websocket";
        assert!(!parse_head(head).unwrap().is_websocket_upgrade());
    }

    #[test]
    fn finds_head_boundary() {
        // "GET / HTTP/1.1" is 14 bytes, so the CRLFCRLF starts at index 14.
        assert_eq!(find_head_end(b"GET / HTTP/1.1\r\n\r\nbody"), Some(14));
        assert_eq!(find_head_end(b"GET / HTTP/1.1\r\n"), None);
    }
}
