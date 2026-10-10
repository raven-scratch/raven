//! `raven explain`: the manual, written for a machine reader.
//!
//! A model that has to emit raven source has no REPL, cannot open the editor and
//! cannot ask a question, so everything it needs has to be printable by the
//! compiler. This module is that reference, split into **pages** so a caller can
//! take the part it needs instead of the whole manual.
//!
//! # Two halves
//!
//! The `start`, `lang`, `lib`, `tool`, `below` and `re` pages are written for a
//! generator: dense, ordered by what a reader needs first, and **generated from
//! the compiler's own tables** wherever a table exists, so a fact cannot be
//! described here and missing from the compiler. `stdlib` and `menus` are the two
//! that matter most: they are built by walking `stdlib::BINDINGS` and
//! `menu::menu_ids()`, not retyped.
//!
//! The `guide` pages are the human documentation under `docs/`, embedded
//! verbatim. They are prose, written for a person, and they are here so that
//! *nothing* about raven requires the repository or a website: where a reference
//! page is terse, the guide page beside it is the long version.
//!
//! # Determinism
//!
//! Every page is a function of the catalog, the binding table, the menu table or
//! a file in the tree, so the output is byte-stable across builds and safe to
//! cache, diff or paste into a context window.

mod below;
mod decompiler;
mod guide;
mod lang;
mod library;
mod start;
mod tool;

use crate::identity;

/// One section of the manual.
pub struct Group {
    /// The name `raven explain <group>` accepts, and the `group` field in JSON.
    pub id: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
}

/// The groups, in the order the index lists them.
///
/// `docs` is the boundary the module doc calls out: everything before it is a
/// reference page written for a generator, everything after it is the human
/// documentation, verbatim.
pub const GROUPS: &[Group] = &[
    Group {
        id: "start",
        title: "Start here",
        summary: "what raven is, and the loop that works",
    },
    Group {
        id: "lang",
        title: "The language",
        summary: "rules, grammar, statements, types, memory, cost",
    },
    Group {
        id: "lib",
        title: "The library",
        summary: "every callable block, every menu, the prelude",
    },
    Group {
        id: "tool",
        title: "The commands",
        summary: "the raven CLI, and what each command writes",
    },
    Group {
        id: "below",
        title: "The layer below",
        summary: "raven-asm, which raven lowers to before Scratch",
    },
    Group {
        id: "re",
        title: "The decompiler",
        summary: "raven-re, a vanilla .sb3 back into raven-asm",
    },
    Group {
        id: "guide",
        title: "The human documentation",
        summary: "docs/, verbatim, for the long version of anything above",
    },
];

/// How a page produces its text.
pub enum Body {
    /// Written once, as a string literal. The heading is part of it.
    Fixed(&'static str),
    /// Built at print time, from a table or a file.
    Built(fn() -> String),
}

/// One page of the manual.
pub struct Page {
    /// The name `raven explain <id>` accepts.
    pub id: &'static str,
    /// Which [`Group`] it belongs to.
    pub group: &'static str,
    pub title: &'static str,
    /// One line, used by the index.
    pub summary: &'static str,
    pub body: Body,
}

/// Every page, in the order the index and `all` print them.
///
/// This is the single place a page is declared. The page's own text repeats its
/// id in the `## <id>` heading the printer and this table must agree on, which
/// `every_page_carries_its_heading` checks.
pub const PAGES: &[Page] = &[
    // --- start -------------------------------------------------------------
    Page {
        id: "overview",
        group: "start",
        title: "What raven is",
        summary: "the two layers, the promise, and which page to read for what",
        body: Body::Built(start::overview),
    },
    Page {
        id: "quickstart",
        group: "start",
        title: "A first project",
        summary: "scaffold, write, check, expand, build — with the exact commands",
        body: Body::Fixed(start::QUICKSTART),
    },
    Page {
        id: "workflow",
        group: "start",
        title: "The loop that works",
        summary: "how to write a program you can prove, and what each command proves",
        body: Body::Fixed(start::WORKFLOW),
    },
    // --- lang --------------------------------------------------------------
    Page {
        id: "rules",
        group: "lang",
        title: "The rules that reject code",
        summary: "every restriction, first, because they are what a generator trips on",
        body: Body::Fixed(lang::RULES),
    },
    Page {
        id: "grammar",
        group: "lang",
        title: "Grammar",
        summary: "the whole EBNF, including every keyword and every operator",
        body: Body::Fixed(lang::GRAMMAR),
    },
    Page {
        id: "syntax",
        group: "lang",
        title: "Syntax in full",
        summary: "lexical rules, items, declarations, statements, expressions",
        body: Body::Fixed(lang::SYNTAX),
    },
    Page {
        id: "types",
        group: "lang",
        title: "Types and shapes",
        summary: "num, str, bool, list, map, struct; ownership; purity; conversions",
        body: Body::Fixed(lang::TYPES),
    },
    Page {
        id: "memory",
        group: "lang",
        title: "The memory model",
        summary: "_vms, _heap, _stackN, _console, what a cell is, watch, decorators",
        body: Body::Fixed(lang::MEMORY),
    },
    Page {
        id: "costs",
        group: "lang",
        title: "What everything costs",
        summary: "blocks emitted per statement, per expression and per definition",
        body: Body::Fixed(lang::COSTS),
    },
    Page {
        id: "macros",
        group: "lang",
        title: "Macros, fn and proc",
        summary: "the one expansion mechanism, its parameters, and its limits",
        body: Body::Fixed(lang::MACROS),
    },
    Page {
        id: "modules",
        group: "lang",
        title: "Modules and visibility",
        summary: "target files, module files, use, pub, and what an import costs",
        body: Body::Fixed(lang::MODULES),
    },
    Page {
        id: "errors",
        group: "lang",
        title: "Reading a diagnostic",
        summary: "the diagnostic format, and every class of error with its fix",
        body: Body::Fixed(lang::ERRORS),
    },
    // --- lib ---------------------------------------------------------------
    Page {
        id: "stdlib",
        group: "lib",
        title: "Every block, by raven name",
        summary: "one line per catalog block: opcode, spelling, arguments, result",
        body: Body::Built(library::stdlib_section),
    },
    Page {
        id: "std",
        group: "lib",
        title: "The standard modules",
        summary: "every module a name can come from, and the rules a call follows",
        body: Body::Built(library::std_section),
    },
    Page {
        id: "menus",
        group: "lib",
        title: "Every dropdown",
        summary: "<MenuType>::<Variant> for every menu in the catalog",
        body: Body::Built(library::menus),
    },
    Page {
        id: "prelude",
        group: "lib",
        title: "The prelude, verbatim",
        summary: "crates/raven/src/prelude.rav, the macros every file gets",
        body: Body::Built(library::prelude),
    },
    // --- tool --------------------------------------------------------------
    Page {
        id: "cli",
        group: "tool",
        title: "The raven command line",
        summary: "every command, every flag, and every file it writes",
        body: Body::Built(tool::cli),
    },
    // --- below -------------------------------------------------------------
    Page {
        id: "asm",
        group: "below",
        title: "raven-asm, the layer below",
        summary: "what it is, what it promises, and why raven lowers to it",
        body: Body::Built(below::asm_overview),
    },
    Page {
        id: "asm-syntax",
        group: "below",
        title: "raven-asm syntax",
        summary: "comments, literals, statements, and the whole grammar",
        body: Body::Built(below::asm_syntax),
    },
    Page {
        id: "asm-procedures",
        group: "below",
        title: "raven-asm procedures and control",
        summary: "calls, arguments, results, warp, and the control blocks",
        body: Body::Built(below::asm_procedures),
    },
    Page {
        id: "asm-variables",
        group: "below",
        title: "raven-asm variables and lists",
        summary: "what a variable is, what may be named, and what is refused",
        body: Body::Built(below::asm_variables),
    },
    Page {
        id: "asm-assets",
        group: "below",
        title: "raven-asm assets",
        summary: "costumes, sounds, rotation centres and what a file must hold",
        body: Body::Built(below::asm_assets),
    },
    Page {
        id: "asm-project",
        group: "below",
        title: "A raven-asm project on disk",
        summary: "the manifest, the file kinds, layout and multi-file projects",
        body: Body::Built(below::asm_project),
    },
    Page {
        id: "asm-cli",
        group: "below",
        title: "The raven-asm command line",
        summary: "build, check, catalog, new, init, clean, and what each writes",
        body: Body::Built(below::asm_cli),
    },
    Page {
        id: "asm-optimizer",
        group: "below",
        title: "The optimiser",
        summary: "raven-opt: what it rewrites, what it refuses, and what it is worth",
        body: Body::Built(below::asm_optimizer),
    },
    // --- re ----------------------------------------------------------------
    Page {
        id: "decompiler",
        group: "re",
        title: "The decompiler",
        summary: "raven-re: a vanilla .sb3 back into raven-asm source",
        body: Body::Built(decompiler::re),
    },
    // --- guide -------------------------------------------------------------
    Page {
        id: "docs/index",
        group: "guide",
        title: "The guide's front page",
        summary: "what raven is, for a reader who is not a model",
        body: Body::Fixed(guide::INDEX),
    },
    Page {
        id: "docs/guide/index",
        group: "guide",
        title: "Guide",
        summary: "where to start reading",
        body: Body::Fixed(guide::GUIDE_INDEX),
    },
    Page {
        id: "docs/guide/getting-started",
        group: "guide",
        title: "Getting started",
        summary: "install, scaffold, and run something",
        body: Body::Fixed(guide::GETTING_STARTED),
    },
    Page {
        id: "docs/guide/for-llms",
        group: "guide",
        title: "For LLMs",
        summary: "how a model is meant to use this toolchain",
        body: Body::Fixed(guide::FOR_LLMS),
    },
    Page {
        id: "docs/raven/index",
        group: "guide",
        title: "The raven language",
        summary: "the front page of the language guide",
        body: Body::Fixed(guide::RAVEN_INDEX),
    },
    Page {
        id: "docs/raven/design",
        group: "guide",
        title: "Design laws",
        summary: "the twelve laws, and the designs that were rejected",
        body: Body::Fixed(guide::RAVEN_DESIGN),
    },
    Page {
        id: "docs/raven/syntax",
        group: "guide",
        title: "Syntax",
        summary: "the long version of the syntax page",
        body: Body::Fixed(guide::RAVEN_SYNTAX),
    },
    Page {
        id: "docs/raven/types",
        group: "guide",
        title: "Types and shapes",
        summary: "the long version of the types page",
        body: Body::Fixed(guide::RAVEN_TYPES),
    },
    Page {
        id: "docs/raven/std",
        group: "guide",
        title: "The standard library",
        summary: "the rules, the console, extensions, and the refused blocks",
        body: Body::Fixed(guide::RAVEN_STD),
    },
    Page {
        id: "docs/raven/macros",
        group: "guide",
        title: "Macros",
        summary: "why there is one expansion mechanism, and how it behaves",
        body: Body::Fixed(guide::RAVEN_MACROS),
    },
    Page {
        id: "docs/raven/modules",
        group: "guide",
        title: "Modules",
        summary: "files, use, visibility, cycles and shared state",
        body: Body::Fixed(guide::RAVEN_MODULES),
    },
    Page {
        id: "docs/raven/lowering",
        group: "guide",
        title: "From raven to Scratch",
        summary: "a worked program lowered step by step, with the cost of everything",
        body: Body::Fixed(guide::RAVEN_LOWERING),
    },
    Page {
        id: "docs/raven/cli",
        group: "guide",
        title: "Command line",
        summary: "the long version of the CLI page",
        body: Body::Fixed(guide::RAVEN_CLI),
    },
    Page {
        id: "docs/raven/from-scrust",
        group: "guide",
        title: "Coming from Scrust",
        summary: "what changed, for a reader who knows the older language",
        body: Body::Fixed(guide::RAVEN_FROM_SCRUST),
    },
    Page {
        id: "docs/raven-asm/index",
        group: "guide",
        title: "raven-asm",
        summary: "the front page of the assembly-level guide",
        body: Body::Fixed(guide::ASM_INDEX),
    },
    Page {
        id: "docs/raven-asm/design",
        group: "guide",
        title: "raven-asm design",
        summary: "the laws the layer below keeps",
        body: Body::Fixed(guide::ASM_DESIGN),
    },
    Page {
        id: "docs/raven-asm/syntax",
        group: "guide",
        title: "raven-asm syntax, long version",
        summary: "every statement form with its reasoning",
        body: Body::Fixed(guide::ASM_SYNTAX),
    },
    Page {
        id: "docs/raven-asm/blocks",
        group: "guide",
        title: "raven-asm blocks",
        summary: "how a block is written and what its shape means",
        body: Body::Fixed(guide::ASM_BLOCKS),
    },
    Page {
        id: "docs/raven-asm/variables",
        group: "guide",
        title: "raven-asm variables",
        summary: "what may be named, and the five refused blocks",
        body: Body::Fixed(guide::ASM_VARIABLES),
    },
    Page {
        id: "docs/raven-asm/procedures",
        group: "guide",
        title: "raven-asm procedures",
        summary: "calls, parameters, results and warp",
        body: Body::Fixed(guide::ASM_PROCEDURES),
    },
    Page {
        id: "docs/raven-asm/assets",
        group: "guide",
        title: "raven-asm assets",
        summary: "costumes, sounds and rotation centres",
        body: Body::Fixed(guide::ASM_ASSETS),
    },
    Page {
        id: "docs/raven-asm/project-structure",
        group: "guide",
        title: "Project structure",
        summary: "the manifest and the files beside it",
        body: Body::Fixed(guide::ASM_PROJECT_STRUCTURE),
    },
    Page {
        id: "docs/raven-asm/multi-file",
        group: "guide",
        title: "Multi-file projects",
        summary: "use, modules and what an import duplicates",
        body: Body::Fixed(guide::ASM_MULTI_FILE),
    },
    Page {
        id: "docs/raven-asm/cli",
        group: "guide",
        title: "raven-asm command line",
        summary: "the long version of the raven-asm CLI page",
        body: Body::Fixed(guide::ASM_CLI),
    },
    Page {
        id: "docs/raven-asm/troubleshooting",
        group: "guide",
        title: "Troubleshooting",
        summary: "the errors a raven-asm project meets, and what they mean",
        body: Body::Fixed(guide::ASM_TROUBLESHOOTING),
    },
    Page {
        id: "docs/raven-asm/optimizer",
        group: "guide",
        title: "What the optimiser does, long version",
        summary: "the two rewrites, the measurements, and the refusals",
        body: Body::Fixed(guide::ASM_OPTIMIZER),
    },
    Page {
        id: "docs/raven-re/index",
        group: "guide",
        title: "raven-re",
        summary: "the decompiler, and the promise it keeps",
        body: Body::Fixed(guide::RE_INDEX),
    },
    Page {
        id: "docs/raven-re/cli",
        group: "guide",
        title: "raven-re command line",
        summary: "reversing one project, and the flags that change it",
        body: Body::Fixed(guide::RE_CLI),
    },
    Page {
        id: "docs/reference/blocks",
        group: "guide",
        title: "The block reference",
        summary: "the generated catalog reference; regenerate with raven-asm catalog",
        body: Body::Fixed(guide::REFERENCE_BLOCKS),
    },
];

/// What `raven explain` was asked for, after the command line has parsed it.
#[derive(Debug, Default)]
pub struct Request {
    /// Page ids, group ids, or `all`. Empty means the index.
    pub pages: Vec<String>,
    /// Print the index even when pages were named.
    pub list: bool,
    /// Print the index as JSON.
    pub json: bool,
    /// Print the pages and lines that contain this text.
    pub grep: Option<String>,
}

/// Print what the request asked for. An unknown name is an error naming the
/// nearest ones rather than a panic.
pub fn run(request: &Request) -> Result<(), String> {
    if let Some(needle) = &request.grep {
        print!("{}", grep(needle));
        return Ok(());
    }
    if request.json {
        print!("{}", index_json());
        return Ok(());
    }
    if request.list || request.pages.is_empty() {
        print!("{}", index());
        return Ok(());
    }
    for wanted in &request.pages {
        print!("{}", resolve(wanted)?);
    }
    Ok(())
}

/// The text of one page, group or `all`.
pub fn resolve(name: &str) -> Result<String, String> {
    if name == "all" {
        return Ok(all());
    }
    if let Some(group) = GROUPS.iter().find(|g| g.id == name) {
        return Ok(PAGES
            .iter()
            .filter(|p| p.group == group.id)
            .map(text)
            .collect::<String>());
    }
    if let Some(page) = PAGES.iter().find(|p| p.id == name) {
        return Ok(text(page));
    }
    Err(format!(
        "unknown page `{name}`\n\
         = note: there are {} pages in {} groups\n\
         = note: `{} explain` prints the index, which lists every one of them\n\
         = note: `{} explain --grep {name}` searches the pages for the text",
        PAGES.len(),
        GROUPS.len(),
        identity::CRATE,
        identity::CRATE,
    ))
}

/// Every page, in index order.
#[must_use]
pub fn all() -> String {
    PAGES.iter().map(text).collect()
}

/// One page's text. Panics only on a page the table does not hold.
#[must_use]
pub fn text(page: &Page) -> String {
    match page.body {
        Body::Fixed(text) => text.to_string(),
        Body::Built(build) => build(),
    }
}

/// The table of contents: every page, its group, its size and its one line.
#[must_use]
pub fn index() -> String {
    let mut out = format!(
        "\n## index\n\
         # {crate} {version} — the language reference written for a machine reader.\n\
         #\n\
         #   {crate} explain <page>           one page\n\
         #   {crate} explain <page> <page>    several, in the order given\n\
         #   {crate} explain <group>          every page in a group\n\
         #   {crate} explain all              every page ({total} bytes)\n\
         #   {crate} explain --list           this index\n\
         #   {crate} explain --json           this index as JSON\n\
         #   {crate} explain --grep <text>    the pages and lines that mention it\n\
         #\n\
         # Read `overview` first, then `rules`, `grammar` and `stdlib`. The pages up to\n\
         # and including `decompiler` are written for a generator, and are generated from\n\
         # the compiler's own tables where a table exists. The `guide` pages are the human\n\
         # documentation under docs/, verbatim, for the long version of anything above.\n\
         #\n\
         # docs: {docs}\n",
        crate = identity::CRATE,
        version = env!("CARGO_PKG_VERSION"),
        total = all().len(),
        docs = identity::DOCS,
    );
    for group in GROUPS {
        out.push_str(&format!("#\n# --- {} — {}\n#\n", group.id, group.summary));
        for page in PAGES.iter().filter(|p| p.group == group.id) {
            out.push_str(&format!(
                "{:<32} {:>7}  {}\n",
                page.id,
                text(page).len(),
                page.title
            ));
        }
    }
    out
}

/// The index as JSON, for a tool that would rather parse than read.
#[must_use]
pub fn index_json() -> String {
    let mut out = format!(
        "{{\n  \"tool\": \"{}\",\n  \"version\": \"{}\",\n  \"docs\": \"{}\",\n  \
         \"groups\": [\n",
        identity::CRATE,
        env!("CARGO_PKG_VERSION"),
        escape(identity::DOCS),
    );
    for (i, group) in GROUPS.iter().enumerate() {
        out.push_str(&format!(
            "    {{ \"id\": \"{}\", \"title\": \"{}\", \"summary\": \"{}\" }}{}\n",
            escape(group.id),
            escape(group.title),
            escape(group.summary),
            if i + 1 == GROUPS.len() { "" } else { "," },
        ));
    }
    out.push_str("  ],\n  \"pages\": [\n");
    for (i, page) in PAGES.iter().enumerate() {
        out.push_str(&format!(
            "    {{ \"id\": \"{}\", \"group\": \"{}\", \"title\": \"{}\", \
             \"summary\": \"{}\", \"bytes\": {} }}{}\n",
            escape(page.id),
            escape(page.group),
            escape(page.title),
            escape(page.summary),
            text(page).len(),
            if i + 1 == PAGES.len() { "" } else { "," },
        ));
    }
    out.push_str("  ]\n}\n");
    out
}

/// Every page that mentions `needle`, and the lines that do.
///
/// This is the cheapest way for a caller to answer "where is this written
/// down?" without taking the whole manual: it searches the reference pages *and*
/// the embedded guide, so one query covers both halves.
#[must_use]
pub fn grep(needle: &str) -> String {
    /// How many matching lines one page prints before it says how many are left.
    const PER_PAGE: usize = 25;

    let mut out = format!(
        "\n## grep {needle}\n# Lines containing `{needle}`, page by page. \
         A page is named by the id `{crate} explain` accepts.\n",
        crate = identity::CRATE,
    );
    let mut pages_hit = 0;
    let mut lines_hit = 0;
    for page in PAGES {
        let body = text(page);
        let hits: Vec<&str> = body.lines().filter(|line| line.contains(needle)).collect();
        if hits.is_empty() {
            continue;
        }
        pages_hit += 1;
        lines_hit += hits.len();
        out.push_str(&format!("\n# --- {} ({})\n", page.id, page.title));
        for line in hits.iter().take(PER_PAGE) {
            out.push_str(line);
            out.push('\n');
        }
        if hits.len() > PER_PAGE {
            out.push_str(&format!(
                "# … and {} more line(s) in this page\n",
                hits.len() - PER_PAGE
            ));
        }
    }
    out.push_str(&format!(
        "\n# {lines_hit} line(s) in {pages_hit} of {} page(s).\n",
        PAGES.len()
    ));
    out
}

/// The pages in a group, for the tests and for anything that walks the manual.
#[must_use]
pub fn group_pages(id: &str) -> Vec<&'static Page> {
    PAGES.iter().filter(|p| p.group == id).collect()
}

/// JSON string escaping, for the handful of strings the index carries. Every one
/// of them is written here or read from a table, so this only has to be correct,
/// not fast.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_carries_its_heading() {
        for page in PAGES {
            let body = text(page);
            assert!(
                body.starts_with(&format!("\n## {}\n", page.id)),
                "`{}` must start with its own heading",
                page.id
            );
            assert!(
                body.len() > 400,
                "`{}` is suspiciously short ({} bytes)",
                page.id,
                body.len()
            );
        }
    }

    #[test]
    fn every_page_names_a_real_group() {
        for page in PAGES {
            assert!(
                GROUPS.iter().any(|g| g.id == page.group),
                "`{}` names group `{}`, which does not exist",
                page.id,
                page.group
            );
        }
    }

    #[test]
    fn page_ids_and_group_ids_do_not_collide() {
        for page in PAGES {
            assert!(
                !GROUPS.iter().any(|g| g.id == page.id),
                "`{}` is both a page and a group",
                page.id
            );
        }
        let mut sorted: Vec<&str> = PAGES.iter().map(|p| p.id).collect();
        let count = sorted.len();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(count, sorted.len(), "two pages share an id");
    }

    #[test]
    fn every_group_has_pages() {
        for group in GROUPS {
            assert!(
                PAGES.iter().any(|p| p.group == group.id),
                "group `{}` has no pages",
                group.id
            );
        }
    }

    #[test]
    fn the_stdlib_page_covers_every_binding() {
        let body = text(PAGES.iter().find(|p| p.id == "stdlib").unwrap());
        for row in crate::stdlib::BINDINGS {
            assert!(
                body.contains(&format!("{} :: ", row.opcode)),
                "`{}` is missing from `raven explain stdlib`",
                row.opcode
            );
        }
    }

    #[test]
    fn the_menus_page_covers_every_menu() {
        let body = text(PAGES.iter().find(|p| p.id == "menus").unwrap());
        for id in crate::menu::menu_ids() {
            assert!(
                body.contains(&format!("{id} -> ")),
                "menu `{id}` is missing from `raven explain menus`"
            );
        }
    }

    #[test]
    fn the_std_page_names_every_module() {
        let body = text(PAGES.iter().find(|p| p.id == "std").unwrap());
        for module in crate::stdlib::modules() {
            assert!(
                body.contains(&format!("std::{module}")),
                "module `{module}` is missing from `raven explain std`"
            );
        }
    }

    #[test]
    fn the_index_lists_every_page_and_the_json_agrees() {
        let listing = index();
        for page in PAGES {
            assert!(
                listing.contains(&format!("\n{:<32}", page.id)),
                "`{}` is missing from the index",
                page.id
            );
        }
        let json = index_json();
        for page in PAGES {
            assert!(
                json.contains(&format!("\"id\": \"{}\"", page.id)),
                "`{}` is missing from the JSON index",
                page.id
            );
        }
        assert_eq!(
            json.matches("\"id\": \"").count(),
            PAGES.len() + GROUPS.len()
        );
    }

    #[test]
    fn a_group_resolves_to_all_of_its_pages() {
        let text = resolve("lang").expect("`lang` is a group");
        for page in group_pages("lang") {
            assert!(
                text.contains(&format!("\n## {}\n", page.id)),
                "group `lang` is missing `{}`",
                page.id
            );
        }
    }

    #[test]
    fn a_name_is_the_page_or_the_group_and_nothing_else() {
        let one = resolve("rules").expect("a page");
        assert!(one.starts_with("\n## rules\n"));
        let everything = resolve("all").expect("everything");
        assert_eq!(everything.len(), all().len());
        assert!(everything.contains("\n## rules\n"));
    }

    #[test]
    fn an_unknown_name_is_an_error_and_not_a_panic() {
        let error = resolve("nonesuch").expect_err("must fail");
        assert!(error.contains("unknown page `nonesuch`"), "{error}");
    }

    #[test]
    fn grep_finds_a_fact_in_the_reference_and_in_the_guide() {
        let found = grep("_vms");
        assert!(found.starts_with("\n## grep _vms\n"), "{found}");
        assert!(
            found.contains("docs/raven/types"),
            "the guide half is searched too"
        );
        let nothing = grep("zzzznotathing");
        assert!(nothing.contains("0 line(s) in 0 of"), "{nothing}");
    }

    #[test]
    fn json_escaping_is_escaping() {
        assert_eq!(escape("a\"b\\c\nd"), "a\\\"b\\\\c\\nd");
    }
}
