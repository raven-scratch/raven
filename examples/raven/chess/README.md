# chess — a game against the maia networks, in raven

Chess against a neural network, on a board drawn entirely with the pen. The
board is one sprite that stamps 64 squares, the tints, the move markers and the
pieces at rest; there is no clone anywhere in the project. There are nine bots —
the `maia` networks, trained to predict what a human of 1100 to 1900 would play —
and either side of the board can be you or any of them, so you can play, watch
two bots play each other, or take both sides yourself. A piece is moved by
dragging it or by clicking it and then its square, and either way it travels on a
spring rather than a fixed curve.
```
tools/maia.py            the Python engine: weights in, move out, and the checks
tools/check.mjs          runs the built project and compares it with maia.py
tools/assets.mjs         the costumes: the lichess pieces, the tiles, the font
tools/glyphs.py          the HUD font: one costume per character, from Montserrat
tools/hud.mjs            what the HUD stamps, page by page, and whether it fits
tools/sounds.mjs         the four sounds
tools/pieces/            the lichess cburnett pieces, as downloaded
tools/policy_tables.json lc0's move numbering, cached so Python need not re-read it
src/engine.rav           the rules and the networks, shared by both sprites
src/layout.rav           where everything is, shared by both sprites
src/stage.rav            what both sprites read, and nothing else
src/sprites/board.rav    the game: drawing, input, motion, the bots
src/sprites/hud.rav      the panel, the menu and every character
src/net.rav              nine bots, packed (generated, 42 MiB)
assets/                  706 costume and sound files (generated)
```

```sh
# the Python engine, and the numbers the raven build is checked against
python examples/raven/chess/tools/maia.py
python examples/raven/chess/tools/maia.py --dump

# the costumes, the font and the sounds (the first two need a browser: playwright)
node examples/raven/chess/tools/assets.mjs
python examples/raven/chess/tools/glyphs.py
node examples/raven/chess/tools/sounds.mjs

# the weights, when the network files change
python examples/raven/chess/tools/maia.py --export

cargo run -p raven -- check -m examples/raven/chess/raven.toml
cargo run -p raven -- build -m examples/raven/chess/raven.toml --debug

# the built project, against the Python engine (--debug writes layout.json,
# which is where the checker finds each list's run of cells)
SCRATCH_VM_ROOT=ref/scratch-editor/packages/scratch-vm \
    node examples/raven/chess/tools/check.mjs

# the built project's HUD: every stamp of every page, and the pages as images
SCRATCH_VM_ROOT=ref/scratch-editor/packages/scratch-vm \
    node examples/raven/chess/tools/hud.mjs --svg examples/raven/chess/dist/hud
```

## The Python engine

`tools/maia.py` is the whole of the network, written to be read: it opens a lc0
`.pb.gz`, dequantises each layer, folds the batch norm into the weights the way
lc0 does at load time, turns a position into the classical 112 input planes, runs
the six SE residual blocks and both heads, and picks the legal move with the
highest policy. There is no training in it and no search: it is weights in, move
out.

It carries a full move generator as well — castling, en passant, promotion and
all — and checks it against the published perft counts of six standard positions
before it is allowed to judge anything:

```
perft ok   rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBN [20, 400, 8902, 197281]
perft ok   r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/P [48, 2039, 97862]
...
maia-1100 plays e2e4 (14.0198)   wdl 0.503 0.039 0.458
```

The policy tables — which convolution cell means which move — are lc0's own. The
first run parses them out of `ref/_ptmp/lc0/` and caches them in
`tools/policy_tables.json`, which is what every later run reads.

## The same engine in raven

`src/engine.rav` is that engine again, statement for statement. The board is a
10-wide mailbox of 120 cells so that a slider walks a direction until it meets a
cell that is not empty and a knight's two-rank jump lands on an edge rather than
off the end of a list. Move generation, legality, the encoder, the convolutions
and the two heads are all there.

`best_move` is the one the Python does — the legal move with the highest policy,
which is the move lc0 without a search plays and the move `tools/check.mjs`
compares against `maia.py --dump` — and it is what a game plays. That is how maia
is meant to be played: `go nodes 1` with no temperature, a body with the search
switched off. The models answer the same position with the same move every time,
and they play the *average* move of the rating they were trained on, which is why
they come out a little stronger than their name. A draw from the policy is not
that player: the head is a distribution over what the rating *might* play, so
sampling it hands the game the rare moves far too often, and a 1% move every
seventy plies is a lost position the value head already had at 75%.

## Nine bots in one project

A maia network is 863,616 weights and 1,955 biases. Written as decimals that is
11.6 MiB for one, and nine of them would be 104 MiB — and `.sb3` files are stored
rather than deflated, so the project would be that size too.

What makes nine fit is that lc0 does not store floats at all. Each weight is two
bytes read as a fraction of a range its layer carries, and three of those fit
inside one Scratch number with room to spare: a list item is a float64, so 53
bits of integer survive exactly, and three 16 bit weights are 48. `wq` therefore
holds all nine networks packed three to an item, `wlmin` and `wlmax` hold the
ranges, and `load_bot` in `src/engine.rav` unpacks one of them, folds its batch
norm in and leaves the two lists the forward pass reads. All nine come to
**42 MiB**, and nothing is lost: the packing is of the bytes the file already
had, and the fold is the same arithmetic on the same numbers.

Two limits shaped that. Scratch refuses to `add to list` past **200,000 items**,
so the 863,616 long working list `w` cannot be built at runtime — it is written
out as a literal and `load_bot` overwrites it in place. And the biases, which are
1,955, are ordinary appends.

`load_bot` takes about three seconds. One set of weights is resident at a time,
so a game with a bot on each side reloads when the turn changes; that is three
seconds next to the ninety a move takes.

## Motion

A piece that moves is not teleported. It is this sprite — the board puts the
piece's costume on, shows itself and moves, and the renderer draws it above the
ink — at a position integrated by a second order system:

```
k1 = zeta / (pi f)          f     how fast it answers, in Hz
k2 = 1 / (2 pi f)^2         zeta  how it settles: under 1 overshoots, 1 does not
k3 = r zeta / (2 pi f)      r     whether it moves off the moment the goal does
```

and stepped with semi-implicit Euler by the time the last frame actually took.
Because the motion is a state rather than a curve, it can be retargeted at any
moment and carries its velocity into the new goal: a piece can be picked up
mid-flight, a drag sets the goal to the pointer, and letting go over a square
turns a thrown piece into a landing one rather than restarting it from rest.

The same step, checked against the frame time, is what keeps it from exploding.
The system is stable only while the step is smaller than `sqrt(4 k2 + k1^2) - k1`,
so a frame long enough to break that — a lag spike, or a frequency as high as the
drag's — is divided into shorter steps until it is not. That is a handful of
multiplications per frame, and it is the difference between a spring that settles
and one that flies off the screen.

The state is the position in the board's own space, not on the stage: `raw_x` and
`raw_y` of the square it is over, which do not move when the board is turned
round. The stage is looked at through one mirror, and every frame draws the piece
through it — so `FLIP` while a piece is in the air turns the piece with the board
and it still lands on the square it was sent to, because the square it was sent to
is a square and not a place. A goal kept as a place on the stage would be the
place that square had before the turn.

The evaluation bar is the same system on one axis, which is why it eases towards
a new value instead of snapping to it, and why a second value arriving while the
first is still moving is a change of goal and not a restart.

## The screen

**The menu** covers the stage with two cards, one per side, each carrying the ten
names it can be played by as chips — `YOU` and the nine networks — with the
picked one outlined and written in the bold weight. A chip is one press: the row
is a row of choices, not a value to drag. Any combination is legal: you against
a bot, two bots against each other, or both sides yourself.

**The settings page** is a grid of all thirty-nine piece sets with every cell
wearing that set's own knight, so the page is the preview and a press is the
choice; the name of the set that is on is written under the grid. Under it is
the row of four board styles, and the way back.

**The board** has its coordinates in the margin round it: rank digits up the
left, file letters under it, both turning with the board and neither drawn over a
piece. It carries a tint on the square the pointer is over, a tint on the move
just played, a ring round a king in check, and a dot or a ring on every square
the piece you picked up may go to. A promotion is four pieces on a slab over the
promoting square, in the colour of the side promoting.

**The panel** is one card per player at either end and the game between them: the
side, who plays it, a dot when it is their turn, the pieces they have taken along
their card, and what they are up on material. Between the cards are the
evaluation as a bar and a percentage, the last twelve plies with move numbers,
whose move it is, how long the game has run, and the three buttons at the foot of
it: `NEW` deals a new game, `FLIP` turns the board round, and `MENU` goes back to
the menu and abandons the game — which is the one way out of the board, and it is
there whether the board is waiting for a move, thinking, or showing the end of a
game.
The bar grows a cell at a time and the clock rewrites one line a second, so
neither costs a redraw of the panel.

Those buttons are the one part of the interface a bot's turn does not take away,
and that is the reason they are not read the way the board is. A turn — the
unpacking of a network, then the search — runs inside a single frame of the
board's loop, and in a game between two bots that is nearly all of the time; a
loop that sampled the mouse would see neither the press nor, if it came and went
inside the window, that there had been one. So the click is heard where it is an
event: the stage's `when stage clicked` hat leaves the coordinates in `click_x`,
`click_y` and a count in `click_at`, and the board's *other* thread — the one the
search is not holding — acts on a count it has not seen before. The loop's own
reading of the mouse skips those three boxes, so one press is one press. A search
that is already running when `MENU` is pressed is not abandoned, it is *noticed*:
`bot_turn` and `new_game` check the page again after each long step and leave the
game alone if the menu has it.

The clock is the one thing on the panel that is meant to move by itself, and it
is the engine that moves it. A bot's turn runs inside a single frame of the
board's loop — the loop hands the search its whole turn and gets no turn of its
own until the search is done — so a clock the loop counted would stand still for
exactly the minute or two the clock is worth reading. `engine::clock_tick` is
what counts it, and the search and the unpacking of one network call it: the
second is noticed where the second is spent.

**The end of a game** is a card over the board with the result, the reason, and
what to do about it. The board draws under that card and only once: the two
things that stamp a frame at a time — a piece still travelling, and the
evaluation bar — are stopped before the move that ended the game is drawn, so
nothing is stamped after the card. A board stamp that came later would be a piece
painted across the result.

## Drawing and clicking

The board is `pen::clear` then a few hundred stamps inside one procedure: it
points at a square, switches costume and stamps. A piece at rest is a costume,
not a sprite to keep in step with the board, and a redraw lands in one frame.

The pen has one layer for the whole stage, and the panel is stamped on the same
one, so a redraw during a flight would take the panel with it and clearing it
thirty times a second is what made a moving piece stutter. A piece in flight is
therefore the exception: it is the sprite, and a frame of it costs no pen work at
all. The move ends by stamping the board once more, with the piece back on its
square.

The pen cannot see a click, a drag *or a hover*, so the board does not ask it to.
It samples the mouse in its own forever loop, reads `mouse x` and `mouse y`, and
works out which square or which button is under the pointer itself. A press on
one of your pieces picks it up and starts it following the pointer; a release
over a square that move is legal on plays it, and a release anywhere else drops
it back where it came from — still picked up, because a press that did not move
is the first half of a click. A hover repaints the two squares it crossed rather
than the layer, so moving the pointer never costs a redraw of the panel, and
there is no click-catching sprite and no clone.

A square is written down twice over: as the game names it, and as the screen
shows it. `FLIP` turns the board round, so everything that reads a position off
the stage — the pointer, and the box a flight repaints as it crosses the board —
goes through one mapping from the screen row and column to the square, and
everything that draws goes the other way. A repaint that skipped the mapping
would erase the square the piece would have crossed without turning the board
round: the piece would be left stamped on the square it left as well as drawn on
the square it reached.

## The pieces

Thirty-nine sets, twelve pieces each, from lichess. A set's files do not agree
about their canvas — nine of them do not — so each piece is fitted by its own
`viewBox`, centred, into the square of a board square, and the fit is a
`transform` rather than a viewBox, the way lichess itself draws them at one size.

What the project carries is not that SVG. It is a PNG per piece per **size the
project ever draws a piece at**, and every one of those stamps is at size 100:

* `p<set><piece>`, **36 units** — a piece standing on a square of the board, and
  each of the four choices offered by a promotion. This is `P_SQUARE`.
* `d<set><piece>`, **40 units** — the piece the pointer has hold of, a shade
  larger than the square it came from. `P_HELD`.
* `c<set><piece>`, **24 units** — one icon in the panel's row of what a side has
  taken. `P_ICON`.
* `s<set><piece>`, **22 units** — one knight in a cell of the settings grid.
  `P_CELL`.

Those four numbers live in `src/layout.rav`, and the generator draws a piece at
each of them: a costume is its own size in stage units, so nothing a piece is
stamped with is ever a scale of something else, and the sizes the layout was
worked out with are the sizes of the files. The one exception is the capture row,
whose pitch closes up as the row fills: a crowded row draws its icons smaller
than 24, downward, which is the harmless direction — fifteen costumes per piece
per set would be the alternative.

Why not SVG at all: every consumer of an SVG costume is a different parser with
its own idea of what an SVG is.

* Scratch hands the file to the browser as an image, after sanitizing it with a
  profile that quietly drops filter primitives, `use` and `foreignObject` — and
  it drops a filter's children while leaving the element, so a shape drawn
  through the emptied filter is drawn as nothing at all, which is a whole piece
  for the sets that put the piece inside one.
* The paint editor imports it with Paper.js, which crashes on a gradient that
  inherits from one defined later in the file — `Cannot read properties of
  undefined (reading 'getGradient')` — and refuses a gradient with a single stop.
* TurboWarp's renderer has a path of its own again, on top of all of it.

A PNG has one reader and it is the same one everywhere. The rasteriser is
playwright's chromium — the same browser `tools/glyphs.py` draws its proof sheets
with — and each piece is drawn as an image of its own rather than inlined into one
page: two lichess files both call their gradient `a`, and a document holding both
gives every `fill="url(#a)"` the first one.

The run then reads every file it wrote back: every piece must be a PNG of exactly
the size its name says, and every SVG that remains — the tiles, the cells, the
panel, the font — must be well-formed, with every prefix it uses declared on its
root, because a file that is not XML is a costume drawn as nothing.

The cost is that a piece no longer has infinite resolution: the paint editor at
8 times zoom shows the pixels of a 22 or a 36 pixel image. Drawing a piece bigger
than these four sizes means adding a size to `PIECE_SIZES` in the generator and
the number it belongs to in `src/layout.rav`.

## The font

Scratch has no text, so the HUD's font is costumes: one SVG per character per
weight, generated from Montserrat by `tools/glyphs.py`, every one of them the
same box with the character's outline at the font's own origin and its own
advance. A line is one stamp per character along the baseline, and it is centred
by the sum of those advances — the string is walked once to measure it and again
to stamp it, which is also what makes an escape like `\l` cost nothing.

Two things about that are worth writing down, because both are silent when they
are wrong:

* A stamp draws whatever costume the sprite is wearing, so every stamp here is
  preceded by the costume it means. A button stamped after a label is that
  label's last letter otherwise, which is how the settings button once came out
  with a `Y` behind it.
* `set size to` clamps what it is given to a floor that depends on the costume
  the sprite is *currently* wearing: five stage units across it, or 100% when
  the costume is smaller than that. A size of 14% set while wearing the 1 by 1
  blank pixel comes back as 100%, and a line measured for 14% is then stamped
  at 100% — its characters pile up on each other. The size is therefore set per
  character, with that character's glyph already on, and never before it.

## What it is checked against

`tools/check.mjs` is the proof that the two engines are the same engine, and that
the mouse reaches the game. It loads the built project into a real Scratch VM,
writes a position into the engine's own lists, presses a key, and reads back what
came out:
* the legal move list of all six standard perft positions, against `maia.py`,
* every published perft count for those positions, walked by the project itself,
* **every one of the nine bots**, sampled across all 863,616 weights and all
  1,955 biases, against what `maia.py` folds,
* the 112 input planes, cell for cell,
* the whole policy head and the value, for 1100 and for 1900, to about 1e-13,
* the move `best_move` picks, which is the move a game plays too — the first move
  of a game between two bots is read back and has to be the one `maia.py --dump`
  says,
* three games: one moved by clicking e2 and then e4, one by dragging e2 to e4,
  and one where the white row's second chip is pressed, where the pointer must
  do nothing and the board must move itself.

Everything that takes time in a Scratch VM takes time here too: a forward pass
is about ninety seconds, so the whole check runs for around twenty minutes.

`tools/hud.mjs` is the same trick for the HUD, and takes seconds. The project is
loaded with a stand-in in place of the WebGL renderer which answers the two
questions the VM asks a renderer about geometry — how big a skin is, and where a
drawable may move to — with the costume's own size, so the VM's own clamp of
`set size to` is in the loop, and records every `pen stamp` as the costume,
position and size the sprite had. The costumes themselves are read through the
real sanitizer, and a piece, which is a PNG, goes in as an SVG that holds it:
the skin is then the size TurboWarp gives it, and the page comes out as TurboWarp
would draw it. It drives the menu, the settings page, a game, the way back from a
game to the menu and the end of a game through the mouse — the last of those by
playing a scholar's mate rather than by writing the end of the game into the
state, so that what the card has to survive is a move like any other — and checks
that every character of a run is stamped at a pitch its own advance and its own
stamped size agree with, that no two runs of text are stamped over each other,
that a button is a button, that nothing from the board is stamped after the card
that ends a game, and that the same move leaves the same page with the board
turned round as without, and on the board turned round while the piece was still
in the air — the square a piece left shows no piece and the square it reached
shows it, which is what the flight repainting a square it never crossed, or
arriving where its square used to be, would leave behind.
With `--slow` it also starts a bot's turn and watches the panel's clock while it
runs, and presses `MENU` in the middle of that turn: both are things the board's
own loop cannot do anything about, because a search runs inside a single frame of
it.
`--svg` writes each page's stamps as an SVG — every target's, in the order they
were stamped, so what is under what is what the file shows — which is what to look
at when the checks pass and the page is still wrong.

## Layout

`src/layout.rav` is the one place the numbers live, because a target cannot read
another target's names and the board would otherwise test a click against a
second copy of the list it drew from.

A square is 36 units and the board's bottom left corner is (-222, -144), so the
centre of the square at file `c` and rank `r` is (-204 + 36c, -126 + 36r). The
board is 288 square, the 18 units left of it and the 36 under it are the
coordinates, and the panel beside it is 150 wide from x = 78: two player cards,
the pieces they have taken, the evaluation, the moves and the buttons.

## What it costs

The network is 37.7 million multiply-accumulates and Scratch does not have a
multiply-accumulate. A forward pass takes about ninety seconds in a plain
scratch-vm with no JIT; TurboWarp's compiler is far quicker, and the game is
perfectly playable there. Nothing was traded for speed: the arithmetic is the
arithmetic, in the order lc0 does it, because the point of the port is that
choosing a different bot changes how strong it is.
