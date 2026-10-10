//! The three pages a reader should meet first.

use crate::identity;

/// `overview` — what raven is, in the terms a generator needs on arrival.
pub const OVERVIEW: &str = "\n## overview\n\
# raven compiles a small typed language (.rav) to Scratch 3 (.sb3), through a\n\
# language with exactly one statement per block (raven-asm, .rasm):\n\
#\n\
#     raven (.rav)  ->  raven-asm (.rasm)  ->  project.json  ->  .sb3\n\
#     sugar, types       one statement,          Scratch 3\n\
#     macros             one block               file format\n\
#\n\
# `raven expand` prints the middle layer, so nothing about a program is hidden\n\
# between the source and the file Scratch runs.\n\
#\n\
# # The promise\n\
#\n\
# Every convenience is a rewrite the compiler can print. A convenience whose\n\
# expansion cannot be printed is not in the language. That is what makes the\n\
# cost table in `costs` a contract rather than an estimate, and it is why\n\
# `raven expand` is the way to answer \"what did that cost?\".\n\
#\n\
# # The law that surprises people\n\
#\n\
# A raven program cannot name a Scratch variable. Its `var`s, `let`s, lists and\n\
# maps are cells of one arena the stage declares (`_vms`), addressed by a\n\
# constant index chosen at compile time; a script's `let` is a push and a pop on\n\
# that script's own `_stackN`. There is no `data_variable(\"score\")` to write and\n\
# no name a running program can look up. The five blocks that name a Scratch\n\
# variable are refused, with a note saying what to write instead. Two escape\n\
# hatches exist and both are written down: `watch` declares a visible mirror to\n\
# be looked at, and `@scratch_global` / `@scratch_sprite` store a declaration in\n\
# a real Scratch variable or list of its own name. See `memory`.\n\
#\n\
# # What a program is made of\n\
#\n\
# A `.rav` file declares a target (`stage { … }` or `sprite \"Name\" { … }`) or it\n\
# declares no target and is a module other files import. Inside a target: `var`,\n\
# `const`, `costume`, `sound`, `broadcast`, `struct` declarations; `proc`, `fn`,\n\
# `macro` definitions; and `on <hat> { … }` scripts. There is no top-level\n\
# statement: a loose stack in Scratch never runs, so every statement is inside a\n\
# script or a `proc` body.\n\
#\n\
# # The three ways to make a value\n\
#\n\
#   fn      a compile-time substitution: inlined at every call site, free at run\n\
#           time, no statements, so it cannot loop, branch or recurse.\n\
#   macro   the same mechanism with expression, name and block parameters, and\n\
#           either an expression or statements as its result.\n\
#   proc    a real Scratch custom block, defined once and shared by every caller;\n\
#           it may hold any statement, and with `-> T` it also returns a value\n\
#           through a cell, costing the call plus a cell read.\n\
#\n\
# `while`, `loop`, `for`, `abs`, `floor` and friends are macros in the prelude, so\n\
# they cost what `costs` says and print like anything else. `f\"…\"`, `match`,\n\
# compound assignment and `return` are keywords with written lowerings.\n\
#\n\
# # Which page for what\n\
#\n\
#   overview    this page\n\
#   quickstart  a project from nothing, with the commands\n\
#   workflow    how to write a program you can prove\n\
#   rules       every restriction that rejects code — read before emitting\n\
#   grammar     the EBNF\n\
#   syntax      lexical rules, declarations, statements, expressions\n\
#   types       num, str, bool, list, map, struct; ownership; purity\n\
#   memory      _vms, _heap, _stackN, watch, decorators\n\
#   costs       blocks per statement, expression and definition\n\
#   macros      fn, macro, proc, parameters, hygiene, totality\n\
#   modules     use, pub, and what an import duplicates\n\
#   errors      the diagnostic format, and every class of error with its fix\n\
#   stdlib      every catalog block by raven name, one line each (generated)\n\
#   std         every module a name can come from\n\
#   menus       every dropdown as a type\n\
#   prelude     crates/raven/src/prelude.rav, verbatim\n\
#   cli         the command line\n\
#   asm*        the layer below: raven-asm, and the optimiser over it\n\
#   re          raven-re, the decompiler\n\
#   docs/*      the human documentation, verbatim\n\
#\n\
# # Related, but not in this tool\n\
#\n\
#   raven-asm catalog --json    every block of the layer below, with its arguments\n\
#   raven explain --json        this manual's index, as data\n\
#   docs: ";

/// `quickstart` — a project end to end.
pub const QUICKSTART: &str = "\n## quickstart\n\
# # A project from nothing\n\
#\n\
#     raven new hello\n\
#     cd hello\n\
#     raven check         parse, resolve, type check, expand; writes nothing\n\
#     raven expand        print the raven-asm this program became\n\
#     raven build         write dist/hello.sb3\n\
#\n\
# `raven new` writes a manifest, a stage, a sprite and a working example.\n\
# `raven init [path]` scaffolds into a directory that already exists. Both take\n\
# `--force`; `raven new` also takes `--here` and `--with-module`.\n\
#\n\
# # The shape of a project\n\
#\n\
#     raven.toml\n\
#     src/stage.rav\n\
#     src/sprites/hello.rav\n\
#     src/lib/…              module files, imported with `use`\n\
#     assets/…               costumes and sounds\n\
#\n\
#     # raven.toml\n\
#     [project]\n\
#     name = \"hello\"\n\
#     output = \"dist\"\n\
#     extensions = []\n\
#\n\
#     [targets]\n\
#     stage = \"src/stage.rav\"\n\
#     sprites = [\"src/sprites/hello.rav\"]\n\
#\n\
# # A target, and a module\n\
#\n\
#     // src/stage.rav — a target file declares exactly one target\n\
#     stage {\n\
#         costume \"paper\" = \"assets/backdrop.svg\";\n\
#     }\n\
#\n\
#     // src/lib/geometry.rav — a module file declares no target\n\
#     pub const TAU: num = 6.283185307179586;\n\
#     pub fn hypot(a: num, b: num) -> num {\n\
#         operators::mathop(MathOp::Sqrt, a * a + b * b)\n\
#     }\n\
#\n\
#     // src/sprites/hello.rav\n\
#     use lib::geometry::hypot;\n\
#\n\
#     sprite \"Hello\" {\n\
#         costume \"dot\" = \"assets/dot.svg\";\n\
#         var count: num = 0;\n\
#\n\
#         proc step() { count += 1; }\n\
#\n\
#         on flag_clicked {\n\
#             repeat 3 { step(); }\n\
#             looks::say(f\"count {count}, distance {hypot(3, 4)}\");\n\
#         }\n\
#     }\n\
#\n\
# Every target needs at least one `costume`, even a stage. A sprite's name must be\n\
# unique and `Stage` is reserved.\n\
#\n\
# # What is written where\n\
#\n\
#   dist/<name>.sb3    the archive Scratch opens. A plain build writes this one\n\
#                      file, and takes back the dist/asm/ a previous --debug left\n\
#   dist/asm/**        with --debug: the raven-asm the project lowered to, with\n\
#                      its own raven-asm.toml, so `cd dist/asm && raven-asm build`\n\
#                      builds the same project without raven\n\
#   dist/project.json  with --debug: the uncompressed Scratch project\n\
#   dist/layout.json   with --debug: where every list and cell landed, by name —\n\
#                      the file a node-side check reads to find `list(\"board\")`\n\
#\n\
# # The first thing to try when something is wrong\n\
#\n\
# `raven check` names the line, the column and the problem, and it never writes\n\
# and never panics. `raven expand` shows what the construct became, which is how a\n\
# cost claim is settled. `raven fmt` re-indents from brace depth without reflowing\n\
# code or dropping a comment.\n";

/// `workflow` — the loop, and what each step proves.
pub const WORKFLOW: &str = "\n## workflow\n\
# # The loop\n\
#\n\
#   1. Read `overview`, `rules`, `grammar` and `stdlib`, and keep them. `stdlib`\n\
#      is what stops a block being invented: a name that is not in it does not\n\
#      exist. Never write a call that is not in `stdlib` and not declared in the\n\
#      project.\n\
#   2. Write `.rav` files.\n\
#   3. Run `raven check`. It parses, resolves, type checks and expands, writes\n\
#      nothing, and reports every error with a line and a caret.\n\
#   4. Run `raven expand` to see the raven-asm the program became. This is how a\n\
#      cost claim is checked and how a macro's lowering is read.\n\
#   5. Run `raven build` for the `.sb3`.\n\
#\n\
# # What proves what\n\
#\n\
#   raven check      the program is well formed: syntax, names, types, macro\n\
#                    arguments, shapes. Not that it does the right thing.\n\
#   raven expand     the program became the blocks you think it did. The only\n\
#                    way to settle a question about cost or about a macro.\n\
#   raven build      the archive exists and Scratch's own reader accepts it.\n\
#   a real VM        `tools/validate-sb3.js <file.sb3> --steps 1500` loads the\n\
#                    archive into a Scratch VM, checks every opcode is one the\n\
#                    runtime knows, runs it and reports runtime errors. This is\n\
#                    the only end-to-end check outside the editor.\n\
#\n\
# # Writing for a machine that will read it back\n\
#\n\
# * Put logic that should be shared and free in a `macro` or an `fn`; put logic\n\
#   that must be a real custom block in a `proc`, and expect one copy per sprite.\n\
# * Prefer `let` over a macro argument when the value comes from a sensor: a\n\
#   `let` reads the world once, a macro parameter is substituted wherever it is\n\
#   used.\n\
# * Keep `warp` on any procedure that must run without yielding. A `warp`\n\
#   procedure cannot be inlined into a yielding caller, which is observable.\n\
# * Nothing is short-circuited. `i != 0 && 10 / i > 1` divides by zero.\n\
# * A comparison does not chain. `a < b < c` is an error; write `a < b && b < c`.\n\
# * When a construct is not in this manual, it is not in the language. The\n\
#   absent list is short and deliberate: no conditional expression, no `break`,\n\
#   no `continue`, no `as`, no general attributes, no string `+`, no\n\
#   `bool(x)`.\n\
#\n\
# # Reporting a problem with the compiler\n\
#\n\
# An internal panic prints a Rust backtrace instead of a diagnostic, which is\n\
# always a bug: a malformed input is a rendered error with a span, a note, and the\n\
# closest match when there is one. Exit codes: 0 success, 1 a diagnostic, 2 bad\n\
# command line usage.\n";

/// The `overview` page ends with the documentation URL, which must come from
/// `identity` rather than a literal.
pub fn overview() -> String {
    format!("{OVERVIEW}{}\n", identity::DOCS)
}
