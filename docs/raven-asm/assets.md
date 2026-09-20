# Assets

Costumes and sounds are declared like anything else, then packed into the `.sb3`
for you.

```rasm
sprite "Player" {
    costume "costume1" = "assets/player-idle.svg" center 32 32;
    costume "costume2" = "assets/player-run.svg";
    sound "jump" = "assets/jump.wav";
}
```

**Asset paths are relative to the project root** — the directory holding
`raven-asm.toml` — so `assets/…` means the same thing from every file.

## Supported formats

| Kind | Formats |
| --- | --- |
| Costumes | `.svg`, `.png`, `.jpg` / `.jpeg`, `.bmp`, `.gif` |
| Sounds | `.wav`, `.mp3` |

Anything else is a compile error naming the file and the line.

## Costume size and rotation centre

Scratch stores a rotation centre for every costume: the point the sprite rotates
around, in costume pixels. raven-asm works it out for you.

* **SVG** — from the root element's `width` and `height`, or its `viewBox` if
  those are percentages or missing.
* **PNG, GIF, BMP, JPEG** — from the image header, and the bytes are checked
  against the extension, so a `.png` that is not a PNG is a build error rather
  than a project that fails to open.
* Every costume is packed with `bitmapResolution: 1`, SVG included, so one image
  pixel is one stage unit and a project round-trips through the editor unchanged.

The centre defaults to the middle of the image. Override it when the sprite
should pivot somewhere else:

```rasm
costume "sword" = "assets/sword.svg" center 8 48;   // pivot at the hilt
```

If raven-asm cannot determine the size — a hand-written SVG with no size
attributes, for instance — that is an **error**, not a guess, because the
rotation centre is what every `motion` block turns around:

```
error: cannot work out the size of `assets/weird.svg`, so its rotation centre is unknown
  = note: give the image a `width` and `height`, or a `viewBox`, in its `<svg>` tag
  = note: or place it yourself: `costume "weird" = "assets/weird.svg" center X Y;`
```

## Sounds

WAV files have their sample rate and frame count read from the file header and
written into the project, which is what the Scratch audio engine reports back
after loading anyway. MP3 files carry no such metadata, so those fields are
omitted.

## How assets get into the `.sb3`

An `.sb3` is a ZIP. Scratch names each asset after the MD5 of its contents:

```
dist/hello.sb3
├── project.json
├── b7c03869bc56eb437473ab6310051403.svg
├── a400725bba5642e1b91798310054356d.svg
└── e5b5710b1ea1f12ac11db972881035a5.wav
```

raven-asm hashes each file, writes the hash into `project.json` as the costume's or
sound's `assetId`, and stores the bytes under `<md5>.<format>`. Nothing is
re-encoded and nothing is uploaded: the files you point at are the files that go
in.

Declaring the same file twice — two sprites sharing one costume, say — stores it
once and lets both targets reference it.

## The default assets

`raven-asm new` writes two small starter SVGs rather than shipping binaries:

* `backdrop1.svg` — a plain white 480 × 360 stage;
* `logo.svg` — the raven-asm logo: a rounded square of diagonal bands with the
  `Ra` wordmark, which is the sprite's costume.

The costume *is* `docs/public/logo.svg`, embedded verbatim — `raven-asm new`
writes the very file the documentation ships, with one attribute changed: the
canvas size. A sprite's on-stage size comes from its costume's intrinsic width
and height, and the documentation canvas is 512 units, wider than the stage, so
the costume is written at 128.

The wordmark is vector outlines rather than a `<text>` element. Scratch's SVG
loader deletes the `x` and `y` attributes of every text element and re-anchors it
from scratch, which would move the letters inside a costume; outlines are just
shapes and go through untouched. That is also why the letters are really there —
the sprite is the logo, not an approximation of it.

Delete or replace both files as you like; they are ordinary assets.
