//! What [`GuiProcessCore::spawn`](super::GuiProcessCore::spawn) is asked to
//! start, and the binaries it starts them with.
//!
//! One process core serves both runtimes: the port allocation, pidfile, log
//! readers and kill path are the same for a llama-server and an `sd-server`.
//! Only the program and its arguments differ, so the request names the
//! runtime and the core holds one binary path for each.

use std::path::{Path, PathBuf};

use gglib_core::domain::RuntimeKind;
use gglib_core::ports::ServerConfig;

use crate::sd::SdServerConfig;

/// A server to start, in the runtime that serves it.
#[derive(Debug, Clone)]
pub enum SpawnConfig {
    /// A llama-server for a model that chats.
    Llama(ServerConfig),
    /// An `sd-server` for a model that draws.
    Sd(SdServerConfig),
}

impl SpawnConfig {
    /// The runtime this config starts.
    #[must_use]
    pub const fn runtime(&self) -> RuntimeKind {
        match self {
            Self::Llama(_) => RuntimeKind::Llama,
            Self::Sd(_) => RuntimeKind::StableDiffusion,
        }
    }

    /// Database ID of the model.
    #[must_use]
    pub const fn model_id(&self) -> i64 {
        match self {
            Self::Llama(c) => c.model_id,
            Self::Sd(c) => c.model_id,
        }
    }

    /// The model's main weights file.
    #[must_use]
    pub fn model_path(&self) -> &Path {
        match self {
            Self::Llama(c) => &c.model_path,
            Self::Sd(c) => &c.model_path,
        }
    }

    /// The port asked for, if any.
    #[must_use]
    pub const fn port(&self) -> Option<u16> {
        match self {
            Self::Llama(c) => c.port,
            Self::Sd(c) => c.port,
        }
    }

    /// The model's name, taken.
    pub(crate) fn into_model_name(self) -> String {
        match self {
            Self::Llama(c) => c.model_name,
            Self::Sd(c) => c.model_name,
        }
    }
}

/// The program each runtime is started with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeBinaries {
    /// llama.cpp's `llama-server`.
    pub llama: PathBuf,
    /// stable-diffusion.cpp's `sd-server`.
    pub sd: PathBuf,
}

impl RuntimeBinaries {
    /// The binary that serves `runtime`.
    #[must_use]
    pub fn for_runtime(&self, runtime: RuntimeKind) -> &Path {
        match runtime {
            RuntimeKind::Llama => &self.llama,
            RuntimeKind::StableDiffusion => &self.sd,
        }
    }
}

#[cfg(test)]
impl RuntimeBinaries {
    /// A llama-server at `llama` and no `sd-server`, for a test that starts
    /// only chat models.
    pub(crate) fn llama_only(llama: impl Into<String>) -> Self {
        Self {
            llama: PathBuf::from(llama.into()),
            sd: PathBuf::from("/nonexistent/sd-server"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_runtime_has_its_own_binary() {
        let binaries = RuntimeBinaries {
            llama: PathBuf::from("/bin/llama-server"),
            sd: PathBuf::from("/bin/sd-server"),
        };
        assert_eq!(
            binaries.for_runtime(RuntimeKind::Llama),
            Path::new("/bin/llama-server")
        );
        assert_eq!(
            binaries.for_runtime(RuntimeKind::StableDiffusion),
            Path::new("/bin/sd-server")
        );
    }
}
