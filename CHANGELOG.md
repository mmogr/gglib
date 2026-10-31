# Changelog

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
