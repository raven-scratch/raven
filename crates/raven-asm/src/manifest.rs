//! The `raven-asm.toml` project manifest.

use raven_scratch::diag::{Diag, Error, Result, Source};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The manifest file name, re-exported so call sites name one thing.
pub const MANIFEST_NAME: &str = crate::identity::MANIFEST;

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub project: ProjectSection,
    #[serde(default)]
    pub targets: TargetsSection,
}

#[derive(Debug, Deserialize)]
pub struct ProjectSection {
    pub name: String,
    /// Directory (or file path) the `.sb3` is written to. Defaults to `dist`.
    #[serde(default)]
    pub output: Option<String>,
    /// Scratch extensions the project uses (`pen`, `music`).
    #[serde(default)]
    pub extensions: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct TargetsSection {
    pub stage: String,
    #[serde(default)]
    pub sprites: Vec<String>,
}

impl Manifest {
    /// Load and validate a manifest. Paths inside are left unresolved; use
    /// [`Manifest::resolve`] with the manifest directory.
    pub fn load(path: &Path) -> Result<Manifest> {
        let text = std::fs::read_to_string(path).map_err(|e| {
            Error::new(Diag::error(format!("cannot read `{}`", path.display())).note(e.to_string()))
        })?;
        let src = Source::new(path, text);
        Self::parse(&src)
    }

    pub fn parse(src: &Source) -> Result<Manifest> {
        let manifest: Manifest = toml::from_str(&src.text).map_err(|e| {
            // Turn the TOML byte offset into a line/column so the diagnostic
            // carries the offending line and its caret.
            let offset = e.span().map(|s| s.start).unwrap_or(0).min(src.text.len());
            let before = &src.text[..offset];
            let line = before.matches('\n').count() as u32 + 1;
            let line_start = before.rfind('\n').map_or(0, |i| i + 1);
            let col = (offset - line_start) as u32 + 1;
            Error::new(
                src.error(raven_scratch::diag::Pos::new(line, col), "invalid manifest")
                    .note(e.to_string())
                    .note(
                        "expected keys: [project] name/output/extensions, [targets] stage/sprites",
                    ),
            )
        })?;

        if manifest.project.name.trim().is_empty() {
            return Err(Error::new(src.error(
                raven_scratch::diag::Pos::new(1, 1),
                "`project.name` must not be empty",
            )));
        }
        if manifest.targets.stage.trim().is_empty() {
            return Err(Error::new(
                src.error(
                    raven_scratch::diag::Pos::new(1, 1),
                    "`targets.stage` is required",
                )
                .note("point it at the file that declares `stage { ... }`"),
            ));
        }

        for ext in &manifest.project.extensions {
            if !matches!(ext.as_str(), "pen" | "music") {
                return Err(Error::new(
                    src.error(
                        raven_scratch::diag::Pos::new(1, 1),
                        format!("unsupported extension `{ext}`"),
                    )
                    .note("raven-asm bundles the `pen` and `music` extensions"),
                ));
            }
        }

        Ok(manifest)
    }

    /// Resolve every declared path against the directory holding the manifest.
    pub fn stage_path(&self, root: &Path) -> PathBuf {
        root.join(&self.targets.stage)
    }

    pub fn sprite_paths(&self, root: &Path) -> Vec<PathBuf> {
        self.targets.sprites.iter().map(|p| root.join(p)).collect()
    }

    /// The directory the `.sb3` is written to, resolved against `root`.
    pub fn output_dir(&self, root: &Path) -> PathBuf {
        let raw = self.project.output.as_deref().unwrap_or("dist");
        let joined = root.join(raw);
        match joined.extension().and_then(|e| e.to_str()) {
            Some("sb3") => joined
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| root.to_path_buf()),
            _ => joined,
        }
    }

    /// The file name of the produced archive.
    ///
    /// A declared `output = "out/foo.sb3"` is authoritative: the archive is
    /// written to `out/` with that exact file name. Otherwise the file is named
    /// after the project.
    pub fn output_file(&self) -> String {
        if let Some(raw) = self.project.output.as_deref() {
            if Path::new(raw).extension().and_then(|e| e.to_str()) == Some("sb3") {
                if let Some(name) = Path::new(raw).file_name() {
                    return name.to_string_lossy().to_string();
                }
            }
        }
        format!("{}.sb3", sanitize_name(&self.project.name))
    }
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Manifest> {
        Manifest::parse(&Source::new("raven-asm.toml", text))
    }

    #[test]
    fn parses_a_minimal_manifest() {
        let m = parse(
            r#"
[project]
name = "demo"

[targets]
stage = "src/stage.rasm"
sprites = ["src/a.rasm", "src/b.rasm"]
"#,
        )
        .unwrap();
        assert_eq!(m.project.name, "demo");
        assert_eq!(m.targets.sprites.len(), 2);
        assert_eq!(m.output_file(), "demo.sb3");
    }

    #[test]
    fn rejects_unknown_extensions() {
        let err = parse(
            r#"
[project]
name = "demo"
extensions = ["videoSensing"]

[targets]
stage = "src/stage.rasm"
"#,
        )
        .unwrap_err();
        assert!(err.render().contains("unsupported extension"));
    }

    #[test]
    fn rejects_missing_stage() {
        let err = parse("[project]\nname = \"demo\"\n").unwrap_err();
        assert!(err.render().contains("targets.stage"));
    }

    #[test]
    fn sanitizes_output_names() {
        let m = parse("[project]\nname = \"a/b\"\n\n[targets]\nstage = \"s.rasm\"\n").unwrap();
        assert_eq!(m.output_file(), "a_b.sb3");
    }

    #[test]
    fn a_declared_output_path_is_authoritative() {
        let root = Path::new("proj");
        let declared = parse(
            "[project]\nname = \"demo\"\noutput = \"out/foo.sb3\"\n\n[targets]\nstage = \"s.rasm\"\n",
        )
        .unwrap();
        assert_eq!(declared.output_file(), "foo.sb3");
        assert_eq!(declared.output_dir(root), root.join("out"));

        // Without `output`, the file is named after the project in `dist`.
        let default =
            parse("[project]\nname = \"demo\"\n\n[targets]\nstage = \"s.rasm\"\n").unwrap();
        assert_eq!(default.output_file(), "demo.sb3");
        assert_eq!(default.output_dir(root), root.join("dist"));
    }

    #[test]
    fn a_malformed_manifest_keeps_its_location() {
        let src = Source::new(
            "raven-asm.toml",
            "[project]\nname = \"demo\"\nbad = \n\n[targets]\nstage = \"s.rasm\"\n",
        );
        let err = Manifest::parse(&src).unwrap_err();
        let rendered = err.render();
        assert!(rendered.contains("invalid manifest"), "{rendered}");
        assert!(
            rendered.contains("raven-asm.toml"),
            "the error should name the manifest:\n{rendered}"
        );
        assert!(
            rendered.contains("-->"),
            "the error should carry a source location:\n{rendered}"
        );
    }
}
