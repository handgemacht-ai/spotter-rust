## Rust Best-Practice Checklist

- [ ] Library errors use `thiserror`; binary entrypoints use `anyhow`.
- [ ] No `unwrap()` / `expect()` outside tests and `main.rs`.
- [ ] Public library APIs have doc comments; examples compile via `cargo test --doc`.
- [ ] I/O paths use `&Path` / `PathBuf`, not string-only APIs.
- [ ] CLI parsing uses `clap` derive macros.
- [ ] Subcommand handlers return typed values or typed records before rendering.
- [ ] Long-running operations have a SIGINT/cancellation story.
- [ ] `docs/subcommand-parity-checklist.md` remains fully checked.
- [ ] Golden output/schema changes are intentional and explained.
