//! Tracing for a daemon: a log file at [`FILE_LEVEL`], and a terminal filtered by [`LOG_LEVEL`].
//!
//! One file per daemon. Every record is stamped with the pid of the process that wrote it.

use std::cell::RefCell;
use std::io;
use std::sync::OnceLock;

use tracing::{Level, warn};

use crate::Instance;
use tracing_subscriber::filter::{self, LevelFilter};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, fmt};

/// Terminal filter when [`LOG_LEVEL`] is unset. Shared with `logs`.
pub(crate) const DEFAULT_LOG_LEVEL: &str = "info";

/// What the log file records. Independent of the terminal filter.
const FILE_LEVEL: LevelFilter = LevelFilter::DEBUG;

/// Writer wrapper that stamps each record with this process's pid and rewrites `target`
/// to the app name. Tracing's `target:` is a compile-time callsite string, so a library
/// cannot name the app at the macro. This crate's records keep the module (`::client`,
/// `::daemon`) under the app name.
struct WithPid<W> {
    inner: W,
    target: String,
}

impl<'a, W: MakeWriter<'a>> MakeWriter<'a> for WithPid<W> {
    type Writer = PidStamped<'a, W::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        PidStamped {
            inner: self.inner.make_writer(),
            target: self.target.as_str(),
        }
    }
}

struct PidStamped<'a, W> {
    inner: W,
    target: &'a str,
}

impl<W: io::Write> io::Write for PidStamped<'_, W> {
    /// One `write_all` for the stamp and the record together. Two appends would let
    /// another process write between them. A record that does not start with `{` is
    /// written through untouched; splicing would destroy it.
    fn write(&mut self, record: &[u8]) -> io::Result<usize> {
        STAMPED.with_borrow_mut(|line| {
            line.clear();
            match record.strip_prefix(b"{") {
                Some(rest) => {
                    line.extend_from_slice(stamp().as_bytes());
                    line.extend_from_slice(rest);
                    put_target(line, self.target);
                }
                None => line.extend_from_slice(record),
            }
            self.inner.write_all(line)
        })?;
        Ok(record.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

const TARGET_KEY: &[u8] = b"\"target\":\"";
const FREDDIE_CLI: &[u8] = b"freddie_cli";

/// How [`put_target`] rewrites a record's tracing target.
enum RecordTarget {
    /// App crate module path (`isograph_cli`, `figaro::daemon`): the whole value becomes the app name.
    AppCrate,
    /// This crate's module path (`freddie_cli::client`): `freddie_cli` becomes the app name and the module stays.
    FreddieCli,
    /// An explicit `target:` or a foreign crate.
    Other,
}

/// Rewrite a crate module path so `logs` and `LOG_LEVEL` name the app.
fn put_target(line: &mut Vec<u8>, app: &str) {
    let Some(key_at) = line.windows(TARGET_KEY.len()).position(|w| w == TARGET_KEY) else {
        return;
    };
    let value_at = key_at + TARGET_KEY.len();
    let Some(value_len) = line[value_at..].iter().position(|&b| b == b'"') else {
        return;
    };
    match record_target(&line[value_at..value_at + value_len], app) {
        RecordTarget::AppCrate => {
            line.splice(value_at..value_at + value_len, app.bytes());
        }
        RecordTarget::FreddieCli => {
            line.splice(value_at..value_at + FREDDIE_CLI.len(), app.bytes());
        }
        RecordTarget::Other => {}
    }
}

fn record_target(current: &[u8], app: &str) -> RecordTarget {
    if let Some(rest) = current.strip_prefix(app.as_bytes())
        && matches!(rest, [] | [b':', b':', ..] | [b'_', ..])
    {
        return RecordTarget::AppCrate;
    }
    if let Some(rest) = current.strip_prefix(FREDDIE_CLI)
        && matches!(rest, [] | [b':', b':', ..])
    {
        return RecordTarget::FreddieCli;
    }
    RecordTarget::Other
}

thread_local! {
    /// Reused across records so a debug-per-event daemon does not allocate per line.
    static STAMPED: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// `{"pid":N,` spliced in front of a record whose own opening brace has been stripped.
fn stamp() -> &'static str {
    static STAMP: OnceLock<String> = OnceLock::new();
    STAMP.get_or_init(|| format!("{{\"pid\":{},", std::process::id()))
}

/// Send tracing to this daemon's log file and to this process's terminal.
///
/// A daemon reads [`LOG_LEVEL`] as a `tracing_subscriber` filter string. A value that
/// does not parse falls back to [`DEFAULT_LOG_LEVEL`].
pub(crate) fn init(instance: &Instance, terminal: Terminal) {
    let dir = instance.log_dir();
    // No subscriber yet; a setup failure is logged after `init` below.
    let mut setup = Vec::new();
    if let Err(e) = std::fs::create_dir_all(dir) {
        setup.push(format!("could not create {}: {e}", dir.display()));
    }

    let file = fmt::layer()
        .json()
        // Flat object: event fields sit beside `timestamp` and `target`, not nested under `fields`.
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(false)
        .with_writer(WithPid {
            inner: tracing_appender::rolling::never(dir, instance.log_file_name()),
            target: instance.tracing_target().to_owned(),
        })
        .with_ansi(false)
        .with_filter(FILE_LEVEL);

    let registry = tracing_subscriber::registry().with(file);
    match terminal {
        Terminal::Daemon => {
            let directives =
                std::env::var(LOG_LEVEL).unwrap_or_else(|_| DEFAULT_LOG_LEVEL.to_owned());
            let filter = EnvFilter::try_new(&directives).unwrap_or_else(|e| {
                setup.push(format!(
                    "{LOG_LEVEL}={directives:?} is not a log filter ({e}); using {DEFAULT_LOG_LEVEL}"
                ));
                EnvFilter::new(DEFAULT_LOG_LEVEL)
            });
            registry
                .with(fmt::layer().with_writer(io::stderr).with_filter(filter))
                .init();
        }
        Terminal::Client => registry.with(client_terminal()).init(),
    }

    for problem in setup {
        warn!("{}: {problem}", instance.display_name());
    }
    log_panics();
}

/// Log every panic, then abort.
///
/// A detached daemon has no terminal for the default hook. Aborting (not unwinding) is
/// required: a panic on the main thread would leave the worker holding the keyboard, and
/// unwinding through an `AppKit` or `AXObserver` C frame is undefined behavior. The file
/// layer writes each record straight through, so `error!` has reached the OS before `abort`.
/// Tests do not install this hook, so `catch_unwind` still works there.
fn log_panics() {
    std::panic::set_hook(Box::new(|info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panicked".to_owned());
        let location = info
            .location()
            .map_or_else(|| "unknown".to_owned(), ToString::to_string);
        let backtrace = std::backtrace::Backtrace::capture();
        tracing::error!(%location, %backtrace, "panic: {message}");
        std::process::abort();
    }));
}

#[cfg(test)]
mod tests {
    use super::{PidStamped, WithPid};
    use std::io::Write;

    #[test]
    fn the_file_layer_writes_a_flat_record_in_logged_order() {
        let path =
            std::env::temp_dir().join(format!("freddie_cli_seam_{}.log", std::process::id()));
        let file = std::fs::File::create(&path).expect("a temp log file");
        let subscriber = tracing_subscriber::fmt()
            .json()
            .flatten_event(true)
            .with_current_span(false)
            .with_span_list(false)
            .with_ansi(false)
            .with_writer(WithPid {
                inner: file,
                target: "mercury".to_owned(),
            })
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(
                target: "mercury::daemon",
                event = "Key(KeyR)",
                effects = "[]",
                state = "Mercury { .. }",
                "dispatch"
            );
        });

        let line = std::fs::read(&path).expect("the temp log file");
        std::fs::remove_file(&path).ok();
        let record: serde_json::Value = serde_json::from_slice(&line).expect("a record");
        assert_eq!(record["pid"], serde_json::json!(std::process::id()));
        assert!(record["timestamp"].is_string());
        assert_eq!(record["level"], serde_json::json!("INFO"));
        assert_eq!(record["target"], serde_json::json!("mercury"));
        assert_eq!(record["message"], serde_json::json!("dispatch"));
        assert_eq!(record["event"], serde_json::json!("Key(KeyR)"));
        assert_eq!(record["state"], serde_json::json!("Mercury { .. }"));

        let keys: Vec<&String> = record.as_object().expect("an object").keys().collect();
        let at = |k: &str| keys.iter().position(|key| *key == k).expect("the key");
        assert!(at("event") < at("effects"), "event should precede effects");
    }

    #[test]
    fn a_line_that_is_not_an_object_is_written_through() {
        let mut written = Vec::new();
        PidStamped {
            inner: &mut written,
            target: "mercury",
        }
        .write_all(b"a stray line")
        .expect("writing to a Vec");
        assert_eq!(written, b"a stray line");
    }

    #[test]
    fn put_target_rewrites_the_app_crate_to_the_app_name() {
        let mut line =
            br#"{"pid":1,"target":"isograph_cli","message":"hello from isograph"}"#.to_vec();
        super::put_target(&mut line, "isograph");
        assert_eq!(
            line,
            br#"{"pid":1,"target":"isograph","message":"hello from isograph"}"#
        );
    }

    #[test]
    fn put_target_rewrites_a_module_under_the_app() {
        let mut line = br#"{"pid":1,"target":"figaro::daemon","message":"dispatch"}"#.to_vec();
        super::put_target(&mut line, "figaro");
        assert_eq!(line, br#"{"pid":1,"target":"figaro","message":"dispatch"}"#);
    }

    #[test]
    fn put_target_rewrites_this_crate_under_the_app_name() {
        let mut line = br#"{"pid":1,"target":"freddie_cli::daemon","message":"logging"}"#.to_vec();
        super::put_target(&mut line, "isograph");
        assert_eq!(
            line,
            br#"{"pid":1,"target":"isograph::daemon","message":"logging"}"#
        );
    }

    #[test]
    fn put_target_rewrites_a_client_record_under_the_app_name() {
        let mut line =
            br#"{"pid":1,"target":"freddie_cli::client","message":"isograph started"}"#.to_vec();
        super::put_target(&mut line, "isograph");
        assert_eq!(
            line,
            br#"{"pid":1,"target":"isograph::client","message":"isograph started"}"#
        );
    }

    #[test]
    fn put_target_leaves_an_explicit_target() {
        let mut line = br#"{"pid":1,"target":"keystroke","message":"tapped"}"#.to_vec();
        super::put_target(&mut line, "figaro");
        assert_eq!(
            line,
            br#"{"pid":1,"target":"keystroke","message":"tapped"}"#
        );
    }
}

/// Environment variable for a daemon's terminal filter. Not a flag: `daemon` is hidden
/// and has no terminal (`start` sends its output to /dev/null).
pub const LOG_LEVEL: &str = "LOG_LEVEL";

/// What this process's terminal is for.
#[derive(Clone, Copy)]
pub(crate) enum Terminal {
    /// Full records, filtered by [`LOG_LEVEL`].
    Daemon,
    /// `INFO` to stdout, `WARN` and above to stderr, bare message only.
    Client,
}

/// Client terminal: no timestamp/level/target, so `status` stays pipeline-usable.
/// Two layers because a layer has one writer: `INFO` exactly to stdout, `WARN`+ to stderr.
fn client_terminal<S>() -> impl Layer<S>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    let results = fmt::layer()
        .without_time()
        .with_level(false)
        .with_target(false)
        .with_ansi(false)
        .with_writer(io::stdout)
        .with_filter(filter::filter_fn(|meta| *meta.level() == Level::INFO));
    let problems = fmt::layer()
        .without_time()
        .with_level(false)
        .with_target(false)
        .with_ansi(false)
        .with_writer(io::stderr)
        .with_filter(LevelFilter::WARN);
    results.and_then(problems)
}
