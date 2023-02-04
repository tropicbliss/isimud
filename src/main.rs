use axum::{
    extract::{
        ws::{Message, WebSocket},
        ConnectInfo, State, WebSocketUpgrade,
    },
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router, TypedHeader,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};
use tokio::sync::broadcast::{self, Sender};
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing_subscriber::{prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt};

#[derive(Deserialize, Debug, Clone)]
struct PublisherMsg {
    topic: String,
    data: String,
}

struct SharedState {
    tx: Sender<PubSubMsg>,
    password: String,
    show_github_page: bool,
}

impl SharedState {
    fn new(show_github_page: bool) -> anyhow::Result<Self> {
        let password = std::env::var("PASSWORD")?;
        let (tx, _) = broadcast::channel(16);
        Ok(Self {
            tx,
            password,
            show_github_page,
        })
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "isimud=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    let show_github_page = std::env::var("HOMEPAGE").unwrap_or("true".to_string());
    let show_github_page = matches!(show_github_page.as_str(), "true" | "t" | "1");
    let app = Router::new()
        .route("/", get(github_redirect))
        .route("/pub", post(pub_handler))
        .route("/sub", get(ws_handler))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        )
        .with_state(Arc::new(SharedState::new(show_github_page)?));
    let ip: Ipv4Addr = std::env::var("IP").unwrap_or("127.0.0.1".into()).parse()?;
    let port: u16 = std::env::var("PORT").unwrap_or("3000".into()).parse()?;
    let addr = SocketAddr::from((ip, port));
    tracing::debug!("listening on {addr}");
    axum::Server::bind(&addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await?;
    Ok(())
}

async fn pub_handler(
    server_info: Option<TypedHeader<headers::Authorization<headers::authorization::Basic>>>,
    state: State<Arc<SharedState>>,
    Json(payload): Json<PublisherMsg>,
) -> Result<Response, AuthError> {
    if let Some(TypedHeader(provided_password)) = server_info {
        if provided_password.password() == &state.password {
            let _ = state.tx.send(PubSubMsg::new(
                payload,
                provided_password.username().to_string(),
            ));
            return Ok(StatusCode::OK.into_response());
        } else {
            return Err(AuthError::WrongCredentials);
        }
    }
    Err(AuthError::MissingCredentials)
}

#[derive(Debug)]
enum AuthError {
    WrongCredentials,
    MissingCredentials,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AuthError::WrongCredentials => (StatusCode::UNAUTHORIZED, "Wrong credentials"),
            AuthError::MissingCredentials => (StatusCode::BAD_REQUEST, "Missing credentials"),
        };
        let body = Json(json!({
            "error": error_message,
        }));
        (status, body).into_response()
    }
}

async fn github_redirect(state: State<Arc<SharedState>>) -> Response {
    if state.show_github_page {
        Redirect::to("https://github.com/tropicbliss/isimud").into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    user_agent: Option<TypedHeader<headers::UserAgent>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    state: State<Arc<SharedState>>,
) -> impl IntoResponse {
    let user_agent = if let Some(TypedHeader(user_agent)) = user_agent {
        user_agent.to_string()
    } else {
        String::from("Unknown browser")
    };
    tracing::info!("`{user_agent}` at {} connected.", addr.to_string());
    ws.on_upgrade(move |socket| handle_socket(socket, addr, state))
}

#[derive(Deserialize)]
struct SubscriberMsg {
    publisher: String,
    topic: String,
}

#[derive(Clone)]
struct PubSubMsg {
    name: String,
    msg: PublisherMsg,
}

impl PubSubMsg {
    fn new(msg: PublisherMsg, name: String) -> Self {
        Self { name, msg }
    }
}

async fn handle_socket(socket: WebSocket, who: SocketAddr, State(state): State<Arc<SharedState>>) {
    let (mut sender, mut receiver) = socket.split();
    let mut sub_data = None;
    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(t) => {
                tracing::info!(">>> {} sent str: {:?}", who, t);
                if let Ok(s) = serde_json::from_str::<SubscriberMsg>(&t) {
                    sub_data = Some(s);
                } else {
                    break;
                }
            }
            Message::Ping(v) => {
                tracing::info!(">>> {} sent ping with {:?}", who, v);
            }
            Message::Close(c) => {
                if let Some(cf) = c {
                    tracing::info!(
                        ">>> {} sent close with code {} and reason `{}`",
                        who,
                        cf.code,
                        cf.reason
                    );
                } else {
                    tracing::info!(">>> {} somehow sent close message without CloseFrame", who);
                }
                break;
            }
            _ => {
                break;
            }
        }
    }
    if let Some(sub_data) = sub_data {
        let mut send_task = tokio::spawn(async move {
            let mut receiver = state.tx.subscribe();
            while let Ok(data) = receiver.recv().await {
                if sub_data.publisher == data.name && sub_data.topic == data.msg.topic {
                    if sender.send(Message::Text(data.msg.data)).await.is_err() {
                        tracing::info!("client {} abruptly disconnected", who);
                        return;
                    }
                }
            }
        });
        let mut recv_task = tokio::spawn(async move {
            while let Some(Ok(msg)) = receiver.next().await {
                match msg {
                    Message::Ping(v) => {
                        tracing::info!(">>> {} sent ping with {:?}", who, v);
                    }
                    Message::Close(c) => {
                        if let Some(cf) = c {
                            tracing::info!(
                                ">>> {} sent close with code {} and reason `{}`",
                                who,
                                cf.code,
                                cf.reason
                            );
                        } else {
                            tracing::info!(
                                ">>> {} somehow sent close message without CloseFrame",
                                who
                            );
                        }
                        break;
                    }
                    Message::Text(t) => {
                        tracing::info!(">>> {} sent str: {:?}", who, t);
                        break;
                    }
                    _ => {
                        break;
                    }
                }
            }
        });
        tokio::select! {
            _ = (&mut send_task) => {
                recv_task.abort();
            },
            _ = (&mut recv_task) => {
                send_task.abort();
            }
        }
    }
    tracing::info!("Websocket context {} destroyed", who);
}
