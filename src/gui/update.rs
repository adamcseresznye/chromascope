//! Tells the user, politely, when a newer release exists.
//!
//! Once a day at most, in a background thread after the window is open, the app asks GitHub for
//! `releases/latest` WITHOUT following the redirect and reads the version from the `Location`
//! header (`.../releases/tag/vX.Y.Z`). That is one ordinary web request: it carries no user name,
//! no file name and no data, and it does not use the GitHub API (whose rate limit is shared by
//! everyone behind the same institute address). A failure of any kind (offline, firewall, timeout)
//! is logged at debug level and never shown. Nothing is downloaded or installed: the notice only
//! offers to open the release page in the system browser.
//!
//! The desktop windows are the only users of this module. The command-line tool and the headless
//! MCP server never contact the network.

use eframe::egui;
use semver::Version;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

/// The page whose redirect names the newest release, and the page the Download link opens. The
/// address opened is always this constant: nothing read from the network is ever opened.
pub(super) const RELEASES_LATEST_URL: &str =
    "https://github.com/chromascope-ms/chromascope/releases/latest";
/// Set to `1` to switch the check off (laboratories, CI).
pub(super) const OPT_OUT_VARIABLE: &str = "CHROMASCOPE_NO_UPDATE_CHECK";
/// At most one request per this many seconds.
pub(super) const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;
/// The whole request, connecting included.
const TIMEOUT: Duration = Duration::from_secs(5);

/// What is kept between sessions, in `settings.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Prefs {
    /// The "Check for updates" setting (default on).
    pub check_for_updates: bool,
    /// When the last request was started, in seconds since 1970.
    pub last_check: Option<u64>,
    /// The version whose notice the user dismissed.
    pub dismissed_version: Option<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            check_for_updates: true,
            last_check: None,
            dismissed_version: None,
        }
    }
}

/// How the releases page answered. `Ok(None)`: it did not redirect (no release exists yet).
pub(super) type Reply = Result<Option<String>, String>;

/// The one network call, behind a trait so that tests never touch the network.
pub(super) trait Fetch: Send + Sync {
    /// The `Location` header of `releases/latest`, redirect not followed.
    fn latest_location(&self) -> Reply;
}

/// What a check found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    /// A release newer than the running version exists.
    Newer(Version),
    /// The newest release is this version or older.
    UpToDate,
    /// No release, or an answer that names no version: nothing to say.
    Unknown,
    /// Offline, timeout, refused: nothing to say.
    Failed(String),
}

/// True when the environment variable asks for no check.
pub(super) fn opted_out_by_environment(value: Option<&str>) -> bool {
    value.is_some_and(|v| {
        let v = v.trim().to_ascii_lowercase();
        !(v.is_empty() || v == "0" || v == "false" || v == "no")
    })
}

/// The version in a redirect target such as
/// `https://github.com/chromascope-ms/chromascope/releases/tag/v0.4.0` (or the same path
/// without the host). `None` when there is no such tag or it is not a semantic version.
pub(super) fn version_from_location(location: &str) -> Option<Version> {
    const MARKER: &str = "/releases/tag/";
    let path = location.split(['?', '#']).next()?;
    let tag = path[path.find(MARKER)? + MARKER.len()..].trim_end_matches('/');
    if tag.contains('/') {
        return None;
    }
    Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()
}

/// Asks, and compares with `current`.
pub(super) fn check(fetch: &dyn Fetch, current: &Version) -> Outcome {
    match fetch.latest_location() {
        Err(e) => Outcome::Failed(e),
        Ok(None) => Outcome::Unknown,
        Ok(Some(location)) => match version_from_location(&location) {
            None => Outcome::Unknown,
            Some(latest) if latest > *current => Outcome::Newer(latest),
            Some(_) => Outcome::UpToDate,
        },
    }
}

/// The version the running program is.
pub(super) fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("the package version is a semantic version")
}

/// The real request.
pub(super) struct GitHub;

impl Fetch for GitHub {
    fn latest_location(&self) -> Reply {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .max_redirects(0)
            .max_redirects_will_error(false)
            .http_status_as_error(false)
            .user_agent(concat!("Chromascope/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        let response = agent
            .get(RELEASES_LATEST_URL)
            .call()
            .map_err(|e| e.to_string())?;
        if !response.status().is_redirection() {
            return Ok(None);
        }
        Ok(response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned))
    }
}

/// What the window knows about updates during a session.
pub(super) struct Updates {
    /// Where the answer comes from: GitHub, or a fake in tests.
    fetch: Arc<dyn Fetch>,
    /// The version of the running program.
    current: Version,
    /// `CHROMASCOPE_NO_UPDATE_CHECK` was set when the program started.
    opted_out: bool,
    /// The newer version a check found.
    available: Option<Version>,
    /// The answer of a request in flight.
    pending: Option<Receiver<Outcome>>,
}

impl Default for Updates {
    fn default() -> Self {
        let mut updates = Self::new(Arc::new(GitHub), current_version());
        updates.opted_out =
            opted_out_by_environment(std::env::var(OPT_OUT_VARIABLE).ok().as_deref());
        updates
    }
}

impl Updates {
    pub(super) fn new(fetch: Arc<dyn Fetch>, current: Version) -> Self {
        Self {
            fetch,
            current,
            opted_out: false,
            available: None,
            pending: None,
        }
    }

    /// True when the environment variable switches the check off (the View menu says so).
    pub(super) fn opted_out(&self) -> bool {
        self.opted_out
    }

    /// The newer version for `get_settings`: whether or not its notice was dismissed, and `None`
    /// while the check is off.
    #[cfg(feature = "mcp")]
    pub(super) fn reported(&self, prefs: &Prefs) -> Option<String> {
        self.available
            .as_ref()
            .filter(|_| prefs.check_for_updates && !self.opted_out)
            .map(Version::to_string)
    }

    /// The version to announce in the status bar: a newer one the user has not dismissed, while
    /// the check is on.
    pub(super) fn notice(&self, prefs: &Prefs) -> Option<&Version> {
        self.available.as_ref().filter(|v| {
            prefs.check_for_updates
                && prefs.dismissed_version.as_deref() != Some(v.to_string().as_str())
        })
    }

    /// Hides the notice for the version it announces. True when `prefs` changed.
    pub(super) fn dismiss(&self, prefs: &mut Prefs) -> bool {
        let Some(version) = self.available.as_ref().map(Version::to_string) else {
            return false;
        };
        if prefs.dismissed_version.as_deref() == Some(&version) {
            return false;
        }
        prefs.dismissed_version = Some(version);
        true
    }

    /// Called every frame: collects an answer, and starts a check when one is due. `now` is in
    /// seconds since 1970. `repaint` wakes the window when the thread has an answer. True when
    /// `prefs` changed and must be saved.
    pub(super) fn tick(
        &mut self,
        prefs: &mut Prefs,
        now: u64,
        repaint: impl Fn() + Send + 'static,
    ) -> bool {
        if self.opted_out || !prefs.check_for_updates {
            // Switched off: forget what was found and ignore an answer still on its way.
            self.available = None;
            self.pending = None;
            return false;
        }
        if let Some(receiver) = &self.pending {
            match receiver.try_recv() {
                Ok(outcome) => {
                    self.pending = None;
                    match outcome {
                        Outcome::Newer(version) => self.available = Some(version),
                        Outcome::UpToDate => self.available = None,
                        Outcome::Unknown => log::debug!("Update check: no release named"),
                        Outcome::Failed(e) => log::debug!("Update check failed: {e}"),
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.pending = None,
            }
            return false;
        }
        if !due(prefs, now) {
            return false;
        }
        // Recorded when the request starts: an offline computer asks once a day, not at every start.
        prefs.last_check = Some(now);
        let (sender, receiver) = mpsc::channel();
        let (fetch, current) = (Arc::clone(&self.fetch), self.current.clone());
        let started = std::thread::Builder::new()
            .name("update-check".into())
            .spawn(move || {
                let _ = sender.send(check(fetch.as_ref(), &current));
                repaint();
            });
        match started {
            Ok(_) => self.pending = Some(receiver),
            Err(e) => log::debug!("Update check not started: {e}"),
        }
        true
    }
}

/// What the user clicked in the notice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Clicked {
    Nothing,
    Download,
    Dismiss,
}

/// "Chromascope X.Y.Z is available — Download · Dismiss" on the line `ui` is in.
fn notice_line(version: &Version, ui: &mut egui::Ui) -> Clicked {
    let mut clicked = Clicked::Nothing;
    ui.separator();
    ui.small(format!("Chromascope {version} is available —"));
    if ui
        .small_button("Download")
        .on_hover_text("Open the release page in your browser. Chromascope downloads and installs nothing itself.")
        .clicked()
    {
        clicked = Clicked::Download;
    }
    ui.small("·");
    if ui
        .small_button("Dismiss")
        .on_hover_text("Hide this notice for this version")
        .clicked()
    {
        clicked = Clicked::Dismiss;
    }
    clicked
}

/// The notice on the status bar line `ui` is in, while a newer version is known and not
/// dismissed. Download opens the release page in the system browser; Dismiss hides the notice
/// for that version only. Nothing is downloaded or installed by the program.
pub(super) fn show_notice(updates: &Updates, ui: &mut egui::Ui) {
    let Some(version) = super::settings::with_update_prefs(|prefs| updates.notice(prefs).cloned())
    else {
        return;
    };
    match notice_line(&version, ui) {
        Clicked::Nothing => {}
        Clicked::Download => ui
            .ctx()
            .open_url(egui::OpenUrl::new_tab(RELEASES_LATEST_URL)),
        Clicked::Dismiss => {
            if super::settings::with_update_prefs(|prefs| updates.dismiss(prefs)) {
                super::settings::save(ui.ctx());
            }
        }
    }
}

/// The same notice on a bar of its own, for the workspaces that have no status bar. Draws
/// nothing while there is no notice.
pub(super) fn show_notice_bar(updates: &Updates, ctx: &egui::Context) {
    if super::settings::with_update_prefs(|prefs| updates.notice(prefs).is_none()) {
        return;
    }
    egui::TopBottomPanel::bottom("update_notice")
        .frame(super::theme::bar_frame(ctx, 4).fill(ctx.style().visuals.faint_bg_color))
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| show_notice(updates, ui));
        });
}

/// True when the last request is older than a day (or was never made, or lies in the future
/// because the clock was set back).
pub(super) fn due(prefs: &Prefs, now: u64) -> bool {
    match prefs.last_check {
        None => true,
        Some(last) => now < last || now - last >= CHECK_INTERVAL_SECS,
    }
}

/// Seconds since 1970 (0 when the clock is before it).
pub(super) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Helpers for the tests that need to look inside or to make a notice without a network.
impl Updates {
    /// The newer version a check found, whether or not its notice was dismissed.
    #[cfg(test)]
    pub(super) fn available(&self) -> Option<&Version> {
        self.available.as_ref()
    }

    /// An `Updates` that has found `version` and could not reach the network anyway (for tests
    /// that need a notice).
    #[cfg(all(test, feature = "mcp"))]
    pub(super) fn offline_with(available: Option<Version>) -> Self {
        struct Offline;
        impl Fetch for Offline {
            fn latest_location(&self) -> Reply {
                Err("tests have no network".into())
            }
        }
        let mut updates = Self::new(Arc::new(Offline), current_version());
        updates.available = available;
        updates
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    /// Answers with a fixed reply and counts the requests.
    struct Fake {
        reply: Reply,
        calls: AtomicUsize,
    }

    impl Fake {
        fn new(reply: Reply) -> Arc<Self> {
            Arc::new(Self {
                reply,
                calls: AtomicUsize::new(0),
            })
        }
        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl Fetch for Fake {
        fn latest_location(&self) -> Reply {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.reply.clone()
        }
    }

    fn tag(version: &str) -> Reply {
        Ok(Some(format!(
            "https://github.com/chromascope-ms/chromascope/releases/tag/v{version}"
        )))
    }

    fn current() -> Version {
        Version::new(0, 3, 0)
    }

    #[test]
    fn a_newer_release_is_reported() {
        assert_eq!(
            check(&*Fake::new(tag("0.4.0")), &current()),
            Outcome::Newer(Version::new(0, 4, 0))
        );
        assert_eq!(
            check(&*Fake::new(tag("0.3.1")), &current()),
            Outcome::Newer(Version::new(0, 3, 1))
        );
        // Numbers, not text: 0.10.0 is newer than 0.9.0.
        assert_eq!(
            check(&*Fake::new(tag("0.10.0")), &Version::new(0, 9, 0)),
            Outcome::Newer(Version::new(0, 10, 0))
        );
    }

    #[test]
    fn the_same_or_an_older_release_is_not_news() {
        assert_eq!(
            check(&*Fake::new(tag("0.3.0")), &current()),
            Outcome::UpToDate
        );
        assert_eq!(
            check(&*Fake::new(tag("0.2.0")), &current()),
            Outcome::UpToDate
        );
        // A pre-release of the running version is older than the release itself.
        assert_eq!(
            check(&*Fake::new(tag("0.3.0-rc.1")), &current()),
            Outcome::UpToDate
        );
        // A development build ahead of the newest release says nothing.
        assert_eq!(
            check(&*Fake::new(tag("0.2.0")), &Version::new(0, 3, 0)),
            Outcome::UpToDate
        );
    }

    #[test]
    fn a_tag_that_is_not_a_version_is_ignored() {
        for location in [
            "https://github.com/chromascope-ms/chromascope/releases/tag/nightly",
            "https://github.com/chromascope-ms/chromascope/releases/tag/v1.2",
            "https://github.com/chromascope-ms/chromascope/releases/tag/v1.2.x",
            "https://github.com/chromascope-ms/chromascope/releases/tag/",
            "https://github.com/chromascope-ms/chromascope/releases/tag/v1.0.0/extra",
            "https://github.com/chromascope-ms/chromascope/releases",
            "https://example.com/login",
            "",
        ] {
            assert_eq!(
                check(&*Fake::new(Ok(Some(location.into()))), &current()),
                Outcome::Unknown,
                "{location:?}"
            );
        }
    }

    #[test]
    fn the_version_is_read_from_a_full_or_a_relative_location() {
        let v040 = Some(Version::new(0, 4, 0));
        for location in [
            "https://github.com/chromascope-ms/chromascope/releases/tag/v0.4.0",
            "/chromascope-ms/chromascope/releases/tag/v0.4.0",
            "/chromascope-ms/chromascope/releases/tag/0.4.0",
            "/chromascope-ms/chromascope/releases/tag/v0.4.0/",
            "/chromascope-ms/chromascope/releases/tag/v0.4.0?expanded=true",
        ] {
            assert_eq!(version_from_location(location), v040, "{location}");
        }
    }

    #[test]
    fn no_redirect_means_no_update() {
        assert_eq!(check(&*Fake::new(Ok(None)), &current()), Outcome::Unknown);
    }

    #[test]
    fn offline_or_timeout_is_a_quiet_failure() {
        let outcome = check(&*Fake::new(Err("timed out".into())), &current());
        assert_eq!(outcome, Outcome::Failed("timed out".into()));
    }

    #[test]
    fn the_environment_variable_switches_the_check_off() {
        assert!(opted_out_by_environment(Some("1")));
        assert!(opted_out_by_environment(Some("true")));
        assert!(!opted_out_by_environment(None));
        assert!(!opted_out_by_environment(Some("")));
        assert!(!opted_out_by_environment(Some("0")));
    }

    #[test]
    fn a_check_is_due_after_a_day_and_not_before() {
        let mut prefs = Prefs::default();
        assert!(due(&prefs, 1_000_000), "never checked");
        prefs.last_check = Some(1_000_000);
        assert!(!due(&prefs, 1_000_000));
        assert!(!due(&prefs, 1_000_000 + CHECK_INTERVAL_SECS - 1));
        assert!(due(&prefs, 1_000_000 + CHECK_INTERVAL_SECS));
        assert!(due(&prefs, 999_000), "the clock was set back");
    }

    /// An `Updates` that asks `fake` and runs as version 0.3.0.
    fn updates_with(fake: &Arc<Fake>) -> Updates {
        let fetch: Arc<dyn Fetch> = fake.clone();
        Updates::new(fetch, current())
    }

    /// Runs `tick` until the thread has answered (or fails after ten seconds).
    fn settle(updates: &mut Updates, prefs: &mut Prefs, now: u64) {
        let start = Instant::now();
        while updates.pending.is_some() {
            updates.tick(prefs, now, || {});
            assert!(start.elapsed() < Duration::from_secs(10), "no answer");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn one_request_a_day_whatever_the_number_of_frames() {
        let fake = Fake::new(tag("0.4.0"));
        let (mut updates, mut prefs) = (updates_with(&fake), Prefs::default());
        let now = 5_000_000;
        assert!(updates.tick(&mut prefs, now, || {}));
        assert_eq!(
            prefs.last_check,
            Some(now),
            "the time is kept for the next start"
        );
        settle(&mut updates, &mut prefs, now);
        assert_eq!(updates.available(), Some(&Version::new(0, 4, 0)));
        for frame in 1..100 {
            assert!(!updates.tick(&mut prefs, now + frame, || {}));
        }
        assert_eq!(fake.calls(), 1);
        // A restart within the day (the time comes back from settings.json) asks nothing.
        let mut restarted = updates_with(&fake);
        assert!(!restarted.tick(&mut prefs, now + 3600, || {}));
        assert_eq!(fake.calls(), 1);
        // The next day it asks again.
        let tomorrow = now + CHECK_INTERVAL_SECS;
        assert!(restarted.tick(&mut prefs, tomorrow, || {}));
        settle(&mut restarted, &mut prefs, tomorrow);
        assert_eq!(fake.calls(), 2);
    }

    #[test]
    fn a_failed_check_is_not_retried_before_tomorrow_and_shows_nothing() {
        let fake = Fake::new(Err("offline".into()));
        let (mut updates, mut prefs) = (updates_with(&fake), Prefs::default());
        updates.tick(&mut prefs, 100, || {});
        settle(&mut updates, &mut prefs, 100);
        assert_eq!(updates.available(), None);
        assert_eq!(updates.notice(&prefs), None);
        assert!(!updates.tick(&mut prefs, 101, || {}));
        assert_eq!(fake.calls(), 1);
    }

    #[test]
    fn the_setting_off_or_the_variable_set_asks_nothing() {
        let fake = Fake::new(tag("0.4.0"));
        let mut updates = updates_with(&fake);
        let mut off = Prefs {
            check_for_updates: false,
            ..Prefs::default()
        };
        assert!(!updates.tick(&mut off, 100, || {}));
        let mut on = Prefs::default();
        updates.opted_out = true;
        assert!(!updates.tick(&mut on, 100, || {}));
        assert!(updates.pending.is_none());
        assert_eq!(on.last_check, None, "no check was recorded");
        assert_eq!(fake.calls(), 0);
    }

    #[test]
    fn turning_the_setting_off_removes_the_notice() {
        let fake = Fake::new(tag("0.4.0"));
        let (mut updates, mut prefs) = (updates_with(&fake), Prefs::default());
        updates.tick(&mut prefs, 100, || {});
        settle(&mut updates, &mut prefs, 100);
        assert!(updates.notice(&prefs).is_some());
        prefs.check_for_updates = false;
        assert_eq!(updates.notice(&prefs), None, "hidden at once");
        updates.tick(&mut prefs, 101, || {});
        assert_eq!(updates.available(), None);
    }

    #[test]
    fn dismissing_hides_that_version_only() {
        let fake = Fake::new(tag("0.4.0"));
        let (mut updates, mut prefs) = (updates_with(&fake), Prefs::default());
        updates.available = Some(Version::new(0, 4, 0));
        assert_eq!(updates.notice(&prefs), Some(&Version::new(0, 4, 0)));
        assert!(updates.dismiss(&mut prefs));
        assert_eq!(prefs.dismissed_version.as_deref(), Some("0.4.0"));
        assert_eq!(updates.notice(&prefs), None);
        assert!(!updates.dismiss(&mut prefs), "nothing new to save");
        // The dismissal survives a restart (the preferences come back from the file) ...
        let mut next_session = updates_with(&fake);
        next_session.available = Some(Version::new(0, 4, 0));
        assert_eq!(next_session.notice(&prefs), None);
        // ... but a later release is announced again.
        next_session.available = Some(Version::new(0, 4, 1));
        assert_eq!(next_session.notice(&prefs), Some(&Version::new(0, 4, 1)));
        // The AI-facing report is not hidden by a dismissal.
        assert_eq!(updates.available(), Some(&Version::new(0, 4, 0)));
    }

    #[test]
    fn nothing_to_dismiss_changes_nothing() {
        let mut prefs = Prefs::default();
        assert!(!updates_with(&Fake::new(Ok(None))).dismiss(&mut prefs));
        assert_eq!(prefs, Prefs::default());
    }

    /// Manual check against the real GitHub (the only test that uses the network, never run by
    /// default): `cargo test --lib real_github -- --ignored --nocapture`.
    #[test]
    #[ignore = "manual: contacts github.com"]
    fn real_github_names_the_latest_release_without_following_the_redirect() {
        let location = GitHub.latest_location();
        println!("Location header: {location:?}");
        println!(
            "outcome for {}: {:?}",
            current_version(),
            check(&GitHub, &current_version())
        );
        assert!(location.is_ok(), "{location:?}");
    }

    /// The text the notice draws, and the click it reports.
    #[test]
    fn the_notice_reads_as_one_polite_line_with_two_links() {
        let ctx = egui::Context::default();
        let mut clicked = Clicked::Dismiss;
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.horizontal(|ui| clicked = notice_line(&Version::new(0, 4, 0), ui));
            });
        });
        let mut text = Vec::new();
        for clipped in &output.shapes {
            if let egui::epaint::Shape::Text(shape) = &clipped.shape {
                text.push(shape.galley.text().to_owned());
            }
        }
        assert_eq!(
            text,
            [
                "Chromascope 0.4.0 is available —",
                "Download",
                "·",
                "Dismiss"
            ]
        );
        assert_eq!(clicked, Clicked::Nothing, "nothing is clicked by drawing");
    }

    #[test]
    fn the_running_version_parses() {
        assert_eq!(current_version().to_string(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn the_notice_and_the_request_use_the_organization_address() {
        assert_eq!(
            RELEASES_LATEST_URL,
            "https://github.com/chromascope-ms/chromascope/releases/latest"
        );
    }
}
