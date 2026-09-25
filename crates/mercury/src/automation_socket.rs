use std::io;
use std::net::{Ipv4Addr, SocketAddr};

use freddie_keys::{Key, ModifierFlags, PressType};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tracing::{debug, warn};

use crate::daemon::RuntimeEffect;

pub(crate) const DEFAULT_PORT: u16 = 3884;
const MAX_COMMAND_BYTES: usize = 16 * 1024;

pub(crate) enum AutomationCommand {
    Tap(AutomationTap),
    Emit(AutomationEmit),
    Foreground(String),
}

pub(crate) struct AutomationTap {
    pub(crate) key: Key,
    pub(crate) flags: ModifierFlags,
}

pub(crate) struct AutomationEmit {
    pub(crate) key: Key,
    pub(crate) press: PressType,
    pub(crate) flags: ModifierFlags,
}

pub(crate) struct AutomationSocket {
    _shutdown: watch::Sender<()>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", content = "value")]
enum WireCommand {
    #[serde(rename = "AutomationCommand.Tap")]
    Tap(WireTap),
    #[serde(rename = "AutomationCommand.Emit")]
    Emit(WireEmit),
    #[serde(rename = "AutomationCommand.Foreground")]
    Foreground(WireForeground),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireTap {
    key: String,
    modifiers: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireEmit {
    key: String,
    press: String,
    modifiers: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireForeground {
    bundle_id: String,
}

pub(crate) async fn listen(
    port: u16,
    effect_tx: mpsc::UnboundedSender<RuntimeEffect>,
) -> io::Result<AutomationSocket> {
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await?;
    let (shutdown, mut closed) = watch::channel(());
    tokio::spawn(async move {
        loop {
            let accepted = tokio::select! {
                () = dropped(&mut closed) => break,
                accepted = listener.accept() => accepted,
            };
            match accepted {
                Ok((stream, peer)) => {
                    debug!(%peer, "accepted automation bridge connection");
                    tokio::spawn(serve(stream, effect_tx.clone()));
                }
                Err(e) => debug!(error = %e, "automation bridge accept failed"),
            }
        }
        debug!("automation bridge closed");
    });
    Ok(AutomationSocket {
        _shutdown: shutdown,
    })
}

async fn dropped(closed: &mut watch::Receiver<()>) {
    while closed.changed().await.is_ok() {}
}

async fn serve(stream: TcpStream, effect_tx: mpsc::UnboundedSender<RuntimeEffect>) {
    let (read, mut write) = stream.into_split();
    let mut line = String::new();
    let read = BufReader::new(read).read_line(&mut line).await;
    let response = match read {
        Ok(0) => return,
        Ok(bytes) if bytes > MAX_COMMAND_BYTES => rejection("command exceeds 16 KiB"),
        Ok(_) => match serde_json::from_str::<WireCommand>(line.trim_end())
            .map_err(|e| e.to_string())
            .and_then(AutomationCommand::try_from)
        {
            Ok(command) => match effect_tx.send(RuntimeEffect::Automation(command)) {
                Ok(()) => confirmation(),
                Err(_) => rejection("Mercury effect loop is unavailable"),
            },
            Err(message) => rejection(&message),
        },
        Err(e) => {
            warn!(error = %e, "automation bridge read failed");
            rejection("could not read command")
        }
    };
    if let Err(e) = write.write_all(response.as_bytes()).await {
        debug!(error = %e, "automation bridge response failed");
    }
}

fn confirmation() -> String {
    "{\"state\":\"confirmed\"}\n".to_owned()
}

fn rejection(message: &str) -> String {
    match serde_json::to_string(message) {
        Ok(message) => format!("{{\"state\":\"rejected\",\"message\":{message}}}\n"),
        Err(_) => "{\"state\":\"rejected\",\"message\":\"invalid command\"}\n".to_owned(),
    }
}

impl TryFrom<WireCommand> for AutomationCommand {
    type Error = String;

    fn try_from(command: WireCommand) -> Result<Self, Self::Error> {
        match command {
            WireCommand::Tap(command) => Ok(Self::Tap(AutomationTap {
                key: parse_key(&command.key)?,
                flags: parse_modifiers(&command.modifiers)?,
            })),
            WireCommand::Emit(command) => Ok(Self::Emit(AutomationEmit {
                key: parse_key(&command.key)?,
                press: parse_press(&command.press)?,
                flags: parse_modifiers(&command.modifiers)?,
            })),
            WireCommand::Foreground(command) => {
                if valid_bundle_id(&command.bundle_id) {
                    Ok(Self::Foreground(command.bundle_id))
                } else {
                    Err("invalid bundle id".to_owned())
                }
            }
        }
    }
}

fn parse_key(name: &str) -> Result<Key, String> {
    let key = match name {
        "key-a" => Key::KeyA,
        "key-b" => Key::KeyB,
        "key-c" => Key::KeyC,
        "key-d" => Key::KeyD,
        "key-e" => Key::KeyE,
        "key-f" => Key::KeyF,
        "key-g" => Key::KeyG,
        "key-h" => Key::KeyH,
        "key-i" => Key::KeyI,
        "key-j" => Key::KeyJ,
        "key-k" => Key::KeyK,
        "key-l" => Key::KeyL,
        "key-m" => Key::KeyM,
        "key-n" => Key::KeyN,
        "key-o" => Key::KeyO,
        "key-p" => Key::KeyP,
        "key-q" => Key::KeyQ,
        "key-r" => Key::KeyR,
        "key-s" => Key::KeyS,
        "key-t" => Key::KeyT,
        "key-u" => Key::KeyU,
        "key-v" => Key::KeyV,
        "key-w" => Key::KeyW,
        "key-x" => Key::KeyX,
        "key-y" => Key::KeyY,
        "key-z" => Key::KeyZ,
        "num-0" => Key::Num0,
        "num-1" => Key::Num1,
        "num-2" => Key::Num2,
        "num-3" => Key::Num3,
        "num-4" => Key::Num4,
        "num-5" => Key::Num5,
        "num-6" => Key::Num6,
        "num-7" => Key::Num7,
        "num-8" => Key::Num8,
        "num-9" => Key::Num9,
        "escape" => Key::Escape,
        "return" => Key::Return,
        "space" => Key::Space,
        "tab" => Key::Tab,
        "backspace" => Key::Backspace,
        "delete" => Key::Delete,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "home" => Key::Home,
        "end" => Key::End,
        "page-up" => Key::PageUp,
        "page-down" => Key::PageDown,
        _ => return Err(format!("unsupported key: {name}")),
    };
    Ok(key)
}

fn parse_press(name: &str) -> Result<PressType, String> {
    match name {
        "down" => Ok(PressType::Down),
        "up" => Ok(PressType::Up),
        _ => Err(format!("unsupported press: {name}")),
    }
}

fn parse_modifiers(names: &[String]) -> Result<ModifierFlags, String> {
    let mut flags = ModifierFlags::empty();
    for name in names {
        let flag = match name.as_str() {
            "control" => ModifierFlags::CONTROL,
            "command" => ModifierFlags::COMMAND,
            "alt" => ModifierFlags::ALT,
            "shift" => ModifierFlags::SHIFT,
            "fn" => ModifierFlags::FN,
            _ => return Err(format!("unsupported modifier: {name}")),
        };
        if flags.contains(flag) {
            return Err(format!("repeated modifier: {name}"));
        }
        flags.set(flag, true);
    }
    Ok(flags)
}

fn valid_bundle_id(bundle_id: &str) -> bool {
    !bundle_id.is_empty()
        && bundle_id.len() <= 255
        && bundle_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::{AutomationCommand, WireCommand};
    use freddie_keys::{Key, ModifierFlags};

    #[test]
    fn tap_command_converts_to_a_chord() {
        let command: WireCommand = serde_json::from_str(
            r#"{"kind":"AutomationCommand.Tap","value":{"key":"key-r","modifiers":["command","shift"]}}"#,
        )
        .expect("fixture deserializes");
        let AutomationCommand::Tap(tap) =
            AutomationCommand::try_from(command).expect("fixture converts")
        else {
            panic!("fixture is a tap");
        };
        assert_eq!(tap.key, Key::KeyR);
        assert_eq!(tap.flags, ModifierFlags::COMMAND | ModifierFlags::SHIFT);
    }

    #[test]
    fn unsupported_commands_are_rejected() {
        for command in [
            r#"{"kind":"AutomationCommand.Tap","value":{"key":"f24","modifiers":[]}}"#,
            r#"{"kind":"AutomationCommand.Emit","value":{"key":"key-a","press":"hold","modifiers":[]}}"#,
            r#"{"kind":"AutomationCommand.Foreground","value":{"bundle_id":"com.apple.Text Edit"}}"#,
        ] {
            let command: WireCommand = serde_json::from_str(command).expect("fixture deserializes");
            assert!(AutomationCommand::try_from(command).is_err());
        }
    }
}
