//! Which daemon a command line named, and where that daemon's files go.

use std::fmt;
use std::path::{Path, PathBuf};

/// One daemon: `slug` names its lock and log files; `display_name` is what the person typed.
#[derive(Clone, Debug)]
pub struct Instance {
    slug: String,
    display_name: String,
    /// Written as the log record's `target` on the app crate's records, so `logs` and
    /// `LOG_LEVEL` name the app.
    tracing_target: String,
    lock_file: PathBuf,
    log_dir: PathBuf,
    log_file_name: String,
}

impl Instance {
    /// The one daemon of an app that has one, keyed to the app itself.
    ///
    /// # Errors
    ///
    /// [`NoUserDir`] when the environment names no per-user directory.
    pub fn global(app: &str) -> Result<Self, NoUserDir> {
        Self::named(app, app, app)
    }

    /// One of many. `slug` is a filename, stable for the same daemon and distinct across daemons.
    ///
    /// # Errors
    ///
    /// [`NoUserDir`] when the environment names no per-user directory.
    pub fn named(
        app: &str,
        slug: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Result<Self, NoUserDir> {
        let slug = slug.into();
        Ok(Self {
            lock_file: freddie_single_instance::lock_path(&slug).ok_or(NoUserDir)?,
            log_dir: log_dir(app)?,
            log_file_name: format!("{slug}.log"),
            display_name: display_name.into(),
            tracing_target: app.to_owned(),
            slug,
        })
    }

    /// The file whose exclusive lock is this daemon's claim to being the only one of itself.
    #[must_use]
    pub fn lock_file(&self) -> &Path {
        &self.lock_file
    }

    /// The directory this daemon's log goes in, one per app.
    ///
    /// Split from the file name because `tracing_appender` takes them that way, and the
    /// directory has to exist before the file is opened.
    #[must_use]
    pub fn log_dir(&self) -> &Path {
        &self.log_dir
    }

    /// The name of this daemon's log file, one per daemon.
    #[must_use]
    pub fn log_file_name(&self) -> &str {
        &self.log_file_name
    }

    /// `log_dir` joined with `log_file_name`.
    #[must_use]
    pub fn log_file(&self) -> PathBuf {
        self.log_dir.join(&self.log_file_name)
    }

    /// What a verb calls this daemon when it says something about it.
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// The tracing target written into this daemon's log records.
    #[must_use]
    pub fn tracing_target(&self) -> &str {
        &self.tracing_target
    }

    /// Use `target` as the tracing target instead of the app name.
    #[must_use]
    pub fn with_tracing_target(mut self, target: impl Into<String>) -> Self {
        self.tracing_target = target.into();
        self
    }

    /// What this daemon's files are keyed to.
    #[must_use]
    pub fn slug(&self) -> &str {
        &self.slug
    }
}

/// The environment names no per-user directory for this daemon's lock and log.
#[derive(Debug)]
pub struct NoUserDir;

impl fmt::Display for NoUserDir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("no per-user directory to keep the lock and the log in; is HOME set?")
    }
}

impl std::error::Error for NoUserDir {}

/// This user's home. Unix only; Windows logs go under `%LOCALAPPDATA%`.
#[cfg(unix)]
fn home() -> Result<PathBuf, NoUserDir> {
    std::env::var_os("HOME").map(PathBuf::from).ok_or(NoUserDir)
}

/// Per-user directory for `app`'s logs, beside the single-instance lock.
#[cfg(target_os = "macos")]
fn log_dir(app: &str) -> Result<PathBuf, NoUserDir> {
    Ok(home()?.join("Library/Logs").join(app))
}

/// `$XDG_STATE_HOME`, defaulting to `~/.local/state`. XDG has no log directory of its own.
#[cfg(all(unix, not(target_os = "macos")))]
fn log_dir(app: &str) -> Result<PathBuf, NoUserDir> {
    let base = match std::env::var_os("XDG_STATE_HOME") {
        Some(base) => PathBuf::from(base),
        None => home()?.join(".local/state"),
    };
    Ok(base.join(app))
}

/// `%LOCALAPPDATA%`, per-machine: a roaming profile must not sync one machine's log onto another.
#[cfg(windows)]
fn log_dir(app: &str) -> Result<PathBuf, NoUserDir> {
    let base = std::env::var_os("LOCALAPPDATA").ok_or(NoUserDir)?;
    Ok(PathBuf::from(base).join(app).join("logs"))
}

#[cfg(not(any(unix, target_os = "windows")))]
compile_error!("freddie_cli has no per-user log directory for this platform");

#[cfg(test)]
mod tests {
    use super::Instance;

    #[test]
    fn a_global_instance_is_named_for_its_app() {
        let instance = Instance::global("testapp").expect("HOME is set in a test run");
        assert_eq!(instance.slug(), "testapp");
        assert_eq!(instance.display_name(), "testapp");
        assert_eq!(instance.tracing_target(), "testapp");
        assert_eq!(instance.log_file_name(), "testapp.log");
    }

    #[test]
    fn two_instances_share_no_file() {
        let a = Instance::named("testapp", "testapp-a", "./a.json").expect("HOME is set");
        let b = Instance::named("testapp", "testapp-b", "./b.json").expect("HOME is set");
        assert_ne!(a.lock_file(), b.lock_file());
        assert_ne!(a.log_file(), b.log_file());
        assert_eq!(a.log_dir(), b.log_dir());
    }

    #[test]
    fn the_display_name_is_what_was_typed() {
        let instance = Instance::named("testapp", "testapp-a", "./a.json").expect("HOME is set");
        assert_eq!(instance.display_name(), "./a.json");
    }

    #[test]
    fn the_tracing_target_is_the_app_name_until_replaced() {
        let instance = Instance::named("testapp", "testapp-a", "./a.json").expect("HOME is set");
        assert_eq!(instance.tracing_target(), "testapp");
        assert_eq!(
            instance.with_tracing_target("other").tracing_target(),
            "other"
        );
    }
}
