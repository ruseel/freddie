//! A loopback WebSocket that hands each text frame to a callback.
//!
//! [`listen`] binds the port. Dropping the returned [`EventSocket`] closes everything.
//! Web pages are refused at the handshake: a WebSocket handshake is exempt from the
//! same-origin policy, so without this check any page in any open tab could drive the socket.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpListener as StdTcpListener};

use futures_util::StreamExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::accept_hdr_async_with_config;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tracing::{debug, warn};

/// A frame past this closes the connection that sent it.
const MAX_FRAME_BYTES: usize = 64 * 1024;

/// The listener. Dropping it stops accepting and closes every live connection: it owns
/// the only [`watch::Sender`], and every task holds a receiver.
pub struct EventSocket {
    _shutdown: watch::Sender<()>,
    local_addr: SocketAddr,
}

impl EventSocket {
    /// The loopback address this socket is accepting on. Captured at bind, so `listen(0, ...)`
    /// can read the OS-assigned port.
    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
}

/// Bind `127.0.0.1:port` and call `on_message` for each text frame any client sends.
///
/// `on_message` runs on the socket's runtime and must not block. Every connection forwards
/// its frames to one task that owns `on_message`, so the calls are serialized and it need
/// not be `Sync`. The bind is synchronous through `std`, so a busy port is an `Err` from
/// this call. A caller that passed `0` reads the assigned port from [`EventSocket::local_addr`].
///
/// # Errors
///
/// If the port is taken, or loopback cannot be bound.
///
/// # Panics
///
/// If called outside a tokio runtime.
pub fn listen<F>(port: u16, on_message: F) -> io::Result<EventSocket>
where
    F: Fn(&str) + Send + 'static,
{
    let std_listener = StdTcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))?;
    std_listener.set_nonblocking(true)?;
    let local_addr = std_listener.local_addr()?;
    let listener = TcpListener::from_std(std_listener)?;

    let (shutdown, mut closed) = watch::channel(());
    // One task owns `on_message` and drains; that drops the `Sync` bound on the callback.
    let (forward, mut frames) = mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        while let Some(frame) = frames.recv().await {
            on_message(&frame);
        }
        debug!("the event socket's dispatch ended");
    });

    tokio::spawn(async move {
        loop {
            let accepted = tokio::select! {
                () = dropped(&mut closed) => break,
                accepted = listener.accept() => accepted,
            };
            match accepted {
                Ok((stream, peer)) => {
                    debug!(%peer, "accepted");
                    tokio::spawn(serve(stream, forward.clone(), closed.clone()));
                }
                Err(e) => debug!(error = %e, "accept failed"),
            }
        }
        debug!("the event socket closed");
    });

    Ok(EventSocket {
        _shutdown: shutdown,
        local_addr,
    })
}

/// Resolves once the [`EventSocket`] has been dropped.
async fn dropped(closed: &mut watch::Receiver<()>) {
    while closed.changed().await.is_ok() {}
}

/// Handshake, then forward every text frame until the connection ends or the socket is dropped.
async fn serve(
    stream: TcpStream,
    forward: mpsc::UnboundedSender<String>,
    mut closed: watch::Receiver<()>,
) {
    let config = WebSocketConfig {
        max_message_size: Some(MAX_FRAME_BYTES),
        ..WebSocketConfig::default()
    };
    let mut ws = match accept_hdr_async_with_config(stream, check_origin, Some(config)).await {
        Ok(ws) => ws,
        Err(e) => {
            debug!(error = %e, "handshake failed");
            return;
        }
    };

    loop {
        let frame = tokio::select! {
            // Close cleanly. Dropping `ws` here would reset the connection.
            () = dropped(&mut closed) => {
                if let Err(e) = ws.close(None).await {
                    debug!(error = %e, "could not close cleanly");
                }
                break;
            }
            frame = ws.next() => frame,
        };
        match frame {
            Some(Ok(Message::Text(text))) => {
                let _ = forward.send(text.as_str().to_owned());
            }
            Some(Ok(Message::Binary(_))) => debug!("dropping a binary frame"),
            Some(Ok(_)) => {}
            Some(Err(e)) => {
                debug!(error = %e, "connection ended");
                break;
            }
            None => break,
        }
    }
}

/// Refuse a web page, admit everything else.
#[expect(clippy::result_large_err)]
fn check_origin(request: &Request, response: Response) -> Result<Response, ErrorResponse> {
    let origin = match request.headers().get(http::header::ORIGIN) {
        None => None,
        Some(value) => match value.to_str() {
            Ok(origin) => Some(origin),
            Err(_) => return Err(refuse()),
        },
    };
    if origin_allowed(origin) {
        Ok(response)
    } else {
        warn!(?origin, "refusing a handshake from a web page");
        Err(refuse())
    }
}

fn refuse() -> ErrorResponse {
    let mut response = ErrorResponse::new(Some("origin not allowed".to_owned()));
    *response.status_mut() = http::StatusCode::FORBIDDEN;
    response
}

/// Whether a handshake carrying this `Origin` may connect.
///
/// Native clients send none, so absent connects. `http`/`https` does not, including loopback.
/// Anything else (in practice `chrome-extension://<id>`) connects; the id is not matched,
/// because an unpacked build's id follows from where it was loaded.
fn origin_allowed(origin: Option<&str>) -> bool {
    origin.is_none_or(|origin| !origin.starts_with("http://") && !origin.starts_with("https://"))
}

#[cfg(test)]
mod tests {
    use super::origin_allowed;

    #[test]
    fn web_origins_are_refused_and_others_are_not() {
        for allowed in [None, Some("chrome-extension://abcdef"), Some("file://")] {
            assert!(origin_allowed(allowed), "{allowed:?} should connect");
        }
        for refused in [
            "https://evil.com",
            "http://evil.com",
            "http://localhost:3000",
            "http://127.0.0.1:8797",
        ] {
            assert!(
                !origin_allowed(Some(refused)),
                "{refused} should not connect"
            );
        }
    }
}
