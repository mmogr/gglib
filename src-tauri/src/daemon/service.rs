//! The service this app talks to, once it is known.
//!
//! As today: the app asks nothing about its launch, so everything is known
//! at once. The next commit makes the question wait for the launch.

use std::sync::Arc;

use super::Ownership;

/// What a launch that never said how it ended is reported as.
const CUT_SHORT: &str = "gglib's start was cut short before it answered";

/// The service this app talks to.
pub(crate) struct Service;

/// A launch.
pub(crate) struct Launch;

impl Service {
    /// A service and its first launch.
    pub(crate) fn launching() -> (Self, Launch) {
        (Self, Launch)
    }

    /// Start a launch.
    pub(crate) fn begin(&self) -> Option<Launch> {
        Some(Launch)
    }

    /// How the app came by its service: as today, at once.
    pub(crate) async fn settled(&self) -> Result<Ownership, String> {
        Ok(Ownership::Unresolved)
    }

    /// Wait for a launch in flight: as today, every start runs its own.
    pub(crate) async fn join(&self) -> Started {
        Started::Ran(self.settled().await.map(drop))
    }

    /// Whether a launch is in flight now.
    pub(crate) fn is_starting(&self) -> bool {
        false
    }

    /// The service answered.
    pub(crate) fn found(&self) {}
}

/// How a Start gglib ended.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Started {
    /// This caller ran the launch.
    Ran(Result<(), String>),
    /// This caller joined a launch in flight.
    Joined(Result<(), String>),
}

impl Started {
    /// The outcome.
    pub(crate) fn result(self) -> Result<(), String> {
        match self {
            Self::Ran(outcome) | Self::Joined(outcome) => outcome,
        }
    }

    /// The failure this caller reports: as today, every failure.
    pub(crate) fn failure_to_report(self) -> Option<String> {
        self.result().err()
    }
}

impl Launch {
    /// The ownership the service had before this launch.
    pub(crate) const fn before(&self) -> Ownership {
        Ownership::Unresolved
    }

    /// Hand over how the launch ended.
    pub(crate) fn settle(self, _outcome: Result<Ownership, String>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether `task` is still waiting once everything runnable has run.
    async fn waits<T>(task: &tokio::task::JoinHandle<T>) -> bool {
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }
        !task.is_finished()
    }

    fn ask(service: &Arc<Service>) -> tokio::task::JoinHandle<Result<Ownership, String>> {
        let service = Arc::clone(service);
        tokio::spawn(async move { service.settled().await })
    }

    /// A launch that ends without saying how (its task panicked) still
    /// answers everyone waiting on it, as a failure. Otherwise the page would
    /// wait for ever on a launch nothing will finish.
    #[tokio::test]
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

        assert_eq!(
            waiting.await.expect("the waiter"),
            Err(CUT_SHORT.to_owned())
        );
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
    #[tokio::test]
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
            Started::Joined(Err(why.to_owned())).failure_to_report(),
            None
        );
        assert_eq!(joined.result(), Err(why.to_owned()));

        let ran = Started::Ran(Err(why.to_owned()));
        assert_eq!(ran.failure_to_report(), Some(why.to_owned()));
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

    /// A failed launch is not the last word: a service heard answering
    /// afterwards (started from a terminal) is the app's service, though not
    /// its to stop.
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
