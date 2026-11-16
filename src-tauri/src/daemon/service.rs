//! The service this app talks to, once it is known.
//!
//! The app comes up before its service does: the window and the tray are
//! drawn at once, and the launch (`Daemon::connect_or_launch`) runs beside
//! them. Anything that has to know where the service is, or whose it is,
//! asks [`Service`], and while a launch is in flight the question waits for
//! the launch's answer instead of being given a guess: no address that
//! nothing listens on yet, no token the service has not minted yet, no
//! ownership decided before the launch decided it.
//!
//! Every launch settles. Its outcome is handed over through [`Launch`], and a
//! `Launch` dropped without one (the task panicked, or was cancelled) settles
//! as a failure on its way out, so no waiter is left waiting. The launch's
//! own budget (`LAUNCH_WAIT`, plus its first probe and its last poll) is
//! therefore the bound on every wait here, and there is no second clock.
//!
//! A failed launch keeps the ownership it had: a Start gglib that failed
//! after a daemon this app launched had stopped is still this app's, so a
//! daemon it spawned that answers late is found as this app's, and quitting
//! stops it.
//!
//! Pure: no `AppHandle`, no I/O, so every rule is tested without a window or
//! a service.

use std::sync::Arc;

use tokio::sync::watch;

use super::Ownership;

/// What the app knows about its service.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Known {
    /// A launch or a restart is in flight.
    Starting,
    /// It answered, and this is how the app came by it.
    Running(Ownership),
    /// The last launch ended without one, in words the page can show, and
    /// with the ownership it had.
    Failed(String, Ownership),
}

/// What a launch that never said how it ended is reported as.
const CUT_SHORT: &str = "gglib's start was cut short before it answered";

/// The service this app talks to, once known. See the module docs.
pub(crate) struct Service {
    known: Arc<watch::Sender<Known>>,
}

/// The one launch in flight, which is the only thing that can settle it.
///
/// Dropping it unsettled settles it as a failure.
pub(crate) struct Launch {
    known: Option<Arc<watch::Sender<Known>>>,
    /// The ownership a restart keeps, whether it succeeds or fails: a
    /// service started again is not re-decided (`Daemon::restart` says why).
    before: Ownership,
}

impl Service {
    /// A service whose first launch is in flight, and that launch.
    ///
    /// The app is never without a launch at startup, so the two are made
    /// together and nothing can ask before there is a launch to wait for.
    pub(crate) fn launching() -> (Self, Launch) {
        let known = Arc::new(watch::Sender::new(Known::Starting));
        let launch = Launch {
            known: Some(Arc::clone(&known)),
            before: Ownership::Unresolved,
        };
        (Self { known }, launch)
    }

    /// Start a launch (a restart), or `None` while one is already in flight,
    /// whose answer is then this caller's too ([`Self::settled`]).
    pub(crate) fn begin(&self) -> Option<Launch> {
        let mut before = None;
        self.known.send_if_modified(|known| {
            if *known == Known::Starting {
                return false;
            }
            before = Some(ownership_of(known));
            *known = Known::Starting;
            true
        });
        before.map(|before| Launch {
            known: Some(Arc::clone(&self.known)),
            before,
        })
    }

    /// How the app came by its service, once the launch in flight (if any)
    /// has settled.
    ///
    /// # Errors
    ///
    /// The failed launch's words.
    pub(crate) async fn settled(&self) -> Result<Ownership, String> {
        match self.settle_wait().await {
            Known::Running(ownership) => Ok(ownership),
            Known::Failed(words, _) => Err(words),
            Known::Starting => Err(CUT_SHORT.to_owned()),
        }
    }

    /// Whose the service is, once the launch in flight (if any) has settled,
    /// whether or not it came up: what quitting decides by.
    pub(crate) async fn ownership(&self) -> Ownership {
        ownership_of(&self.settle_wait().await)
    }

    /// The first state that is not [`Known::Starting`].
    async fn settle_wait(&self) -> Known {
        let mut known = self.known.subscribe();
        // The sender lives as long as `self`, so the wait cannot see it close.
        known
            .wait_for(|known| *known != Known::Starting)
            .await
            .map_or(Known::Starting, |known| known.clone())
    }

    /// Wait for the launch in flight, as a start that found one and joined
    /// it rather than starting a second.
    pub(crate) async fn join(&self) -> Started {
        Started::Joined(self.settled().await.map(drop))
    }

    /// Whether a launch is in flight now.
    pub(crate) fn is_starting(&self) -> bool {
        *self.known.borrow() == Known::Starting
    }

    /// The service answered, whoever started it.
    ///
    /// Clears a failed launch, so a daemon that answers after the launch
    /// gave up (one it spawned that came up late, or a `gglib` started from a
    /// terminal) is found rather than reported missing for good. It keeps
    /// the ownership the failed launch had: a daemon a Start gglib spawned
    /// after this app's own had stopped is still this app's to stop.
    /// Nothing changes while a launch is in flight (its own answer is
    /// coming) or once one is known.
    pub(crate) fn found(&self) {
        self.known.send_if_modified(|known| {
            let Known::Failed(_, ownership) = known else {
                return false;
            };
            *known = Known::Running(*ownership);
            true
        });
    }
}

/// How a Start gglib ended, and whether its caller is the one to report it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Started {
    /// This caller ran the launch, so a failure is its to report.
    Ran(Result<(), String>),
    /// A launch was already in flight and this caller waited for it. Its
    /// failure is the one that launch's owner reports: one failure, one
    /// report.
    Joined(Result<(), String>),
}

impl Started {
    /// The outcome, whoever ran it: what a caller that shows it inline
    /// (the page's Start gglib) says.
    pub(crate) fn result(self) -> Result<(), String> {
        match self {
            Self::Ran(outcome) | Self::Joined(outcome) => outcome,
        }
    }

    /// The failure this caller reports, if it is this caller's to report:
    /// never while the app is shutting down, since a dialog for an app the
    /// user has just quit is no use to anyone, and a modal one can hold the
    /// exit.
    pub(crate) fn failure_to_report(self, shutting_down: bool) -> Option<String> {
        match self {
            Self::Ran(Err(words)) if !shutting_down => Some(words),
            Self::Ran(_) | Self::Joined(_) => None,
        }
    }
}

impl Launch {
    /// The ownership the service had before this launch.
    pub(crate) const fn before(&self) -> Ownership {
        self.before
    }

    /// Hand over how the launch ended; every waiter gets it. A failure keeps
    /// the ownership the service had before the launch.
    pub(crate) fn settle(mut self, outcome: Result<Ownership, String>) {
        if let Some(known) = self.known.take() {
            known.send_replace(match outcome {
                Ok(ownership) => Known::Running(ownership),
                Err(words) => Known::Failed(words, self.before),
            });
        }
    }
}

impl Drop for Launch {
    fn drop(&mut self) {
        if let Some(known) = self.known.take() {
            known.send_replace(Known::Failed(CUT_SHORT.to_owned(), self.before));
        }
    }
}

/// The ownership a state carries.
const fn ownership_of(known: &Known) -> Ownership {
    match known {
        Known::Running(ownership) | Known::Failed(_, ownership) => *ownership,
        Known::Starting => Ownership::Unresolved,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::LAUNCH_WAIT;

    /// Whether `task` is still waiting after far longer than any launch may
    /// take: the clock is paused, so this runs at once, and an answer given
    /// after any fixed delay would be seen.
    async fn waits<T>(task: &tokio::task::JoinHandle<T>) -> bool {
        tokio::time::sleep(LAUNCH_WAIT * 4).await;
        !task.is_finished()
    }

    fn ask(service: &Arc<Service>) -> tokio::task::JoinHandle<Result<Ownership, String>> {
        let service = Arc::clone(service);
        tokio::spawn(async move { service.settled().await })
    }

    /// A launch that ends without saying how (its task panicked) still
    /// answers everyone waiting on it, as a failure. Otherwise the page would
    /// wait for ever on a launch nothing will finish.
    #[tokio::test(start_paused = true)]
    async fn a_launch_that_never_says_how_it_ended_fails_its_waiters() {
        let (service, launch) = Service::launching();
        let service = Arc::new(service);
        let waiting = ask(&service);
        assert!(waits(&waiting).await, "nothing has settled the launch yet");

        let launcher = tokio::spawn(async move {
            let _launch = launch;
            panic!("the launch task dies");
        });
        assert!(launcher.await.is_err(), "the launch task panicked");

        let answered = tokio::time::timeout(LAUNCH_WAIT, waiting)
            .await
            .expect("a dropped launch must answer its waiters, not leave them waiting");
        assert_eq!(answered.expect("the waiter"), Err(CUT_SHORT.to_owned()));
    }

    /// A Start gglib while the app's own launch is still in flight starts
    /// nothing: it waits for that launch's answer. Two launches at once
    /// would be two `gglib daemon run`, one of them refused by the lock.
    #[tokio::test]
    async fn a_second_start_joins_the_launch_in_flight() {
        let (service, launch) = Service::launching();
        assert!(service.begin().is_none(), "a launch is in flight");
        assert!(service.is_starting());

        launch.settle(Ok(Ownership::Launched));
        assert!(!service.is_starting());
        assert!(service.begin().is_some(), "nothing is in flight now");
    }

    /// A Start gglib that joined a launch which then failed gets the failure
    /// (the page shows it inline) but does not report it: the launch's owner
    /// does, so one failed launch is one dialog, not two.
    #[tokio::test(start_paused = true)]
    async fn a_start_that_joined_a_failed_launch_leaves_the_report_to_its_owner() {
        let (service, launch) = Service::launching();
        let service = Arc::new(service);
        assert!(service.begin().is_none(), "a launch is in flight");

        let joining = Arc::clone(&service);
        let joined = tokio::spawn(async move { joining.join().await });
        assert!(waits(&joined).await, "a joined start waits for the launch");

        let why = "no `gglib` binary beside the app";
        launch.settle(Err(why.to_owned()));
        let joined = joined.await.expect("the joined start");
        assert_eq!(joined, Started::Joined(Err(why.to_owned())));
        assert_eq!(
            Started::Joined(Err(why.to_owned())).failure_to_report(false),
            None
        );
        assert_eq!(joined.result(), Err(why.to_owned()));

        let ran = Started::Ran(Err(why.to_owned()));
        assert_eq!(ran.failure_to_report(false), Some(why.to_owned()));
    }

    /// A launch that fails while the app is shutting down (the user quit
    /// during it) is not reported: a dialog for an app just quit is no use,
    /// and on macOS a modal one can hold the exit.
    #[test]
    fn a_failure_while_shutting_down_is_not_reported() {
        let why = || Err("the gglib daemon did not come up in time".to_owned());
        assert_eq!(Started::Ran(why()).failure_to_report(true), None);
        assert!(Started::Ran(why()).failure_to_report(false).is_some());
    }

    /// A restart keeps the ownership the service had, so quitting decides
    /// the same way it did before the service was started again.
    #[tokio::test]
    async fn a_restart_keeps_the_ownership_the_launch_decided() {
        let (service, launch) = Service::launching();
        launch.settle(Ok(Ownership::Launched));

        let restart = service.begin().expect("nothing in flight");
        assert_eq!(restart.before(), Ownership::Launched);
        restart.settle(Ok(Ownership::Launched));
        assert_eq!(service.settled().await, Ok(Ownership::Launched));
    }

    /// A Start gglib that failed (the daemon it spawned was not up in time)
    /// keeps the ownership of the daemon this app launched: when the spawned
    /// one answers late it is found as this app's, so quitting stops it.
    #[tokio::test]
    async fn a_failed_restart_keeps_the_ownership_and_finds_the_late_daemon_as_its_own() {
        let (service, launch) = Service::launching();
        launch.settle(Ok(Ownership::Launched));

        let restart = service.begin().expect("nothing in flight");
        restart.settle(Err("the gglib daemon did not come up within 15s".to_owned()));
        assert!(service.settled().await.is_err());

        service.found();
        assert_eq!(service.settled().await, Ok(Ownership::Launched));
    }

    /// A restart cut short (its task dropped after the spawn) keeps the
    /// ownership too.
    #[tokio::test(start_paused = true)]
    async fn a_restart_cut_short_keeps_the_ownership() {
        let (service, launch) = Service::launching();
        launch.settle(Ok(Ownership::Launched));

        drop(service.begin().expect("nothing in flight"));
        let settled = tokio::time::timeout(LAUNCH_WAIT, service.settled())
            .await
            .expect("a dropped restart must settle, not leave its waiters waiting");
        assert_eq!(settled, Err(CUT_SHORT.to_owned()));

        service.found();
        assert_eq!(service.settled().await, Ok(Ownership::Launched));
    }

    /// Quitting while a Start gglib has failed still decides by the
    /// ownership: a daemon this app launched is this app's to stop, if it is
    /// there.
    #[tokio::test]
    async fn quitting_after_a_failed_restart_still_owns_what_the_app_launched() {
        let (service, launch) = Service::launching();
        launch.settle(Ok(Ownership::Launched));

        let restart = service.begin().expect("nothing in flight");
        restart.settle(Err("the gglib daemon did not come up within 15s".to_owned()));

        assert_eq!(service.ownership().await, Ownership::Launched);
        assert!(service.ownership().await.ends_with_the_app());
    }

    /// A failed first launch is not the last word: a service heard answering
    /// afterwards (started from a terminal, or the app's own come up late)
    /// is the app's service, though not its to stop, as before.
    #[tokio::test]
    async fn a_service_heard_after_a_failed_launch_is_found() {
        let (service, launch) = Service::launching();
        service.found();
        assert!(
            service.is_starting(),
            "the launch in flight answers, not a poll"
        );

        launch.settle(Err("port 9887 is held by another program".to_owned()));
        assert!(service.settled().await.is_err());

        service.found();
        assert_eq!(service.settled().await, Ok(Ownership::Unresolved));
    }
}
