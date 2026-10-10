//! The layer below raven: raven-asm, the optimiser over it, and the files they
//! read and write.
//!
//! Built rather than written as literals, so the manifest name, the source
//! extension and the output directory come from `raven_asm::identity` rather
//! than from a retyped string.

use raven_asm::identity as asm;

/// `asm` — what raven-asm is and what it promises.
pub fn asm_overview() -> String {
    format!(
        "\n## asm\n\
         # raven-asm is the middle layer: a language with exactly one statement per\n\
         # Scratch block, which raven lowers to and which the .sb3 is compiled from.\n\
         #\n\
         #   raven (.rav)  ->  raven-asm (.rasm)  ->  project.json  ->  .sb3\n\
         #\n\
         # # The promise\n\
         #\n\
         # One statement is one Scratch block, always. The compiler never rewrites and\n\
         # never folds: what a file says is what the editor shows, and a reader can count\n\
         # the blocks. That is why a convenience that needs several blocks belongs in\n\
         # raven, as a macro, rather than here.\n\
         #\n\
         # Folding is possible, but it is a separate program over a separate input:\n\
         # {opt} maps a raven-asm project to an optimised raven-asm project. See the\n\
         # `asm-optimizer` page. {crate} runs it as a step of its own build.\n\
         #\n\
         # # Who writes it\n\
         #\n\
         #   raven build --debug   writes the lowered project to {out}/asm/, complete with\n\
         #                         its own {manifest} and relative asset paths\n\
         #   raven expand          prints the same raven-asm to standard output\n\
         #   a person              `{crate} new` scaffolds one and `{crate} build` compiles\n\
         #   raven-re              reverses a vanilla .sb3 back into a raven-asm project\n\
         #\n\
         # A raven-asm project is therefore a real, checkable artefact: `{crate} check`\n\
         # validates a lowered project exactly as it validates a hand-written one, and\n\
         # `{crate} build` compiles it to the same archive raven built.\n\
         #\n\
         # # The block reference\n\
         #\n\
         # Every opcode raven-asm can name, with its arguments, is in the catalog:\n\
         #\n\
         #   {crate} catalog              one line per block\n\
         #   {crate} catalog --json       the same, as data, with shapes and fields\n\
         #   {crate} catalog --markdown   the block reference the documentation ships\n\
         #   {crate} catalog -c motion    only blocks in a category\n\
         #\n\
         # docs: {docs}\n",
        crate = asm::CRATE,
        opt = "raven-opt",
        manifest = asm::MANIFEST,
        out = asm::DEFAULT_OUTPUT_DIR,
        docs = asm::DOCS,
    )
}

/// `asm-syntax` — lexical rules, items, statements and expressions.
pub fn asm_syntax() -> String {
    format!(
        "\n## asm-syntax\n\
         # # Lexical rules\n\
         #\n\
         #   comments      // to end of line, and /* block comments */\n\
         #   identifiers   letters, digits and `_`, not starting with a digit\n\
         #   numbers       integer and decimal, negative as well\n\
         #   strings       \"…\" with the usual escapes\n\
         #   booleans      true, false\n\
         #   whitespace    insignificant\n\
         #   termination   every statement ends with `;`, or opens a substack with `{{ … }}`\n\
         #\n\
         # # A file\n\
         #\n\
         # A .{ext} file holds `use` declarations first, then EITHER one target block OR\n\
         # top-level items. A file may not do both: after a target declaration no further\n\
         # top-level item is allowed, and a file may not declare two targets. A file that\n\
         # is completely empty is an error.\n\
         #\n\
         #   use \"lib/shapes\";\n\
         #\n\
         #   stage {{\n\
         #       var best = 0;\n\
         #       costume \"backdrop1\" = \"assets/backdrop1.svg\";\n\
         #       event_whenflagclicked {{ }}\n\
         #   }}\n\
         #\n\
         #   sprite \"Player\" {{\n\
         #       var score = 0;\n\
         #       costume \"idle\" = \"assets/idle.svg\";\n\
         #       proc move(to: num) {{ motion_gotoxy(0, 0); motion_glidesecstoxy(to, 1); }}\n\
         #       event_whenflagclicked {{ move(50); }}\n\
         #   }}\n\
         #\n\
         # `use` takes a STRING, not an identifier: `use \"lib/shapes\";`. The path names a\n\
         # module file without its extension.\n\
         #\n\
         # # Statements\n\
         #\n\
         # A statement is an opcode and its arguments. There are no statement keywords: the\n\
         # opcode IS the statement.\n\
         #\n\
         #   <opcode> ( <expr>, … ) ;                          one block\n\
         #   <opcode> ( <expr>, … ) {{ … }}                     a substack\n\
         #   <opcode> ( <expr>, … ) {{ … }} else {{ … }}        two substacks\n\
         #\n\
         #   data_setvariableto(\"score\", 0);\n\
         #   data_changevariableby(\"score\", 1);\n\
         #   data_addtolist(1, \"trail\");\n\
         #   control_if(operator_gt(data_variable(\"score\"), 2)) {{\n\
         #       looks_say(\"done\");\n\
         #   }}\n\
         #   control_if_else(operator_lt(1, 2)) {{ }} else {{ }}\n\
         #   control_repeat(10) {{ motion_movesteps(1); }}\n\
         #   event_whenflagclicked {{ }}\n\
         #\n\
         # A hat is a statement whose opcode is a hat opcode; it is written the same way.\n\
         # `else` is accepted after any block by the parser and rejected by the compiler\n\
         # unless the opcode takes one.\n\
         #\n\
         # # Expressions\n\
         #\n\
         # An argument is one of four things and nothing else:\n\
         #\n\
         #   <number>          a literal, written as it appears in the project\n\
         #   \"<text>\"         a string literal\n\
         #   true | false      a boolean literal\n\
         #   <opcode>( args )  a reporter or a boolean block, nested\n\
         #\n\
         # A bare identifier is NOT a value. `x` alone is an error whose note says to\n\
         # write `data_variable(\"x\")` to read a variable and `x(...)` to call a block.\n\
         # Menus are literals too: the dropdown value is written as the string the block\n\
         # stores, so `motion_pointindirection(90)` and the key dropdown\n\
         # `event_whenkeypressed(\"space\")` are both just arguments.\n\
         #\n\
         # There is no operator precedence to remember because there are no operators:\n\
         # every level of the expression is an explicit call, and the nesting IS the tree.\n\
         #\n\
         # # Declarations\n\
         #\n\
         #   var name = <literal> ;\n\
         #   list name = [ <literal>, … ] ;\n\
         #   broadcast \"name\" ;\n\
         #   costume \"name\" = \"path\" [ center <num> <num> ] ;\n\
         #   sound \"name\" = \"path\" ;\n\
         #   proc name ( p: str|num|bool, … ) [ warp ] {{ … }}\n\
         #\n\
         # Two modifiers precede a declaration, in this order:\n\
         #\n\
         #   global   the declaration belongs to the stage, so every sprite can reach it\n\
         #   visible  the declaration's monitor starts shown rather than hidden\n\
         #\n\
         #   visible var score = 0;\n\
         #   visible list trail = [];\n\
         #   var speed = 10;\n\
         #   global var games = 0;\n\
         #   global visible var best = 0;\n\
         #\n\
         # A declaration's starting value is a literal: not an expression, not a call.\n\
         #\n\
         # # The monitor clause\n\
         #\n\
         # Written AFTER the declaration's semicolon, in the editor's own words. It\n\
         # describes the monitor; it is not a value.\n\
         #\n\
         #   at <x> <y>            where the monitor sits, in stage pixels\n\
         #   large                 the large readout, mode: \"large\"\n\
         #   slider <min> <max>    a slider between two bounds, mode: \"slider\"\n\
         #   continuous            a slider that steps by 0.01 instead of 1\n\
         #   default               the ordinary readout, when a later clause would override it\n\
         #\n\
         #   visible var power = 0; slider 0 10\n\
         #   visible var fine = 0; slider 0 1 continuous\n\
         #   var speed = 10; at 5 30\n\
         #\n\
         # A declaration that says none of them still gets a position: monitors are\n\
         # stacked down the left edge of the stage, a readout every 38 stage pixels and a\n\
         # list every 205, so two never land on top of each other.\n",
        ext = asm::SOURCE_EXTENSION,
    )
}

/// `asm-procedures` — definitions, calls, parameters and warp.
pub fn asm_procedures() -> String {
    "\n## asm-procedures\n\
     # # Defining\n\
     #\n\
     #   proc name ( p: str, q: num, r: bool, … ) [ warp ] { … }\n\
     #\n\
     #   proc zigzag(degrees: num, steps: num) warp {\n\
     #       motion_turnright(degrees);\n\
     #       motion_movesteps(steps);\n\
     #   }\n\
     #\n\
     # Every parameter carries its type and the type is one of three, because those are\n\
     # Scratch's own input shapes:\n\
     #\n\
     #   : str    %s in the prototype, read with argument_reporter_string_number\n\
     #   : num    %n in the prototype, read with argument_reporter_string_number\n\
     #   : bool   %b in the prototype, read with argument_reporter_boolean\n\
     #\n\
     # A parameter without a type is refused, and the note says so: an untyped parameter\n\
     # would mean `str` by omission, which is exactly the sort of thing a reader cannot\n\
     # see. A duplicate parameter name is refused too. Inside the body the parameter is\n\
     # written as `argument_reporter_string_number(\"q\")` (or the boolean reporter),\n\
     # which is the block the type chose.\n\
     #\n\
     # `warp` goes after the parameter list and before the body, and means the custom\n\
     # block runs without screen refresh.\n\
     #\n\
     # # Calling\n\
     #\n\
     # A call is the procedure's BARE NAME, with the arguments:\n\
     #\n\
     #   zigzag(90, 10);\n\
     #   draw_square(80);\n\
     #\n\
     # A call may open a substack with `{ … }` when the procedure takes one. The\n\
     # compiler emits the procedures_call block with the matching mutation, including\n\
     # the parameter types.\n\
     #\n\
     # A procedure is emitted once per target that has it. One written in a module is\n\
     # copied into EVERY target that uses the module, because a Scratch custom block\n\
     # belongs to exactly one target and cannot be shared — the same rule raven follows,\n\
     # and the reason `raven expand` prints the copies.\n\
     #\n\
     # `return` does not exist in raven-asm: a Scratch custom block has no return\n\
     # value. A value a procedure produces is stored in a variable or a list by the\n\
     # procedure itself, and read by the caller afterwards.\n"
        .to_string()
}

/// `asm-variables` — ownership, naming, lists and broadcasts.
pub fn asm_variables() -> String {
    "\n## asm-variables\n\
     # # What a variable is\n\
     #\n\
     # `var name = value;` declares a real Scratch variable, reached by that name.\n\
     # Reading one is the reporter block `data_variable(\"name\")`; writing one is\n\
     # `data_setvariableto(\"name\", value)` or `data_changevariableby(\"name\", n)`,\n\
     # exactly as in the editor. Unlike raven, raven-asm has variables and says so.\n\
     #\n\
     #   var speed = 10; at 5 30\n\
     #\n\
     # # Ownership\n\
     #\n\
     # Scratch has two scopes, and raven-asm writes the second one explicitly:\n\
     #\n\
     #   var score = 0;              in a sprite file   for this sprite only\n\
     #   var best = 0;               in the stage file  for all sprites\n\
     #   global var best = 0;        in ANY file        for all sprites\n\
     #\n\
     # The rule is about ownership, not about which file the declaration is in: a\n\
     # `global var` is placed on the stage wherever it is written, including in a sprite\n\
     # and including in a module, so there is exactly one of it for the whole project.\n\
     # That is what lets a spriteless module declare project-wide state.\n\
     #\n\
     # A sprite that mentions a variable neither it nor the stage declares is a compile\n\
     # error. SHADOWING IS REJECTED: a sprite may not declare a variable with the same\n\
     # name as a global, even though Scratch permits it, because two variables with one\n\
     # name is a bug waiting to happen. Project-wide names must also be unique across\n\
     # files, so `global var dup` in two files is an error rather than a silent merge.\n\
     #\n\
     # # Lists\n\
     #\n\
     #   list trail = [];\n\
     #   list options = [\"a\", \"b\", \"c\"];\n\
     #\n\
     # Lists share the scoping rules exactly, and starting items must be literals. The\n\
     # list blocks all take the list NAME as their last argument, because the Scratch\n\
     # block does:\n\
     #\n\
     #   data_addtolist(item, \"trail\")            add (item) to (trail)\n\
     #   data_insertatlist(item, 1, \"trail\")      insert (item) at (1) of (trail)\n\
     #   data_replaceitemoflist(1, \"trail\", v)    replace item (1) of (trail) with (v)\n\
     #   data_deleteoflist(1, \"trail\")            delete (1) of (trail)\n\
     #   data_deletealloflist(\"trail\")            delete all of (trail)\n\
     #   data_itemoflist(1, \"trail\")              item (1) of (trail)\n\
     #   data_lengthoflist(\"trail\")               length of (trail)\n\
     #   data_listcontainsitem(\"trail\", \"x\")     (trail) contains (x)?\n\
     #\n\
     # Note the argument order follows the block's phrasing: add takes item then list,\n\
     # because the block reads \"add (thing) to (list)\".\n\
     #\n\
     # # Broadcasts\n\
     #\n\
     # Broadcast messages are project-wide. Declare one anywhere — the compiler stores it\n\
     # on the stage, which is where Scratch keeps them — then send and receive it:\n\
     #\n\
     #   broadcast \"reset\";\n\
     #\n\
     #   event_whenbroadcastreceived(\"reset\") { data_setvariableto(\"score\", 0); }\n\
     #   event_broadcast(\"reset\");\n\
     #   event_broadcastandwait(\"reset\");\n\
     #\n\
     # Using a message that was never declared is an error, with a suggestion when an\n\
     # existing one is a near miss.\n\
     #\n\
     # # Monitors\n\
     #\n\
     # raven-asm writes a monitor record for every variable and list, hidden by default.\n\
     # `data_showvariable`, `data_hidevariable`, `data_showlist` and `data_hidelist`\n\
     # toggle it as in the editor, and `visible` in the declaration makes it start\n\
     # shown. A hidden monitor still exists, so the editor's checkbox works as expected.\n\
     #\n\
     # # Cloud variables\n\
     #\n\
     # Scratch cloud variables are ordinary variables whose name starts with the cloud\n\
     # character. raven-asm does not treat them specially: declaring one works and the\n\
     # project records it like any other. Whether it syncs depends on the player the\n\
     # project is loaded into.\n"
        .to_string()
}

/// `asm-assets` — costumes, sounds and rotation centres.
pub fn asm_assets() -> String {
    "\n## asm-assets\n\
     # # Declaring\n\
     #\n\
     #   costume \"costume1\" = \"assets/player-idle.svg\" center 32 32;\n\
     #   costume \"costume2\" = \"assets/player-run.svg\";\n\
     #   sound \"jump\" = \"assets/jump.wav\";\n\
     #\n\
     # Asset paths are relative to the PROJECT ROOT, the directory holding the manifest,\n\
     # so `assets/…` means the same thing from every file. `center` takes two NUMBERS in\n\
     # raven-asm — the rotation centre in costume pixels.\n\
     #\n\
     # # Formats\n\
     #\n\
     #   costumes   .svg .png .jpg .jpeg .bmp .gif\n\
     #   sounds     .wav .mp3\n\
     #\n\
     # Anything else is a compile error naming the file and the line.\n\
     #\n\
     # # Costume size and rotation centre\n\
     #\n\
     # Scratch stores a rotation centre for every costume: the point the sprite rotates\n\
     # around. raven-asm works it out.\n\
     #\n\
     #   SVG                         from the root element's width and height, or its\n\
     #                               viewBox when those are percentages or missing\n\
     #   PNG, GIF, BMP, JPEG         from the image header, with the bytes checked\n\
     #                               against the extension, so a .png that is not a PNG\n\
     #                               is a build error rather than a project that fails\n\
     #                               to open\n\
     #   every costume               packed with bitmapResolution: 1, SVG included, so\n\
     #                               one image pixel is one stage unit and a project\n\
     #                               round-trips through the editor unchanged\n\
     #\n\
     # The centre defaults to the middle of the image. Override it when the sprite should\n\
     # pivot somewhere else:\n\
     #\n\
     #   costume \"sword\" = \"assets/sword.svg\" center 8 48;   // pivot at the hilt\n\
     #\n\
     # A size that cannot be determined — a hand-written SVG with no size attributes, for\n\
     # instance — is an ERROR and not a guess, because the rotation centre is what every\n\
     # motion block turns around. The note says to give the image a width and height, or\n\
     # a viewBox, or to place the costume's centre by hand.\n\
     #\n\
     # # Sounds\n\
     #\n\
     # A WAV's sample rate and frame count are read from its header and written into the\n\
     # project, which is what the Scratch audio engine reports after loading anyway. MP3\n\
     # carries no such metadata, so those fields are omitted.\n\
     #\n\
     # # How assets get into the archive\n\
     #\n\
     # A .sb3 is a ZIP, and Scratch names each asset after the MD5 of its contents.\n\
     # raven-asm hashes each file, writes the hash into project.json as the costume's or\n\
     # sound's assetId, and stores the bytes under <md5>.<format>. Nothing is re-encoded\n\
     # and nothing is uploaded: the files pointed at are the files that go in. Declaring\n\
     # the same file twice — two sprites sharing one costume — stores it once and lets\n\
     # both targets reference it.\n\
     #\n\
     # # The default assets\n\
     #\n\
     # `raven-asm new` writes two small starter SVGs rather than shipping binaries: a\n\
     # plain white 480x360 backdrop, and the raven-asm logo as the sprite's costume.\n\
     # They are ordinary assets and can be deleted or replaced.\n"
        .to_string()
}

/// `asm-project` — the manifest, the layout, and multi-file projects.
pub fn asm_project() -> String {
    format!(
        "\n## asm-project\n\
         # # The manifest\n\
         #\n\
         #   # {manifest}\n\
         #   [project]\n\
         #   name = \"demo\"            the output file name: <output>/<name>.sb3\n\
         #   output = \"dist\"          where the archive goes; defaults to \"{out}\"\n\
         #   extensions = []          Scratch extensions the project uses; \"pen\" and\n\
         #                            \"music\" are bundled\n\
         #\n\
         #   [targets]\n\
         #   stage = \"src/stage.rasm\"        exactly one stage file\n\
         #   sprites = [\"src/sprites/a.rasm\", …]   one file per sprite\n\
         #\n\
         # Paths in the manifest are relative to the manifest itself. The project name is\n\
         # the output file's stem and is also what `raven-asm new` names the directory.\n\
         #\n\
         # # The layout\n\
         #\n\
         #   {manifest}            the manifest\n\
         #   src/stage.rasm               the stage\n\
         #   src/sprites/<name>.rasm      one file per sprite, named after the sprite\n\
         #   src/lib/<module>.rasm        a module, imported with `use \"lib/<module>\"`\n\
         #   assets/…                     every costume and sound, named as the project\n\
         #                                uses it\n\
         #   {out}/<name>.sb3             the archive\n\
         #   {out}/project.json           with --debug: the uncompressed project\n\
         #\n\
         # A file declares a target (`stage {{ … }}` or `sprite \"Name\" {{ … }}`) or it\n\
         # declares no target and is a module. Both kinds may begin with `use`\n\
         # declarations.\n\
         #\n\
         # # use, and what an import duplicates\n\
         #\n\
         #   use \"lib/shapes\";\n\
         #\n\
         # A path names a module file without its extension. raven-asm has no per-item\n\
         # import: using a module puts its items in scope.\n\
         #\n\
         #   a procedure in a module is copied into EVERY target that uses it, because a\n\
         #     Scratch custom block belongs to exactly one target and cannot be shared\n\
         #   a global declaration is not copied: it belongs to the stage, so there is one\n\
         #     of it however many files name it\n\
         #\n\
         # That is the same rule raven has, and the same reason `raven expand` prints the\n\
         # copies.\n\
         #\n\
         # # Checking a project\n\
         #\n\
         #   {crate} check                 parse and validate; writes nothing\n\
         #   {crate} check --strict        also refuse blocks vanilla Scratch cannot run\n\
         #   {crate} build                 write {out}/<name>.sb3\n\
         #   {crate} build --debug         also write {out}/project.json\n\
         #   {crate} build --strict        refuse TurboWarp-only blocks\n\
         #   {crate} clean                 remove the output directory\n\
         #   {crate} new <dir>             scaffold: --here, --force, --with-module\n\
         #   {crate} init <dir>            scaffold into an existing directory: --force\n\
         #   {crate} catalog               the block reference: --json, --markdown, -c TEXT\n\
         #\n\
         # Exit codes are the same everywhere in the workspace: 0 success, 1 a diagnostic,\n\
         # 2 bad command line usage.\n",
        crate = asm::CRATE,
        manifest = asm::MANIFEST,
        out = asm::DEFAULT_OUTPUT_DIR,
    )
}

/// `asm-cli` — the tool, its flags, and what it writes.
pub fn asm_cli() -> String {
    format!(
        "\n## asm-cli\n\
         # One command per stage of the compiler, and nothing hidden:\n\
         #\n\
         #   {crate} new <dir>       a project with a working example\n\
         #     --here                write into the current directory\n\
         #     --force               overwrite files if the directory is not empty\n\
         #     --with-module         also write src/lib/shapes.rasm, shared with `use`\n\
         #   {crate} init <dir>      set up a project in an existing directory\n\
         #     --force               overwrite an existing {manifest}\n\
         #   {crate} check           parse and validate the project; writes nothing\n\
         #     -m, --manifest <path> defaults to ./{manifest}\n\
         #     --strict              reject blocks vanilla Scratch cannot run\n\
         #   {crate} build           compile the project into {out}/<name>.sb3\n\
         #     -m, --manifest <path> defaults to ./{manifest}\n\
         #     --debug               also write {out}/project.json\n\
         #     --strict              reject TurboWarp-only blocks\n\
         #   {crate} clean           remove the build output\n\
         #     -m, --manifest <path> defaults to ./{manifest}\n\
         #   {crate} catalog         print the block catalog\n\
         #     --markdown            emit the reference the documentation ships\n\
         #     --json                emit it as data\n\
         #     -c, --category <text> only blocks from categories containing the text\n\
         #\n\
         #   {opt}                   the optimiser, as a program of its own:\n\
         #     -m, --manifest <path>  the project to read\n\
         #     -o, --output <dir>     write a copy there; defaults to <manifest dir>/dist/optimized\n\
         #     --in-place             rewrite the project's own files instead\n\
         #\n\
         # Exit codes: 0 success, 1 a diagnostic, 2 bad command line usage.\n\
         #\n\
         # docs: {docs}\n",
        crate = asm::CRATE,
        opt = "raven-opt",
        manifest = asm::MANIFEST,
        out = asm::DEFAULT_OUTPUT_DIR,
        docs = asm::DOCS,
    )
}

/// `asm-optimizer` — what the optimiser rewrites, and what it will not.
pub fn asm_optimizer() -> String {
    "\n## asm-optimizer\n\
     # The optimiser is a separate program over a separate input: a raven-asm project\n\
     # in, an optimised raven-asm project out. raven-asm itself never folds, because one\n\
     # statement is one block is the promise the language is built on.\n\
     #\n\
     #   raven-opt --manifest-path raven-asm.toml --output optimised/   # a copy\n\
     #   raven-opt --manifest-path raven-asm.toml --in-place            # over the project\n\
     #\n\
     # `raven` runs the same crate as a step of its own build, because its macros are\n\
     # already lowerings a reader did not write; `raven build --no-optimize` turns it\n\
     # off.\n\
     #\n\
     # # The two rewrites\n\
     #\n\
     # Both are identities in Scratch's own semantics, applied to operands the compiler\n\
     # already knows. That phrasing is the whole safety argument: a rewrite is allowed\n\
     # when it depends only on what a block does, never on what a program means.\n\
     #\n\
     #   constant folding    a reporter whose arguments are all literals is replaced by\n\
     #                       the value it would have produced\n\
     #\n\
     #       data_setvariableto(\"t\", operator_add(operator_multiply(2, 3), 4));\n\
     #       data_setvariableto(\"t\", 10);\n\
     #\n\
     #   branch simplification and merging    an `if` whose condition is a literal the\n\
     #                       compiler put there is replaced by the branch that runs; both\n\
     #                       `if` and `if/else` are handled\n\
     #\n\
     # The arithmetic follows the VM's own definitions rather than a convenient\n\
     # approximation, which matters because Scratch's operators are JavaScript's and\n\
     # Rust's are not:\n\
     #\n\
     #   mod(-7, 3)     2       floored, not truncated\n\
     #   mod(1, 0)      NaN     not an error and not zero\n\
     #   round(-2.5)    -2      Math.round rounds half UP; Rust's rounds half away\n\
     #   \"1\" = 1        true    `=` casts before it compares\n\
     #   mathop(\"sin\", 90)  1  degrees, not radians\n\
     #\n\
     # A fold this list cannot answer is not made, which is why the list is short.\n\
     #\n\
     # # The one that surprises people: a boolean is a block\n\
     #\n\
     # Scratch has no boolean LITERAL. A hexagonal input holds a boolean block and\n\
     # nothing else, so there is nothing to fold `operator_lt(1, 2)` into that is not\n\
     # itself a block. The smallest block that is always true is `operator_equals(1, 1)`,\n\
     # and that is what the optimiser emits. So folding a boolean does not always save a\n\
     # block — it saves the operand tree and it makes the branch simplification possible,\n\
     # which is where the real saving is. Writing a bare `true` where a condition belongs\n\
     # stays a compile error: `expected a condition, found true`.\n\
     #\n\
     # # What it will not do\n\
     #\n\
     #   propagating a variable        `x` may be assigned between two reads, so\n\
     #                                 replacing the second read with the first value\n\
     #                                 changes the program\n\
     #   propagating a raven local     a `let`, a `for` counter and a procedure result\n\
     #                                 are cells of the shared `_vms` list, and the cells\n\
     #                                 are mutated, so a substituted value can be stale\n\
     #   folding sensing_timer(), looks_size(), sensing_answer()\n\
     #                                 these read the world; two evaluations can differ\n\
     #   duplicating any such reporter  the same reason, from the other side\n\
     #   folding a condition that only LOOKS constant\n\
     #                                 the optimiser acts on literals, not inferences\n\
     #   inlining a procedure into its call sites\n\
     #                                 measured, and it does not pay — see below\n\
     #\n\
     # # Inlining and reporter substitution: tried, measured, removed\n\
     #\n\
     # Inlining a procedure into its call sites was written and measured before being\n\
     # left out. Three findings, each of which alone is enough:\n\
     #\n\
     #   1. with the full guard set (one-statement body, each parameter read at most\n\
     #      once, pure arguments, a compatible warp setting, and a strict check that the\n\
     #      body is cheaper than the call plus its arguments) it inlined ZERO calls\n\
     #      across desktop, chess, sudoku, penfont and case. Without the cheaper-than-\n\
     #      the-call check it inlined four calls in desktop and ADDED 69 blocks.\n\
     #   2. the dynamic cost is not where the static cost is: cpu_translate has 37 static\n\
     #      call sites and 16.5 million dynamic ones, so duplicating its body 37 times\n\
     #      changes nothing about the loop that pays for it.\n\
     #   3. a warp procedure cannot be inlined into a yielding caller. warp means the\n\
     #      runtime will not interrupt the thread; a copy runs under the caller's\n\
     #      schedule and can be interrupted. That is observable, so the guard is\n\
     #      required — and it rules out most procedures in a program like the desktop\n\
     #      example, which is warp throughout.\n\
     #\n\
     # Substituting a reporter into the place that reads it is impossible rather than\n\
     # merely unprofitable: a raven local is not a value but a cell of the shared `_vms`\n\
     # list addressed by a compile-time index, and the cells are mutated. Replacing a\n\
     # read with the value an earlier write computed would propagate a value across a\n\
     # later write. The one case that looks safe is a cell written and read once with\n\
     # nothing in between, and recognising it needs dataflow over the whole arena.\n\
     #\n\
     # The saving is real, though, and the place to take it is the SOURCE: a reporter\n\
     # computed where it is used is cheaper than one stored and read back, because a\n\
     # stored value costs a cell read or a call frame on top of the computation itself.\n\
     # In raven that is what an `fn`, a `macro` or a `const` buys.\n\
     #\n\
     # # How to tell it did not change the program\n\
     #\n\
     #   the round-trip test   compiles a project, reverses it with raven-re, compiles it\n\
     #                         again, and compares the two through a fingerprint of\n\
     #                         everything Scratch can observe — with the optimiser on AND\n\
     #                         off, so a rewrite that changed the emitted blocks fails\n\
     #   the unit tests        pin the arithmetic against the VM's definitions, and assert\n\
     #                         the rewrites that must NOT happen\n\
     #   the benchmarks        build a project both ways and run both in the same process\n\
     #                         on the same host; identical guest instruction counts mean\n\
     #                         the two builds are running the same program\n\
     #\n\
     # # What it is worth\n\
     #\n\
     #   example               plain     optimised   delta\n\
     #   examples/raven/desktop  12075     11544     -531\n\
     #   examples/raven/chess    31158     31043     -115\n\
     #   examples/raven/sudoku    2027      2013      -14\n\
     #   examples/raven/penfont    891       887       -4\n\
     #   examples/raven/case       170       170        0\n\
     #   total                   46321     45657      -664  (-1.43%)\n\
     #\n\
     # Modest, and worth being honest about: a boot of the desktop example retires the\n\
     # same guest instructions either way and the wall clock differs by less than the\n\
     # host's own run-to-run noise. What the layer buys is fewer blocks and the\n\
     # guarantee that it cannot have changed anything — not a faster boot. The wins that\n\
     # ARE large in that project came from removing procedure calls at the source, which\n\
     # is a decision a compiler cannot make for you.\n"
        .to_string()
}
