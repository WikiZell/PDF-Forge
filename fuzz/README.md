# Fuzzing

Requires nightly Rust and `cargo install cargo-fuzz`.

```bash
cd fuzz
cargo +nightly fuzz run load_build  -- -max_total_time=600
cargo +nightly fuzz run split_ranges -- -max_total_time=60
```

Seed the corpus with `tests/fixtures/*.pdf`. `load_build` exercises the parser, page-list editing,
the page-tree rebuild and both signing modes on arbitrary bytes. Any crash or timeout is a bug
(parser panics are also contained at runtime by `load_bytes`, but should be fixed at the source).

The same pipeline runs deterministically in CI as
`crates/pdf-core/tests/robustness.rs::byte_flip_mutation_never_panics_through_load_and_build`.
