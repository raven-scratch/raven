//! The decompiler, as a page. Built rather than written, so the raven-asm file
//! names come from `raven_asm::identity` rather than from a retyped string.

use raven_asm::identity as asm;

/// `re` — what raven-re reads, what it writes, and what it refuses.
pub fn re() -> String {
    format!(
        "\n## decompiler\n\
         # raven-re is the other direction: it reads a Scratch 3 .sb3 and writes the\n\
         # raven-asm project that reproduces it.\n\
         #\n\
         #   .rav  ->  .rasm  ->  project.json  ->  .sb3\n\
         #                                  <-  raven-re\n\
         #\n\
         # The arrow is reversible because of the rule the workspace is built on: in\n\
         # raven-asm one statement is exactly one Scratch block. Nothing is guessed at or\n\
         # reconstructed, because nothing was ever collapsed. raven-re walks a project's\n\
         # blocks and writes one line for each of them.\n\
         #\n\
         # # What it writes\n\
         #\n\
         # An ordinary raven-asm project, laid out exactly as `{asm} new` lays one out:\n\
         # nothing else, no README and no .gitignore.\n\
         #\n\
         #   {manifest}            which files are targets, and which extensions are used\n\
         #   src/stage.rasm               the stage\n\
         #   src/sprites/<name>.rasm      one file per sprite, named after the sprite\n\
         #   assets/<md5>.<ext>           every costume and sound, under the name the .sb3\n\
         #                                used\n\
         #\n\
         # After writing, it hands the project straight back to the raven-asm compiler, so\n\
         # the command exits zero only when what it wrote really rebuilds. A failure names\n\
         # the generated line, and the sources are left on disk either way.\n\
         #\n\
         # # The command\n\
         #\n\
         #   raven-re [OPTIONS] <INPUT.sb3>\n\
         #\n\
         #   -o, --output <DIR>   where to write; defaults to the input's file name in the\n\
         #                        current directory\n\
         #   --force              write into an output directory that already holds files\n\
         #   -h, --help, -V, --version\n\
         #\n\
         #   raven-re my-game.sb3                 ./my-game/{manifest}\n\
         #   raven-re my-game.sb3 -o work/game    ./work/game/{manifest}\n\
         #   raven-re my-game.sb3 -o .            the current directory\n\
         #\n\
         # A directory that already holds files is refused without --force, so a reversal\n\
         # cannot scatter itself over an unrelated project. Exit codes: 0 the project was\n\
         # reversed, written and rebuilt; 1 a refusal, a bad file, or a failure to write;\n\
         # 2 bad command line usage. Warnings go to stderr and do not stop the reversal.\n\
         #\n\
         # # Vanilla Scratch 3 only\n\
         #\n\
         # raven-asm targets vanilla Scratch 3, and so does raven-re. A block vanilla\n\
         # Scratch does not have has no raven-asm spelling at all, so a project that uses\n\
         # one is refused before anything is written, naming what gave it away. The check\n\
         # is on what the project CONTAINS, not on who saved it. A project is refused when\n\
         # it:\n\
         #\n\
         #   uses a block only an extended runtime provides — control_while,\n\
         #     control_for_each, control_all_at_once, the counter blocks, sensing_online\n\
         #   uses a block no Scratch has at all — any TurboWarp, Penguinet or fork-specific\n\
         #     opcode\n\
         #   lists an extension other than pen and music, or loads one from a URL\n\
         #   says `TurboWarp` in meta.agent\n\
         #   holds a shadow block that is neither a dropdown, a custom block prototype nor\n\
         #     one of its parameter reporters\n\
         #   is missing an asset it refers to, or was zipped as a folder rather than as an\n\
         #     .sb3 (its files sit under a directory instead of at the root)\n\
         #\n\
         # # The names Scratch allows and raven-asm does not\n\
         #\n\
         # Scratch lets a variable, a list, a custom block or a parameter be called anything\n\
         # at all — `\"foo`, `< Perfect`, `a&b` all occur in the wild. A raven-asm name is an\n\
         # identifier, so a name that cannot be written as one is replaced by an encoding of\n\
         # itself: `re_` followed by the name's UTF-8 bytes in hex.\n\
         #\n\
         #   var re_6d792073636f7265 = 0;   // the project calls this `my score`\n\
         #\n\
         # The encoding is a pure function of the name, so reversing the same file twice\n\
         # writes the same source, and reversing twice through raven-asm is stable. A name\n\
         # is moved aside rather than encoded only when two names in one target would\n\
         # collide, or when a sprite's own variable would shadow a stage variable, which\n\
         # raven-asm refuses.\n\
         #\n\
         # # What has no raven-asm syntax\n\
         #\n\
         # Each of these is a warning and is dropped rather than approximated:\n\
         #\n\
         #   Scratch comments            raven-asm has no comment blocks\n\
         #   a costume's bitmapResolution  raven-asm always writes 1, so a costume drawn at\n\
         #                               resolution 2 changes size in the rebuilt project\n\
         #   monitors that watch a reporter  raven-asm writes a monitor for a variable and\n\
         #                               for a list and for nothing else\n\
         #   blocks loose in the workspace  an unattached block never runs, and raven-asm\n\
         #                               has no way to write one\n\
         #   volume, layer order, tempo, draggable, rotation style, the current costume\n\
         #                               raven-asm's target defaults; a costume or sound's\n\
         #                               own data is kept, these fields are not\n\
         #\n\
         # Two further differences are the compiler's own normalisations and are invisible\n\
         # to the running project: an input's literal is written as text, so a 10 stored as\n\
         # a JSON number comes back as \"10\"; and the spelling of a fixed dropdown is\n\
         # canonicalised, so \"color\" becomes \"COLOR\" and \"TIMER\" becomes \"timer\".\n\
         #\n\
         # Three more come from raven-asm's shape and are worth knowing when reading the\n\
         # output:\n\
         #\n\
         #   a custom block's label is re-spelled. Scratch puts each %s, %n or %b wherever\n\
         #     the parameter goes and raven-asm can only write a parameter at the end, so\n\
         #     the label is what remains once the placeholders are taken out. Every call is\n\
         #     rewritten with it, so the block still means what it meant.\n\
         #   a reporter standing alone in an input is written out again: the serializer\n\
         #     compresses a lone variable, list or broadcast reporter into [12, \"score\",\n\
         #     \"id\"] inside the input, and raven-asm writes the block it is,\n\
         #     data_variable(\"score\"), which is what Scratch expands it back to.\n\
         #   a long list is wrapped, a few items per line, because a five-megabyte line is\n\
         #     a file no editor or diff can open. Whitespace between items means nothing.\n\
         #\n\
         # # What survives the round trip\n\
         #\n\
         # A reversal followed by a build is the identity on everything Scratch runs: every\n\
         # block, variable, list, broadcast, costume, sound, custom block and monitor that\n\
         # Scratch can observe is the same. The archive is not byte-identical — identifiers\n\
         # are derived from the source, monitors are rewritten, and the normalisations above\n\
         # apply — and that equality is what raven-re's own test suite checks, by compiling\n\
         # a project, reversing it, compiling it again and comparing the two.\n\
         #\n\
         # Everything raven-re decides is a function of the file it read: names are encoded\n\
         # from their own text, collisions are broken with a hash of the id the project\n\
         # already gave the declaration, and the output directory is written in a fixed\n\
         # order. Two runs over the same bytes write the same bytes.\n\
         #\n\
         # docs: {docs}\n",
        asm = asm::CRATE,
        manifest = asm::MANIFEST,
        docs = asm::DOCS,
    )
}
