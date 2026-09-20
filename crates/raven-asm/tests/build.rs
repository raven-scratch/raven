//! End-to-end build tests: write a project to a temporary directory, compile it
//! and inspect the resulting `project.json`.

use raven_asm::compile;
use raven_asm::scaffold;
use raven_scratch::catalog::{self, MenuDomain, Shape};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct TempProject {
    dir: PathBuf,
}

impl TempProject {
    fn new(name: &str) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("raven-test-{}-{}-{}", std::process::id(), n, name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp project");
        TempProject { dir }
    }

    fn write(&self, rel: &str, contents: &str) -> &Self {
        let path = self.dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(path, contents).expect("write file");
        self
    }

    fn manifest(&self) -> PathBuf {
        self.dir.join(raven_asm::manifest::MANIFEST_NAME)
    }

    fn build(&self) -> Result<compile::BuildOutput, raven_scratch::diag::Error> {
        compile::build(&self.manifest())
    }

    fn build_with(
        &self,
        options: compile::BuildOptions,
    ) -> Result<compile::BuildOutput, raven_scratch::diag::Error> {
        compile::build_with_options(&self.manifest(), options)
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn object(value: &Value) -> &serde_json::Map<String, Value> {
    value.as_object().expect("object")
}

/// `Result::expect_err` needs `Debug` on the success type, which `BuildOutput`
/// deliberately does not implement; this keeps the call sites readable.
fn expect_build_error(project: &TempProject, message: &str) -> raven_scratch::diag::Error {
    match project.build() {
        Ok(_) => panic!("{message}"),
        Err(e) => e,
    }
}

fn blocks_of(target: &Value) -> Vec<Value> {
    object(&target["blocks"]).values().cloned().collect()
}

fn opcodes(target: &Value) -> Vec<String> {
    blocks_of(target)
        .iter()
        .filter_map(|b| b.get("opcode").and_then(Value::as_str).map(str::to_string))
        .collect()
}

fn find_target<'a>(project: &'a Value, name: &str) -> &'a Value {
    project["targets"]
        .as_array()
        .expect("targets")
        .iter()
        .find(|t| t["name"] == name)
        .unwrap_or_else(|| panic!("no target named {name}"))
}

/// The first block in `target` with this opcode, as `(id, block)`.
fn block_by_opcode<'a>(target: &'a Value, opcode: &str) -> (String, &'a Value) {
    object(&target["blocks"])
        .iter()
        .find(|(_, b)| b["opcode"] == opcode)
        .map(|(id, b)| (id.clone(), b))
        .unwrap_or_else(|| panic!("target has no `{opcode}` block"))
}

// ---------------------------------------------------------------------------
// The default project
// ---------------------------------------------------------------------------

#[test]
fn default_scaffold_is_empty() {
    let project = TempProject::new("scaffold");
    scaffold::default_project("hello")
        .write(&project.dir)
        .expect("scaffold writes");

    let out = project.build().expect("the default project must build");
    let json = serde_json::to_value(&out.project).expect("serialize");

    let targets = json["targets"].as_array().expect("targets");
    assert_eq!(targets.len(), 2, "stage plus one sprite");
    assert_eq!(targets[0]["name"], "Stage");
    assert_eq!(targets[0]["isStage"], true);
    assert_eq!(targets[1]["name"], "Sprite1");
    assert_eq!(targets[1]["isStage"], false);

    // A fresh project is a blank canvas: no variables, no lists, no broadcasts,
    // no sounds, therefore no monitors, and no blocks at all.
    assert!(object(&targets[0]["variables"]).is_empty());
    assert!(object(&targets[0]["lists"]).is_empty());
    assert!(object(&targets[0]["broadcasts"]).is_empty());
    assert!(object(&targets[0]["blocks"]).is_empty());
    assert!(out.project.monitors.is_empty());
    assert!(object(&targets[1]["variables"]).is_empty());
    assert!(object(&targets[1]["lists"]).is_empty());
    assert!(targets[1]["sounds"].as_array().unwrap().is_empty());
    assert!(object(&targets[1]["blocks"]).is_empty());

    // The sprite wears the logo, and the stage has a backdrop.
    assert_eq!(targets[0]["costumes"].as_array().unwrap().len(), 1);
    let costumes = targets[1]["costumes"].as_array().unwrap();
    assert_eq!(costumes.len(), 1);
    assert_eq!(costumes[0]["name"], "logo");
    assert_eq!(costumes[0]["dataFormat"], "svg");

    assert!(
        out.warnings.is_empty(),
        "the default project should be warning-free: {:?}",
        out.warnings
    );
    assert!(out.project.extensions.is_empty());
    assert_eq!(out.output_file, "hello.sb3");
}

/// The costume must be the logo, letters included, and must not be a `<text>`
/// element (Scratch re-anchors text and would move the wordmark).
#[test]
fn the_default_costume_is_the_real_logo() {
    let project = TempProject::new("logo-costume");
    scaffold::default_project("hello")
        .write(&project.dir)
        .expect("scaffold writes");

    let costume = std::fs::read_to_string(project.dir.join("assets/logo.svg")).expect("logo.svg");
    assert!(costume.contains("width=\"128\" height=\"128\""));
    assert!(!costume.contains("<text"));
    // Eight band paths plus the wordmark.
    assert_eq!(costume.matches("<path ").count(), 8);
    // The wordmark is the last path, filled white.
    let wordmark = costume.rfind("<path d=\"").expect("a wordmark path");
    assert!(
        costume[wordmark..].contains("fill=\"#ffffff\""),
        "the last path should be the white wordmark"
    );
}

#[test]
fn custom_block_mutations_share_a_proccode() {
    // The default project has no custom blocks, so use a small fixture that does.
    let project = TempProject::new("proccode");
    project
        .write(
            "raven-asm.toml",
            "[project]\nname = \"p\"\n\n[targets]\nstage = \"src/stage.rasm\"\nsprites = [\"src/s.rasm\"]\n",
        )
        .write(
            "src/stage.rasm",
            "stage {\n    costume \"b\" = \"assets/a.svg\";\n}\n",
        )
        .write("assets/a.svg", "<svg width=\"10\" height=\"10\"></svg>")
        .write(
            "src/s.rasm",
            r#"sprite "S" {
    costume "c" = "assets/a.svg";

    proc zigzag(degrees: num, steps: num) warp {
        motion_turnright(argument_reporter_string_number("degrees"));
        motion_movesteps(argument_reporter_string_number("steps"));
    }

    event_whenflagclicked {
        zigzag(15, 10);
    }
}
"#,
        );

    let out = project.build().expect("build");
    let json = serde_json::to_value(&out.project).expect("serialize");
    let sprite = find_target(&json, "S");

    let mut proccodes = Vec::new();
    for block in blocks_of(sprite) {
        if block["opcode"] == "procedures_call" || block["opcode"] == "procedures_prototype" {
            proccodes.push(block["mutation"]["proccode"].as_str().unwrap().to_string());
        }
    }
    assert_eq!(proccodes.len(), 2, "one definition and one call");
    assert_eq!(proccodes[0], proccodes[1]);
    assert_eq!(proccodes[0], "zigzag %n %n");

    // The call plugs its arguments into the same argument ids the prototype
    // declares, which is how the runtime matches a call to its definition.
    let prototype = blocks_of(sprite)
        .into_iter()
        .find(|b| b["opcode"] == "procedures_prototype")
        .expect("prototype");
    let call = blocks_of(sprite)
        .into_iter()
        .find(|b| b["opcode"] == "procedures_call")
        .expect("call");
    let ids: Vec<String> = prototype["mutation"]["argumentids"]
        .as_str()
        .map(|s| serde_json::from_str::<Vec<String>>(s).unwrap())
        .unwrap_or_default();
    assert_eq!(ids.len(), 2);
    for id in &ids {
        assert!(
            call["inputs"].get(id.as_str()).is_some(),
            "the call must supply argument {id}"
        );
    }
    assert_eq!(call["mutation"]["warp"], "true");
    assert_eq!(prototype["mutation"]["warp"], "true");
}

#[test]
fn the_module_variant_of_the_scaffold_builds() {
    let project = TempProject::new("scaffold-module");
    scaffold::default_project_with_module("hello")
        .write(&project.dir)
        .expect("scaffold writes");

    let out = project.build().expect("the module variant must build");
    let json = serde_json::to_value(&out.project).expect("serialize");
    let sprite = find_target(&json, "Sprite1");
    let ops = opcodes(sprite);

    // The sprite itself defines nothing; every custom block it has came from the
    // module it pulled in with `use`.
    let prototype = blocks_of(sprite)
        .into_iter()
        .find(|b| b["opcode"] == "procedures_prototype")
        .expect("the module's procedure");
    assert_eq!(prototype["mutation"]["proccode"], "draw_square %n");
    assert_eq!(
        ops.iter().filter(|o| *o == "procedures_definition").count(),
        1
    );
    assert!(ops.iter().any(|o| o == "procedures_call"));
    // The module's `pen_*` blocks pull the extension in without being declared.
    assert_eq!(out.project.extensions, vec!["pen"]);
}

#[test]
fn rejects_a_target_without_costumes() {
    let project = TempProject::new("no-costume");
    project
        .write(
            "raven-asm.toml",
            "[project]\nname = \"n\"\n\n[targets]\nstage = \"src/stage.rasm\"\n",
        )
        .write("src/stage.rasm", "stage {\n    var v = 0;\n}\n");

    let err = expect_build_error(
        &project,
        "a target that declares no costumes must not build",
    );
    let rendered = err.render();
    assert!(rendered.contains("declares no costumes"), "{rendered}");
    assert!(
        rendered.contains("at least one `costume` declaration"),
        "{rendered}"
    );
}

#[test]
fn rejects_a_sprite_named_stage() {
    let project = TempProject::new("stage-name");
    project
        .write(
            "raven-asm.toml",
            "[project]\nname = \"n\"\n\n[targets]\nstage = \"src/stage.rasm\"\nsprites = [\"src/s.rasm\"]\n",
        )
        .write(
            "src/stage.rasm",
            "stage {\n    costume \"b\" = \"assets/a.svg\";\n}\n",
        )
        .write("assets/a.svg", "<svg width=\"10\" height=\"10\"></svg>")
        .write(
            "src/s.rasm",
            "sprite \"Stage\" {\n    costume \"c\" = \"assets/a.svg\";\n}\n",
        );

    let err = expect_build_error(&project, "a sprite named `Stage` must be refused");
    assert!(
        err.render().contains("a sprite cannot be named `Stage`"),
        "{}",
        err.render()
    );
}

#[test]
fn boolean_parameters_default_to_false() {
    let project = TempProject::new("arg-defaults");
    project
        .write(
            "raven-asm.toml",
            "[project]\nname = \"a\"\n\n[targets]\nstage = \"src/stage.rasm\"\nsprites = [\"src/s.rasm\"]\n",
        )
        .write(
            "src/stage.rasm",
            "stage {\n    costume \"b\" = \"assets/a.svg\";\n}\n",
        )
        .write("assets/a.svg", "<svg width=\"10\" height=\"10\"></svg>")
        .write(
            "src/s.rasm",
            r#"sprite "S" {
    costume "c" = "assets/a.svg";

    proc choose(flag: bool, label: str, count: num) warp { }
}
"#,
        );

    let out = project.build().expect("build");
    let json = serde_json::to_value(&out.project).expect("serialize");
    let prototype = blocks_of(find_target(&json, "S"))
        .into_iter()
        .find(|b| b["opcode"] == "procedures_prototype")
        .expect("prototype");
    let defaults: Vec<String> = serde_json::from_str(
        prototype["mutation"]["argumentdefaults"]
            .as_str()
            .expect("defaults"),
    )
    .expect("defaults are JSON");
    assert_eq!(defaults, vec!["false", "", ""]);
}

#[test]
fn costumes_always_carry_a_bitmap_resolution() {
    let project = TempProject::new("bitmap-resolution");
    project
        .write(
            "raven-asm.toml",
            "[project]\nname = \"b\"\n\n[targets]\nstage = \"src/stage.rasm\"\nsprites = [\"src/s.rasm\"]\n",
        )
        .write(
            "src/stage.rasm",
            "stage {\n    costume \"b\" = \"assets/a.svg\";\n}\n",
        )
        .write("assets/a.svg", "<svg width=\"10\" height=\"10\"></svg>")
        .write(
            "src/s.rasm",
            "sprite \"S\" {\n    costume \"c\" = \"assets/a.svg\";\n}\n",
        );

    let out = project.build().expect("build");
    let json = serde_json::to_value(&out.project).expect("serialize");
    for target in json["targets"].as_array().unwrap() {
        for costume in target["costumes"].as_array().unwrap() {
            assert_eq!(
                costume["bitmapResolution"], 1,
                "every costume must carry bitmapResolution, SVG included: {costume}"
            );
        }
    }
}

#[test]
fn builds_are_byte_for_byte_reproducible() {
    let a = TempProject::new("repro-a");
    let b = TempProject::new("repro-b");
    scaffold::default_project("hello").write(&a.dir).unwrap();
    scaffold::default_project("hello").write(&b.dir).unwrap();

    let ja = serde_json::to_string(&a.build().unwrap().project).unwrap();
    let jb = serde_json::to_string(&b.build().unwrap().project).unwrap();
    assert_eq!(ja, jb, "identical sources must produce identical projects");
}

// ---------------------------------------------------------------------------
// Reporter-accepting menus
// ---------------------------------------------------------------------------

/// A sprite whose script drops a reporter into three different menu inputs and
/// puts a literal in a fourth.
fn menu_reporter_project(name: &str) -> TempProject {
    let project = TempProject::new(name);
    project
        .write(
            "raven-asm.toml",
            "[project]\nname = \"m\"\nextensions = [\"pen\", \"music\"]\n\n[targets]\nstage = \"src/stage.rasm\"\nsprites = [\"src/s.rasm\"]\n",
        )
        .write(
            "src/stage.rasm",
            "stage {\n    costume \"b\" = \"assets/a.svg\";\n}\n",
        )
        .write("assets/a.svg", "<svg width=\"10\" height=\"10\"></svg>")
        .write(
            "src/s.rasm",
            r#"sprite "S" {
    costume "c" = "assets/a.svg";
    var target = 0;

    event_whenflagclicked {
        motion_goto(data_variable("target"));
        pen_changePenColorParamBy(data_variable("target"), 1);
        music_playDrumForBeats(data_variable("target"), 1);
        motion_glideto(1, "_mouse_");
    }
}
"#,
        );
    project
}

/// A reporter in a menu Scratch marks `acceptReporters` becomes the
/// obscured-shadow triple `[3, reporter, shadow]`.
#[test]
fn a_reporter_fills_a_reporter_accepting_menu() {
    let project = menu_reporter_project("menu-reporter");
    let out = project.build().expect("a reporter may fill a menu input");
    let json = serde_json::to_value(&out.project).expect("serialize");
    let sprite = find_target(&json, "S");
    let blocks = object(&sprite["blocks"]);

    for (opcode, input, shadow_opcode) in [
        ("motion_goto", "TO", "motion_goto_menu"),
        (
            "pen_changePenColorParamBy",
            "COLOR_PARAM",
            "pen_menu_colorParam",
        ),
        ("music_playDrumForBeats", "DRUM", "music_menu_DRUM"),
    ] {
        let (_, block) = block_by_opcode(sprite, opcode);
        let value = block["inputs"][input]
            .as_array()
            .unwrap_or_else(|| panic!("{opcode}.{input} is not an input"));
        assert_eq!(value.len(), 3, "{opcode}.{input} must be a triple");
        assert_eq!(value[0], 3, "{opcode}.{input} must be an obscured shadow");
        let reporter_id = value[1].as_str().expect("reporter id").to_string();
        let shadow_id = value[2].as_str().expect("shadow block id").to_string();
        assert_eq!(blocks[&reporter_id]["opcode"], "data_variable");
        assert_eq!(blocks[&reporter_id]["fields"]["VARIABLE"][0], "target");
        assert_eq!(blocks[&shadow_id]["opcode"], shadow_opcode);
        assert_eq!(blocks[&shadow_id]["shadow"], true);
        // The shadow behind the reporter still carries a value.
        assert_eq!(
            blocks[&shadow_id]["fields"]
                .as_object()
                .expect("shadow fields")
                .len(),
            1
        );
    }
}

/// A string literal keeps building the plain shadow menu block.
#[test]
fn a_literal_menu_value_still_makes_a_shadow() {
    let project = menu_reporter_project("menu-literal");
    let out = project
        .build()
        .expect("a literal menu value must keep building");
    let json = serde_json::to_value(&out.project).expect("serialize");
    let sprite = find_target(&json, "S");
    let blocks = object(&sprite["blocks"]);

    let (_, glide) = block_by_opcode(sprite, "motion_glideto");
    let to = glide["inputs"]["TO"].as_array().expect("TO input");
    assert_eq!(to.len(), 2, "an unobscured shadow is a two-element pair");
    assert_eq!(to[0], 1);
    let shadow_id = to[1].as_str().expect("shadow block id");
    assert_eq!(blocks[shadow_id]["opcode"], "motion_glideto_menu");
    assert_eq!(blocks[shadow_id]["fields"]["TO"][0], "_mouse_");
}

/// A reporter in a menu that does not accept one is rejected by name.
#[test]
fn a_reporter_in_a_non_reporter_menu_is_rejected() {
    let project = TempProject::new("menu-non-reporter");
    project
        .write(
            "raven-asm.toml",
            "[project]\nname = \"m\"\n\n[targets]\nstage = \"src/stage.rasm\"\nsprites = [\"src/s.rasm\"]\n",
        )
        .write(
            "src/stage.rasm",
            "stage {\n    costume \"b\" = \"assets/a.svg\";\n}\n",
        )
        .write("assets/a.svg", "<svg width=\"10\" height=\"10\"></svg>")
        .write(
            "src/s.rasm",
            r#"sprite "S" {
    costume "c" = "assets/a.svg";
    var target = 0;

    event_whenflagclicked {
        motion_setrotationstyle(data_variable("target"));
    }
}
"#,
        );

    let err = expect_build_error(&project, "a reporter in a field-only menu must not build");
    let rendered = err.render();
    assert!(
        rendered.contains("rotation_style"),
        "the error must name the menu: {rendered}"
    );
    assert!(
        rendered.contains("does not accept a reporter"),
        "{rendered}"
    );
}

// ---------------------------------------------------------------------------
// `--strict`
// ---------------------------------------------------------------------------

fn extended_project(name: &str) -> TempProject {
    let project = TempProject::new(name);
    project
        .write(
            "raven-asm.toml",
            "[project]\nname = \"m\"\n\n[targets]\nstage = \"src/stage.rasm\"\nsprites = [\"src/s.rasm\"]\n",
        )
        .write(
            "src/stage.rasm",
            "stage {\n    costume \"b\" = \"assets/a.svg\";\n}\n",
        )
        .write("assets/a.svg", "<svg width=\"10\" height=\"10\"></svg>")
        .write(
            "src/s.rasm",
            r#"sprite "S" {
    costume "c" = "assets/a.svg";
    var v = 0;

    event_whenflagclicked {
        control_while(sensing_mousedown()) {
            motion_movesteps(1);
        }
        data_setvariableto("v", control_get_counter());
    }
}
"#,
        );
    project
}

/// Without `--strict` an extended block builds and keeps its warning.
#[test]
fn extended_blocks_build_with_a_warning_by_default() {
    let project = extended_project("extended-default");
    let out = project
        .build()
        .expect("extended blocks build unless `--strict` is passed");
    assert!(
        out.warnings.iter().any(|w| {
            w.message.contains("vanilla Scratch does not have")
                && w.notes.iter().any(|n| n.contains("control_while"))
                && w.notes.iter().any(|n| n.contains("control_get_counter"))
        }),
        "the aggregate warning must survive: {:?}",
        out.warnings
    );
}

/// With `--strict` an extended block is a hard error naming the block and line.
#[test]
fn strict_rejects_extended_blocks() {
    let project = extended_project("extended-strict");
    let err = match project.build_with(compile::BuildOptions { strict: true }) {
        Ok(_) => panic!("`--strict` must reject extended blocks"),
        Err(e) => e,
    };
    let rendered = err.render();
    assert!(rendered.contains("control_while"), "{rendered}");
    assert!(
        rendered.contains("vanilla Scratch does not have"),
        "{rendered}"
    );
    assert!(rendered.contains("--strict"), "{rendered}");
    assert!(
        rendered.contains("s.rasm:"),
        "the error must carry the source line: {rendered}"
    );
}

/// A reporter-only extended block is caught too.
#[test]
fn strict_rejects_extended_reporters() {
    let project = TempProject::new("extended-reporter");
    project
        .write(
            "raven-asm.toml",
            "[project]\nname = \"m\"\n\n[targets]\nstage = \"src/stage.rasm\"\nsprites = [\"src/s.rasm\"]\n",
        )
        .write(
            "src/stage.rasm",
            "stage {\n    costume \"b\" = \"assets/a.svg\";\n}\n",
        )
        .write("assets/a.svg", "<svg width=\"10\" height=\"10\"></svg>")
        .write(
            "src/s.rasm",
            r#"sprite "S" {
    costume "c" = "assets/a.svg";
    var v = 0;

    event_whenflagclicked {
        data_setvariableto("v", control_get_counter());
    }
}
"#,
        );

    let err = match project.build_with(compile::BuildOptions { strict: true }) {
        Ok(_) => panic!("`--strict` must reject extended reporters"),
        Err(e) => e,
    };
    assert!(
        err.render().contains("control_get_counter"),
        "{}",
        err.render()
    );
}

// ---------------------------------------------------------------------------
// The catalog
// ---------------------------------------------------------------------------

/// Build a raven-asm source argument for one catalog argument.
fn argument_text(shape: Shape) -> String {
    match shape {
        Shape::Number | Shape::Positive | Shape::Whole | Shape::Integer | Shape::Angle => {
            "1".into()
        }
        Shape::Text => "\"text\"".into(),
        Shape::Color => "\"#ff8800\"".into(),
        Shape::Variable => "\"v\"".into(),
        Shape::List => "\"l\"".into(),
        Shape::Broadcast => "\"b\"".into(),
        Shape::Bool => "operator_not(sensing_mousedown())".into(),
        Shape::ParamName => "\"pa\"".into(),
        Shape::Menu(id) => {
            let value = if let Some(values) = catalog::fixed_menu_values(id) {
                values[0].to_string()
            } else if let Some(spec) = catalog::MENUS.iter().find(|m| m.id == id) {
                match spec.domain {
                    MenuDomain::Fixed(values) => values[0].to_string(),
                    MenuDomain::Costumes => "c".to_string(),
                    MenuDomain::Backdrops => "b".to_string(),
                    MenuDomain::Sounds => "s".to_string(),
                    MenuDomain::Sprites(_) => "S".to_string(),
                    MenuDomain::Open => "1".to_string(),
                }
            } else {
                // Virtual menus that are only known at run time.
                "x position".to_string()
            };
            format!("\"{value}\"")
        }
    }
}

/// `opcode(arg, arg)` — always with parentheses, so it works in any position.
fn invocation(block: &catalog::BlockSpec) -> String {
    let args: Vec<String> = block
        .args
        .iter()
        .map(|a| {
            let _ = a.wire;
            argument_text(a.shape)
        })
        .collect();
    format!("{}({})", block.opcode, args.join(", "))
}

fn call_text(block: &catalog::BlockSpec) -> String {
    let base = invocation(block);
    match block.body {
        catalog::Body::None | catalog::Body::Next => format!("{base};"),
        catalog::Body::Substack => format!("{base} {{ }}"),
        catalog::Body::SubstackElse => format!("{base} {{ }} else {{ }}"),
    }
}

/// Every block in the catalog must be expressible in raven-asm and survive a build.
#[test]
fn every_catalog_block_compiles() {
    let project = TempProject::new("catalog");
    project
        .write(
            "raven-asm.toml",
            r#"
[project]
name = "catalog"
extensions = ["pen", "music"]

[targets]
stage = "src/stage.rasm"
sprites = ["src/sprite.rasm"]
"#,
        )
        .write(
            "src/stage.rasm",
            r#"stage {
    costume "b" = "assets/backdrop.svg";
    broadcast "b";
    var sv = 0;
}
"#,
        )
        .write(
            "assets/backdrop.svg",
            "<svg width=\"480\" height=\"360\"></svg>",
        )
        .write(
            "assets/costume.svg",
            "<svg width=\"64\" height=\"64\"></svg>",
        )
        .write("assets/beep.wav", "");

    let mut sprite = String::from(
        r#"sprite "S" {
    costume "c" = "assets/costume.svg";
    sound "s" = "assets/beep.wav";
    var v = 0;
    list l = [];

    proc p(pa: str, pb: bool) warp {
        data_setvariableto("v", argument_reporter_string_number("pa"));
        control_if(argument_reporter_boolean("pb")) { }
    }

"#,
    );

    let mut expected_command_blocks = 0usize;
    let mut expected_reporter_blocks = 0usize;
    for block in catalog::BLOCKS {
        // Argument reporters only make sense inside a procedure, which the
        // generated `proc p` above already covers.
        if block.opcode.starts_with("argument_reporter_") {
            continue;
        }
        match block.kind {
            catalog::BlockKind::Hat => {
                sprite.push_str(&format!("    {}\n", call_text(block)));
            }
            catalog::BlockKind::Reporter => {
                expected_reporter_blocks += 1;
                sprite.push_str(&format!(
                    "    event_whenflagclicked {{ data_setvariableto(\"v\", {}); }}\n",
                    invocation(block)
                ));
            }
            catalog::BlockKind::Boolean => {
                expected_reporter_blocks += 1;
                sprite.push_str(&format!(
                    "    event_whenflagclicked {{ control_if({}) {{ }} }}\n",
                    invocation(block)
                ));
            }
            catalog::BlockKind::Stack | catalog::BlockKind::Cap => {
                expected_command_blocks += 1;
                sprite.push_str(&format!(
                    "    event_whenflagclicked {{ {} }}\n",
                    call_text(block)
                ));
            }
        }
    }
    sprite.push_str("}\n");
    project.write("src/sprite.rasm", &sprite);

    let out = project.build().unwrap_or_else(|e| {
        panic!(
            "the catalog fixture must build:\n{}\n\nsource:\n{sprite}",
            e.render()
        )
    });
    let json = serde_json::to_value(&out.project).expect("serialize");
    let sprite_json = find_target(&json, "S");
    let ops = opcodes(sprite_json);

    for block in catalog::BLOCKS {
        if block.opcode.starts_with("argument_reporter_") {
            assert!(
                ops.iter().any(|o| o == block.opcode),
                "`{}` is in the catalog but never reached the output",
                block.opcode
            );
            continue;
        }
        assert!(
            ops.iter().any(|o| o == block.opcode),
            "`{}` is in the catalog but never reached the output",
            block.opcode
        );
    }
    assert!(
        expected_command_blocks > 60,
        "catalog looks unexpectedly small"
    );
    assert!(expected_reporter_blocks > 20);

    // Both extensions are pulled in by the blocks that need them.
    assert_eq!(out.project.extensions, vec!["music", "pen"]);

    // Every opcode in the output must be a catalog block (plus generated ones).
    let mut unknown: Vec<&String> = ops
        .iter()
        .filter(|o| {
            catalog::block(o).is_none()
                && !o.starts_with("procedures_")
                && !o.starts_with("argument_reporter_")
                && !catalog::MENUS.iter().any(|m| m.opcode == o.as_str())
                && !matches!(
                    o.as_str(),
                    "text"
                        | "math_number"
                        | "math_positive_number"
                        | "math_whole_number"
                        | "math_integer"
                        | "math_angle"
                        | "colour_picker"
                        | "event_broadcast_menu"
                        | "data_variable"
                        | "data_listcontents"
                )
        })
        .collect();
    unknown.sort();
    unknown.dedup();
    assert!(unknown.is_empty(), "unexpected opcodes: {unknown:?}");
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

fn expect_error(source: &str, needle: &str) {
    let project = TempProject::new("err");
    project.write(
        "raven-asm.toml",
        "[project]\nname = \"err\"\n\n[targets]\nstage = \"src/stage.rasm\"\nsprites = [\"src/sprite.rasm\"]\n",
    );
    project.write(
        "src/stage.rasm",
        "stage {\n    costume \"b\" = \"assets/a.svg\";\n}\n",
    );
    project.write("assets/a.svg", "<svg width=\"10\" height=\"10\"></svg>");
    project.write("src/sprite.rasm", source);
    let err = match project.build() {
        Ok(_) => panic!("expected a build failure for:\n{source}"),
        Err(e) => e,
    };
    let rendered = err.render();
    assert!(
        rendered.contains(needle),
        "expected the error to mention `{needle}`:\n{rendered}"
    );
}

#[test]
fn unknown_opcode_suggests_a_near_miss() {
    expect_error(
        "sprite \"S\" {\n    costume \"c\" = \"assets/a.svg\";\n    event_whenflagclicked {\n        motion_movestep(10);\n    }\n}\n",
        "did you mean `motion_movesteps`",
    );
}

#[test]
fn rejects_wrong_argument_counts() {
    expect_error(
        "sprite \"S\" {\n    costume \"c\" = \"assets/a.svg\";\n    event_whenflagclicked {\n        motion_movesteps(10, 20);\n    }\n}\n",
        "takes 1 argument",
    );
}

#[test]
fn rejects_bad_dropdown_values() {
    expect_error(
        "sprite \"S\" {\n    costume \"c\" = \"assets/a.svg\";\n    event_whenflagclicked {\n        looks_seteffectto(\"sparkle\", 50);\n    }\n}\n",
        "not a valid value for this dropdown",
    );
}

#[test]
fn rejects_unknown_variables() {
    expect_error(
        "sprite \"S\" {\n    costume \"c\" = \"assets/a.svg\";\n    event_whenflagclicked {\n        data_setvariableto(\"nope\", 1);\n    }\n}\n",
        "unknown variable `nope`",
    );
}

#[test]
fn rejects_undeclared_broadcasts() {
    expect_error(
        "sprite \"S\" {\n    costume \"c\" = \"assets/a.svg\";\n    event_whenflagclicked {\n        event_broadcast(\"nope\");\n    }\n}\n",
        "unknown broadcast message `nope`",
    );
}

#[test]
fn rejects_blocks_after_a_cap() {
    expect_error(
        "sprite \"S\" {\n    costume \"c\" = \"assets/a.svg\";\n    event_whenflagclicked {\n        control_forever { }\n        looks_say(\"never\");\n    }\n}\n",
        "nothing can follow `control_forever`",
    );
}

#[test]
fn rejects_bodyless_loops() {
    expect_error(
        "sprite \"S\" {\n    costume \"c\" = \"assets/a.svg\";\n    event_whenflagclicked {\n        control_repeat(4);\n    }\n}\n",
        "needs a body",
    );
}

#[test]
fn rejects_non_boolean_conditions() {
    expect_error(
        "sprite \"S\" {\n    costume \"c\" = \"assets/a.svg\";\n    event_whenflagclicked {\n        control_if(1) { }\n    }\n}\n",
        "expected a condition",
    );
}

#[test]
fn rejects_a_script_without_a_hat() {
    expect_error(
        "sprite \"S\" {\n    costume \"c\" = \"assets/a.svg\";\n    motion_movesteps(10);\n}\n",
        "must be a hat block",
    );
}

// ---------------------------------------------------------------------------
// Multi-file projects
// ---------------------------------------------------------------------------

#[test]
fn modules_are_inlined_into_each_using_target() {
    let project = TempProject::new("modules");
    project
        .write(
            "raven-asm.toml",
            "[project]\nname = \"m\"\n\n[targets]\nstage = \"src/stage.rasm\"\nsprites = [\"src/a.rasm\", \"src/b.rasm\"]\n",
        )
        .write(
            "src/stage.rasm",
            "stage {\n    costume \"b\" = \"assets/a.svg\";\n}\n",
        )
        .write("assets/a.svg", "<svg width=\"10\" height=\"10\"></svg>")
        .write(
            "src/lib/shapes.rasm",
            "proc square(size: num) warp {\n    control_repeat(4) { motion_movesteps(argument_reporter_string_number(\"size\")); }\n}\n",
        )
        .write(
            "src/a.rasm",
            "use \"lib/shapes.rasm\";\n\nsprite \"A\" {\n    costume \"c\" = \"assets/a.svg\";\n    event_whenflagclicked {\n        square(10);\n    }\n}\n",
        )
        .write(
            "src/b.rasm",
            "use \"lib/shapes\";\n\nsprite \"B\" {\n    costume \"c\" = \"assets/a.svg\";\n    event_whenflagclicked {\n        square(20);\n    }\n}\n",
        );

    let out = project.build().expect("modules build");
    let json = serde_json::to_value(&out.project).expect("serialize");
    for name in ["A", "B"] {
        let target = find_target(&json, name);
        let ops = opcodes(target);
        assert!(
            ops.iter().any(|o| o == "procedures_definition"),
            "{name} should have the inlined procedure"
        );
        assert!(ops.iter().any(|o| o == "procedures_call"));
    }
}

#[test]
fn rejects_module_cycles() {
    let project = TempProject::new("cycle");
    project
        .write(
            "raven-asm.toml",
            "[project]\nname = \"c\"\n\n[targets]\nstage = \"src/stage.rasm\"\n",
        )
        .write(
            "src/stage.rasm",
            "use \"lib/a.rasm\";\n\nstage {\n    costume \"b\" = \"assets/a.svg\";\n}\n",
        )
        .write("assets/a.svg", "<svg width=\"10\" height=\"10\"></svg>")
        .write("src/lib/a.rasm", "use \"b.rasm\";\n\nproc a() { }\n")
        .write("src/lib/b.rasm", "use \"a.rasm\";\n\nproc b() { }\n");

    let err = expect_build_error(&project, "cycles must be rejected");
    assert!(
        err.render().contains("circular module inclusion"),
        "{}",
        err.render()
    );
}

#[test]
fn rejects_declarations_in_a_module() {
    let project = TempProject::new("module-decl");
    project
        .write(
            "raven-asm.toml",
            "[project]\nname = \"m\"\n\n[targets]\nstage = \"src/stage.rasm\"\n",
        )
        .write(
            "src/stage.rasm",
            "use \"lib/a.rasm\";\n\nstage {\n    costume \"b\" = \"assets/a.svg\";\n}\n",
        )
        .write("assets/a.svg", "<svg width=\"10\" height=\"10\"></svg>")
        .write("src/lib/a.rasm", "var shared = 0;\n");

    let err = expect_build_error(&project, "modules may only hold procedures");
    assert!(
        err.render().contains("cannot declare a variable"),
        "{}",
        err.render()
    );
}

#[test]
fn sprite_blocks_can_read_stage_variables() {
    let project = TempProject::new("globals");
    project
        .write(
            "raven-asm.toml",
            "[project]\nname = \"g\"\n\n[targets]\nstage = \"src/stage.rasm\"\nsprites = [\"src/s.rasm\"]\n",
        )
        .write(
            "src/stage.rasm",
            "stage {\n    costume \"b\" = \"assets/a.svg\";\n    var high = 0;\n}\n",
        )
        .write("assets/a.svg", "<svg width=\"10\" height=\"10\"></svg>")
        .write(
            "src/s.rasm",
            "sprite \"S\" {\n    costume \"c\" = \"assets/a.svg\";\n    event_whenflagclicked {\n        data_setvariableto(\"high\", 1);\n    }\n}\n",
        );

    let out = project.build().expect("globals build");
    let stage_id = out
        .project
        .targets
        .iter()
        .find(|t| t.is_stage)
        .and_then(|t| t.variables.keys().next().cloned())
        .expect("stage variable id");
    let sprite = serde_json::to_value(&out.project).expect("serialize");
    let sprite = find_target(&sprite, "S");
    let uses_global = blocks_of(sprite).iter().any(|b| {
        b["fields"]["VARIABLE"]
            .as_array()
            .and_then(|a| a.get(1))
            .and_then(Value::as_str)
            == Some(stage_id.as_str())
    });
    assert!(
        uses_global,
        "the sprite must reference the stage variable id"
    );
}

#[test]
fn a_sprite_may_not_read_another_sprites_variable() {
    let project = TempProject::new("private");
    project
        .write(
            "raven-asm.toml",
            "[project]\nname = \"p\"\n\n[targets]\nstage = \"src/stage.rasm\"\nsprites = [\"src/a.rasm\", \"src/b.rasm\"]\n",
        )
        .write(
            "src/stage.rasm",
            "stage {\n    costume \"b\" = \"assets/a.svg\";\n}\n",
        )
        .write("assets/a.svg", "<svg width=\"10\" height=\"10\"></svg>")
        .write(
            "src/a.rasm",
            "sprite \"A\" {\n    costume \"c\" = \"assets/a.svg\";\n    var secret = 1;\n}\n",
        )
        .write(
            "src/b.rasm",
            "sprite \"B\" {\n    costume \"c\" = \"assets/a.svg\";\n    event_whenflagclicked {\n        data_setvariableto(\"secret\", 1);\n    }\n}\n",
        );

    let err = expect_build_error(&project, "sprite-local variables stay private");
    assert!(
        err.render().contains("unknown variable `secret`"),
        "{}",
        err.render()
    );
}

// ---------------------------------------------------------------------------
// Scope: who owns a variable
// ---------------------------------------------------------------------------

/// A module's `global` declarations land on the stage and mean the same thing to
/// every sprite that uses the module — the property that makes sharing safe.
#[test]
fn a_modules_global_belongs_to_the_stage_and_is_shared() {
    let project = TempProject::new("module-global");
    project
        .write(
            "raven-asm.toml",
            "[project]\nname = \"g\"\n\n[targets]\nstage = \"src/stage.rasm\"\nsprites = [\"src/a.rasm\", \"src/b.rasm\"]\n",
        )
        .write(
            "src/stage.rasm",
            "stage {\n    costume \"b\" = \"assets/a.svg\";\n}\n",
        )
        .write("assets/a.svg", "<svg width=\"10\" height=\"10\"></svg>")
        .write(
            "src/lib/stats.rasm",
            r#"global var runs = 0;
global list log = [];

proc record(turns: num) {
    data_changevariableby("runs", 1);
    data_addtolist(argument_reporter_string_number("turns"), "log");
}
"#,
        )
        .write(
            "src/a.rasm",
            "use \"lib/stats.rasm\";\n\nsprite \"A\" {\n    costume \"c\" = \"assets/a.svg\";\n    event_whenflagclicked { record(1); }\n}\n",
        )
        .write(
            "src/b.rasm",
            "use \"lib/stats.rasm\";\n\nsprite \"B\" {\n    costume \"c\" = \"assets/a.svg\";\n    event_whenflagclicked { record(2); }\n}\n",
        );

    let out = project.build().expect("module globals must build");
    let json = serde_json::to_value(&out.project).expect("serialize");
    let stage = find_target(&json, "Stage");

    // Declared in the module, owned by the stage, exactly once.
    let stage_vars = object(&stage["variables"]);
    assert_eq!(stage_vars.len(), 1);
    assert_eq!(stage_vars.values().next().unwrap()[0], "runs");
    let stage_lists = object(&stage["lists"]);
    assert_eq!(stage_lists.len(), 1);
    assert_eq!(stage_lists.values().next().unwrap()[0], "log");
    let runs_id = stage_vars.keys().next().unwrap().clone();

    // Neither sprite keeps a copy, and both bind to the one on the stage.
    for name in ["A", "B"] {
        let sprite = find_target(&json, name);
        assert!(
            object(&sprite["variables"]).is_empty(),
            "{name} should not own the module's global"
        );
        let binds = blocks_of(sprite).iter().any(|b| {
            b["opcode"] == "data_changevariableby"
                && b["fields"]["VARIABLE"][1].as_str() == Some(runs_id.as_str())
        });
        assert!(binds, "{name} must bind `runs` to the stage's variable");
    }

    // One monitor for the global, owned by the stage, plus none for the sprites.
    assert_eq!(out.project.monitors.len(), 2);
    assert!(out.project.monitors.iter().all(|m| m.sprite_name.is_none()));
}

/// The second half of the rule: a module may borrow a name it does not declare,
/// and the target that pulled it in owns it.
#[test]
fn a_module_may_reach_a_variable_its_user_declares() {
    let project = TempProject::new("module-borrow");
    project
        .write(
            "raven-asm.toml",
            "[project]\nname = \"b\"\n\n[targets]\nstage = \"src/stage.rasm\"\nsprites = [\"src/a.rasm\"]\n",
        )
        .write(
            "src/stage.rasm",
            "stage {\n    costume \"b\" = \"assets/a.svg\";\n}\n",
        )
        .write("assets/a.svg", "<svg width=\"10\" height=\"10\"></svg>")
        .write(
            "src/lib/m.rasm",
            "proc bump() {\n    data_changevariableby(\"local_one\", 1);\n}\n",
        )
        .write(
            "src/a.rasm",
            "use \"lib/m.rasm\";\n\nsprite \"A\" {\n    costume \"c\" = \"assets/a.svg\";\n    var local_one = 5;\n    event_whenflagclicked { bump(); }\n}\n",
        );

    let out = project.build().expect("borrowed locals must build");
    let json = serde_json::to_value(&out.project).expect("serialize");
    let sprite = find_target(&json, "A");
    let local_id = object(&sprite["variables"]).keys().next().unwrap().clone();
    assert_eq!(object(&sprite["variables"])[&local_id][1], 5);
    let binds_local = blocks_of(sprite).iter().any(|b| {
        b["opcode"] == "data_changevariableby"
            && b["fields"]["VARIABLE"][1].as_str() == Some(local_id.as_str())
    });
    assert!(
        binds_local,
        "the module's copy should bind the sprite's local"
    );
}

#[test]
fn a_sprite_can_declare_a_global() {
    let project = TempProject::new("sprite-global");
    project
        .write(
            "raven-asm.toml",
            "[project]\nname = \"g\"\n\n[targets]\nstage = \"src/stage.rasm\"\nsprites = [\"src/a.rasm\"]\n",
        )
        .write(
            "src/stage.rasm",
            "stage {\n    costume \"b\" = \"assets/a.svg\";\n}\n",
        )
        .write("assets/a.svg", "<svg width=\"10\" height=\"10\"></svg>")
        .write(
            "src/a.rasm",
            "sprite \"A\" {\n    costume \"c\" = \"assets/a.svg\";\n    global var shared = 7;\n}\n",
        );

    let out = project.build().expect("a sprite may declare a global");
    let json = serde_json::to_value(&out.project).expect("serialize");
    let stage = find_target(&json, "Stage");
    assert_eq!(object(&stage["variables"]).len(), 1);
    assert_eq!(
        object(&stage["variables"]).values().next().unwrap()[0],
        "shared"
    );
    // The sprite that wrote it does not also own a copy.
    assert!(object(&find_target(&json, "A")["variables"]).is_empty());
}

#[test]
fn a_module_cannot_declare_a_target_owned_variable() {
    let project = TempProject::new("module-var-error");
    project
        .write(
            "raven-asm.toml",
            "[project]\nname = \"e\"\n\n[targets]\nstage = \"src/stage.rasm\"\n",
        )
        .write(
            "src/stage.rasm",
            "use \"lib/m.rasm\";\n\nstage {\n    costume \"b\" = \"assets/a.svg\";\n}\n",
        )
        .write("assets/a.svg", "<svg width=\"10\" height=\"10\"></svg>")
        .write("src/lib/m.rasm", "var nope = 0;\n");

    let err = expect_build_error(&project, "a module cannot own a plain variable");
    let rendered = err.render();
    assert!(
        rendered.contains("a module cannot declare a variable"),
        "{rendered}"
    );
    assert!(rendered.contains("global var"), "{rendered}");
}

#[test]
fn a_local_may_not_shadow_a_global() {
    let project = TempProject::new("shadow");
    project
        .write(
            "raven-asm.toml",
            "[project]\nname = \"s\"\n\n[targets]\nstage = \"src/stage.rasm\"\nsprites = [\"src/a.rasm\"]\n",
        )
        .write(
            "src/stage.rasm",
            "stage {\n    costume \"b\" = \"assets/a.svg\";\n    var shared = 0;\n}\n",
        )
        .write("assets/a.svg", "<svg width=\"10\" height=\"10\"></svg>")
        .write(
            "src/a.rasm",
            "sprite \"A\" {\n    costume \"c\" = \"assets/a.svg\";\n    var shared = 1;\n}\n",
        );

    let err = expect_build_error(&project, "shadowing must be refused");
    assert!(err.render().contains("would shadow it"), "{}", err.render());
}

#[test]
fn two_globals_may_not_share_a_name() {
    let project = TempProject::new("dup-global");
    project
        .write(
            "raven-asm.toml",
            "[project]\nname = \"d\"\n\n[targets]\nstage = \"src/stage.rasm\"\nsprites = [\"src/a.rasm\", \"src/b.rasm\"]\n",
        )
        .write(
            "src/stage.rasm",
            "stage {\n    costume \"b\" = \"assets/a.svg\";\n}\n",
        )
        .write("assets/a.svg", "<svg width=\"10\" height=\"10\"></svg>")
        .write(
            "src/a.rasm",
            "sprite \"A\" {\n    costume \"c\" = \"assets/a.svg\";\n    global var dup = 1;\n}\n",
        )
        .write(
            "src/b.rasm",
            "sprite \"B\" {\n    costume \"c\" = \"assets/a.svg\";\n    global var dup = 2;\n}\n",
        );

    let err = expect_build_error(&project, "duplicate globals must be refused");
    assert!(
        err.render().contains("duplicate global variable `dup`"),
        "{}",
        err.render()
    );
}
// ---------------------------------------------------------------------------
// Initial values keep their type
// ---------------------------------------------------------------------------

#[test]
fn initial_values_and_monitors_keep_their_json_types() {
    let project = TempProject::new("literal-types");
    project
        .write(
            "raven-asm.toml",
            "[project]\nname = \"t\"\n\n[targets]\nstage = \"src/stage.rasm\"\n",
        )
        .write(
            "src/stage.rasm",
            r#"stage {
    costume "b" = "assets/a.svg";
    var n = 1;
    var score = 1.50;
    var flag = false;
    var label = "hi";
    list xs = [1, "a", true];
}
"#,
        )
        .write("assets/a.svg", "<svg width=\"10\" height=\"10\"></svg>");

    let out = project.build().expect("build");
    let json = serde_json::to_value(&out.project).expect("serialize");
    let stage = find_target(&json, "Stage");
    let vars = object(&stage["variables"]);
    let var = |name: &str| vars.values().find(|v| v[0] == name).expect("variable");

    assert_eq!(var("n")[1], 1);
    assert!(var("n")[1].is_number(), "a number literal stays a number");
    assert_eq!(var("score")[1], 1.5);
    assert_eq!(var("flag")[1], false);
    assert!(var("flag")[1].is_boolean());
    assert_eq!(var("label")[1], "hi");
    assert!(var("label")[1].is_string());

    let xs = object(&stage["lists"]).values().next().expect("list");
    assert_eq!(xs[1], serde_json::json!([1, "a", true]));

    // Monitors mirror the initial value, so the editor shows a number, not text.
    let monitor = |name: &str| {
        out.project
            .monitors
            .iter()
            .find(|m| m.params.values().any(|v| v == name))
            .unwrap_or_else(|| panic!("monitor for {name}"))
    };
    assert_eq!(monitor("n").value, 1);
    assert!(monitor("n").value.is_number());
    assert_eq!(monitor("flag").value, false);
    assert_eq!(monitor("xs").value, serde_json::json!([1, "a", true]));
}
