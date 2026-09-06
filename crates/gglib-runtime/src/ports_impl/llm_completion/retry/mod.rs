#![doc = include_str!("README.md")]
mod classify;
mod execute;
mod headers;

pub(crate) use execute::send_with_retry;

#[cfg(test)]
#[path = "test_server.rs"]
mod test_server;

#[cfg(test)]
#[path = "execute_tests.rs"]
mod execute_tests;

#[cfg(test)]
#[path = "bearer_tests.rs"]
mod bearer_tests;
