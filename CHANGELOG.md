# Changelog

## [0.30.0-rc.2](https://github.com/mmogr/gglib/compare/v0.30.0-rc.1...v0.30.0-rc.2) (2026-11-01)


### ⚠ BREAKING CHANGES

* **cli:** `gglib --remote <command>` no longer parses; the flag goes after the command it is for.

### What gglib now does

* **cli:** --remote is offered only by the commands that reach the paired machine ([#1709](https://github.com/mmogr/gglib/issues/1709)) ([dcfba8b](https://github.com/mmogr/gglib/commit/dcfba8b56ec04ac00cb5c59a30fb82e9008e71c6))


### What it stopped getting wrong

* **cli:** gglib image --remote draws with the paired machine's image model ([#1711](https://github.com/mmogr/gglib/issues/1711)) ([728ee36](https://github.com/mmogr/gglib/commit/728ee3618df814a52f46300d569e28b523addc23)), closes [#1707](https://github.com/mmogr/gglib/issues/1707)


### Internal

* **cli:** every route the tunnel carries is reached by a command or exempted ([#1708](https://github.com/mmogr/gglib/issues/1708)) ([5727676](https://github.com/mmogr/gglib/commit/5727676e9e7c2da1058f2d61eaee90f896123af1))
* **cli:** the CLI reaches the paired machine through FarProxy ([#1710](https://github.com/mmogr/gglib/issues/1710)) ([8eaf859](https://github.com/mmogr/gglib/commit/8eaf85946658361c2469be9253343257d43bff5b))

## [0.30.0-rc.1](https://github.com/mmogr/gglib/compare/v0.29.0...v0.30.0-rc.1) (2026-11-01)


### ⚠ BREAKING CHANGES

* **mcp:** the manual lifecycle is gone ([#1704](https://github.com/mmogr/gglib/issues/1704))
* **mcp:** tools and list ask the daemon, and start and stop are gone ([#1703](https://github.com/mmogr/gglib/issues/1703))

### What it stopped getting wrong

* **benchmark:** compare counts its tokens and keeps the thinking apart ([#1702](https://github.com/mmogr/gglib/issues/1702)) ([2999e9c](https://github.com/mmogr/gglib/commit/2999e9c371cc03961e6d075315b4be1736454254)), closes [#1690](https://github.com/mmogr/gglib/issues/1690)
* **mcp:** tools and list ask the daemon, and start and stop are gone ([#1703](https://github.com/mmogr/gglib/issues/1703)) ([318a022](https://github.com/mmogr/gglib/commit/318a0223d31398de64482783d441a9484cad16c1)), closes [#1689](https://github.com/mmogr/gglib/issues/1689)
* **proxy:** two models with one name get two ids ([#1705](https://github.com/mmogr/gglib/issues/1705)) ([be96abb](https://github.com/mmogr/gglib/commit/be96abb2d0af6c51dc7af605e3edd93d70d941b8)), closes [#1648](https://github.com/mmogr/gglib/issues/1648)
* **remote:** disable lets go of the serve slot before it drains ([#1701](https://github.com/mmogr/gglib/issues/1701)) ([514f504](https://github.com/mmogr/gglib/commit/514f5041d53e4750fc432c3ddc4b30ab28a239d4)), closes [#1646](https://github.com/mmogr/gglib/issues/1646)
* **runtime:** a pid that names a process group is never signalled ([#1700](https://github.com/mmogr/gglib/issues/1700)) ([6ac773e](https://github.com/mmogr/gglib/commit/6ac773e7dc8ff46ecb8d939d8fca45fa4b265acc)), closes [#1647](https://github.com/mmogr/gglib/issues/1647)


### Internal

* **mcp:** the manual lifecycle is gone ([#1704](https://github.com/mmogr/gglib/issues/1704)) ([11208b8](https://github.com/mmogr/gglib/commit/11208b84617d793b07bb164b4b1de9b086453077)), closes [#1699](https://github.com/mmogr/gglib/issues/1699)
* **promote:** the stable release is made from the candidate's tag, not from main ([#1697](https://github.com/mmogr/gglib/issues/1697)) ([48303e8](https://github.com/mmogr/gglib/commit/48303e814905739ea04ee4e0bab4b9a24862814d))

## [0.29.0](https://github.com/mmogr/gglib/compare/v0.28.0...v0.29.0) (2026-10-31)

0.29.0-rc.7, promoted unchanged. The candidates below make up this release.

## [0.29.0-rc.7](https://github.com/mmogr/gglib/compare/v0.29.0-rc.6...v0.29.0-rc.7) (2026-10-31)


### ⚠ BREAKING CHANGES

* **cli:** -y/--yes for a flag that only answers the confirmation, and no global --models-dir ([#1693](https://github.com/mmogr/gglib/issues/1693))

### What it stopped getting wrong

* **daemon:** a stop waits until the daemon has let go of its lock ([#1691](https://github.com/mmogr/gglib/issues/1691)) ([7898b3d](https://github.com/mmogr/gglib/commit/7898b3d64323714d6adc7582e5bfa81bfdac2b61))


### Documentation

* **cli:** ROCm in backticks, as clippy's doc_markdown asks ([#1695](https://github.com/mmogr/gglib/issues/1695)) ([cef3f28](https://github.com/mmogr/gglib/commit/cef3f280063cbed083b827719e17f3d4c87582a9))
* the architecture diagrams and tables match the code ([#1692](https://github.com/mmogr/gglib/issues/1692)) ([039e7a6](https://github.com/mmogr/gglib/commit/039e7a6ebd1d3d8e154a8dcb7702bb6ea0f46583))


### Internal

* **cli:** -y/--yes for a flag that only answers the confirmation, and no global --models-dir ([#1693](https://github.com/mmogr/gglib/issues/1693)) ([8c8ec2d](https://github.com/mmogr/gglib/commit/8c8ec2d5bad68df9dd9cef78e487a4652520afb7))

## [0.29.0-rc.6](https://github.com/mmogr/gglib/compare/v0.29.0-rc.5...v0.29.0-rc.6) (2026-10-31)


### What it stopped getting wrong

* **ci:** the tag is made with the App's token on every path ([#1686](https://github.com/mmogr/gglib/issues/1686)) ([503d563](https://github.com/mmogr/gglib/commit/503d56334c4ae4f39c606467b1372777b4847bba)), closes [#1685](https://github.com/mmogr/gglib/issues/1685)
* **ci:** the tagging workflow run by hand tags with the App's token ([#1684](https://github.com/mmogr/gglib/issues/1684)) ([f2604e2](https://github.com/mmogr/gglib/commit/f2604e2bf4b00b15ea1d37d9cd5ae16ff7ce7108)), closes [#1683](https://github.com/mmogr/gglib/issues/1683)


### Internal

* **release:** the tagging workflow runs by hand, without waiting for CI ([#1682](https://github.com/mmogr/gglib/issues/1682)) ([7d44a34](https://github.com/mmogr/gglib/commit/7d44a34536ce0d98fd33e959db5cf22d2e08b848))

## [0.29.0-rc.5](https://github.com/mmogr/gglib/compare/v0.29.0-rc.4...v0.29.0-rc.5) (2026-10-31)


### What it stopped getting wrong

* **ci:** the release job can relabel the release PR ([#1681](https://github.com/mmogr/gglib/issues/1681)) ([12c4430](https://github.com/mmogr/gglib/commit/12c44308c2539621f757ebfb4d5cf7a4c9d8b6ce)), closes [#1679](https://github.com/mmogr/gglib/issues/1679)


### Documentation

* **readme:** one badge for the stable release, one for the release candidate ([#1678](https://github.com/mmogr/gglib/issues/1678)) ([240a8fd](https://github.com/mmogr/gglib/commit/240a8fdded555b64a6723658c100a8f210622d89))
* **readme:** the badges say where gglib installs from and how each workflow stands ([#1677](https://github.com/mmogr/gglib/issues/1677)) ([7328f74](https://github.com/mmogr/gglib/commit/7328f74e91af2e8791b88b07ca2e48ef7a8501ac))

## [0.29.0-rc.4](https://github.com/mmogr/gglib/compare/v0.29.0-rc.3...v0.29.0-rc.4) (2026-10-31)


### What gglib now does

* **packaging:** pacman and dnf repositories, and one install command for every distro ([#1669](https://github.com/mmogr/gglib/issues/1669)) ([253a523](https://github.com/mmogr/gglib/commit/253a52319e2071b66317b124e742c3ffb6db3a29))


### What it stopped getting wrong

* **ci:** rustdoc renders each crate after the crates it links to ([#1665](https://github.com/mmogr/gglib/issues/1665)) ([c964ee6](https://github.com/mmogr/gglib/commit/c964ee65308b42b93e3c19058acb734959c25b02)), closes [#1664](https://github.com/mmogr/gglib/issues/1664)
* **download:** a cancel is answered once the .part holds what arrived ([#1662](https://github.com/mmogr/gglib/issues/1662)) ([1932f30](https://github.com/mmogr/gglib/commit/1932f30a796dc4072ec9dc55e0284152033998d6)), closes [#1661](https://github.com/mmogr/gglib/issues/1661)
* **download:** the requirements test bounds the packages rather than pinning them ([#1671](https://github.com/mmogr/gglib/issues/1671)) ([d78338e](https://github.com/mmogr/gglib/commit/d78338e4273479775a5ec885a34c836cef4264ac)), closes [#1670](https://github.com/mmogr/gglib/issues/1670)


### Internal

* **cost:** the macOS and Windows jobs run only when a change can reach them ([#1663](https://github.com/mmogr/gglib/issues/1663)) ([cbea86a](https://github.com/mmogr/gglib/commit/cbea86a7ec87c14e96294e1c59f780824eaad06d))
* **deps:** update hf-xet requirement in /crates/gglib-download/scripts ([#1666](https://github.com/mmogr/gglib/issues/1666)) ([d88e717](https://github.com/mmogr/gglib/commit/d88e717f6dd43c071f31474a33962e62a02ca77a))
* **deps:** update huggingface-hub requirement ([#1668](https://github.com/mmogr/gglib/issues/1668)) ([67c9adb](https://github.com/mmogr/gglib/commit/67c9adb13772f14ab081fdcc89942448cb840c93))
* **hygiene:** actionlint in the gate, pinned actions, and least access per job ([#1672](https://github.com/mmogr/gglib/issues/1672)) ([c0ebf03](https://github.com/mmogr/gglib/commit/c0ebf0399fcaf9c6231814330c2eaf0257e04617))
* **release:** the release builds outlive the CI run that tags ([#1667](https://github.com/mmogr/gglib/issues/1667)) ([b8d3937](https://github.com/mmogr/gglib/commit/b8d3937693a7b659519f695ceb356e904c11414e))

## [0.29.0-rc.3](https://github.com/mmogr/gglib/compare/v0.29.0-rc.2...v0.29.0-rc.3) (2026-10-31)


### Internal

* **badges:** one crate list drives the badges, coverage and boundaries, and the badges branch is pruned ([#1652](https://github.com/mmogr/gglib/issues/1652)) ([e380b3c](https://github.com/mmogr/gglib/commit/e380b3cce6ab12ff60fd85261024117361d4a996))
* **labels:** a fix closes a triaged issue ([#1655](https://github.com/mmogr/gglib/issues/1655)) ([1820ec3](https://github.com/mmogr/gglib/commit/1820ec385cfec1b28a0b3806d6aaa81271197741))
* **release:** release-please keeps the release PR, and Promote ships the stable ([#1654](https://github.com/mmogr/gglib/issues/1654)) ([d12ed14](https://github.com/mmogr/gglib/commit/d12ed143a15b199f855716271bef3b6193123300))
