# The library, nightly

One record per line, sorted by key: {"k": "<identity>", "v": {<the record>}}

Written by .github/workflows/backup.yml and read by
scripts/restore_library.sh, which puts it back. Nothing else should
depend on it: the live library is the Cloudflare KV namespace, and
the board is generated from that.

It is sorted so git stores a night as a delta rather than as a file.
