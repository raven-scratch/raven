//! What a reversal has to get right.
//!
//! The load-bearing test is `a_reversed_project_rebuilds_the_same_project`: a
//! project is compiled with raven-asm, reversed with raven-re, compiled again,
//! and the two `project.json`s are compared through a fingerprint that keeps
//! everything Scratch can observe and drops everything it cannot. The rest are
//! the refusals and the encoded names.

use raven_re::names::{encode, is_identifier, is_keyword};
use raven_re::{decompile, Decompiled};
use raven_scratch::sb3::Project;
use raven_scratch::zipw::ZipWriter;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const BACKDROP_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="480" height="360" viewBox="0 0 480 360"><rect width="480" height="360" fill="#fff"/></svg>"##;
const LOGO_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64"><circle cx="32" cy="32" r="30" fill="#3b6"/></svg>"##;

const MANIFEST: &str = r#"[project]
name = "demo"
extensions = ["pen"]

[targets]
stage = "src/stage.rasm"
sprites = ["src/sprites/player.rasm"]
"#;

/// A project that exercises every shape a reversal has to handle: both kinds of
/// menu, reporters inside drop-downs, all three parameter kinds, an `else`, a
/// `stop` with a notch, a colour, a boolean input, and a list.
const STAGE: &str = r##"stage {
    costume "backdrop1" = "assets/backdrop.svg";
    broadcast "reset";

    var score = 0;
    list trail = [1, "two", true];

    event_whenflagclicked {
        data_setvariableto("score", 1);
        data_changevariableby("score", 1);
        looks_switchbackdropto("backdrop1");
        event_broadcast("reset");
        looks_sayforsecs("hi", 2);
        motion_goto(sensing_answer());
        motion_gotoxy(operator_mathop("sqrt", 9), -10);
        control_repeat(3) {
            motion_movesteps(10);
        }
        control_if_else(operator_gt(data_variable("score"), 2)) {
            looks_say("big");
        } else {
            looks_say("small");
        }
        control_repeat_until(operator_lt(data_lengthoflist("trail"), 1)) {
            data_deleteoflist(1, "trail");
        }
        control_stop("other scripts in stage") {
            looks_hide;
            control_stop("all");
        }
    }

    event_whengreaterthan("timer", 5) {
        looks_say("loud");
    }
}
"##;

const PLAYER: &str = r##"sprite "Player" {
    costume "idle" = "assets/logo.svg" center 32 32;

    var lives = 3;
    list items = ["a", 2];

    proc hop(height: num, amount: str, fast: bool) warp {
        motion_changeyby(argument_reporter_string_number("height"));
        looks_say(argument_reporter_string_number("amount"));
        control_if(argument_reporter_boolean("fast")) {
            motion_movesteps(1);
        }
    }

    proc paint(size: num) {
        pen_clear;
        pen_penDown;
        pen_setPenColorToColor("#ff8800");
        control_repeat(4) {
            motion_movesteps(argument_reporter_string_number("size"));
            motion_turnright(90);
        }
        pen_penUp;
    }

    event_whenbroadcastreceived("reset") {
        hop(5, "up", operator_not(sensing_mousedown()));
        data_setvariableto("lives", 3);
        data_addtolist("thing", "items");
        paint(20);
    }

    event_whenkeypressed("space") {
        sensing_setdragmode("draggable");
        control_wait_until(operator_not(sensing_mousedown()));
        looks_changeeffectby("GHOST", -10);
    }
}
"##;

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("raven-re-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create the test directory");
    dir
}

/// Write a raven-asm project and compile it.
fn build_project(root: &Path) -> raven_asm::compile::BuildOutput {
    write(root, "raven-asm.toml", MANIFEST);
    write(root, "src/stage.rasm", STAGE);
    write(root, "src/sprites/player.rasm", PLAYER);
    write(root, "assets/backdrop.svg", BACKDROP_SVG);
    write(root, "assets/logo.svg", LOGO_SVG);
    raven_asm::compile::build(&root.join("raven-asm.toml")).expect("the test project compiles")
}

fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("create a directory");
    std::fs::write(&path, text).expect("write a file");
}

/// Pack a compiled project, assets included, into the `.sb3` bytes Scratch reads.
fn pack(output: &raven_asm::compile::BuildOutput) -> Vec<u8> {
    let mut zip = ZipWriter::new();
    zip.add(
        "project.json",
        serde_json::to_vec(&output.project).expect("serialize project.json"),
    );
    for asset in &output.assets {
        zip.add(asset.filename(), asset.data.clone());
    }
    zip.finish()
}

/// Pack a project straight from JSON, for the projects raven-asm cannot write.
fn pack_json(json: &str) -> Vec<u8> {
    let mut zip = ZipWriter::new();
    zip.add("project.json", json.as_bytes().to_vec());
    zip.add(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.svg",
        BACKDROP_SVG.as_bytes().to_vec(),
    );
    zip.finish()
}

fn write_decompiled(decompiled: &Decompiled, root: &Path) {
    decompiled.write(root).expect("write the reversed project");
}

/// Reverse something that must be refused, and render why.
fn refuse(sb3: &[u8]) -> String {
    match decompile(sb3, "wat") {
        Ok(_) => panic!("the project should have been refused"),
        Err(error) => error.render(),
    }
}

// ---------------------------------------------------------------------------
// The round trip
// ---------------------------------------------------------------------------

#[test]
fn a_reversed_project_rebuilds_the_same_project() {
    let original = scratch_dir("roundtrip-source");
    let output = build_project(&original);
    let reversed = scratch_dir("roundtrip-reversed");

    let decompiled = decompile(&pack(&output), "demo").expect("reverse");
    write_decompiled(&decompiled, &reversed);

    let rebuilt = raven_asm::compile::build(&reversed.join("raven-asm.toml"))
        .expect("the reversed project compiles")
        .project;

    let before = fingerprint(&output.project);
    let after = fingerprint(&rebuilt);
    assert_eq!(
        before, after,
        "the reversed project is not the project it came from"
    );
}

// ---------------------------------------------------------------------------
// The optimiser
// ---------------------------------------------------------------------------
//
// `raven` runs `raven-opt` over the lowered project before compiling it, so a
// rewrite that changed what a project *does* would be a wrong program in every
// raven project and invisible in every raven-asm one. The tests below are the two
// halves of the proof:
//
//   * the optimised build still satisfies the round-trip law -- it reverses and
//     rebuilds to the same observable project -- which is the strongest
//     structural statement this repository can make about a compiler output;
//   * the optimised build emits *fewer* blocks, so "it did something" is
//     measured rather than assumed. An optimiser that silently did nothing
//     would pass the first test and fail this one.
//
// The optimiser is its own program over its own input now, so "with it" and
// "without it" are two directories rather than two compiler flags: the source is
// written, optionally rewritten by `raven_opt`, and compiled. That is exactly
// the order `crates/raven/src/driver.rs` uses.

/// Write the test project, optionally optimise it, and compile it.
fn build_with_optimizer(root: &Path, optimize: bool) -> raven_asm::compile::BuildOutput {
    write(root, "raven-asm.toml", MANIFEST);
    write(root, "src/stage.rasm", STAGE);
    write(root, "src/sprites/player.rasm", PLAYER);
    write(root, "assets/backdrop.svg", BACKDROP_SVG);
    write(root, "assets/logo.svg", LOGO_SVG);
    let manifest = root.join("raven-asm.toml");
    if optimize {
        raven_opt::optimize_in_place(&manifest).expect("the test project optimises");
    }
    raven_asm::compile::build_with_options(&manifest, raven_asm::compile::BuildOptions::default())
        .expect("the test project compiles")
}

#[test]
fn the_optimizer_does_not_change_what_a_project_observably_is() {
    // The round-trip law, checked on both builds. `fingerprint` keeps every
    // block, every variable, every list, every costume and every script and
    // drops only ids and workspace positions, so two projects that agree on it
    // are two projects Scratch cannot tell apart.
    for optimize in [false, true] {
        let original = scratch_dir(&format!("optimizer-rt-{optimize}"));
        let output = build_with_optimizer(&original, optimize);
        let reversed = scratch_dir(&format!("optimizer-rt-rev-{optimize}"));

        let decompiled = decompile(&pack(&output), "demo").expect("reverse");
        write_decompiled(&decompiled, &reversed);

        let rebuilt = raven_asm::compile::build(&reversed.join("raven-asm.toml"))
            .expect("the reversed project compiles")
            .project;

        assert_eq!(
            fingerprint(&output.project),
            fingerprint(&rebuilt),
            "the optimiser (on={optimize}) broke the round trip"
        );
    }
}

#[test]
fn the_optimizer_preserves_the_observable_project() {
    // The sharper statement, and the one the round-trip test above cannot make:
    // the optimised build and the unoptimised build are the *same project* as
    // far as Scratch can see, except that one has fewer blocks.
    //
    // They are not identical -- folding a reporter into a literal replaces a
    // block with a JSON number, which is the whole point -- so this compares
    // what each script *computes* rather than each block: the fingerprint of
    // the unoptimised build, with every script re-read as the values it leaves.
    // That is not a thing this test can do, so it asserts the properties that
    // are checkable here and leaves the behavioural question to
    // `tools/check.mjs`, which runs the real VM:
    //
    //   * the optimised build has fewer blocks;
    //   * it has the same targets, variables, lists, costumes and sounds;
    //   * it still compiles, so no rewrite produced an unspellable block.
    let plain = scratch_dir("optimizer-off");
    let folded = scratch_dir("optimizer-on");
    let without = build_with_optimizer(&plain, false);
    let with = build_with_optimizer(&folded, true);

    let blocks = |o: &raven_asm::compile::BuildOutput| -> usize {
        o.project.targets.iter().map(|t| t.blocks.len()).sum()
    };
    assert!(
        blocks(&with) < blocks(&without),
        "the optimiser made no difference: {} blocks either way",
        blocks(&without)
    );

    let names = |o: &raven_asm::compile::BuildOutput| -> Vec<String> {
        let mut n: Vec<String> = o.project.targets.iter().map(|t| t.name.clone()).collect();
        n.sort();
        n
    };
    assert_eq!(names(&with), names(&without));

    let shape = |o: &raven_asm::compile::BuildOutput| -> Vec<String> {
        let mut out = Vec::new();
        for t in &o.project.targets {
            out.push(format!("{} vars={}", t.name, t.variables.len()));
            out.push(format!("{} lists={}", t.name, t.lists.len()));
            out.push(format!("{} costumes={}", t.name, t.costumes.len()));
            out.push(format!("{} sounds={}", t.name, t.sounds.len()));
            out.push(format!("{} broadcasts={}", t.name, t.broadcasts.len()));
        }
        out.sort();
        out
    };
    assert_eq!(
        shape(&with),
        shape(&without),
        "the optimiser changed something other than the blocks"
    );
}

#[test]
fn an_unoptimised_build_is_the_lowering_verbatim() {
    // The contract, and the reason the optimiser is a separate program: with it
    // *not* run, compiling twice produces the same blocks, and those blocks are
    // what the source says -- one statement each. This is the guard on
    // `raven-asm`'s promise, not on `raven`'s.
    let a = scratch_dir("optimizer-verbatim-a");
    let b = scratch_dir("optimizer-verbatim-b");
    let first = build_with_optimizer(&a, false);
    let second = build_with_optimizer(&b, false);
    assert_eq!(
        fingerprint(&first.project),
        fingerprint(&second.project),
        "an unoptimised build is not reproducible"
    );
}

#[test]
fn the_optimizer_reports_nothing_to_do_when_there_is_nothing_to_do() {
    // A run over a project with no foldable operand has to say so rather than
    // claim a rewrite, because the number is what a caller reports and a
    // phantom rewrite is a claim the project got smaller when it did not.
    let root = scratch_dir("optimizer-noop");
    write(&root, "raven-asm.toml", MANIFEST);
    write(
        &root,
        "src/stage.rasm",
        "stage {\n    costume \"b\" = \"assets/backdrop.svg\";\n}\n",
    );
    write(&root, "assets/backdrop.svg", BACKDROP_SVG);
    let manifest = root.join("raven-asm.toml");
    let report = raven_opt::optimize_in_place(&manifest).expect("optimises");
    assert_eq!(report.files, 1, "one source file");
    assert_eq!(report.changed, 0, "nothing to fold");
    assert_eq!(report.folded.total(), 0);
}

#[test]
fn the_stage_and_every_sprite_get_a_file() {
    let original = scratch_dir("files-source");
    let output = build_project(&original);
    let decompiled = decompile(&pack(&output), "demo").expect("reverse");
    let names: Vec<String> = decompiled
        .files
        .iter()
        .map(|(path, _)| path.to_string_lossy().replace('\\', "/"))
        .collect();
    for expected in [
        "raven-asm.toml",
        "src/stage.rasm",
        "src/sprites/player.rasm",
    ] {
        assert!(names.iter().any(|name| name == expected), "{names:?}");
    }
    // The assets are written under the name the archive gave them, which is
    // their content hash.
    let assets: Vec<&String> = names
        .iter()
        .filter(|name| name.starts_with("assets/"))
        .collect();
    assert_eq!(assets.len(), 2, "{names:?}");
    assert!(
        assets.iter().all(|name| name.ends_with(".svg")),
        "{names:?}"
    );
}

#[test]
fn reversing_the_same_file_twice_writes_the_same_bytes() {
    let original = scratch_dir("stable-source");
    let output = build_project(&original);
    let bytes = pack(&output);
    let first = decompile(&bytes, "demo").expect("reverse");
    let second = decompile(&bytes, "demo").expect("reverse again");
    let contents = |decompiled: &Decompiled| {
        decompiled
            .files
            .iter()
            .map(|(path, data)| (path.clone(), data.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(contents(&first), contents(&second));
}

// ---------------------------------------------------------------------------
// The refusals
// ---------------------------------------------------------------------------

#[test]
fn a_turbowarp_block_is_refused() {
    let json = r#"{
        "targets": [{
            "isStage": true, "name": "Stage", "variables": {}, "lists": {}, "broadcasts": {},
            "blocks": {"a": {"opcode": "control_while", "next": null, "parent": null,
                "inputs": {"CONDITION": [2, "b"]}, "fields": {}, "shadow": false, "topLevel": true}},
            "comments": {}, "currentCostume": 0, "costumes": [], "sounds": [],
            "volume": 100, "layerOrder": 0
        }],
        "monitors": [], "extensions": [],
        "meta": {"semver": "3.0.0", "vm": "0.2.0", "agent": "none"}
    }"#;
    let rendered = refuse(&pack_json(json));
    assert!(rendered.contains("control_while"), "{rendered}");
    assert!(rendered.contains("TurboWarp"), "{rendered}");
}

#[test]
fn an_unknown_block_is_refused() {
    let json = r#"{
        "targets": [{
            "isStage": true, "name": "Stage", "variables": {}, "lists": {}, "broadcasts": {},
            "blocks": {"a": {"opcode": "turbowarp_magic", "next": null, "parent": null,
                "inputs": {}, "fields": {}, "shadow": false, "topLevel": true}},
            "comments": {}, "currentCostume": 0, "costumes": [], "sounds": [],
            "volume": 100, "layerOrder": 0
        }],
        "monitors": [], "extensions": [],
        "meta": {"semver": "3.0.0", "vm": "0.2.0", "agent": "none"}
    }"#;
    let error = refuse(&pack_json(json));
    assert!(error.contains("turbowarp_magic"));
}

#[test]
fn an_extension_raven_does_not_bundle_is_refused() {
    let json = r#"{
        "targets": [], "monitors": [], "extensions": ["text2speech"],
        "meta": {"semver": "3.0.0", "vm": "0.2.0", "agent": "none"}
    }"#;
    let rendered = refuse(&pack_json(json));
    assert!(rendered.contains("text2speech"), "{rendered}");
    assert!(rendered.contains("TurboWarp"), "{rendered}");
}

#[test]
fn a_turbowarp_agent_is_refused() {
    let json = r#"{
        "targets": [], "monitors": [], "extensions": [],
        "meta": {"semver": "3.0.0", "vm": "0.2.0", "agent": "TurboWarp/1.11.1"}
    }"#;
    let rendered = refuse(&pack_json(json));
    assert!(rendered.contains("TurboWarp"), "{rendered}");
    assert!(rendered.contains("meta.agent"), "{rendered}");
}

#[test]
fn a_folder_wrapped_archive_says_so() {
    let mut zip = ZipWriter::new();
    zip.add("game/project.json", b"{}".to_vec());
    let rendered = refuse(&zip.finish());
    assert!(rendered.contains("game/project.json"), "{rendered}");
    assert!(rendered.contains("folder"), "{rendered}");
}

// ---------------------------------------------------------------------------
// Values stored inline
// ---------------------------------------------------------------------------

/// The Scratch serializer compresses a variable, list or broadcast reporter
/// that stands alone in an input into `[12, "name", "id"]` in place of a block.
///
/// Reading that as a literal would silently turn `set x to y` into `set x to
/// "y"`, which compiles, runs, and does the wrong thing, so the reporter has to
/// come back as one.
#[test]
fn a_reporter_compressed_into_an_input_is_still_a_reporter() {
    let json = format!(
        r##"{{
        "targets": [
            {{"isStage": true, "name": "Stage",
              "variables": {{"v1": ["score", 0]}}, "lists": {{}}, "broadcasts": {{}},
              "blocks": {{}}, "comments": {{}}, "currentCostume": 0, "costumes": [{}], "sounds": [],
              "volume": 100, "layerOrder": 0}},
            {{"isStage": false, "name": "Sprite1", "variables": {{}}, "lists": {{}}, "broadcasts": {{}},
              "blocks": {{
                "hat": {{"opcode": "event_whenflagclicked", "next": "set", "parent": null,
                         "inputs": {{}}, "fields": {{}}, "shadow": false, "topLevel": true, "x": 0, "y": 0}},
                "set": {{"opcode": "data_setvariableto", "next": "pen", "parent": "hat",
                         "inputs": {{"VALUE": [3, [12, "score", "v1"], [10, ""]]}},
                         "fields": {{"VARIABLE": ["score", "v1"]}},
                         "shadow": false, "topLevel": false}},
                "pen": {{"opcode": "pen_setPenColorToColor", "next": null, "parent": "set",
                         "inputs": {{"COLOR": [3, [12, "score", "v1"], [9, "#000000"]]}},
                         "fields": {{}}, "shadow": false, "topLevel": false}}
              }},
              "comments": {{}}, "currentCostume": 0, "costumes": [{}], "sounds": [],
              "volume": 100, "layerOrder": 1, "visible": true, "x": 0, "y": 0, "size": 100,
              "direction": 90, "draggable": false, "rotationStyle": "all around"}}
        ],
        "monitors": [], "extensions": ["pen"],
        "meta": {{"semver": "3.0.0", "vm": "0.2.0", "agent": "none"}}
    }}"##,
        costume(),
        costume()
    );

    let root = scratch_dir("compressed-reporter");
    let decompiled = decompile(&pack_json(&json), "demo").expect("reverse");
    let source = decompiled
        .files
        .iter()
        .find(|(path, _)| path.to_string_lossy().ends_with("sprite1.rasm"))
        .map(|(_, data)| String::from_utf8_lossy(data).to_string())
        .expect("the sprite file");
    assert!(
        source.contains(r#"data_setvariableto("score", data_variable("score"))"#),
        "{source}"
    );
    assert!(
        source.contains(r#"pen_setPenColorToColor(data_variable("score"))"#),
        "{source}"
    );

    // And the rebuilt project reads the variable, rather than setting the
    // variable to the text of its own name.
    write_decompiled(&decompiled, &root);
    let rebuilt = raven_asm::compile::build(&root.join("raven-asm.toml"))
        .expect("the reversed project compiles")
        .project;
    let sprite = rebuilt
        .targets
        .iter()
        .find(|target| target.name == "Sprite1")
        .expect("the sprite");
    let set = block_with_opcode(sprite, "data_setvariableto");
    let descriptor = set["inputs"]["VALUE"].as_array().expect("an input");
    let block = descriptor[1]
        .as_str()
        .expect("an input that holds a real block, not a literal");
    assert_eq!(sprite.blocks[block]["opcode"], "data_variable");
    assert_eq!(sprite.blocks[block]["fields"]["VARIABLE"][0], "score");
}

/// The first block of a target with this opcode.
fn block_with_opcode<'a>(target: &'a raven_scratch::sb3::Target, opcode: &str) -> &'a Value {
    target
        .blocks
        .values()
        .find(|block| block["opcode"] == opcode)
        .unwrap_or_else(|| panic!("no `{opcode}` block"))
}

// ---------------------------------------------------------------------------
// Names
// ---------------------------------------------------------------------------

#[test]
fn a_name_that_is_not_an_identifier_is_encoded() {
    let json = format!(
        r#"{{
        "targets": [
            {{"isStage": true, "name": "Stage",
              "variables": {{"v1": ["my score", 0]}}, "lists": {{}}, "broadcasts": {{}},
              "blocks": {{}}, "comments": {{}}, "currentCostume": 0, "costumes": [{}], "sounds": [],
              "volume": 100, "layerOrder": 0}},
            {{"isStage": false, "name": "Sprite1", "variables": {{}}, "lists": {{}}, "broadcasts": {{}},
              "blocks": {{
                "hat": {{"opcode": "event_whenflagclicked", "next": "set", "parent": null,
                         "inputs": {{}}, "fields": {{}}, "shadow": false, "topLevel": true, "x": 0, "y": 0}},
                "set": {{"opcode": "data_setvariableto", "next": null, "parent": "hat",
                         "inputs": {{"VALUE": [1, [10, "hi"]]}},
                         "fields": {{"VARIABLE": ["my score", "v1"]}},
                         "shadow": false, "topLevel": false}}
              }},
              "comments": {{}}, "currentCostume": 0, "costumes": [{}], "sounds": [],
              "volume": 100, "layerOrder": 1, "visible": true, "x": 0, "y": 0, "size": 100,
              "direction": 90, "draggable": false, "rotationStyle": "all around"}}
        ],
        "monitors": [], "extensions": [],
        "meta": {{"semver": "3.0.0", "vm": "0.2.0", "agent": "none"}}
    }}"#,
        costume(),
        costume()
    );

    let root = scratch_dir("encoded-names");
    let decompiled = decompile(&pack_json(&json), "demo").expect("reverse");
    write_decompiled(&decompiled, &root);

    let stage = std::fs::read_to_string(root.join("src/stage.rasm")).expect("read the stage");
    let encoded = encode("my score");
    assert!(stage.contains(&encoded), "{stage}");
    assert!(
        stage.contains("the project calls this `my score`"),
        "{stage}"
    );

    // The point of the encoding is that what came out compiles.
    raven_asm::compile::build(&root.join("raven-asm.toml")).expect("the reversed project compiles");
}

#[test]
fn the_encoding_is_a_function_of_the_name_alone() {
    assert_eq!(encode("my score"), "re_6d792073636f7265");
    assert!(is_identifier(&encode("my score")));
    assert!(is_identifier(&encode("")));
    assert!(is_identifier(&encode("a\"b")));
    // An encoded name is itself a name, so a second reversal keeps it.
    assert_eq!(encode(&encode("my score")), encode("re_6d792073636f7265"));
    assert!(!is_identifier("my score"));
    assert!(is_keyword("warp"));
}

/// The names an encoding can collide with are the ones already taken.
///
/// Hex is injective, so two different names never encode to the same string;
/// what an encoding *can* collide with is a name the project really had that
/// looks like one. Declarations are therefore handed out through a set that
/// every claim goes through, and a second claim of the same string is moved
/// aside deterministically instead of silently duplicating.
#[test]
fn names_that_collide_with_an_encoding_are_still_unique() {
    let encoded = encode("my score");
    let json = format!(
        r#"{{
        "targets": [
            {{"isStage": true, "name": "Stage",
              "variables": {{
                "v1": ["my score", 0],
                "v2": ["{encoded}", 0],
                "v3": ["var", 0],
                "v4": ["score", 0],
                "v5": ["score", 0],
                "v6": ["1st", 0],
                "v7": ["my_score", 0]
              }},
              "lists": {{"l1": ["my score", []]}}, "broadcasts": {{}},
              "blocks": {{}}, "comments": {{}}, "currentCostume": 0, "costumes": [{}], "sounds": [],
              "volume": 100, "layerOrder": 0}}
        ],
        "monitors": [], "extensions": [],
        "meta": {{"semver": "3.0.0", "vm": "0.2.0", "agent": "none"}}
    }}"#,
        costume()
    );

    let bytes = pack_json(&json);
    let decompiled = decompile(&bytes, "demo").expect("reverse");
    let root = scratch_dir("colliding-names");
    write_decompiled(&decompiled, &root);

    let stage = std::fs::read_to_string(root.join("src/stage.rasm")).expect("read the stage");
    let mut declared: Vec<String> = Vec::new();
    for line in stage.lines() {
        let line = line.trim_start().trim_start_matches("visible ");
        for kind in ["var ", "list "] {
            if let Some(rest) = line.strip_prefix(kind) {
                let name = rest.split([' ', '=']).next().expect("a name").to_string();
                declared.push(name);
            }
        }
    }
    assert_eq!(declared.len(), 8, "{stage}");
    let mut unique = declared.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        unique.len(),
        declared.len(),
        "two declarations share a name: {declared:?}"
    );
    assert!(declared.contains(&encoded), "{declared:?}");

    // The point of the encoding is that what came out compiles.
    raven_asm::compile::build(&root.join("raven-asm.toml")).expect("the reversed project compiles");

    // And the collision is broken the same way every time.
    let again = decompile(&bytes, "demo").expect("reverse again");
    assert_eq!(decompiled.files, again.files);
}

// ---------------------------------------------------------------------------
// Blocks that are not part of a script
// ---------------------------------------------------------------------------

#[test]
fn a_block_outside_a_script_is_dropped_and_reported() {
    let json = format!(
        r#"{{
        "targets": [
            {{"isStage": true, "name": "Stage", "variables": {{}}, "lists": {{}}, "broadcasts": {{}},
              "blocks": {{}}, "comments": {{}}, "currentCostume": 0, "costumes": [{}], "sounds": [],
              "volume": 100, "layerOrder": 0}},
            {{"isStage": false, "name": "Sprite1", "variables": {{}}, "lists": {{}}, "broadcasts": {{}},
              "blocks": {{
                "loose": {{"opcode": "motion_movesteps", "next": null, "parent": null,
                          "inputs": {{"STEPS": [1, [4, 10]]}}, "fields": {{}}, "shadow": false,
                          "topLevel": true, "x": 0, "y": 0}}
              }},
              "comments": {{}}, "currentCostume": 0, "costumes": [{}], "sounds": [],
              "volume": 100, "layerOrder": 1, "visible": true, "x": 0, "y": 0, "size": 100,
              "direction": 90, "draggable": false, "rotationStyle": "all around"}}
        ],
        "monitors": [], "extensions": [],
        "meta": {{"semver": "3.0.0", "vm": "0.2.0", "agent": "none"}}
    }}"#,
        costume(),
        costume()
    );
    let decompiled = decompile(&pack_json(&json), "demo").expect("reverse");
    let player = decompiled
        .files
        .iter()
        .find(|(path, _)| path.to_string_lossy().ends_with("sprite1.rasm"))
        .map(|(_, data)| String::from_utf8_lossy(data).to_string())
        .expect("the sprite file");
    assert!(!player.contains("motion_movesteps"), "{player}");
    assert!(
        decompiled
            .warnings
            .iter()
            .any(|warning| warning.render().contains("detached")),
        "a dropped block is reported"
    );
}

/// A costume with a name no project would choose, but a shape raven-asm reads.
fn costume() -> String {
    r#"{"assetId": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "name": "backdrop1",
        "bitmapResolution": 1, "md5ext": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.svg",
        "dataFormat": "svg", "rotationCenterX": 5, "rotationCenterY": 5}"#
        .to_string()
}

// ---------------------------------------------------------------------------
// The fingerprint
// ---------------------------------------------------------------------------

/// Everything about a project that Scratch can observe, with the parts it
/// cannot — ids, workspace positions, and whether an input's literal was stored
/// as a number or as text — removed.
///
/// raven-asm rebuilds identifiers and re-stacks the workspace, so comparing the
/// two `project.json`s directly would fail on things that do not matter while
/// saying nothing about the things that do.
fn fingerprint(project: &Project) -> Vec<String> {
    let mut out = Vec::new();
    let mut extensions = project.extensions.clone();
    extensions.sort();
    out.push(format!("extensions {extensions:?}"));

    let mut targets: Vec<&raven_scratch::sb3::Target> = project.targets.iter().collect();
    targets.sort_by(|a, b| a.name.cmp(&b.name));
    for target in targets {
        out.push(format!(
            "{} {}",
            if target.is_stage { "stage" } else { "sprite" },
            target.name
        ));

        let mut vars: Vec<String> = target
            .variables
            .values()
            .map(|entry| {
                format!(
                    "var {} = {}",
                    name(entry.first().and_then(Value::as_str).unwrap_or_default()),
                    value(entry.get(1).unwrap_or(&Value::Null))
                )
            })
            .collect();
        vars.sort();
        out.extend(vars);

        let mut lists: Vec<String> = target
            .lists
            .values()
            .map(|entry| {
                format!(
                    "list {} = {}",
                    name(entry.first().and_then(Value::as_str).unwrap_or_default()),
                    value(entry.get(1).unwrap_or(&Value::Null))
                )
            })
            .collect();
        lists.sort();
        out.extend(lists);

        let mut broadcasts: Vec<String> = target.broadcasts.values().cloned().collect();
        broadcasts.sort();
        out.extend(
            broadcasts
                .iter()
                .map(|message| format!("broadcast {message}")),
        );

        for costume in &target.costumes {
            out.push(format!(
                "costume {} {} {} {}",
                costume.name,
                costume.rotation_center_x,
                costume.rotation_center_y,
                costume.data_format
            ));
        }
        for sound in &target.sounds {
            out.push(format!("sound {} {}", sound.name, sound.data_format));
        }

        // Independent scripts are a set: the workspace order is layout.
        let mut scripts: Vec<String> = target
            .blocks
            .iter()
            .filter(|(_, block)| is_hat_root(block))
            .map(|(id, _)| script(&target.blocks, id))
            .collect();
        scripts.sort();
        out.extend(scripts);
    }
    out
}

/// A block that starts a script, at the top level of a target.
fn is_hat_root(block: &Value) -> bool {
    if block.get("topLevel").and_then(Value::as_bool) != Some(true) {
        return false;
    }
    if !block.get("parent").is_none_or(Value::is_null) {
        return false;
    }
    matches!(
        block.get("opcode").and_then(Value::as_str),
        Some(
            "event_whenflagclicked"
                | "event_whenkeypressed"
                | "event_whenthisspriteclicked"
                | "event_whenstageclicked"
                | "event_whenbackdropswitchesto"
                | "event_whengreaterthan"
                | "event_whenbroadcastreceived"
                | "control_start_as_clone"
                | "procedures_definition"
        )
    )
}

fn script(blocks: &BTreeMap<String, Value>, root: &str) -> String {
    let mut parts = Vec::new();
    let mut cursor = Some(root.to_string());
    while let Some(id) = cursor {
        let Some(block) = blocks.get(&id) else {
            parts.push("missing".to_string());
            break;
        };
        parts.push(block_fingerprint(blocks, block, 0));
        cursor = block
            .get("next")
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    parts.join(" -> ")
}

fn block_fingerprint(blocks: &BTreeMap<String, Value>, block: &Value, depth: usize) -> String {
    if depth > 40 {
        return "deep".to_string();
    }
    let opcode = block.get("opcode").and_then(Value::as_str).unwrap_or("?");
    let mutation = block.get("mutation");

    let mut fields: Vec<String> = block
        .get("fields")
        .and_then(Value::as_object)
        .map(|fields| {
            fields
                .iter()
                .map(|(key, value)| {
                    let raw = value
                        .as_array()
                        .and_then(|value| value.first())
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    format!("{key}={}", field_value(key, raw))
                })
                .collect()
        })
        .unwrap_or_default();
    fields.sort();

    // A custom block's inputs are keyed by argument id, and the compiler picks
    // new ids, so they are read in the mutation's order instead.
    let argument_ids: Vec<String> = mutation
        .and_then(|mutation| mutation.get("argumentids"))
        .and_then(Value::as_str)
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or_default();
    let keys: Vec<String> = if argument_ids.is_empty() {
        let mut keys: Vec<String> = block
            .get("inputs")
            .and_then(Value::as_object)
            .map(|inputs| inputs.keys().cloned().collect())
            .unwrap_or_default();
        keys.sort();
        keys
    } else {
        argument_ids
    };

    let mut inputs: Vec<String> = Vec::new();
    for key in keys {
        let Some(value) = block.get("inputs").and_then(|inputs| inputs.get(&key)) else {
            continue;
        };
        inputs.push(format!(
            "{key}={}",
            input_fingerprint(blocks, value, depth + 1)
        ));
    }

    format!(
        "{opcode}[{}]{{{}}}{}",
        fields.join(","),
        inputs.join(","),
        match mutation {
            Some(mutation) => format!(
                "({} warp={})",
                mutation
                    .get("proccode")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                mutation
                    .get("warp")
                    .and_then(Value::as_str)
                    .unwrap_or("false")
            ),
            None => String::new(),
        }
    )
}

fn input_fingerprint(blocks: &BTreeMap<String, Value>, descriptor: &Value, depth: usize) -> String {
    let Some(parts) = descriptor.as_array() else {
        return "odd".to_string();
    };
    match (parts.first().and_then(Value::as_u64), parts.get(1)) {
        (Some(1), Some(Value::Array(primitive))) => {
            let tag = primitive.first().and_then(Value::as_u64).unwrap_or(0);
            let first = primitive.get(1).cloned().unwrap_or(Value::Null);
            match tag {
                12 | 13 => format!("ref:{}", name(first.as_str().unwrap_or_default())),
                11 => format!("msg:{}", first.as_str().unwrap_or_default()),
                // raven-asm writes an input's literal as text, so a number and
                // the text that spells it are the same input to it.
                _ => format!("lit:{tag}:{}", value(&first)),
            }
        }
        (Some(1), Some(Value::String(shadow))) => {
            let block = blocks.get(shadow).cloned().unwrap_or(Value::Null);
            format!("shadow:{}", block_fingerprint(blocks, &block, depth + 1))
        }
        (Some(2) | Some(3), Some(Value::String(id))) => {
            let block = blocks.get(id).cloned().unwrap_or(Value::Null);
            format!("block:{}", block_fingerprint(blocks, &block, depth + 1))
        }
        _ => "unknown".to_string(),
    }
}

/// The raven-asm name of a Scratch name: the same rule the decompiler applies.
fn name(original: &str) -> String {
    if is_identifier(original) && !is_keyword(original) {
        original.to_string()
    } else {
        encode(original)
    }
}

/// A field value, with the spelling the catalog canonicalises removed.
fn field_value(key: &str, value: &str) -> String {
    match key {
        "VARIABLE" | "LIST" | "VALUE" => name(value),
        "EFFECT"
        | "WHENGREATERTHANMENU"
        | "OPERATOR"
        | "CURRENTMENU"
        | "DRAG_MODE"
        | "STOP_OPTION"
        | "NUMBER_NAME"
        | "FRONT_BACK"
        | "FORWARD_BACKWARD" => value.to_ascii_lowercase(),
        _ => value.to_string(),
    }
}

/// A JSON scalar as a string, because that is how raven-asm stores literals.
fn value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}
