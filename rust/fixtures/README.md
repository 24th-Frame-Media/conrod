# Fixtures

The JSON here is ground truth recorded from the Python app (branch `legacy-python`), which the Rust crates are
checked against: `parity.rs` in conrod-core and conrod-vision, `plates_text.json`, `vlm.json`, and for conrod-store
`python_library.sql` (a small library as the Python app wrote it) and `python_schema.json` (its fresh schema), recorded
by `tools/gen_store_fixtures.py` on that branch.

`*_local.json` files point at a photographer's own frames and are never committed. To regenerate any fixture, or to
compare against a real library, check the Python app out beside this repository and run its tools:

```bash
git worktree add ../conrod-legacy legacy-python
cd ../conrod-legacy
python -m venv .venv && .venv/Scripts/python -m pip install -r requirements.txt
.venv/Scripts/python tools/gen_golden.py            # also: gen_ocr_local, gen_plates_local, gen_vision_local, ...
CONROD_CLI=<this repo>/rust/target/release/conrod-cli.exe .venv/Scripts/python tools/parity_ops.py group 39
```

`parity_ops.py` compares a Rust operation with Python's stored result on a private backup copy (in `%TEMP%`) of
`~/.conrod/conrod.db`, which is only ever opened read-only. Build the CLI first with `cargo build --release -p conrod-cli`.
