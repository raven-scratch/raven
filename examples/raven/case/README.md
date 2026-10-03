# case — the demo for `lib/case`

Scratch's `=` lowercases both sides, so `"Raven"` and `"raven"` are one string
and a project cannot ask for a word in a case. This project asks for `Raven` and
answers which of three things you typed:

* **exact** — what Scratch cannot tell you on its own;
* **the right word, in the wrong case** — what `given == "Raven"` answers, and
  the reason the library exists;
* **not the word**.

```sh
cargo run -p raven -- check -m examples/raven/case/raven.toml
cargo run -p raven -- build -m examples/raven/case/raven.toml --debug
node tools/validate-sb3.js examples/raven/case/dist/case.sb3 --steps 300
```

## The shape

`src/sprites/check.rav` is the whole program: ask, compare, say, in one script.
`src/case/engine.rav` is `lib/case` copied in — the copy *is* the install — and
`use case::engine;` is what makes this sprite wear the module's 53 costumes.

The sprite declares no costume of its own: the first of the 53, `cs_none`, is
what it is seen in, and it is a transparent unit square. `cs_eq` switches through
the costumes by name and puts `cs_none` back before it returns, so the sprite
looks the same before and after.

`src/stage.rav` is the paper and nothing else.
