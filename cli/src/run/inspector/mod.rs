use std::cell::RefCell;
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::Arc;
use std::thread;

use rand::Rng;
use serde_json::json;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use v8::inspector::V8InspectorClient;

use crate::run::DebugOptions;

mod server;

use server::{Frame, FrameReader, FrameWriter};

// Macro for inspector debug logging
#[cfg(feature = "inspector-debug")]
macro_rules! inspector_debug {
    ($($arg:tt)*) => {
        println!($($arg)*)
    };
}

#[cfg(not(feature = "inspector-debug"))]
macro_rules! inspector_debug {
    ($($arg:tt)*) => {
        ()
    };
}

#[derive(Clone)]
pub struct Inspector {
    address: SocketAddr,
    session_tx: mpsc::Sender<InspectorSessionRequest>,
    session_id: String,
}

pub struct InspectorSessionRequest {
    pub session: InspectorSession,
    pub msg_rx: mpsc::UnboundedReceiver<String>,
}

impl Inspector {
    pub fn new(options: &DebugOptions, session_tx: mpsc::Sender<InspectorSessionRequest>) -> Self {
        let address = format!("127.0.0.1:{}", options.port).parse().unwrap();
        let session_id = random_session_id();
        Self {
            address,
            session_tx,
            session_id,
        }
    }

    pub fn start(&self) {
        let address = self.address;
        let session_tx = self.session_tx.clone();
        let session_id = self.session_id.clone();

        thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async move {
                let inspector = Arc::new(InspectorState {
                    session_tx,
                    session_id: session_id.clone(),
                    port: address.port(),
                });

                let listener = match TcpListener::bind(address).await {
                    Ok(listener) => listener,
                    Err(_e) => {
                        inspector_debug!("Failed to bind inspector server to {}: {}", address, _e);
                        return;
                    }
                };

                inspector_debug!("Myco inspector server listening on http://{}", address);
                inspector_debug!(
                    "WebSocket debugger available at ws://{}/ws/{}",
                    address,
                    session_id
                );
                inspector_debug!("To debug, open Chrome and go to chrome://inspect");
                inspector_debug!("Or manually add the following network target: {}", address);

                loop {
                    match listener.accept().await {
                        Ok((stream, _)) => {
                            let inspector = inspector.clone();
                            tokio::spawn(handle_connection(stream, inspector));
                        }
                        Err(_e) => {
                            inspector_debug!("Inspector accept failed: {}", _e);
                        }
                    }
                }
            });
        });
    }
}

/// Session id used both as the DevTools target id and as the WebSocket path
/// segment. It only needs to be unguessable enough to avoid collisions between
/// concurrent local sessions, not to be a security boundary.
fn random_session_id() -> String {
    let bytes: [u8; 16] = rand::thread_rng().gen();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

async fn handle_connection(mut stream: TcpStream, inspector: Arc<InspectorState>) {
    let Ok(Some((request, leftover))) = server::read_request(&mut stream).await else {
        return;
    };

    // CORS preflight, as the old `warp::cors()` layer answered.
    if request.method == "OPTIONS" {
        let _ = write_all(&mut stream, &server::empty_response("204 No Content")).await;
        return;
    }

    if request.method != "GET" {
        let _ = write_all(
            &mut stream,
            &server::empty_response("405 Method Not Allowed"),
        )
        .await;
        return;
    }

    match request.path.as_str() {
        "/json" | "/json/list" => {
            let body = json_list_body(&inspector);
            let _ = write_all(&mut stream, &server::json_response(&body)).await;
        }
        "/json/version" => {
            let body = json_version_body();
            let _ = write_all(&mut stream, &server::json_response(&body)).await;
        }
        path => {
            let expected = format!("/ws/{}", inspector.session_id);
            if path == expected && request.is_websocket_upgrade() {
                // `is_websocket_upgrade` guarantees the key is present.
                let key = request.header("sec-websocket-key").unwrap().to_string();
                if server::write_handshake(&mut stream, &key).await.is_ok() {
                    handle_websocket_connection(stream, leftover, inspector).await;
                }
            } else {
                let _ = write_all(&mut stream, &server::empty_response("404 Not Found")).await;
            }
        }
    }
}

async fn write_all(stream: &mut TcpStream, bytes: &[u8]) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    stream.write_all(bytes).await?;
    stream.flush().await
}

fn json_list_body(inspector: &InspectorState) -> String {
    let targets = json!([
        {
            "description": "Myco JavaScript Runtime",
            "devtoolsFrontendUrl": format!(
                "devtools://devtools/bundled/js_app.html?experiments=true&v8only=true&ws=127.0.0.1:{}/ws/{}",
                inspector.port,
                inspector.session_id
            ),
            "id": inspector.session_id,
            "title": "Myco JavaScript Runtime",
            "type": "myco",
            "url": "file://",
            "webSocketDebuggerUrl": format!("ws://127.0.0.1:{}/ws/{}", inspector.port, inspector.session_id)
        }
    ]);
    targets.to_string()
}

fn json_version_body() -> String {
    json!({
        "Browser": "Myco JavaScript Runtime/1.0.0",
        "Protocol-Version": "1.3",
        "User-Agent": "Myco JavaScript Runtime/1.0.0",
        "V8-Version": "12.4.254.20",
        "WebKit-Version": "537.36",
        "Runtime": "Myco",
        "Runtime-Version": "1.0.0"
    })
    .to_string()
}

#[derive(Clone)]
struct InspectorState {
    session_tx: mpsc::Sender<InspectorSessionRequest>,
    session_id: String,
    port: u16,
}

async fn handle_websocket_connection(
    stream: TcpStream,
    leftover: Vec<u8>,
    inspector: Arc<InspectorState>,
) {
    inspector_debug!("WebSocket connection established for debugging session");

    let (read_half, write_half) = stream.into_split();
    // The handshake read may have consumed the first frame bytes; hand them to
    // the reader so the stream stays intact.
    let mut reader = FrameReader::new(read_half, leftover);
    let mut writer = FrameWriter::new(write_half);

    let (to_client_tx, mut to_client_rx) = mpsc::unbounded_channel::<InspectorMsg>();
    let (from_client_tx, from_client_rx) = mpsc::unbounded_channel::<String>();
    // Both the CDP pump and the read loop (for pong replies) need to write, so
    // outbound frames are funnelled through a single channel to one writer.
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Frame>();

    let session = InspectorSession::new(to_client_tx);

    // Send the session to the V8 runtime
    let request = InspectorSessionRequest {
        session,
        msg_rx: from_client_rx,
    };

    if inspector.session_tx.send(request).await.is_err() {
        inspector_debug!("Failed to register inspector session with V8 runtime");
        return;
    }

    inspector_debug!("Inspector session registered with V8 runtime");

    let write_task = tokio::spawn(async move {
        while let Some(frame) = out_rx.recv().await {
            if writer.write(&frame).await.is_err() {
                inspector_debug!("Failed to send message to client, connection probably closed");
                break;
            }
        }
    });

    let pump_tx = out_tx.clone();
    let pump_task = tokio::spawn(async move {
        while let Some(msg) = to_client_rx.recv().await {
            inspector_debug!("Sending to client: {}", msg.content);
            if pump_tx.send(Frame::Text(msg.content)).is_err() {
                break;
            }
        }
    });

    let read_task = tokio::spawn(async move {
        loop {
            match reader.next_message().await {
                Ok(Frame::Text(text)) => {
                    inspector_debug!("Received from client: {}", text);
                    if from_client_tx.send(text).is_err() {
                        inspector_debug!("Failed to send message to V8, runtime probably stopped");
                        break;
                    }
                }
                Ok(Frame::Ping(payload)) => {
                    if out_tx.send(Frame::Pong(payload)).is_err() {
                        break;
                    }
                }
                Ok(Frame::Close) => {
                    inspector_debug!("WebSocket connection closed by client");
                    let _ = out_tx.send(Frame::Close);
                    break;
                }
                // CDP is text-only; binary and pong frames are simply ignored.
                Ok(_) => {}
                Err(_e) => {
                    inspector_debug!("WebSocket error: {}", _e);
                    break;
                }
            }
        }
    });

    // Whichever side finishes first ends the session; the others are aborted so
    // the connection does not linger.
    tokio::select! {
        _ = write_task => inspector_debug!("Write task completed"),
        _ = pump_task => inspector_debug!("Pump task completed"),
        _ = read_task => inspector_debug!("Read task completed"),
    }

    inspector_debug!("WebSocket connection terminated");
}

pub enum InspectorMsgKind {
    Notification,
    Message(i32), // The call_id
}

pub struct InspectorMsg {
    pub kind: InspectorMsgKind,
    pub content: String,
}

#[derive(Debug)]
pub struct InspectorSession {
    to_client_tx: mpsc::UnboundedSender<InspectorMsg>,
}

impl InspectorSession {
    fn new(to_client_tx: mpsc::UnboundedSender<InspectorMsg>) -> Self {
        Self { to_client_tx }
    }

    pub fn send_to_client(&self, msg: InspectorMsg) {
        let _ = self.to_client_tx.send(msg);
    }
}

// A new flags struct to hold the inspector's state, as planned.
// This replaces the old `SharedInspectorState`.
#[derive(Debug, Default)]
pub struct InspectorFlags {
    pub on_pause: bool,
    pub waiting_for_session: bool,
}

// The unified inspector struct. It now holds all state and implements the
// V8 inspector client trait itself.
pub struct MycoInspector {
    v8_inspector: Rc<RefCell<Option<v8::inspector::V8Inspector>>>,
    sessions: Vec<MycoSession>,
    session_requests: mpsc::Receiver<InspectorSessionRequest>,
    flags: RefCell<InspectorFlags>,
    break_on_start: bool,
    wait_for_connection: bool,
    isolate_ptr: *mut v8::Isolate,
    context: Option<v8::Global<v8::Context>>,
}

pub struct MycoSession {
    // The channel is owned by the V8 session, which keeps it alive for us.
    v8_session: v8::inspector::V8InspectorSession,
    message_queue: VecDeque<String>,
    msg_rx: mpsc::UnboundedReceiver<String>,
    terminated: bool,
}

pub struct MycoChannel {
    session: InspectorSession,
}

impl MycoChannel {
    fn new(session: InspectorSession) -> Self {
        Self { session }
    }
}

impl v8::inspector::ChannelImpl for MycoChannel {
    fn send_response(&self, call_id: i32, message: v8::UniquePtr<v8::inspector::StringBuffer>) {
        let content = message.unwrap().string().to_string();
        let msg = InspectorMsg {
            kind: InspectorMsgKind::Message(call_id),
            content,
        };
        self.session.send_to_client(msg);
    }

    fn send_notification(&self, message: v8::UniquePtr<v8::inspector::StringBuffer>) {
        let content = message.unwrap().string().to_string();
        let msg = InspectorMsg {
            kind: InspectorMsgKind::Notification,
            content,
        };
        self.session.send_to_client(msg);
    }

    fn flush_protocol_notifications(&self) {}
}

impl MycoInspector {
    pub fn new(
        isolate: &mut v8::Isolate,
        context: v8::Global<v8::Context>,
        session_requests: mpsc::Receiver<InspectorSessionRequest>,
        break_on_start: bool,
        wait_for_connection: bool,
    ) -> Rc<RefCell<Self>> {
        let isolate_ptr = isolate as *mut v8::Isolate;

        let self_rc = Rc::new(RefCell::new(Self {
            v8_inspector: Default::default(),
            sessions: vec![],
            session_requests,
            flags: Default::default(),
            break_on_start,
            wait_for_connection,
            isolate_ptr,
            context: Some(context),
        }));

        let mut self_borrow = self_rc.borrow_mut();
        // V8 holds a raw pointer to the client for the lifetime of the inspector,
        // and calls back into it re-entrantly (e.g. while a message is being
        // dispatched). The client therefore points straight at the `MycoInspector`
        // inside the `Rc<RefCell<..>>`, whose address is stable, rather than going
        // through the `RefCell` (which would deadlock on re-entrant calls).
        let client = V8InspectorClient::new(Box::new(MycoInspectorClient {
            inspector: &mut *self_borrow as *mut MycoInspector,
        }));
        let v8_inspector = v8::inspector::V8Inspector::create(isolate, client);

        // Tell V8 about the context.
        let context_name = v8::inspector::StringView::from(&b"main realm"[..]);
        let aux_data = r#"{"isDefault": true}"#;
        let aux_data_view = v8::inspector::StringView::from(aux_data.as_bytes());
        v8::scope!(let scope, isolate);
        let context_local = v8::Local::new(scope, self_borrow.context.as_ref().unwrap());
        v8_inspector.context_created(
            context_local,
            1, // context_group_id
            context_name,
            aux_data_view,
        );

        self_borrow.v8_inspector = Rc::new(RefCell::new(Some(v8_inspector)));

        drop(self_borrow); // release borrow
        self_rc
    }

    pub fn poll_sessions(&mut self) -> std::thread::Result<()> {
        // Handle new session requests
        while let Ok(request) = self.session_requests.try_recv() {
            self.connect_session(request);
        }

        // Poll existing sessions for messages
        for session in self.sessions.iter_mut() {
            while let Ok(msg) = session.msg_rx.try_recv() {
                session.message_queue.push_back(msg);
            }
        }

        // Dispatch one message per session
        for session in self.sessions.iter_mut() {
            if let Some(msg) = session.message_queue.pop_front() {
                let msg_v8 = v8::inspector::StringView::from(msg.as_bytes());
                session.v8_session.dispatch_protocol_message(msg_v8);
            }
        }

        // Remove terminated sessions
        self.sessions.retain(|s| !s.terminated);

        Ok(())
    }

    fn connect_session(&mut self, request: InspectorSessionRequest) {
        let channel = v8::inspector::Channel::new(Box::new(MycoChannel::new(request.session)));
        let v8_session = self.v8_inspector.borrow().as_ref().unwrap().connect(
            1, // context_group_id
            channel,
            v8::inspector::StringView::empty(),
            v8::inspector::V8InspectorClientTrustLevel::FullyTrusted,
        );

        let session = MycoSession {
            v8_session,
            message_queue: VecDeque::new(),
            msg_rx: request.msg_rx,
            terminated: false,
        };
        self.sessions.push(session);
        inspector_debug!("Debugger session connected.");
    }

    pub fn should_break_on_start(&self) -> bool {
        self.break_on_start
    }

    pub fn should_wait_for_connection(&self) -> bool {
        self.wait_for_connection
    }

    pub fn is_paused(&self) -> bool {
        self.flags.borrow().on_pause
    }

    pub fn get_context(&self) -> Option<&v8::Global<v8::Context>> {
        self.context.as_ref()
    }

    pub fn break_on_next_statement(&mut self) {
        if let Some(session) = self.sessions.first_mut() {
            let reason = v8::inspector::StringView::from(&b"debugCommand"[..]);
            let detail = v8::inspector::StringView::empty();
            session
                .v8_session
                .schedule_pause_on_next_statement(reason, detail);
            inspector_debug!("Inspector: Scheduled to break on next statement.");
        }
    }

    pub fn wait_for_session(&mut self) {
        inspector_debug!("Inspector: Waiting for debugger to connect...");
        // Block until at least one session is connected.
        while self.sessions.is_empty() {
            self.poll_blocking();
        }

        // Give the client time to process initial handshake messages
        inspector_debug!("Inspector: Debugger client connected, processing handshake...");
        for _ in 0..10 {
            self.poll_blocking();
        }

        inspector_debug!("Inspector: Ready for debugging.");
    }

    // A shared helper for blocking polling, used when paused or waiting for a connection.
    fn poll_blocking(&mut self) {
        // Poll for new connections
        if let Ok(request) = self.session_requests.try_recv() {
            self.connect_session(request);
        }

        // Poll existing sessions for messages
        let mut got_message = false;
        for session in self.sessions.iter_mut() {
            if let Ok(msg) = session.msg_rx.try_recv() {
                let msg_v8 = v8::inspector::StringView::from(msg.as_bytes());
                session.v8_session.dispatch_protocol_message(msg_v8);
                got_message = true;
            }
        }

        if !got_message {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

// The V8 inspector client. V8 owns this (boxed inside `V8InspectorClient`) and
// calls into it re-entrantly, so it holds a raw pointer back to the
// `MycoInspector` that owns the `V8Inspector` rather than an `Rc<RefCell<..>>`.
// The pointee lives inside an `Rc<RefCell<MycoInspector>>` allocation, so its
// address is stable for as long as the inspector is alive.
struct MycoInspectorClient {
    inspector: *mut MycoInspector,
}

impl MycoInspectorClient {
    #[allow(clippy::mut_from_ref)]
    fn inspector(&self) -> &mut MycoInspector {
        unsafe { &mut *self.inspector }
    }
}

// Implementation of the V8 client trait. Methods delegate to the unified
// inspector struct.
impl v8::inspector::V8InspectorClientImpl for MycoInspectorClient {
    // This is now fully implemented as of Stage 2.
    fn ensure_default_context_in_group(
        &self,
        context_group_id: i32,
    ) -> Option<v8::Local<'_, v8::Context>> {
        // Myco uses a single context group with ID 1.
        assert_eq!(context_group_id, 1);
        let this = self.inspector();
        let isolate: &mut v8::Isolate = unsafe { &mut *this.isolate_ptr };
        v8::callback_scope!(unsafe let scope, isolate);
        this.context
            .as_ref()
            .map(|ctx| v8::Local::new(scope, ctx.clone()))
    }

    fn run_message_loop_on_pause(&self, _context_group_id: i32) {
        let this = self.inspector();
        this.flags.borrow_mut().on_pause = true;

        inspector_debug!("Inspector: Paused. Entering blocking message loop.");
        while this.flags.borrow().on_pause {
            this.poll_blocking();
        }
        inspector_debug!("Inspector: Resumed. Exiting blocking message loop.");
    }

    fn quit_message_loop_on_pause(&self) {
        self.inspector().flags.borrow_mut().on_pause = false;
    }

    fn run_if_waiting_for_debugger(&self, _context_group_id: i32) {
        inspector_debug!("Debugger connected, V8 is resuming execution.");
        self.inspector().flags.borrow_mut().waiting_for_session = false;
    }

    // This is a helper to map file paths for V8
    fn resource_name_to_url(
        &self,
        resource_name: &v8::inspector::StringView,
    ) -> Option<v8::UniquePtr<v8::inspector::StringBuffer>> {
        let resource_name_str = resource_name.to_string();
        let url = url::Url::from_file_path(resource_name_str).ok()?;
        let url_str = url.as_str();
        let v8_str = v8::inspector::StringView::from(url_str.as_bytes());
        Some(v8::inspector::StringBuffer::create(v8_str))
    }
}
