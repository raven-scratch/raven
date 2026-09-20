# raven-scratch

The Scratch 3 domain model both languages share. It is a library, not a tool: it
holds the facts about Scratch, and the two compilers hold the languages.

| Module | Role |
| --- | --- |
| `catalog.rs` | The block table: opcode, inputs, fields, shape, category and docs. Every block either language can reach is a row here. |
| `sb3.rs`, `zipw.rs` | The Scratch 3 project format, and a dependency-free ZIP writer. |
| `assets.rs` | Costume and sound loading: format sniffing, pixel sizes, rotation centres, MD5 asset ids. |
| `ids.rs` | Deterministic identifiers, derived from content so a rebuild is byte-identical. |
| `identity.rs` | The names the whole workspace answers to: organisation, repository, documentation root. |
| `diag.rs` | The diagnostic type every compiler renders: source spans, notes, and a text renderer. |

Adding a block to `catalog.rs` is the whole of adding a block: it compiles,
validates, appears in the generated documentation, and the `raven` test suite
fails until it has a raven name — see the workspace
[README](../../README.md#how-it-works).
