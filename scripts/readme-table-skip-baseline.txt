# READMEs that carry no file-naming rows, and so are not compared against
# their directory. Regenerate with `./scripts/check_readme_tables.py --update`.
#
# A new entry appearing here without being recorded is a failure: deleting a
# table, or dropping the extensions from its first cells, would otherwise move
# a README out of the checked set and print a larger skip count above a green
# tick. Recording one is a reviewable line in a diff, which is the same
# bargain `scripts/rust-complexity-baseline.txt` strikes.
#
# Three are exemptions by design — `src/types`, `src/commands` and
# `src/styles` tabulate type names, command names and migration phases rather
# than files. The rest are stubs from `generate_submodule_readmes.sh`, which
# emits TypeScript READMEs with no table at all; each is a directory nothing
# compares, and shortening this list is the way to fix that.
src/commands/README.md
src/components/README.md
src/styles/README.md
src/types/README.md
tests/README.md
tests/ts/README.md
tests/ts/components/README.md
tests/ts/hooks/README.md
tests/ts/hooks/useGglibRuntime/README.md
tests/ts/services/README.md
tests/ts/services/api/README.md
tests/ts/services/clients/README.md
tests/ts/services/server/README.md
tests/ts/services/tools/README.md
tests/ts/services/transport/README.md
tests/ts/utils/README.md
tests/ts/utils/messages/README.md
