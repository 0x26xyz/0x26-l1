# RFC 056 policy authority-scope fixture

`source_snapshot_bab6253.k26s` was captured from commit `bab6253`
(`Tighten delegated MM review follow-ups`), immediately before the RFC 056
policy schema cut.

The file intentionally keeps the old `K26S` pre-fingerprint logical-snapshot
envelope. RFC 055 forbids production migration from pre-fingerprint snapshots,
so command-level tests only extract the captured legacy `engine_canonical_v1`
record and wrap that record in a test-only `K26F` envelope.

Do not regenerate this fixture from the current binary.
