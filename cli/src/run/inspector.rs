use std::cell::RefCell;
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::Arc;
use std::thread;

use futures_util::stream::StreamExt;
use futures_util::SinkExt;
use serde_json::json;
use tokio::sync::mpsc;
use uuid::Uuid;
use v8::inspector::{ChannelBase, V8InspectorClientBase};
use warp::ws::{Message, WebSocket};
use warp::Filter;

use crate::run::DebugOptions;

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
        let session_id = Uuid::new_v4().to_string();
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

                let json_list = warp::path("json")
                    .and(warp::path::end())
                    .and(with_inspector(inspector.clone()))
                    .and_then(handle_json_list);

                let json_list_explicit = warp::path!("json" / "list")
                    .and(with_inspector(inspector.clone()))
                    .and_then(handle_json_list);

                let json_version = warp::path!("json" / "version").and_then(handle_json_version);

                let websocket = warp::path!("ws" / String)
                    .and(warp::ws())
                    .and(with_inspector(inspector.clone()))
                    .and_then(handle_websocket_upgrade);

                let routes = json_list
                    .or(json_list_explicit)
                    .or(json_version)
                    .or(websocket)
                    .with(
                        warp::cors()
                            .allow_any_origin()
                            .allow_headers(vec!["content-type"])
                            .allow_methods(vec!["GET", "POST", "OPTIONS"]),
                    );

                inspector_debug!("Myco inspector server listening on http://{}", address);
                inspector_debug!(
                    "WebSocket debugger available at ws://{}/ws/{}",
                    address,
                    session_id
                );
                inspector_debug!("To debug, open Chrome and go to chrome://inspect");
                inspector_debug!("Or manually add the following network target: {}", address);

                warp::serve(routes).run(address).await;
            });
        });
    }
}

#[derive(Clone)]
struct InspectorState {
    session_tx: mpsc::Sender<InspectorSessionRequest>,
    session_id: String,
    port: u16,
}

fn with_inspector(
    inspector: Arc<InspectorState>,
) -> impl Filter<Extract = (Arc<InspectorState>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || inspector.clone())
}

async fn handle_json_list(
    inspector: Arc<InspectorState>,
) -> Result<impl warp::Reply, warp::Rejection> {
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

    Ok(warp::reply::with_header(
        warp::reply::json(&targets),
        "Content-Type",
        "application/json",
    ))
}

async fn handle_json_version() -> Result<impl warp::Reply, warp::Rejection> {
    let version_info = json!({
        "Browser": "Myco JavaScript Runtime/1.0.0",
        "Protocol-Version": "1.3",
        "User-Agent": "Myco JavaScript Runtime/1.0.0",
        "V8-Version": "12.4.254.20",
        "WebKit-Version": "537.36",
        "Runtime": "Myco",
        "Runtime-Version": "1.0.0"
    });

    Ok(warp::reply::json(&version_info))
}

async fn handle_websocket_upgrade(
    session_id: String,
    ws: warp::ws::Ws,
    inspector: Arc<InspectorState>,
) -> Result<impl warp::Reply, warp::Rejection> {
    if session_id != inspector.session_id {
        return Err(warp::reject::not_found());
    }

    Ok(ws.on_upgrade(move |websocket| handle_websocket_connection(websocket, inspector)))
}

async fn handle_websocket_connection(websocket: WebSocket, inspector: Arc<InspectorState>) {
    inspector_debug!("WebSocket connection established for debugging session");

    let (mut ws_tx, mut ws_rx) = websocket.split();
    let (to_client_tx, mut to_client_rx) = mpsc::unbounded_channel::<InspectorMsg>();
    let (from_client_tx, from_client_rx) = mpsc::unbounded_channel::<String>();

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

    let send_task = tokio::spawn(async move {
        while let Some(msg) = to_client_rx.recv().await {
            inspector_debug!("Sending to client: {}", msg.content);
            if ws_tx.send(Message::text(msg.content)).await.is_err() {
                inspector_debug!("Failed to send message to client, connection probably closed");
                break;
            }
        }
    });

    let receive_task = tokio::spawn(async move {
        while let Some(result) = ws_rx.next().await {
            match result {
                Ok(msg) => {
                    if msg.is_text() {
                        let text = msg.to_str().unwrap_or("");
                        inspector_debug!("Received from client: {}", text);
                        if from_client_tx.send(text.to_string()).is_err() {
                            inspector_debug!(
                                "Failed to send message to V8, runtime probably stopped"
                            );
                            break;
                        }
                    } else if msg.is_close() {
                        inspector_debug!("WebSocket connection closed by client");
                        break;
                    }
                }
                Err(_e) => {
                    inspector_debug!("WebSocket error: {}", _e);
                    break;
                }
            }
        }
    });

    tokio::select! {
        _ = send_task => inspector_debug!("Send task completed"),
        _ = receive_task => inspector_debug!("Receive task completed"),
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
    v8_inspector: Rc<RefCell<v8::UniquePtr<v8::inspector::V8Inspector>>>,
    v8_inspector_client: v8::inspector::V8InspectorClientBase,
    sessions: Vec<MycoSession>,
    session_requests: mpsc::Receiver<InspectorSessionRequest>,
    flags: RefCell<InspectorFlags>,
    break_on_start: bool,
    wait_for_connection: bool,
    isolate_ptr: *mut v8::Isolate,
    context: Option<v8::Global<v8::Context>>,
}

pub struct MycoSession {
    v8_session: v8::UniqueRef<v8::inspector::V8InspectorSession>,
    _channel: Box<MycoChannel>, // Keep ownership but don't access directly
    message_queue: VecDeque<String>,
    msg_rx: mpsc::UnboundedReceiver<String>,
    terminated: bool,
}

pub struct MycoChannel {
    base: ChannelBase,
    session: InspectorSession,
}

impl MycoChannel {
    fn new(session: InspectorSession) -> Self {
        Self {
            base: ChannelBase::new::<Self>(),
            session,
        }
    }
}

impl v8::inspector::ChannelImpl for MycoChannel {
    fn base(&self) -> &ChannelBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut ChannelBase {
        &mut self.base
    }

    unsafe fn base_ptr(this: *const Self) -> *const ChannelBase {
        std::ptr::addr_of!((*this).base)
    }

    fn send_response(&mut self, call_id: i32, message: v8::UniquePtr<v8::inspector::StringBuffer>) {
        let content = message.unwrap().string().to_string();
        let msg = InspectorMsg {
            kind: InspectorMsgKind::Message(call_id),
            content,
        };
        self.session.send_to_client(msg);
    }

    fn send_notification(&mut self, message: v8::UniquePtr<v8::inspector::StringBuffer>) {
        let content = message.unwrap().string().to_string();
        let msg = InspectorMsg {
            kind: InspectorMsgKind::Notification,
            content,
        };
        self.session.send_to_client(msg);
    }

    fn flush_protocol_notifications(&mut self) {}
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
            v8_inspector_client: V8InspectorClientBase::new::<Self>(),
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
        let mut v8_inspector = v8::inspector::V8Inspector::create(isolate, &mut *self_borrow);

        // Tell V8 about the context.
        let context_name = v8::inspector::StringView::from(&b"main realm"[..]);
        let aux_data = r#"{"isDefault": true}"#;
        let aux_data_view = v8::inspector::StringView::from(aux_data.as_bytes());
        let scope = &mut v8::HandleScope::new(isolate);
        let context_local = v8::Local::new(scope, self_borrow.context.as_ref().unwrap());
        v8_inspector.context_created(
            context_local,
            1, // context_group_id
            context_name,
            aux_data_view,
        );

        self_borrow.v8_inspector = Rc::new(RefCell::new(v8_inspector.into()));

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
        let mut channel = Box::new(MycoChannel::new(request.session));
        let v8_session = self.v8_inspector.borrow_mut().as_mut().unwrap().connect(
            1, // context_group_id
            channel.as_mut(),
            v8::inspector::StringView::empty(),
            v8::inspector::V8InspectorClientTrustLevel::FullyTrusted,
        );

        let session = MycoSession {
            v8_session,
            _channel: channel,
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

// Implementation of the V8 client trait on our unified inspector struct.
// Methods are moved from the old `MycoInspectorClient`.
impl v8::inspector::V8InspectorClientImpl for MycoInspector {
    fn base(&self) -> &V8InspectorClientBase {
        &self.v8_inspector_client
    }

    fn base_mut(&mut self) -> &mut V8InspectorClientBase {
        &mut self.v8_inspector_client
    }

    unsafe fn base_ptr(this: *const Self) -> *const V8InspectorClientBase {
        std::ptr::addr_of!((*this).v8_inspector_client)
    }

    // This is now fully implemented as of Stage 2.
    fn ensure_default_context_in_group(
        &mut self,
        context_group_id: i32,
    ) -> Option<v8::Local<v8::Context>> {
        // Myco uses a single context group with ID 1.
        assert_eq!(context_group_id, 1);
        let isolate: &mut v8::Isolate = unsafe { &mut *self.isolate_ptr };
        let scope = &mut unsafe { v8::CallbackScope::new(isolate) };
        self.context
            .as_ref()
            .map(|ctx| v8::Local::new(scope, ctx.clone()))
    }

    fn run_message_loop_on_pause(&mut self, _context_group_id: i32) {
        self.flags.borrow_mut().on_pause = true;

        inspector_debug!("Inspector: Paused. Entering blocking message loop.");
        while self.flags.borrow().on_pause {
            self.poll_blocking();
        }
        inspector_debug!("Inspector: Resumed. Exiting blocking message loop.");
    }

    fn quit_message_loop_on_pause(&mut self) {
        self.flags.borrow_mut().on_pause = false;
    }

    fn run_if_waiting_for_debugger(&mut self, _context_group_id: i32) {
        inspector_debug!("Debugger connected, V8 is resuming execution.");
        self.flags.borrow_mut().waiting_for_session = false;
    }

    // This is a helper to map file paths for V8
    fn resource_name_to_url(
        &mut self,
        resource_name: &v8::inspector::StringView,
    ) -> Option<v8::UniquePtr<v8::inspector::StringBuffer>> {
        let resource_name_str = resource_name.to_string();
        let url = url::Url::from_file_path(resource_name_str).ok()?;
        let url_str = url.as_str();
        let v8_str = v8::inspector::StringView::from(url_str.as_bytes());
        Some(v8::inspector::StringBuffer::create(v8_str))
    }
}
