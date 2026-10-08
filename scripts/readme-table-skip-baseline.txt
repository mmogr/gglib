# READMEs that carry no file-naming rows, and so are not compared against
# their directory. Regenerate with `./scripts/check_readme_tables.py --update`.
#
# A new entry appearing here without being recorded is a failure: deleting a
# table, or dropping the extensions from its first cells, would otherwise move
# a README out of the checked set and print a larger skip count above a green
# tick. Recording one is a reviewable line in a diff, which is the same
# bargain `scripts/rust-complexity-baseline.txt` strikes.
#
# One is an exemption by design: `src/types` tabulates type names rather than
# files. The rest describe their directory with no file table; each is a
# directory nothing compares, and shortening this list is the way to fix that.
src/components/README.md
src/types/README.md
tests/README.md
tests/ts/services/clients/README.md
