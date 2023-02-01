use anyhow::Result;
use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket},
        ConnectInfo, State, WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::get,
    Router, TypedHeader,
};
use dotenvy::dotenv;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::{
    borrow::Cow,
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};
use strmatch::strmatch;
use tokio::sync::broadcast::{self, Sender};
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing_subscriber::{prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt};

#[derive(Deserialize, Clone)]
struct PublisherMsg {
    topic: String,
    data: String,
}

struct SharedState {
    tx: Sender<PubSubMsg>,
    password: String,
}

impl SharedState {
    fn new() -> Result<Self> {
        let password = std::env::var("PASSWORD")?;
        let (tx, _) = broadcast::channel(16);
        Ok(Self { tx, password })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "isimud=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    let app = Router::new()
        .route("/", get(ws_handler))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        )
        .with_state(Arc::new(SharedState::new()?));
    let ip: Ipv4Addr = std::env::var("IP").unwrap_or("127.0.0.1".into()).parse()?;
    let port: u16 = std::env::var("PORT").unwrap_or("3000".into()).parse()?;
    let addr = SocketAddr::from((ip, port));
    tracing::debug!("listening on {addr}");
    axum::Server::bind(&addr)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await?;
    Ok(())
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
    println!("`{user_agent}` at {} connected.", addr.to_string());
    ws.on_upgrade(move |socket| handle_socket(socket, addr, state))
}

enum SocketState {
    Pending,
    Authed,
    Pubbed { publisher: String },
    Subbed { metadata: SubscriberMsg },
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
    let mut socket_state = SocketState::Pending;
    'recv: while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(text) => {
                println!(">>> {} sent str: {:?}", who, text);
                match socket_state {
                    SocketState::Pending => {
                        strmatch!(text.as_str() => {
                            "^pub auth .+$" => {
                                let password = text.split_whitespace().nth(2);
                                if let Some(password) = password {
                                    if password == state.password {
                                        socket_state = SocketState::Authed;
                                    } else {
                                        if let Err(e) = sender.send(Message::Close(Some(CloseFrame {
                                            code: axum::extract::ws::close_code::INVALID,
                                            reason: Cow::from("Invalid password"),
                                        }))).await {
                                            println!("Could not send Close due to {}, probably it is ok?", e);
                                        }
                                        return;
                                    }
                                } else {
                                    if let Err(e) = sender.send(Message::Close(Some(CloseFrame {
                                        code: axum::extract::ws::close_code::INVALID,
                                        reason: Cow::from("Malformed command"),
                                    }))).await {
                                        println!("Could not send Close due to {}, probably it is ok?", e);
                                    }
                                    return;
                                }
                            },
                            _ => {
                                if let Ok(sub_data) = serde_json::from_str::<SubscriberMsg>(&text) {
                                    socket_state = SocketState::Subbed { metadata: sub_data };
                                    break 'recv;
                                } else {
                                    if let Err(e) = sender.send(Message::Close(Some(CloseFrame {
                                        code: axum::extract::ws::close_code::INVALID,
                                        reason: Cow::from("Invalid message"),
                                    }))).await {
                                        println!("Could not send Close due to {}, probably it is ok?", e);
                                    }
                                    return;
                                }
                            },
                        })
                    }
                    SocketState::Authed => {
                        strmatch!(text.as_str() => {
                            "^pub name .+$" => {
                                let publisher = text.split_whitespace().nth(2);
                                if let Some(publisher) = publisher {
                                    socket_state = SocketState::Pubbed { publisher: publisher.to_string() };
                                } else {
                                    if let Err(e) = sender.send(Message::Close(Some(CloseFrame {
                                        code: axum::extract::ws::close_code::INVALID,
                                        reason: Cow::from("Malformed command"),
                                    }))).await {
                                        println!("Could not send Close due to {}, probably it is ok?", e);
                                    }
                                    return;
                                }
                            },
                            _ => {
                                if let Err(e) = sender.send(Message::Close(Some(CloseFrame {
                                    code: axum::extract::ws::close_code::INVALID,
                                    reason: Cow::from("Invalid command"),
                                }))).await {
                                    println!("Could not send Close due to {}, probably it is ok?", e);
                                }
                                return;
                            }
                        })
                    }
                    SocketState::Pubbed { ref publisher } => {
                        match serde_json::from_str::<PublisherMsg>(&text) {
                            Ok(data) => {
                                let publisher_msg = PubSubMsg::new(data, publisher.to_string());
                                let _ = state.tx.send(publisher_msg);
                            }
                            Err(e) => {
                                if let Err(e) = sender
                                    .send(Message::Close(Some(CloseFrame {
                                        code: axum::extract::ws::close_code::INVALID,
                                        reason: Cow::from(format!(
                                            "Invalid JSON: {}",
                                            e.to_string()
                                        )),
                                    })))
                                    .await
                                {
                                    println!(
                                        "Could not send Close due to {}, probably it is ok?",
                                        e
                                    );
                                }
                                return;
                            }
                        }
                    }
                    _ => {}
                }
            }
            Message::Close(c) => {
                if let Some(cf) = c {
                    println!(
                        ">>> {} sent close with code {} and reason `{}`",
                        who, cf.code, cf.reason
                    );
                } else {
                    println!(">>> {} somehow sent close message without CloseFrame", who);
                }
                return;
            }
            Message::Ping(v) => {
                println!(">>> {} sent ping with {:?}", who, v);
            }
            _ => {
                if let Err(e) = sender
                    .send(Message::Close(Some(CloseFrame {
                        code: axum::extract::ws::close_code::INVALID,
                        reason: Cow::from("Invalid message"),
                    })))
                    .await
                {
                    println!("Could not send Close due to {}, probably it is ok?", e);
                }
                return;
            }
        }
    }
    if let SocketState::Subbed { metadata } = socket_state {
        let mut send_task = tokio::spawn(async move {
            let mut count = 0;
            let mut receiver = state.tx.subscribe();
            while let Ok(data) = receiver.recv().await {
                if metadata.publisher == data.name && metadata.topic == data.msg.topic {
                    count += 1;
                    if sender.send(Message::Text(data.msg.data)).await.is_err() {
                        println!("client {} abruptly disconnected", who);
                        return count;
                    }
                }
            }
            count
        });
        let mut recv_task = tokio::spawn(async move {
            if let Some(Ok(_)) = receiver.next().await {
                return;
            }
        });
        tokio::select! {
            rv_a = (&mut send_task) => {
                match rv_a {
                    Ok(a) => println!("{} messages sent to {}", a, who),
                    Err(a) => println!("Error sending messages {:?}", a)
                }
                recv_task.abort();
            },
            _ = (&mut recv_task) => {
                send_task.abort();
            }
        }
    }
    println!("Websocket context {} destroyed", who);
}
