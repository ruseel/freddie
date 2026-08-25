//! Frames from `freddie_event_socket`, parsed into mercury events.

use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, warn};

use crate::{MercuryEvent, tab};

/// The port mercury listens on when nothing overrides it. Hardcoded in the extension too.
///
/// Below 49152, where macOS starts handing out ephemeral ports (`net.inet.ip.portrange.first`): a listener up there can find its port already taken by an outbound socket.
pub const DEFAULT_PORT: u16 = 3883;

/// What an outside process may send. Separate from `MercuryEvent` so a wire frame cannot be a key or a quit.
#[derive(serde::Deserialize, Debug)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "typescript",
    ts(export, export_to = "../../../chrome-extension/src/wire/")
)]
#[serde(tag = "kind", content = "value")]
pub enum IncomingEvent {
    /// The front browser tab's URL changed.
    #[serde(rename = "IncomingEvent.Tab")]
    Tab(TabMessage),
}

#[derive(serde::Deserialize, Debug)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "typescript",
    ts(export, export_to = "../../../chrome-extension/src/wire/")
)]
pub struct TabMessage {
    pub url: String,
}

/// Parse one frame and send it. An invalid frame is logged and dropped; the connection stays open.
pub fn on_message(text: &str, event_tx: &UnboundedSender<MercuryEvent>) {
    match serde_json::from_str::<IncomingEvent>(text) {
        Ok(IncomingEvent::Tab(TabMessage { url })) => {
            debug!(%url, "tab");
            // The event loop has ended.
            let _ = event_tx.send(tab(url));
        }
        Err(e) => warn!(error = %e, frame = text, "undeserializable frame"),
    }
}

#[cfg(test)]
mod tests {
    use super::IncomingEvent;

    #[test]
    fn a_tab_frame_carries_its_url() {
        let frame = r#"{"kind":"IncomingEvent.Tab","value":{"url":"https://claude.ai/new"}}"#;
        let IncomingEvent::Tab(tab) =
            serde_json::from_str::<IncomingEvent>(frame).expect("a tab frame deserializes");
        assert_eq!(tab.url, "https://claude.ai/new");
    }

    #[test]
    fn nothing_outside_the_vocabulary_deserializes() {
        for frame in [
            r#"{"kind":"MercuryEvent.Key","value":{"key":"KeyQ"}}"#,
            r#"{"kind":"IncomingEvent.Quit","value":null}"#,
            r#"{"kind":"IncomingEvent.Tab","value":{}}"#,
            "{}",
            "not json at all",
        ] {
            assert!(
                serde_json::from_str::<IncomingEvent>(frame).is_err(),
                "{frame} should not deserialize"
            );
        }
    }
}
