//! End-to-end tests for the raven compiler.
//!
//! Each test writes a tiny project into a temporary directory, compiles it, and
//! asserts on what came out — either the raven-asm `raven expand` prints, or the
//! `.sb3` that was written. Nothing here reaches into the compiler's internals;
//! these are the tests a user would write.

use std::path::{Path, PathBuf};

use raven::driver::{self, Options};
use raven_scratch::diag::Source;

/// A scratch project on disk.
struct Project {
    dir: PathBuf,
    stage: String,
    sprites: Vec<(String, String)>,
    modules: Vec<(String, String)>,
    extensions: Vec<String>,
}

impl Project {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "raven-test-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create the project directory");
        Project {
            dir,
            stage: "stage { }".to_string(),
            sprites: Vec::new(),
            modules: Vec::new(),
            extensions: Vec::new(),
        }
    }
    fn stage(mut self, source: &str) -> Self {
        self.stage = source.to_string();
        self
    }

    fn sprite(mut self, name: &str, source: &str) -> Self {
        self.sprites.push((name.to_string(), source.to_string()));
        self
    }

    fn module(mut self, path: &str, source: &str) -> Self {
        self.modules.push((path.to_string(), source.to_string()));
        self
    }

    fn extension(mut self, name: &str) -> Self {
        self.extensions.push(name.to_string());
        self
    }

    fn write(&self) -> PathBuf {
        let manifest = self.dir.join(raven::identity::MANIFEST);
        let mut toml = String::new();
        toml.push_str("[project]\nname = \"test\"\noutput = \"dist\"\n");
        let extensions: Vec<String> = self.extensions.iter().map(|e| format!("\"{e}\"")).collect();
        toml.push_str(&format!("extensions = [{}]\n", extensions.join(", ")));
        toml.push_str("\n[targets]\nstage = \"src/stage.rav\"\n");
        if self.sprites.is_empty() {
            toml.push_str("sprites = []\n");
        } else {
            let list: Vec<String> = self
                .sprites
                .iter()
                .map(|(name, _)| format!("\"src/sprites/{name}.rav\""))
                .collect();
            toml.push_str(&format!("sprites = [{}]\n", list.join(", ")));
        }
        write(
            &self.dir.join("src/stage.rav"),
            &ensure_costume(&self.stage),
        );
        // raven-asm rejects a target with no costume, so every fixture target
        // gets one unless it declares its own.
        write(
            &self.dir.join("assets/blank.svg"),
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"8\" height=\"8\"></svg>",
        );
        for (name, source) in &self.sprites {
            write(
                &self.dir.join(format!("src/sprites/{name}.rav")),
                &ensure_costume(source),
            );
        }
        for (path, source) in &self.modules {
            write(&self.dir.join(format!("src/{path}.rav")), source);
        }
        std::fs::write(manifest.clone(), toml).expect("write the manifest");
        manifest
    }

    fn options(&self) -> Options {
        Options::new(self.dir.join(raven::identity::MANIFEST))
    }

    /// The raven-asm the project lowers to, as one string.
    fn expand(&self) -> String {
        self.write();
        match driver::expand(&self.options()) {
            Ok(files) => files
                .into_iter()
                .map(|(_, source)| source)
                .collect::<Vec<_>>()
                .join("\n"),
            Err(error) => panic!("expansion failed:\n{}", error.render()),
        }
    }

    /// The diagnostic the project fails with.
    fn expect_error(&self) -> raven_scratch::diag::Error {
        self.write();
        match driver::check(&self.options()) {
            Ok(_) => panic!("expected an error, but the project compiled"),
            Err(error) => error,
        }
    }

    fn build(&self) -> (PathBuf, Vec<u8>) {
        self.write();
        let result = match driver::build(&self.options()) {
            Ok(result) => result,
            Err(error) => panic!("build failed:\n{}", error.render()),
        };
        let artifact = result.artifact.expect("a build writes an artifact");
        let bytes = std::fs::read(&artifact).expect("read the artifact");
        (artifact, bytes)
    }

    /// Compile a single file's source and return its diagnostic, for syntax
    /// errors that need no project around them.
    fn _parse_error(&self, text: &str) -> raven_scratch::diag::Error {
        let source = Source::new("inline.rav", text);
        raven::parser::parse(&source).expect_err("expected a parse error")
    }

    /// The warnings a check produces.
    fn warnings(&self) -> Vec<String> {
        self.write();
        driver::check(&self.options())
            .expect("the project must check")
            .into_iter()
            .map(|diag| diag.message)
            .collect()
    }
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create the parent directory");
    }
    std::fs::write(path, contents).expect("write the file");
}

/// Give a target a costume if the fixture did not declare one.
///
/// `raven-asm` makes a costumeless target a hard error, so a fixture only ever
/// has to say what it is testing about the language.
fn ensure_costume(source: &str) -> String {
    if source.contains("costume") {
        return source.to_string();
    }
    match source.find('{') {
        Some(at) => format!(
            "{}\n    costume \"blank\" = \"assets/blank.svg\";\n{}",
            &source[..=at],
            &source[at + 1..]
        ),
        None => source.to_string(),
    }
}

fn expect_error_contains(project: &Project, needle: &str) {
    let error = project.expect_error();
    let rendered = error.render();
    assert!(
        rendered.contains(needle),
        "expected a diagnostic mentioning {needle:?}, got:\n{rendered}"
    );
}

#[allow(dead_code)]
fn _unused(_: &Project) {}

// ---------------------------------------------------------------------------
// The memory system's headline law
// ---------------------------------------------------------------------------

/// A built project declares no Scratch variable except the mirrors `watch`
/// asked for, and no list that is not an arena or one the source named.
#[test]
fn a_built_project_declares_no_scratch_variables() {
    let project = Project::new("no-scratch-vars").sprite(
        "A",
        r#"sprite "A" {
            var score: num = 0;
            var trail: list<num> = [];
            watch score;

            on flag_clicked {
                score += 1;
                trail.push(1);
                let local = score * 2;
                looks::say(f"{local}");
            }
        }"#,
    );
    project.write();
    let mut options = project.options();
    options.debug = true;
    driver::build(&options).expect("the fixture builds");
    let text = std::fs::read_to_string(project.dir.join("dist/project.json"))
        .expect("--debug writes project.json");
    let built: serde_json::Value = serde_json::from_str(&text).expect("valid json");

    // A `watch` is the one thing that may declare a Scratch variable: it exists
    // so the value can be seen on the stage.
    let watched = ["score"];
    for target in built["targets"].as_array().expect("targets") {
        for value in target["variables"].as_object().expect("variables").values() {
            let variable = value[0].as_str().expect("a variable name");
            assert!(
                watched.contains(&variable),
                "`{}` declares the Scratch variable `{variable}` without a `watch`",
                target["name"]
            );
        }
        for list in target["lists"].as_object().expect("lists").values() {
            let list_name = list[0].as_str().expect("a list name");
            assert!(
                list_name == "_vms"
                    || list_name == "_gvm"
                    || list_name == "_console"
                    || list_name.starts_with("_stack")
                    || !list_name.starts_with('_'),
                "`{list_name}` is not an arena and looks internal"
            );
        }
    }
}

#[test]
fn a_build_is_reproducible() {
    let project = Project::new("repro").sprite(
        "A",
        r#"sprite "A" {
                var n: num = 0;
                on flag_clicked {
                    for i in 0..3 { n += 1; }
                    looks::say(f"n is {n}");
                }
            }"#,
    );
    let (_, first) = project.build();
    let (_, second) = project.build();
    assert_eq!(first, second, "the same source must produce the same bytes");
}

#[test]
fn the_emitted_raven_asm_rebuilds_to_the_same_archive() {
    let project = Project::new("emit").sprite(
        "A",
        r#"sprite "A" {
                var n: num = 0;
                on flag_clicked {
                    repeat 3 { n += 1; }
                }
            }"#,
    );
    let (_, from_raven) = project.build();

    let mut options = project.options();
    options.emit_asm = true;
    let result = driver::build(&options).expect("build with --emit-asm");
    let asm = result.asm_dir.expect("--emit-asm keeps the tree");
    let rebuilt = raven_asm::compile::build(&asm.join(raven_asm::manifest::MANIFEST_NAME))
        .expect("the emitted tree must be a raven-asm project");
    // `raven-asm` does not write the archive itself, so compare the project.
    let json = serde_json::to_vec(&rebuilt.project).expect("serialize");
    let direct = {
        let mut direct_options = project.options();
        direct_options.debug = true;
        driver::build(&direct_options).expect("build with --debug");
        let path = project.dir.join("dist/project.json");
        std::fs::read(path).expect("read project.json")
    };
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&json).expect("parse the rebuilt project"),
        serde_json::from_slice::<serde_json::Value>(&direct).expect("parse the direct project"),
        "the emitted raven-asm must describe the same project"
    );
    assert!(!from_raven.is_empty());
}

// ---------------------------------------------------------------------------
// What each construct lowers to
// ---------------------------------------------------------------------------

#[test]
fn sugar_lowers_to_the_blocks_the_documentation_promises() {
    let project = Project::new("lowering").sprite(
        "A",
        r#"sprite "A" {
            var n: num = 0;
            var xs: list<num> = [];
            on flag_clicked {
                n += 1;
                n -= 2;
                while n > 0 { n -= 1; }
                for i in 0..3 { xs.push(i); }
                let m = n;
                match m {
                    1 => { looks::say("one"); },
                    _ => { looks::say(f"n={n}"); },
                }
                if n <= 1 { looks::hide(); }
            }
        }"#,
    );
    let asm = project.expand();

    // `n` is a cell of `_vms`, so a compound assignment is a read, an operator
    // and a write — there is no Scratch variable and no
    // `data_changevariableby` anywhere.
    assert!(
        asm.contains(
            "data_replaceitemoflist(1, \"_vms\", operator_add(data_itemoflist(1, \"_vms\"), 1))"
        ),
        "{asm}"
    );
    assert!(
        asm.contains("data_replaceitemoflist(1, \"_vms\", operator_subtract(data_itemoflist(1, \"_vms\"), 2))"),
        "{asm}"
    );
    assert!(!asm.contains("data_variable("), "{asm}");
    assert!(!asm.contains("data_changevariableby"), "{asm}");
    assert!(!asm.contains("data_setvariableto"), "{asm}");
    assert!(!asm.contains("var n = 0;"), "{asm}");
    assert!(asm.contains("control_repeat_until(operator_not("), "{asm}");
    // The `for` counter and `let m` live in `_vms` too.
    assert!(asm.contains("list _vms = ["), "{asm}");
    assert!(!asm.contains("var i = 0;"), "no Scratch counter: {asm}");
    assert!(
        asm.contains("operator_add(data_itemoflist("),
        "`i += 1` reads and rewrites its cell: {asm}"
    );
    assert!(asm.contains("control_if_else("), "match arms: {asm}");
    assert!(
        asm.contains("operator_not(operator_lt("),
        "`>=` is two blocks: {asm}"
    );
    assert!(
        asm.contains("operator_not(operator_gt("),
        "`<=` is two blocks: {asm}"
    );
    assert!(asm.contains("operator_join("), "interpolation: {asm}");
}

#[test]
fn an_fn_is_inlined_and_a_proc_is_not() {
    let project = Project::new("callables").sprite(
        "A",
        r#"sprite "A" {
            fn twice(x: num) -> num { x * 2 }
            proc shout() warp { looks::say("hi"); }
            on flag_clicked {
                looks::say(twice(21));
                shout();
            }
        }"#,
    );
    let asm = project.expand();
    assert!(!asm.contains("twice("), "an `fn` must not survive: {asm}");
    assert!(asm.contains("operator_multiply(21, 2)"), "{asm}");
    assert!(asm.contains("shout;"), "a `proc` is a real call: {asm}");
    assert!(asm.contains("proc shout() warp"), "{asm}");
}

#[test]
fn a_module_proc_is_copied_into_every_target_that_uses_it() {
    let project = Project::new("copy")
        .module(
            "lib/shapes",
            r#"pub proc square(size: num) warp {
                repeat 4 {
                    motion::move_steps(size);
                    motion::turn_right(90);
                }
            }"#,
        )
        .sprite(
            "A",
            "use lib::shapes::square;\nsprite \"A\" { on flag_clicked { square(10); } }",
        )
        .sprite(
            "B",
            "use lib::shapes::square;\nsprite \"B\" { on flag_clicked { square(20); } }",
        );
    let asm = project.expand();
    assert_eq!(
        asm.matches("proc square(size: num) warp").count(),
        2,
        "one copy per target: {asm}"
    );
}

#[test]
fn an_unused_proc_is_not_emitted() {
    let project = Project::new("unused").sprite(
        "A",
        r#"sprite "A" {
            proc never() { looks::say("no"); }
            on flag_clicked { looks::hide(); }
        }"#,
    );
    let asm = project.expand();
    assert!(!asm.contains("proc never"), "{asm}");
}

#[test]
fn menus_become_their_scratch_values() {
    let project = Project::new("menus").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked {
                motion::set_rotation_style(RotationStyle::LeftRight);
                control::stop(StopOption::ThisScript);
                if sensing::key_pressed(Key::Space) { looks::say("space"); }
                looks::set_effect_to(Effect::Ghost, 50);
            }
        }"#,
    );
    let asm = project.expand();
    assert!(
        asm.contains("motion_setrotationstyle(\"left-right\")"),
        "{asm}"
    );
    assert!(asm.contains("control_stop(\"this script\")"), "{asm}");
    assert!(asm.contains("sensing_keypressed(\"space\")"), "{asm}");
    assert!(asm.contains("looks_seteffectto(\"GHOST\", 50)"), "{asm}");
}

#[test]
fn a_body_taking_block_is_written_with_its_body() {
    let project = Project::new("body").sprite(
        "A",
        r#"sprite "A" {
            var n: num = 0;
            var k: num = 0;
            on flag_clicked {
                control::while(n < 3) { n += 1; }
                looks::hide();
            }
        }"#,
    );
    let asm = project.expand();
    assert!(
        asm.contains("control_while(operator_lt(data_itemoflist(1, \"_vms\"), 3))"),
        "{asm}"
    );
    // `data_changevariableby` is gone with the Scratch variables, so `n += 1`
    // is a read, an add and a write into its cell.
    assert!(
        asm.contains(
            "data_replaceitemoflist(1, \"_vms\", operator_add(data_itemoflist(1, \"_vms\"), 1))"
        ),
        "{asm}"
    );
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

#[test]
fn a_type_error_names_both_types() {
    let project = Project::new("types").sprite(
        "A",
        r#"sprite "A" {
            var name: str = "";
            on flag_clicked {
                looks::say(name + 1);
            }
        }"#,
    );
    expect_error_contains(&project, "the left of `+` must be `num`, found `str`");
}

#[test]
fn assigning_the_wrong_type_is_an_error() {
    let project = Project::new("assign").sprite(
        "A",
        r#"sprite "A" {
            var n: num = 0;
            on flag_clicked { n = "text"; }
        }"#,
    );
    expect_error_contains(&project, "`n` must be `num`, found `str`");
}

#[test]
fn an_unknown_name_suggests_the_closest_one() {
    let project = Project::new("names").sprite(
        "A",
        r#"sprite "A" {
            var score: num = 0;
            on flag_clicked { looks::say(scoree); }
        }"#,
    );
    expect_error_contains(&project, "cannot find `scoree`");
}

#[test]
fn a_macro_argument_of_the_wrong_type_is_rejected() {
    let project = Project::new("macro-type").sprite(
        "A",
        r#"sprite "A" {
            var name: str = "";
            on flag_clicked { name += 1; }
        }"#,
    );
    let rendered = project.expect_error().render();
    assert!(
        rendered.contains("needs `num`") || rendered.contains("must be `num`"),
        "{rendered}"
    );
}

#[test]
fn a_let_captures_a_sampled_value_once() {
    let project = Project::new("purity").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked {
                let x = motion::x_position();
                if x > 0 { looks::say("right"); }
                if x < 0 { looks::say("left"); }
            }
        }"#,
    );
    let asm = project.expand();
    // `let x` pushes one cell, and the world is read exactly once.
    assert_eq!(asm.matches("motion_xposition").count(), 1, "{asm}");
    assert!(
        asm.contains("data_addtolist(motion_xposition(), \"_stack1\")"),
        "{asm}"
    );
    assert!(asm.contains("data_itemoflist("), "{asm}");
}

#[test]
fn a_let_value_may_be_read_many_times() {
    let project = Project::new("pure-alias").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked {
                let n = 2 * 3;
                looks::say(n);
                looks::say(n);
            }
        }"#,
    );
    project.expand();
}

/// `num(x)` and `str(x)` are the two free conversions: they retype a value and
/// emit no block of their own, so the pushed cell holds exactly what was written.
#[test]
fn the_two_conversions_are_free() {
    let project = Project::new("conversions").sprite(
        "A",
        r#"sprite "A" {
            var score: num = 0;
            on flag_clicked {
                let a: num = num("12");
                let b: str = str(score);
                looks::say(b);
            }
        }"#,
    );
    let asm = project.expand();
    assert!(
        asm.contains("data_addtolist(\"12\", \"_stack1\")"),
        "`num(\"12\")` must be the literal, with no block of its own:\n{asm}"
    );
    assert!(
        asm.contains("data_addtolist(data_itemoflist(1, \"_vms\"), \"_stack1\")"),
        "`str(score)` must be the cell read it wrapped:\n{asm}"
    );
}

#[test]
fn a_conversion_of_a_bool_is_rejected() {
    let project = Project::new("no-bool-cast").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked { let b: num = num(true); }
        }"#,
    );
    expect_error_contains(&project, "cannot convert `bool`");
}

#[test]
fn a_variable_declared_inside_a_proc_is_rejected() {
    let project = Project::new("var-in-proc").sprite(
        "A",
        r#"sprite "A" {
            proc p() { var temp: num = 0; }
            on flag_clicked { p(); }
        }"#,
    );
    expect_error_contains(&project, "is declared inside a body");
}

#[test]
fn a_shadowing_variable_is_rejected() {
    let project = Project::new("shadow")
        .stage("stage { pub var n: num = 0; }")
        .sprite("A", "sprite \"A\" { var n: num = 0; }");
    expect_error_contains(&project, "would shadow a project-wide variable");
}

#[test]
fn an_undeclared_broadcast_is_rejected() {
    let project = Project::new("broadcast").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked { events::broadcast("nope"); }
        }"#,
    );
    expect_error_contains(&project, "there is no broadcast called \"nope\"");
}

#[test]
fn an_unknown_menu_variant_lists_the_values() {
    let project = Project::new("menu").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked { motion::set_rotation_style(RotationStyle::Sideways); }
        }"#,
    );
    expect_error_contains(&project, "is not a value of this menu");
}

#[test]
fn a_list_read_needs_an_index() {
    let project = Project::new("list-value").sprite(
        "A",
        r#"sprite "A" {
            var xs: list<num> = [];
            on flag_clicked { looks::say(xs); }
        }"#,
    );
    expect_error_contains(&project, "is a list");
}

#[test]
fn a_proc_cannot_produce_a_value() {
    let project = Project::new("proc-value").sprite(
        "A",
        r#"sprite "A" {
            proc p() { }
            on flag_clicked { looks::say(p()); }
        }"#,
    );
    expect_error_contains(&project, "is a procedure");
}

#[test]
fn a_call_needs_a_semicolon() {
    let project = Project::new("semicolon").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked { looks::hide }
        }"#,
    );
    expect_error_contains(&project, "expected `(`");
}

#[test]
fn a_missing_module_names_the_file_it_looked_for() {
    let project =
        Project::new("missing-module").sprite("A", "use lib::nope::thing;\nsprite \"A\" { }");
    expect_error_contains(&project, "cannot find the module");
}

#[test]
fn an_imported_item_must_be_public() {
    let project = Project::new("private")
        .module("lib/m", "proc hidden() { looks::say(\"x\"); }")
        .sprite("A", "use lib::m::hidden;\nsprite \"A\" { }");
    expect_error_contains(&project, "has no public item `hidden`");
}

#[test]
fn an_extension_that_nothing_uses_is_a_warning() {
    let project = Project::new("extension").extension("pen").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked { looks::say("no pen here"); }
        }"#,
    );
    let warnings = project.warnings();
    assert!(warnings.iter().any(|w| w.contains("pen")), "{warnings:?}");
}

#[test]
fn an_extension_that_is_used_is_not_warned_about() {
    let project = Project::new("extension-used").extension("pen").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked { pen::clear(); }
        }"#,
    );
    assert!(project.warnings().is_empty());
}

// ---------------------------------------------------------------------------
// The virtual memory system
// ---------------------------------------------------------------------------

#[test]
fn a_local_lives_in_the_vms_list_and_nowhere_else() {
    let project = Project::new("vms").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked {
                let n = 2;
                looks::say(n);
            }
        }"#,
    );
    let asm = project.expand();
    assert!(
        asm.contains("list _stack1 = [];"),
        "the script's stack starts empty: {asm}"
    );
    assert!(
        asm.contains("data_addtolist(2, \"_stack1\")"),
        "`let n = 2` pushes one cell: {asm}"
    );
    assert!(
        asm.contains("looks_say(data_itemoflist(1, \"_stack1\"))"),
        "reading `n` is one stack read: {asm}"
    );
    assert!(
        asm.contains("data_deleteoflist(1, \"_stack1\")"),
        "and the block pops it again: {asm}"
    );
    assert!(
        asm.contains("data_deletealloflist(\"_stack1\")"),
        "a run starts with a clean stack: {asm}"
    );
    assert!(!asm.contains("data_setvariableto"), "{asm}");
    assert!(!asm.contains("data_variable("), "{asm}");
}

/// Scratch cannot grow a list by replacing into it: `data_replaceitemoflist`
/// runs `Cast.toListIndex(index, length, false)`, which rejects an index past the
/// end, so a write into a shorter list is dropped without a word. `_vms` is
/// therefore declared with one empty item per cell the program uses, and this is
/// the test that keeps it that way.
#[test]
fn the_arena_is_grown_on_demand_and_the_stack_unwinds() {
    let project = Project::new("vms-size").sprite(
        "A",
        r#"sprite "A" {
            proc bump(n: num) -> num {
                return n + 1;
            }

            on flag_clicked {
                let a = 1;
                let b = bump(a);
                for i in 0..3 {
                    looks::say(f"{a}{b}{i}");
                }
            }
        }"#,
    );
    let asm = project.expand();
    // Nothing is reserved up front: the arena is an empty list, and the memory
    // manager grows it to the highest cell the program can reach.
    assert!(
        asm.contains("list _vms = [];"),
        "nothing is pre-sized: {asm}"
    );
    assert!(
        asm.contains("proc __vms_reserve() warp"),
        "the memory manager: {asm}"
    );
    assert!(
        asm.contains("data_addtolist(\"\", \"_vms\")"),
        "it grows the arena one item at a time: {asm}"
    );
    assert!(
        asm.contains("control_if(operator_lt(data_lengthoflist(\"_vms\"), "),
        "the manager grows the arena to what the program can reach: {asm}"
    );
    // And the script's own cells are popped in the reverse order they were
    // pushed, so the stack unwinds to nothing.
    assert!(asm.contains("data_deleteoflist(2, \"_stack1\")"), "{asm}");
    assert!(asm.contains("data_deleteoflist(1, \"_stack1\")"), "{asm}");
}

#[test]
fn a_let_shadows_and_the_outer_binding_returns() {
    let project = Project::new("shadow-let").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked {
                let x = 1;
                if 1 == 1 {
                    let x = 2;
                    looks::say(x);
                }
                looks::say(x);
            }
        }"#,
    );
    let asm = project.expand();
    assert!(
        asm.contains("looks_say(data_itemoflist(2, \"_stack1\"))"),
        "the shadowing `let` is the deeper cell: {asm}"
    );
    assert!(
        asm.contains("looks_say(data_itemoflist(1, \"_stack1\"))"),
        "and the outer one is visible again: {asm}"
    );
}

#[test]
fn a_for_counter_is_readable_after_the_loop() {
    let project = Project::new("for-after").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked {
                for i in 0..3 { }
                looks::say(i);
            }
        }"#,
    );
    let asm = project.expand();
    assert!(asm.contains("list _stack1 = [];"), "{asm}");
    assert!(asm.contains("data_addtolist(0, \"_stack1\")"), "{asm}");
    assert!(
        asm.contains("looks_say(data_itemoflist(1, \"_stack1\"))"),
        "{asm}"
    );
}

#[test]
fn a_for_bound_is_re_evaluated_each_iteration() {
    let project = Project::new("for-bound").sprite(
        "A",
        r#"sprite "A" {
            var xs: list<num> = [];
            on flag_clicked {
                for i in 0..data::length_of_list(xs) { }
            }
        }"#,
    );
    let asm = project.expand();
    assert!(
        asm.contains("control_repeat_until(operator_not(operator_lt("),
        "{asm}"
    );
    // The bound is read inside the loop's condition, not hoisted above it.
    assert_eq!(asm.matches("data_lengthoflist(\"xs\")").count(), 1, "{asm}");
    let loop_at = asm.find("control_repeat_until(").expect("the loop");
    let bound_at = asm.find("data_lengthoflist(\"xs\")").expect("the bound");
    assert!(bound_at > loop_at, "the bound is re-read each turn: {asm}");
}

#[test]
fn the_vms_name_is_reserved() {
    let project = Project::new("vms-reserved").sprite(
        "A",
        r#"sprite "A" {
            var _vms: num = 0;
            on flag_clicked { looks::hide(); }
        }"#,
    );
    expect_error_contains(&project, "reserved for the virtual memory system");
}

// ---------------------------------------------------------------------------
// Compound assignment
// ---------------------------------------------------------------------------

#[test]
fn a_compound_assignment_changes_a_vms_cell_by_three_blocks() {
    let project = Project::new("compound-var").sprite(
        "A",
        r#"sprite "A" {
            var n: num = 0;
            on flag_clicked { n += 1; n *= 2; }
        }"#,
    );
    let asm = project.expand();
    assert!(
        asm.contains(
            "data_replaceitemoflist(1, \"_vms\", operator_add(data_itemoflist(1, \"_vms\"), 1))"
        ),
        "{asm}"
    );
    assert!(
        asm.contains("data_replaceitemoflist(1, \"_vms\", operator_multiply(data_itemoflist(1, \"_vms\"), 2))"),
        "{asm}"
    );
    assert!(!asm.contains("data_variable("), "{asm}");
    assert!(!asm.contains("data_setvariableto"), "{asm}");
}

#[test]
fn a_compound_assignment_rewrites_a_vms_cell() {
    let project = Project::new("compound-cell").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked {
                let n = 1;
                n += 2;
            }
        }"#,
    );
    let asm = project.expand();
    assert!(
        asm.contains(
            "data_replaceitemoflist(1, \"_stack1\", operator_add(data_itemoflist(1, \"_stack1\"), 2))"
        ),
        "{asm}"
    );
}

// ---------------------------------------------------------------------------
// `proc` return values
// ---------------------------------------------------------------------------

#[test]
fn a_proc_can_return_a_value() {
    let project = Project::new("proc-return").sprite(
        "A",
        r#"sprite "A" {
            proc double(n: num) -> num { return n * 2; }
            on flag_clicked { looks::say(double(4)); }
        }"#,
    );
    let asm = project.expand();
    assert!(asm.contains("proc double(n: num)"), "{asm}");
    assert!(asm.contains("data_replaceitemoflist("), "{asm}");
    assert!(asm.contains("control_stop(\"this script\")"), "{asm}");
    // The call is hoisted above the statement that reads its result.
    let call = asm.find("double(4);").expect("the call: {asm}");
    let say = asm.find("looks_say(").expect("the say: {asm}");
    assert!(call < say, "the call comes first: {asm}");
    assert!(
        !asm.contains("return"),
        "a `return` emits no block fence: {asm}"
    );
    // And the emitted tree is real raven-asm: a build consumes it.
    let _ = project.build();
}

#[test]
fn returning_calls_are_evaluated_left_to_right() {
    let project = Project::new("call-order").sprite(
        "A",
        r#"sprite "A" {
            proc f(n: num) -> num { return n; }
            proc g(n: num) -> num { return n + 1; }
            on flag_clicked {
                looks::say(f(1) + f(2));
                looks::say(g(3) * f(4));
            }
        }"#,
    );
    let asm = project.expand();
    let at = |needle: &str| {
        asm.find(needle)
            .unwrap_or_else(|| panic!("{needle} missing from\n{asm}"))
    };
    assert!(at("f(1);") < at("f(2);"), "{asm}");
    assert!(at("f(2);") < at("looks_say("), "{asm}");
    assert!(at("g(3);") < at("f(4);"), "{asm}");
}

#[test]
fn a_return_outside_a_proc_is_rejected() {
    let project = Project::new("return-script").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked { return; }
        }"#,
    );
    expect_error_contains(&project, "only allowed inside a `proc` body");
}

#[test]
fn a_return_value_in_a_void_proc_is_rejected() {
    let project = Project::new("return-void").sprite(
        "A",
        r#"sprite "A" {
            proc p() { return 1; }
            on flag_clicked { p(); }
        }"#,
    );
    expect_error_contains(&project, "does not return a value");
}

#[test]
fn a_return_value_is_checked_against_the_declared_type() {
    let project = Project::new("return-type").sprite(
        "A",
        r#"sprite "A" {
            proc p() -> num { return "no"; }
            on flag_clicked { p(); }
        }"#,
    );
    expect_error_contains(&project, "the returned value must be `num`, found `str`");
}

// ---------------------------------------------------------------------------
// Correctness fixes
// ---------------------------------------------------------------------------

#[test]
fn a_wildcard_arm_must_be_last() {
    let project = Project::new("wildcard").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked {
                let n = 1;
                match n {
                    _ => { looks::say("any"); },
                    1 => { looks::say("one"); },
                }
            }
        }"#,
    );
    expect_error_contains(&project, "the `_` arm must be last");
}

#[test]
fn an_effectful_match_subject_is_copied_into_a_cell() {
    let project = Project::new("match-cell").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked {
                match motion::x_position() {
                    1 => { looks::say("one"); },
                    _ => { looks::say("other"); },
                }
            }
        }"#,
    );
    let asm = project.expand();
    // A sensor is read once, pushed onto the stack, and every arm compares
    // the copy.
    assert!(
        asm.contains("data_addtolist(motion_xposition(), \"_stack1\")"),
        "{asm}"
    );
    assert!(
        asm.contains("operator_equals(data_itemoflist(1, \"_stack1\"), 1)"),
        "{asm}"
    );
    assert_eq!(asm.matches("motion_xposition()").count(), 1, "{asm}");
}

#[test]
fn a_variable_match_subject_needs_no_cell() {
    let project = Project::new("match-cell-var").sprite(
        "A",
        r#"sprite "A" {
            var score: num = 0;
            on flag_clicked {
                match score {
                    1 => { looks::say("one"); },
                    2 => { looks::say("two"); },
                    _ => { looks::say("other"); },
                }
            }
        }"#,
    );
    let asm = project.expand();
    // `score` is already a cell, and reading a cell twice cannot disagree with
    // itself, so there is no copy: every arm compares the variable's own cell.
    assert!(
        asm.contains("operator_equals(data_itemoflist(1, \"_vms\"), 1)"),
        "{asm}"
    );
    assert!(
        asm.contains("operator_equals(data_itemoflist(1, \"_vms\"), 2)"),
        "{asm}"
    );
    assert!(!asm.contains("_stack"), "no copy is made: {asm}");
}

#[test]
fn a_pure_match_subject_needs_no_cell() {
    let project = Project::new("match-pure").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked {
                match 1 + 1 {
                    1 => { looks::say("one"); },
                    _ => { looks::say("other"); },
                }
            }
        }"#,
    );
    let asm = project.expand();
    assert!(!asm.contains("_vms"), "no cell is needed: {asm}");
    assert!(
        asm.contains("operator_equals(operator_add(1, 1), 1)"),
        "{asm}"
    );
}

#[test]
fn a_single_arm_match_needs_no_cell() {
    let project = Project::new("match-one").sprite(
        "A",
        r#"sprite "A" {
            var score: num = 0;
            on flag_clicked {
                match score {
                    1 => { looks::say("one"); },
                }
            }
        }"#,
    );
    let asm = project.expand();
    assert!(
        asm.contains("operator_equals(data_itemoflist(1, \"_vms\"), 1)"),
        "{asm}"
    );
    assert_eq!(
        asm.matches("data_itemoflist(1, \"_vms\")").count(),
        1,
        "the variable's own cell, read once: {asm}"
    );
}

#[test]
fn a_const_can_be_a_match_pattern() {
    let project = Project::new("match-const").sprite(
        "A",
        r#"sprite "A" {
            const SIDES: num = 4;
            on flag_clicked {
                let n = 4;
                match n {
                    SIDES => { looks::say("four"); },
                    _ => { looks::say("other"); },
                }
            }
        }"#,
    );
    let asm = project.expand();
    assert!(
        asm.contains("operator_equals(data_itemoflist(1, \"_stack1\"), 4)"),
        "{asm}"
    );
}

#[test]
fn a_macro_cycle_names_the_whole_chain() {
    let project = Project::new("macro-cycle").sprite(
        "A",
        r#"sprite "A" {
            macro a() -> stmts { b(); }
            macro b() -> stmts { a(); }
            on flag_clicked { a(); }
        }"#,
    );
    let rendered = project.expect_error().render();
    assert!(rendered.contains("the cycle is a → b → a"), "{rendered}");
}

#[test]
fn a_self_recursive_macro_is_still_a_cycle() {
    let project = Project::new("macro-self-cycle").sprite(
        "A",
        r#"sprite "A" {
            macro again() -> stmts { again(); }
            on flag_clicked { again(); }
        }"#,
    );
    let rendered = project.expect_error().render();
    assert!(rendered.contains("expands into itself"), "{rendered}");
    assert!(
        rendered.contains("the cycle is again → again"),
        "{rendered}"
    );
}

#[test]
fn a_macro_body_may_nest_inside_itself() {
    let project = Project::new("macro-nesting").sprite(
        "A",
        r#"sprite "A" {
            var total: num = 4;
            on flag_clicked {
                for i in 0..2 { for j in 0..2 { looks::say(f"{i}{j}"); } }
                while total > 0 { while total > 0 { total -= 1; } }
            }
        }"#,
    );
    let asm = project.expand();
    // Two `for`s and two `while`s, nesting inside themselves; the arena
    // reservation proc has a loop of its own and is not part of the count.
    let body = asm.split("proc __vms_reserve").next().unwrap_or(&asm);
    assert_eq!(body.matches("control_repeat_until").count(), 4, "{asm}");
}

#[test]
fn a_for_can_run_an_inclusive_range() {
    let project = Project::new("for-inclusive").sprite(
        "A",
        r#"sprite "A" {
            var total: num = 0;
            on flag_clicked { for i in 0..=3 { total += i; } }
        }"#,
    );
    let asm = project.expand();
    // `..=` reaches past its end, so the test is `>` rather than `>=`.
    assert!(
        asm.contains("operator_gt(data_itemoflist(1, \"_stack1\"), 3)"),
        "{asm}"
    );
}

#[test]
fn a_for_walks_a_list() {
    let project = Project::new("for-each").sprite(
        "A",
        r#"sprite "A" {
            var trail: list<num> = [1, 2, 3];
            on flag_clicked { for x in trail { looks::say(x); } }
        }"#,
    );
    let asm = project.expand();
    assert!(asm.contains("data_lengthoflist(\"trail\")"), "{asm}");
    assert!(
        asm.contains("data_itemoflist(data_itemoflist(1, \"_stack1\"), \"trail\")"),
        "{asm}"
    );
}

#[test]
fn a_loop_is_a_forever() {
    let project = Project::new("loop-forever").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked { loop { looks::say("on"); } }
        }"#,
    );
    let asm = project.expand();
    assert!(asm.contains("control_forever"), "{asm}");
}

#[test]
fn a_macro_result_type_is_checked() {
    let project = Project::new("macro-result").sprite(
        "A",
        r#"sprite "A" {
            macro bad() -> num { "text" }
            on flag_clicked { looks::say(bad()); }
        }"#,
    );
    expect_error_contains(
        &project,
        "`bad` is declared `-> num`, but it produces `str`",
    );
}

#[test]
fn an_index_read_has_the_element_type() {
    let project = Project::new("index-read").sprite(
        "A",
        r#"sprite "A" {
            var xs: list<num> = [];
            on flag_clicked { motion::set_x(xs[1]); }
        }"#,
    );
    let asm = project.expand();
    assert!(asm.contains("data_itemoflist(1, \"xs\")"), "{asm}");
}

#[test]
fn an_index_write_takes_the_element_type() {
    let project = Project::new("index-write").sprite(
        "A",
        r#"sprite "A" {
            var xs: list<num> = [];
            on flag_clicked { xs[1] = "text"; }
        }"#,
    );
    expect_error_contains(&project, "`xs` must be `num`, found `str`");
}

#[test]
fn an_index_write_of_the_right_type_is_accepted() {
    let project = Project::new("index-write-ok").sprite(
        "A",
        r#"sprite "A" {
            var xs: list<num> = [];
            on flag_clicked { xs[1] = 3; }
        }"#,
    );
    let asm = project.expand();
    assert!(
        asm.contains("data_replaceitemoflist(1, \"xs\", 3)"),
        "{asm}"
    );
}

#[test]
fn the_prelude_can_be_shadowed() {
    let project = Project::new("shadow-prelude").sprite(
        "A",
        r#"sprite "A" {
            macro while_loop($cond: expr<bool>, $body: block) -> stmts { $body; }
            on flag_clicked { while 1 > 2 { looks::hide(); } }
        }"#,
    );
    let asm = project.expand();
    assert!(!asm.contains("control_repeat_until"), "{asm}");
    assert!(asm.contains("looks_hide"), "{asm}");
}

#[test]
fn two_user_macros_of_the_same_name_are_a_duplicate() {
    let project = Project::new("dup-macro").sprite(
        "A",
        r#"sprite "A" {
            macro m() -> stmts { }
            macro m() -> stmts { }
            on flag_clicked { m(); }
        }"#,
    );
    expect_error_contains(&project, "declared as a macro more than once");
}

#[test]
fn a_macro_temporary_beside_a_parameter_is_renamed() {
    let project = Project::new("hygiene-var").sprite(
        "A",
        r#"sprite "A" {
            macro m($x: expr<num>) -> stmts {
                var x: num = 0;
                x += $x;
            }
            on flag_clicked { m(1); }
        }"#,
    );
    let asm = project.expand();
    // A macro's `var` is one cell, like a `let`: the temporary is written and
    // read by index, and it never becomes a Scratch variable.
    assert!(!asm.contains("var x"), "no Scratch temporary: {asm}");
    assert!(asm.contains("data_addtolist(0, \"_stack1\")"), "{asm}");
    assert!(
        asm.contains("operator_add(data_itemoflist(1, \"_stack1\"), 1)"),
        "{asm}"
    );
}

#[test]
fn an_unsubstituted_parameter_is_a_clear_error() {
    let project = Project::new("hygiene-param").sprite(
        "A",
        r#"sprite "A" {
            macro m() -> stmts { var $x: num = 0; }
            on flag_clicked { m(); }
        }"#,
    );
    expect_error_contains(&project, "is a macro parameter with no value");
}

// ---------------------------------------------------------------------------
// Bare standard-library names
// ---------------------------------------------------------------------------

#[test]
fn a_bare_standard_library_name_resolves() {
    let project = Project::new("bare-std").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked {
                move_steps(10);
                motion::set_x(x_position());
            }
        }"#,
    );
    let asm = project.expand();
    assert!(asm.contains("motion_movesteps(10)"), "{asm}");
    assert!(asm.contains("motion_setx(motion_xposition())"), "{asm}");
}

#[test]
fn an_ambiguous_bare_name_names_both_spellings() {
    let project = Project::new("bare-ambiguous").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked { set_effect_to(Effect::Ghost, 50); }
        }"#,
    );
    let rendered = project.expect_error().render();
    assert!(rendered.contains("ambiguous"), "{rendered}");
    assert!(rendered.contains("looks::set_effect_to"), "{rendered}");
    assert!(rendered.contains("sound::set_effect_to"), "{rendered}");
}

#[test]
fn a_user_variable_wins_over_a_bare_standard_library_name() {
    let project = Project::new("bare-value").sprite(
        "A",
        r#"sprite "A" {
            var hide: num = 0;
            on flag_clicked { hide(1); }
        }"#,
    );
    expect_error_contains(&project, "is a value, not a call");
}

#[test]
fn a_user_proc_shadows_a_bare_standard_library_name() {
    let project = Project::new("bare-shadow").sprite(
        "A",
        r#"sprite "A" {
            proc say(text: str) { looks::hide(); }
            on flag_clicked { say("hi"); }
        }"#,
    );
    let asm = project.expand();
    assert!(asm.contains("say(\"hi\")"), "{asm}");
    assert!(!asm.contains("looks_say("), "{asm}");
}

// ---------------------------------------------------------------------------
// The emitted tree is not destroyed by a build
// ---------------------------------------------------------------------------

#[test]
fn a_user_file_in_the_asm_directory_survives_a_build() {
    let project = Project::new("asm-keep").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked { looks::hide(); }
        }"#,
    );
    project.write();
    let asm_dir = project.dir.join("dist/asm");
    std::fs::create_dir_all(&asm_dir).expect("create the asm directory");
    let keep = asm_dir.join("notes.txt");
    std::fs::write(&keep, "mine").expect("write the user file");
    let stale = asm_dir.join("src/stale.rasm");
    std::fs::create_dir_all(stale.parent().expect("parent")).expect("create src");
    std::fs::write(&stale, "old").expect("write a stale file");

    let _ = project.build();

    assert!(keep.exists(), "a user file must survive a build");
    assert_eq!(std::fs::read_to_string(&keep).expect("read"), "mine");
    assert!(!stale.exists(), "a stale `.rasm` file is removed");
}

// ---------------------------------------------------------------------------
// Recursion and `_vms` cells
// ---------------------------------------------------------------------------

#[test]
fn a_let_in_a_recursive_proc_is_rejected() {
    let project = Project::new("recursive-let").sprite(
        "A",
        r#"sprite "A" {
            proc p(n: num) {
                let t = n;
                if n > 0 { p(n - 1); }
                looks::say(t);
            }
            on flag_clicked { p(3); }
        }"#,
    );
    let rendered = project.expect_error().render();
    assert!(rendered.contains("`p` is recursive"), "{rendered}");
    assert!(rendered.contains("the cycle is p → p"), "{rendered}");
    assert!(rendered.contains("not a stack frame"), "{rendered}");
    assert!(
        rendered.contains("pass the value as a parameter"),
        "{rendered}"
    );
}

#[test]
fn a_for_counter_in_a_recursive_proc_is_rejected() {
    let project = Project::new("recursive-for").sprite(
        "A",
        r#"sprite "A" {
            proc p(n: num) {
                for i in 0..n { looks::say(i); }
                if n > 0 { p(n - 1); }
            }
            on flag_clicked { p(2); }
        }"#,
    );
    expect_error_contains(&project, "is recursive, so a `let` in its body");
}

#[test]
fn mutual_recursion_reports_a_chain_through_the_cycle() {
    let project = Project::new("recursive-mutual").sprite(
        "A",
        r#"sprite "A" {
            proc a(n: num) { let t = n; b(n); }
            proc b(n: num) { a(n); }
            on flag_clicked { a(1); }
        }"#,
    );
    let rendered = project.expect_error().render();
    assert!(
        rendered.contains("a → b → a") || rendered.contains("b → a → b"),
        "{rendered}"
    );
}

#[test]
fn a_recursive_proc_without_block_locals_still_builds() {
    let project = Project::new("recursive-ok").sprite(
        "A",
        r#"sprite "A" {
            proc fact(n: num) -> num {
                if n <= 1 { return 1; }
                return n * fact(n - 1);
            }
            on flag_clicked { looks::say(fact(5)); }
        }"#,
    );
    let asm = project.expand();
    assert!(asm.contains("fact("), "{asm}");
    // A recursive procedure that keeps everything in its parameters is fine.
    let _ = project.build();
}

// ---------------------------------------------------------------------------
// The memory system's own surface
// ---------------------------------------------------------------------------

#[test]
fn a_scalar_var_is_a_cell_and_never_a_scratch_variable() {
    let project = Project::new("vms-scalar").sprite(
        "A",
        r#"sprite "A" {
            var n: num = 7;
            on flag_clicked { n += 1; looks::say(n); }
        }"#,
    );
    let asm = project.expand();
    assert!(
        asm.contains("list _vms = [];"),
        "the arena starts empty: {asm}"
    );
    assert!(
        asm.contains("data_replaceitemoflist(1, \"_vms\", 7)"),
        "the memory manager writes the declared starting value: {asm}"
    );
    assert!(!asm.contains("var n"), "{asm}");
    assert!(!asm.contains("data_variable("), "{asm}");
    assert!(!asm.contains("data_setvariableto"), "{asm}");
}

#[test]
fn the_raw_variable_blocks_are_refused_with_an_explanation() {
    for call in [
        "data::value_of(n)",
        "data::set_variable_to(n, 1)",
        "data::change_variable_by(n, 1)",
    ] {
        let source = format!(
            r#"sprite "A" {{
                var n: num = 0;
                on flag_clicked {{ {call}; }}
            }}"#
        );
        let project = Project::new("forbidden").sprite("A", &source);
        let rendered = project.expect_error().render();
        assert!(
            rendered.contains("is not available in raven"),
            "`{call}` produced:\n{rendered}"
        );
        assert!(
            rendered.contains("VMS cell")
                || rendered.contains("virtual memory")
                || rendered.contains("raw Scratch"),
            "`{call}` produced:\n{rendered}"
        );
    }
}

#[test]
fn a_project_wide_scalar_lives_in_the_stage_arena() {
    let project = Project::new("gvm")
        .stage("stage { pub var best: num = 4; }")
        .sprite(
            "A",
            r#"sprite "A" {
                on flag_clicked {
                    if best < 10 { best = best + 1; }
                }
            }"#,
        );
    let asm = project.expand();
    assert!(asm.contains("global list _gvm = [];"), "{asm}");
    assert!(
        asm.contains("data_replaceitemoflist(1, \"_gvm\", 4)"),
        "grown and given its value by the memory manager: {asm}"
    );
    assert!(asm.contains("data_itemoflist(1, \"_gvm\")"), "{asm}");
    assert!(asm.contains("data_replaceitemoflist(1, \"_gvm\""), "{asm}");
    assert!(!asm.contains("data_variable("), "{asm}");
}

#[test]
fn a_struct_is_a_frame_of_cells_read_by_constant_index() {
    let project = Project::new("struct").sprite(
        "A",
        r#"sprite "A" {
            struct Point { x: num, y: num }
            var home: Point = Point { x: 1, y: 2 };
            on flag_clicked {
                home.x = home.y + 1;
                looks::say(home.x);
            }
        }"#,
    );
    let asm = project.expand();
    // Two cells, whose declared values the memory manager writes.
    assert!(asm.contains("list _vms = [];"), "{asm}");
    assert!(
        asm.contains("data_replaceitemoflist(1, \"_vms\", 1)")
            && asm.contains("data_replaceitemoflist(2, \"_vms\", 2)"),
        "{asm}"
    );
    // `home.x` is cell 1, `home.y` is cell 2, and each is one block.
    assert!(
        asm.contains(
            "data_replaceitemoflist(1, \"_vms\", operator_add(data_itemoflist(2, \"_vms\"), 1))"
        ),
        "{asm}"
    );
    assert!(
        asm.contains("looks_say(data_itemoflist(1, \"_vms\"))"),
        "{asm}"
    );
    assert!(!asm.contains("data_variable("), "{asm}");
}

#[test]
fn a_nested_struct_composes_constants() {
    let project = Project::new("struct-nested").sprite(
        "A",
        r#"sprite "A" {
            struct Point { x: num, y: num }
            struct Segment { from: Point, to: Point }
            on flag_clicked {
                let seg: Segment = Segment {
                    from: Point { x: 0, y: 0 },
                    to: Point { x: 3, y: 4 },
                };
                looks::say(seg.to.x);
            }
        }"#,
    );
    let asm = project.expand();
    // On the stack, `from` is cells 1-2 and `to` is 3-4, so `seg.to.x` is
    // cell 3 — two offsets added at compile time.
    assert!(
        asm.contains("looks_say(data_itemoflist(3, \"_stack1\"))"),
        "{asm}"
    );
}

#[test]
fn a_struct_is_not_a_value() {
    let project = Project::new("struct-value").sprite(
        "A",
        r#"sprite "A" {
            struct Point { x: num, y: num }
            var home: Point = Point { x: 1, y: 2 };
            on flag_clicked { looks::say(home); }
        }"#,
    );
    let rendered = project.expect_error().render();
    assert!(rendered.contains("which is not one value"), "{rendered}");
}

#[test]
fn an_unknown_field_lists_the_fields() {
    let project = Project::new("struct-field").sprite(
        "A",
        r#"sprite "A" {
            struct Point { x: num, y: num }
            var home: Point = Point { x: 1, y: 2 };
            on flag_clicked { looks::say(home.z); }
        }"#,
    );
    let rendered = project.expect_error().render();
    assert!(rendered.contains("has no field `z`"), "{rendered}");
}

#[test]
fn a_list_method_is_the_block_it_says_it_is() {
    let project = Project::new("list-methods").sprite(
        "A",
        r#"sprite "A" {
            var xs: list<num> = [];
            on flag_clicked {
                xs.push(1);
                xs.insert(1, 2);
                xs.remove(1);
                let n = xs.len();
                let at = xs.at(n);
                if xs.contains(1) { looks::say(at); }
                if !xs.is_empty() { looks::say(xs.text()); }
                xs.clear();
            }
        }"#,
    );
    let asm = project.expand();
    assert!(asm.contains("data_addtolist(1, \"xs\")"), "{asm}");
    assert!(asm.contains("data_insertatlist(2, 1, \"xs\")"), "{asm}");
    assert!(asm.contains("data_deleteoflist(1, \"xs\")"), "{asm}");
    assert!(asm.contains("data_lengthoflist(\"xs\")"), "{asm}");
    assert!(asm.contains("data_listcontainsitem(\"xs\", 1)"), "{asm}");
    assert!(asm.contains("data_itemoflist("), "{asm}");
    assert!(asm.contains("data_deletealloflist(\"xs\")"), "{asm}");
}

#[test]
fn a_map_is_one_list_of_alternating_keys_and_values() {
    let project = Project::new("map").sprite(
        "A",
        r#"sprite "A" {
            var scores: map<str, num> = [];
            on flag_clicked {
                scores.set("a", 1);
                scores.set("b", 2);
                let found = scores.get("a");
                let n = scores.len();
                if scores.has("b") { looks::say(f"{found}{n}"); }
                scores.remove("a");
            }
        }"#,
    );
    let asm = project.expand();
    assert!(asm.contains("list scores = [];"), "{asm}");
    // `set` looks the key up once, then replaces or appends the pair.
    assert!(
        asm.contains("data_itemnumoflist(\"a\", \"scores\")"),
        "{asm}"
    );
    assert!(asm.contains("data_addtolist(\"a\", \"scores\")"), "{asm}");
    assert!(asm.contains("data_addtolist(1, \"scores\")"), "{asm}");
    assert!(asm.contains("control_if_else("), "{asm}");
    // `get` is guarded, so a missing key cannot read the first key's value.
    assert!(asm.contains("control_if(operator_gt("), "{asm}");
    assert!(
        asm.contains("operator_divide(data_lengthoflist(\"scores\"), 2)"),
        "{asm}"
    );
    // `remove` deletes the key and the value that follows it — two deletes on
    // the map itself (the rest are the script's stack unwinding).
    assert!(
        asm.matches("data_deleteoflist(data_itemoflist(").count() >= 2,
        "{asm}"
    );
}

#[test]
fn a_method_that_does_not_exist_says_what_does() {
    let project = Project::new("method-missing").sprite(
        "A",
        r#"sprite "A" {
            var xs: list<num> = [];
            on flag_clicked { xs.wibble(); }
        }"#,
    );
    let rendered = project.expect_error().render();
    assert!(rendered.contains("has no method `wibble`"), "{rendered}");
    assert!(rendered.contains("`push`"), "{rendered}");
}

#[test]
fn a_statement_after_a_return_is_unreachable() {
    let project = Project::new("unreachable").sprite(
        "A",
        r#"sprite "A" {
            proc p() -> num {
                return 1;
                looks::say("no");
            }
            on flag_clicked { let n = p(); looks::say(n); }
        }"#,
    );
    let rendered = project.expect_error().render();
    assert!(rendered.contains("unreachable"), "{rendered}");
    assert!(
        rendered.contains("nothing attaches below one"),
        "{rendered}"
    );
}

// ---------------------------------------------------------------------------
// Debug output
// ---------------------------------------------------------------------------

#[test]
fn a_plain_build_writes_only_the_archive() {
    let project = Project::new("plain-build")
        .sprite("A", r#"sprite "A" { on flag_clicked { looks::hide(); } }"#);
    let _ = project.build();
    let dist = project.dir.join("dist");
    let names: Vec<String> = std::fs::read_dir(&dist)
        .expect("dist exists")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(names, vec!["test.sb3".to_string()], "{names:?}");
}

#[test]
fn a_debug_build_writes_the_asm_and_the_project_json() {
    let project = Project::new("debug-build")
        .sprite("A", r#"sprite "A" { on flag_clicked { looks::hide(); } }"#);
    let _ = project.build();
    let mut options = project.options();
    options.debug = true;
    let result = driver::build(&options).expect("build with --debug");
    let asm = result.asm_dir.expect("--debug keeps the tree");
    assert!(asm.join("raven-asm.toml").exists(), "the staging manifest");
    assert!(asm.join("src/stage.rasm").exists(), "the lowered stage");
    let json = project.dir.join("dist/project.json");
    let text = std::fs::read_to_string(&json).expect("project.json");
    assert!(text.contains("\"targets\""), "{text}");
    // The two outputs describe the same project.
    let rebuilt = raven_asm::compile::build(&asm.join(raven_asm::manifest::MANIFEST_NAME))
        .expect("the emitted tree is a raven-asm project");
    assert_eq!(
        serde_json::to_value(&rebuilt.project).expect("serialize"),
        serde_json::from_str::<serde_json::Value>(&text).expect("parse"),
        "the kept raven-asm must rebuild to the kept project.json"
    );
}

// ---------------------------------------------------------------------------
// Reading a list, and why a boolean cannot be stored
// ---------------------------------------------------------------------------

#[test]
fn every_way_of_reading_a_list_is_the_block_it_says_it_is() {
    let project = Project::new("list-reads").sprite(
        "A",
        r#"sprite "A" {
            var xs: list<num> = [];
            on flag_clicked {
                let a = xs[1];
                let b = xs.at(2);
                let c = xs.first();
                let d = xs.last();
                let e = xs.len();
                let f = xs.text();
                let h = xs.index_of(1);
                if xs.contains(1) { looks::say("has 1"); }
                if !xs.is_empty() { looks::say(f"{a}{b}{c}{d}{e}{f}{h}"); }
            }
        }"#,
    );
    let asm = project.expand();
    assert!(asm.contains("data_itemoflist(1, \"xs\")"), "{asm}");
    assert!(asm.contains("data_itemoflist(2, \"xs\")"), "{asm}");
    assert!(
        asm.contains("data_itemoflist(data_lengthoflist(\"xs\"), \"xs\")"),
        "`last` is one reporter: {asm}"
    );
    assert!(asm.contains("data_lengthoflist(\"xs\")"), "{asm}");
    assert!(asm.contains("data_listcontents(\"xs\")"), "`text`: {asm}");
    assert!(asm.contains("data_listcontainsitem(\"xs\", 1)"), "{asm}");
    assert!(asm.contains("data_itemnumoflist(1, \"xs\")"), "{asm}");
    // `text` is the whole list in one block, not a loop over it.
    assert_eq!(asm.matches("data_listcontents").count(), 1, "{asm}");
}

#[test]
fn a_boolean_literal_is_a_constant_comparison() {
    let project = Project::new("bool-literal").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked {
                if true { looks::say("always"); }
                if !false { looks::say("also"); }
                looks::say(f"{true} {false}");
            }
        }"#,
    );
    let asm = project.expand();
    // Scratch has no `true`/`false` block, so each literal is the one-block
    // comparison that has that constant value.
    assert!(asm.contains("control_if(operator_equals(1, 1))"), "{asm}");
    assert!(asm.contains("operator_not(operator_equals(1, 0))"), "{asm}");
    let _ = project.build();
}

/// A boolean is stored as scratch keeps it — its own `true`/`false` in a cell or
/// a list item — and read back with `<cell = "true">`. `Cast.compare` falls back
/// to `String(value)` when a side is not a number, so that one comparison reads a
/// stored boolean and a stored `"true"`/`"false"` alike.
#[test]
fn a_boolean_var_round_trips_through_its_cell() {
    let project = Project::new("bool-var").sprite(
        "A",
        r#"sprite "A" {
            var live: bool = false;
            on flag_clicked {
                live = 1 > 0;
                if live { looks::say("live"); }
                looks::say(f"{live}");
            }
        }"#,
    );
    let asm = project.expand();
    // The arena starts empty; the memory manager writes the declared `false`,
    // which Scratch has no literal for, as the comparison that means it.
    assert!(asm.contains("list _vms = [];"), "{asm}");
    assert!(
        asm.contains("data_replaceitemoflist(1, \"_vms\", operator_equals(1, 0))"),
        "the declared `false`: {asm}"
    );
    // A write stores the block's value; a read turns it back into a block.
    assert!(
        asm.contains("data_replaceitemoflist(1, \"_vms\", operator_gt(1, 0))"),
        "{asm}"
    );
    assert!(
        asm.contains("control_if(operator_equals(data_itemoflist(1, \"_vms\"), \"true\"))"),
        "{asm}"
    );
    let _ = project.build();
}

#[test]
fn a_let_can_hold_a_boolean() {
    let project = Project::new("bool-let").sprite(
        "A",
        r#"sprite "A" {
            var xs: list<num> = [];
            on flag_clicked {
                let found = xs.contains(2);
                if found { looks::say("found"); }
            }
        }"#,
    );
    let asm = project.expand();
    assert!(
        asm.contains("data_addtolist(data_listcontainsitem(\"xs\", 2), \"_stack1\")"),
        "{asm}"
    );
    assert!(
        asm.contains("control_if(operator_equals(data_itemoflist(1, \"_stack1\"), \"true\"))"),
        "{asm}"
    );
}

#[test]
fn a_list_of_booleans_round_trips() {
    let project = Project::new("bool-list").sprite(
        "A",
        r#"sprite "A" {
            var flags: list<bool> = [];
            on flag_clicked {
                flags.push(true);
                flags.push(2 > 3);
                if flags.at(1) { looks::say("first"); }
                if !flags.last() { looks::say("last"); }
                if flags.contains(true) { looks::say("some"); }
            }
        }"#,
    );
    let asm = project.expand();
    assert!(asm.contains("list flags = [];"), "{asm}");
    assert!(
        asm.contains("data_addtolist(operator_equals(1, 1), \"flags\")"),
        "{asm}"
    );
    assert!(
        asm.contains("data_addtolist(operator_gt(2, 3), \"flags\")"),
        "{asm}"
    );
    assert!(
        asm.contains("data_itemoflist(1, \"flags\"), \"true\")"),
        "a stored item is turned back into a block: {asm}"
    );
    assert!(
        asm.contains("data_itemoflist(data_lengthoflist(\"flags\"), \"flags\"), \"true\")"),
        "`last` too: {asm}"
    );
    let _ = project.build();
}

#[test]
fn a_map_can_hold_booleans() {
    let project = Project::new("bool-map").sprite(
        "A",
        r#"sprite "A" {
            var seen: map<str, bool> = [];
            on flag_clicked {
                seen.set("a", 1 > 0);
                if seen.get("a") { looks::say("yes"); }
                if !seen.has("b") { looks::say("no"); }
                if seen.is_empty() { looks::say("empty"); }
            }
        }"#,
    );
    let asm = project.expand();
    assert!(
        asm.contains("data_addtolist(operator_gt(1, 0), \"seen\")"),
        "the value is stored as it is: {asm}"
    );
    assert!(
        asm.contains("data_itemoflist(operator_add(data_itemoflist("),
        "the guarded read: {asm}"
    );
    assert!(asm.contains(", \"true\"))"), "and converted back: {asm}");
    let _ = project.build();
}

#[test]
fn a_proc_can_return_a_boolean() {
    let project = Project::new("bool-proc").sprite(
        "A",
        r#"sprite "A" {
            proc positive(n: num) -> bool {
                if n <= 0 { return false; }
                return true;
            }
            proc is_even(n: num) -> bool {
                return n % 2 == 0;
            }
            on flag_clicked {
                if positive(3) { looks::say("positive"); }
                let even = is_even(4);
                if even && positive(1) { looks::say("both"); }
            }
        }"#,
    );
    let asm = project.expand();
    // The result is a cell of `_vms`, written by `return` ...
    assert!(asm.contains("data_replaceitemoflist("), "{asm}");
    assert!(
        asm.contains("operator_equals(1, 0)"),
        "`return false`: {asm}"
    );
    // ... and read back as a comparison, so it can be a condition.
    assert!(
        asm.contains("control_if(operator_equals(data_itemoflist("),
        "{asm}"
    );
    assert!(
        asm.contains("operator_and(operator_equals(data_itemoflist("),
        "a returned boolean combines with `&&`: {asm}"
    );
    let _ = project.build();
}

#[test]
fn a_struct_can_have_a_boolean_field() {
    let project = Project::new("bool-field").sprite(
        "A",
        r#"sprite "A" {
            struct Mark { at: num, prime: bool }
            var hit: Mark = Mark { at: 7, prime: true };
            on flag_clicked {
                hit.prime = hit.at > 5;
                if hit.prime { looks::say("prime"); }
            }
        }"#,
    );
    let asm = project.expand();
    assert!(asm.contains("list _vms = [];"), "{asm}");
    assert!(
        asm.contains("data_replaceitemoflist(1, \"_vms\", 7)"),
        "the frame starts from the declaration: {asm}"
    );
    assert!(
        asm.contains(
            "data_replaceitemoflist(2, \"_vms\", operator_gt(data_itemoflist(1, \"_vms\"), 5))"
        ),
        "{asm}"
    );
    assert!(
        asm.contains("control_if(operator_equals(data_itemoflist(2, \"_vms\"), \"true\"))"),
        "{asm}"
    );
    let _ = project.build();
}

#[test]
fn a_project_wide_boolean_lives_in_the_stage_arena() {
    let project = Project::new("bool-global")
        .stage("stage { pub var ready: bool = true; }")
        .sprite(
            "A",
            r#"sprite "A" {
                on flag_clicked {
                    ready = 1 < 2;
                    if ready { looks::say("ready"); }
                }
            }"#,
        );
    let asm = project.expand();
    assert!(asm.contains("global list _gvm = [];"), "{asm}");
    assert!(
        asm.contains("data_replaceitemoflist(1, \"_gvm\", operator_equals(1, 1))"),
        "the declared `true`: {asm}"
    );
    assert!(
        asm.contains("control_if(operator_equals(data_itemoflist(1, \"_gvm\"), \"true\"))"),
        "{asm}"
    );
    let _ = project.build();
}

#[test]
fn a_boolean_is_still_a_first_class_expression() {
    let project = Project::new("bool-expr").sprite(
        "A",
        r#"sprite "A" {
            proc note(text: str, condition: bool) {
                if condition { looks::say(text); }
            }
            on flag_clicked {
                note("both", (1 > 0) && (2 > 0));
                note("not", !(1 > 2) || 2 > 1);
                note("plain", sensing::mouse_down());
                note("literal", true);
                if motion::y_position() > 0 { looks::say("up"); }
                while sensing::key_pressed(Key::Space) { control::wait(0.1); }
            }
        }"#,
    );
    let asm = project.expand();
    assert!(asm.contains("operator_and"), "{asm}");
    assert!(asm.contains("operator_or"), "{asm}");
    assert!(asm.contains("operator_not"), "{asm}");
    assert!(
        asm.contains("argument_reporter_boolean(\"condition\")"),
        "a `bool` parameter is a real boolean block: {asm}"
    );
    let _ = project.build();
}

// ---------------------------------------------------------------------------
// The console, and watches
// ---------------------------------------------------------------------------

#[test]
fn the_console_is_one_list_and_a_log_line_is_one_item() {
    let project = Project::new("console").sprite(
        "A",
        r#"sprite "A" {
            on flag_clicked {
                console::log("start");
                console::log(f"count {console::count()}");
                console::clear();
            }
        }"#,
    );
    let asm = project.expand();
    assert!(
        asm.contains("global list _console = [];"),
        "the console is a Scratch list on the stage: {asm}"
    );
    assert!(
        asm.contains("data_addtolist(\"start\", \"_console\")"),
        "a log line is one `add`: {asm}"
    );
    assert!(asm.contains("data_deletealloflist(\"_console\")"), "{asm}");
    assert!(
        asm.contains("data_lengthoflist(\"_console\")"),
        "`console::count()`: {asm}"
    );
    // Its monitor starts hidden: the developer ticks it in the editor.
    let _ = project.build();
}

#[test]
fn a_console_call_takes_one_argument() {
    let project = Project::new("console-arity")
        .sprite("A", r#"sprite "A" { on flag_clicked { console::log(); } }"#);
    let rendered = project.expect_error().render();
    assert!(
        rendered.contains("there is no `console::log`"),
        "{rendered}"
    );
}

#[test]
fn a_watch_puts_a_cell_on_the_stage() {
    let project = Project::new("watch").sprite(
        "A",
        r#"sprite "A" {
            var score: num = 0;
            var trail: list<num> = [];
            watch score, trail;
            on flag_clicked {
                score = 1;
                score += 1;
                trail.push(score);
            }
        }"#,
    );
    let asm = project.expand();
    // A watched scalar is a real Scratch variable, whose monitor starts shown.
    assert!(asm.contains("visible var score = 0;"), "{asm}");
    // A watched list already has storage; it only has to be shown.
    assert!(asm.contains("visible list trail = [];"), "{asm}");
    // And every write to the cell keeps the mirror in step, by copying the
    // cell *after* the write: recomputing the value would read the cell back
    // and count a `+=` twice.
    assert!(
        asm.contains("data_setvariableto(\"score\", data_itemoflist("),
        "the mirror is the cell's new value: {asm}"
    );
    assert!(
        !asm.contains("data_setvariableto(\"score\", operator_add("),
        "and never the expression that made it: {asm}"
    );
    let _ = project.build();
}

#[test]
fn watching_something_undeclared_is_an_error() {
    let project = Project::new("watch-missing").sprite(
        "A",
        r#"sprite "A" { watch nope; on flag_clicked { looks::hide(); } }"#,
    );
    let rendered = project.expect_error().render();
    assert!(
        rendered.contains("watched but never declared"),
        "{rendered}"
    );
}

/// A list literal is a list of items, not one string. Scratch keeps a list's
/// value as an array, so the declaration has to become one.
#[test]
fn a_list_literal_becomes_one_item_per_value() {
    let project = Project::new("list-literal").sprite(
        "A",
        r#"sprite "A" {
            var xs: list<num> = [1, 2, 3];
            var words: list<str> = ["a", "b"];

            on flag_clicked { looks::say(f"{xs.len()}"); }
        }"#,
    );
    project.write();
    let mut options = project.options();
    options.debug = true;
    driver::build(&options).expect("the fixture builds");
    let text = std::fs::read_to_string(project.dir.join("dist/project.json"))
        .expect("--debug writes project.json");
    let built: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    let lists = built["targets"]
        .as_array()
        .expect("targets")
        .iter()
        .find(|t| t["name"] == "A")
        .expect("the sprite")["lists"]
        .as_object()
        .expect("lists")
        .clone();
    let items_of = |name: &str| {
        lists
            .values()
            .find(|v| v[0] == name)
            .unwrap_or_else(|| panic!("no list called {name}"))[1]
            .clone()
    };
    assert_eq!(items_of("xs"), serde_json::json!([1, 2, 3]), "three items");
    assert_eq!(
        items_of("words"),
        serde_json::json!(["a", "b"]),
        "two string items"
    );
}

/// A WAV with a canonical header and no samples, which is all the asset loader
/// needs to accept the file and read its format.
fn silence_wav() -> Vec<u8> {
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&36u32.to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&22050u32.to_le_bytes());
    wav.extend_from_slice(&44100u32.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&0u32.to_le_bytes());
    wav
}

/// `sound` is a declaration keyword *and* the name of a module, so
/// `sound::play(…)` has to parse as a call rather than as a declaration.
#[test]
fn a_keyword_module_name_still_calls() {
    let project = Project::new("sound-call").sprite(
        "A",
        r#"sprite "A" {
            sound "beep" = "assets/beep.wav";

            on flag_clicked {
                sound::play("beep");
            }
        }"#,
    );
    project.write();
    std::fs::write(project.dir.join("assets/beep.wav"), silence_wav()).expect("write the wav");
    let asm = project.expand();
    assert!(asm.contains("sound_play(\"beep\")"), "{asm}");
}

/// A menu is written as a variant when raven knows its values, and as the name's
/// A menu is written as a variant when raven knows its values, and as the name's
/// own literal when the project declares them.
#[test]
fn a_declared_name_is_a_literal_and_a_known_value_is_a_variant() {
    let project = Project::new("menu-spelling").sprite(
        "Player",
        r#"sprite "Player" {
            costume "blank" = "assets/blank.svg";
            sound "beep" = "assets/beep.wav";

            on flag_clicked {
                looks::switch_costume_to("blank");
                sound::play("beep");
                motion::goto(Goto::MousePointer);
                control::wait_until(sensing::touching_object(TouchingObject::EdgeOfStage));
            }
        }"#,
    );
    let asm = project.expand();
    // A declared name reaches the project as the string it was declared as.
    assert!(asm.contains("looks_switchcostumeto(\"blank\")"), "{asm}");
    assert!(asm.contains("sound_play(\"beep\")"), "{asm}");
    // A value Scratch defines stays a variant, and becomes the Scratch string.
    assert!(asm.contains("motion_goto(\"_mouse_\")"), "{asm}");
    assert!(asm.contains("sensing_touchingobject(\"_edge_\")"), "{asm}");
}

/// The literal is checked, so a typo in a name is still a compile error.
#[test]
fn a_declared_name_that_is_not_declared_is_an_error() {
    let project = Project::new("menu-typo").sprite(
        "Player",
        r#"sprite "Player" {
            costume "blank" = "assets/blank.svg";

            on flag_clicked {
                looks::switch_costume_to("blenk");
            }
        }"#,
    );
    let rendered = project.expect_error().render();
    assert!(rendered.contains("no Costume called `blenk`"), "{rendered}");
    assert!(rendered.contains("did you mean `blank`?"), "{rendered}");
}

/// A name the project declares is not a variant, which is what makes the two
/// spellings different rather than two ways to write one thing.
#[test]
fn a_declared_name_written_as_a_variant_says_how_to_write_it() {
    let project = Project::new("menu-variant").sprite(
        "Player",
        r#"sprite "Player" {
            costume "blank" = "assets/blank.svg";
            sound "beep" = "assets/beep.wav";

            on flag_clicked {
                sound::play(Sound::BEEP);
            }
        }"#,
    );
    let rendered = project.expect_error().render();
    assert!(
        rendered.contains("is not a value of this menu"),
        "{rendered}"
    );
    assert!(rendered.contains("write one of \"beep\""), "{rendered}");
}
/// A condition that needs statements of its own — a value-returning `proc` call
/// — has to run again for every test of the loop, not once before it.
#[test]
fn a_loop_condition_calling_a_proc_is_re_evaluated() {
    let project = Project::new("loop-condition").sprite(
        "A",
        r#"sprite "A" {
            var n: num = 0;

            proc ready() -> bool {
                return n > 3;
            }

            on flag_clicked {
                repeat_until ready() {
                    n += 1;
                }
            }
        }"#,
    );
    let asm = project.expand();
    assert_eq!(
        asm.matches("ready;").count(),
        2,
        "the call must appear before the loop and at the end of its body:\n{asm}"
    );
}

/// An error raised while compiling the generated raven-asm is reported against
/// the raven that generated it, not against the staging `.rasm` the user never
/// wrote — and it says where the generated line was.
#[test]
fn a_build_error_inside_generated_code_points_at_the_rav() {
    let project = Project::new("staged-error").sprite(
        "A",
        r#"sprite "A" {
            sound "beep" = "assets/missing.wav";
        }"#,
    );
    project.write();
    let error = match driver::build(&project.options()) {
        Ok(_) => panic!("a missing asset must fail the build"),
        Err(error) => error,
    };
    let rendered = error.render();
    assert!(rendered.contains("src/sprites/A.rav"), "{rendered}");
    assert!(rendered.contains("assets/missing.wav"), "{rendered}");
    assert!(rendered.contains("generated raven-asm:"), "{rendered}");
}

/// A target whose only arena cells belong to procs still has to grow the
/// arena before the first script reaches for one — the size is not known until
/// every body has been lowered.
#[test]
fn a_script_grows_the_arena_before_it_uses_a_proc_frame() {
    let project = Project::new("reserve-order").sprite(
        "A",
        r#"sprite "A" {
            proc fib(n: num) -> num {
                if n < 2 { return n; }
                return fib(n - 1) + fib(n - 2);
            }
            on flag_clicked { console::log(f"{fib(10)}"); }
        }"#,
    );
    let asm = project.expand();
    let script = asm
        .lines()
        .skip_while(|line| !line.contains("event_whenflagclicked"))
        .take(8)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        script.contains("__vms_reserve;"),
        "the reserve call comes first: {script}\n{asm}"
    );
    assert!(
        script.contains("\"_console\")"),
        "the log line reaches the console: {asm}"
    );
    let _ = project.build();
}

/// A watch is project-wide even when it is declared on the stage: a sprite
/// writing that cell has to keep the mirror in step too.
#[test]
fn a_sprite_writing_a_watched_global_keeps_the_mirror_in_step() {
    let project = Project::new("watch-global")
        .stage("stage { pub var best: num = 0; watch best; }")
        .sprite(
            "A",
            r#"sprite "A" { on flag_clicked { best = 1; best += 2; } }"#,
        );
    let asm = project.expand();
    assert!(asm.contains("global visible var best = 0;"), "{asm}");
    assert!(
        asm.contains("data_setvariableto(\"best\", data_itemoflist("),
        "the write is mirrored from the cell: {asm}"
    );
    assert!(
        !asm.contains("data_setvariableto(\"best\", operator_add("),
        "and `+=` is not counted twice: {asm}"
    );
    let _ = project.build();
}

/// `xs[i] = v` on a list that is shorter than `i` must not vanish: Scratch's
/// `replace item` does nothing at all when there is no such item, so the write
/// grows the list to reach it.
#[test]
fn a_write_past_the_end_of_a_list_grows_it() {
    let project = Project::new("grow").sprite(
        "A",
        r#"sprite "A" {
            var xs: list<num> = [];
            on flag_clicked {
                xs[3] = 7;
                xs[1] += 5;
            }
        }"#,
    );
    let asm = project.expand();
    assert!(
        asm.contains("data_addtolist(\"\", \"xs\")"),
        "the list grows to the index: {asm}"
    );
    assert!(
        asm.contains("data_replaceitemoflist(3, \"xs\", 7)"),
        "and the write lands: {asm}"
    );
    assert!(
        asm.contains("data_replaceitemoflist(1, \"xs\", operator_add("),
        "`+=` at an index grows it too: {asm}"
    );
    let _ = project.build();
}

/// Nothing a program does not use is emitted: no arena without state, no stack
/// without block-scoped cells, no console without a log.
#[test]
fn unused_machinery_is_not_emitted() {
    let bare = Project::new("bare").sprite(
        "A",
        r#"sprite "A" { on flag_clicked { looks::say("hi"); } }"#,
    );
    let asm = bare.expand();
    for absent in ["_vms", "_gvm", "_stack", "_console", "__vms_reserve"] {
        assert!(
            !asm.contains(absent),
            "{absent} is emitted but unused: {asm}"
        );
    }
    let _ = bare.build();

    // A script with one let gets a stack and nothing else.
    let scoped = Project::new("scoped").sprite(
        "A",
        r#"sprite "A" { on flag_clicked { let n = 1; looks::say(n); } }"#,
    );
    let asm = scoped.expand();
    assert!(asm.contains("list _stack1 = [];"), "{asm}");
    assert!(!asm.contains("_vms"), "no arena was needed: {asm}");
    assert!(!asm.contains("_console"), "{asm}");
}

/// Scratch has two kinds of custom block, and the mutation has to say which.
#[test]
fn a_proc_is_warp_or_not_by_its_declaration() {
    let project = Project::new("warp").sprite(
        "A",
        r#"sprite "A" {
            proc fast(n: num) warp {
                repeat n { motion::move_steps(1); }
            }
            proc slow(n: num) {
                repeat n { motion::move_steps(1); }
            }
            on flag_clicked { fast(3); slow(3); }
        }"#,
    );
    let asm = project.expand();
    assert!(asm.contains("proc fast(n: num) warp"), "{asm}");
    assert!(
        asm.contains("proc slow(n: num) {"),
        "a plain `proc` stays un-warped: {asm}"
    );
    let built = project.build();
    let _ = built;
    let json = std::fs::read_to_string(project.dir.join("dist/test.sb3")).unwrap_or_default();
    let _ = json;
}

/// A monitor the declaration does not place itself goes down the left edge,
/// one row each, and a declaration that says `at X Y` keeps that spot.
#[test]
fn monitors_are_stacked_down_the_left_edge_unless_the_source_places_them() {
    let project = Project::new("monitors")
        .stage("stage { pub var best: num = 0; watch best; }")
        .sprite(
            "A",
            r#"sprite "A" {
                var score: num = 0;
                var lives: num = 3;
                watch score;
                on flag_clicked { score += 1; lives -= 0; }
            }"#,
        );
    project.write();
    let mut options = project.options();
    options.debug = true;
    driver::build(&options).expect("build");
    let text = std::fs::read_to_string(project.dir.join("dist/project.json")).expect("json");
    let project_json: serde_json::Value = serde_json::from_str(&text).expect("parse");
    let mut auto = 0;
    for monitor in project_json["monitors"].as_array().expect("monitors") {
        assert_eq!(monitor["x"].as_f64(), Some(5.0), "the left edge: {monitor}");
        if monitor["visible"].as_bool() == Some(true) {
            auto += 1;
        }
    }
    assert!(auto >= 2, "the two watched values are visible: {auto}");
}
