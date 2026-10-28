//! Which gglib an app linked.

use gglib_core::domain::engine_pins::PINNED_LLAMA_RELEASE;

/// The versions a build of this library was made from.
///
/// An app shows them (Settings, About) and asserts them in its tests, so a
/// binding taken from one release and a framework from another are visible
/// before anything else is called.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BuildInfo {
    /// gglib's version, which this crate inherits from the workspace's
    /// `Cargo.toml`.
    pub gglib_version: String,
    /// The `uniffi` release that built the scaffolding and generated its
    /// Swift, as the workspace's `Cargo.lock` resolved it.
    pub uniffi_version: String,
    /// The llama.cpp release gglib is pinned to, a `b<n>` tag:
    /// [`PINNED_LLAMA_RELEASE`].
    pub llama_pin: String,
}

/// The versions this library was built from.
#[uniffi::export]
pub fn build_info() -> BuildInfo {
    BuildInfo {
        gglib_version: env!("CARGO_PKG_VERSION").to_owned(),
        uniffi_version: env!("GGLIB_FFI_UNIFFI_VERSION").to_owned(),
        llama_pin: PINNED_LLAMA_RELEASE.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A version read as dotted numbers, `x.y.z` with an optional
    /// pre-release after a `-`.
    fn is_semver(version: &str) -> bool {
        let core = version.split_once('-').map_or(version, |(core, _)| core);
        let parts: Vec<&str> = core.split('.').collect();
        parts.len() == 3
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
    }

    #[test]
    fn build_info_names_the_workspace_version_and_the_pin() {
        let info = build_info();

        assert_eq!(info.gglib_version, env!("CARGO_PKG_VERSION"));
        assert!(is_semver(&info.gglib_version), "{info:?}");
        assert_eq!(info.llama_pin, PINNED_LLAMA_RELEASE);
    }

    /// The version comes out of Cargo.lock whole, and on the minor
    /// modelpipe-ffi binds with: the two bindings share one app.
    #[test]
    fn build_info_names_a_uniffi_on_the_shared_minor() {
        let info = build_info();

        assert!(is_semver(&info.uniffi_version), "{info:?}");
        assert!(
            info.uniffi_version.starts_with("0.32."),
            "modelpipe-ffi binds with uniffi 0.32; a move to another minor \
             moves both crates together: {info:?}"
        );
    }
}
