# CLI Golden Outputs

`tests/cli_goldens.rs` captures `exit`, `stdout`, and `stderr` for representative CLI cases and compares them with the files in this directory.

The redactor normalizes:

- temporary test directories to `<TEMP>` or `<CASE>`
- `$HOME` to `<HOME>`
- RFC3339 UTC timestamps to `<TIMESTAMP>`
- UUIDs, Claude tool-use IDs, server tool-use IDs, and request IDs to stable placeholders

Regenerate the files with:

```sh
./xtask regen-golden
```
