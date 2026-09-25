# Fixtures

The JSON here is frozen regression snapshots, originally recorded from the retired Python app
(branch `legacy-python`) and checked against ever since: `tests/regression.rs` in conrod-core,
`tests/parity.rs` in conrod-vision, `plates_text.json`, `vlm.json`, and for conrod-store
`legacy_library.sql` (a small library as that app wrote it) and `legacy_schema.json` (its fresh
schema).

`*_local.json` files point at a photographer's own frames and are never committed.

These snapshots are not meant to be regenerated against a live app -- there is no live app to
regenerate them from. When a change to the Rust code deliberately changes an output (a better
heuristic, a fixed bug, a new field), update the affected JSON by hand or with a small throwaway
script, and review the diff as part of the change: the diff *is* the record of what moved and why.
An unreviewed diff here is a regression; a reviewed one is the new ground truth.
