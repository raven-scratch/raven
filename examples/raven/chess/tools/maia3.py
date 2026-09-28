#!/usr/bin/env python3
"""Maia3 3M in numpy: a reference forward pass, written to be ported.

This reads the Maia3 3M checkpoint (`UofTCSSLab/Maia3-ablate-3M`, file
`maia3-3m.pt`) with the standard library alone -- a torch zip archive is a
pickle of tensor records plus one blob per storage -- and then runs the network
the way `ref/maia3/maia3/models.py` runs it: 64 square tokens, an ELO pair, eight
post-LayerNorm encoder blocks with a smolgen attention bias, and three heads
(policy 4352, WDL 3, ponder 1). `torch` is never imported and there is no
training here; every step is a handful of numpy operations so it can be read
against `models.py` line by line.

The board and the legal moves are `maia.py`'s: a 120 cell mailbox, `WALL = 99`,
cell (r, f) = `(r + 2) * 10 + f + 1`, side 0 white and 1 black. `maia.py` runs its
perft checks only under `__main__`, so importing it here executes nothing.

    python examples/raven/chess/tools/maia3.py --selftest
    python examples/raven/chess/tools/maia3.py --export   # regenerate src/net3.rav
    python examples/raven/chess/tools/maia3.py --dump     # write tools/checkdata3.json
    python examples/raven/chess/tools/maia3.py --probe 0  # one cell of every stage

Maia3 names squares in the frame of the side to move (a1 = 0 for white, a8 = 0
for black), one flat index per move: `from * 64 + to`, then 256 promotion
entries. `move_index` and `move_of_index` below are that mapping, and
`forward`'s policy head fills exactly those 4352 logits.
"""

import io
import json
import math
import os
import pickle
import sys
import time
import zipfile

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import maia  # noqa: E402  (the board, the legal moves, the FEN parser)

# The network's shape, from MODEL_SPECS["maia3-3m-ablation"].
DIM = 192          # d_model
HEADS = 6          # attention heads
HEAD_DIM = 32      # DIM / HEADS
GEN = 64           # smolgen generator width
TOKENS = 64        # board squares
HISTORY = 8        # positions kept
PLANES = 12        # piece planes per position
SQUARES = 64
BASE_MOVES = 4096  # 64 x 64 from-square to to-square
PROMO_MOVES = 256  # 8 from-files x 8 to-files x 4 kinds
POLICY = BASE_MOVES + PROMO_MOVES  # 4352

# The promotion kinds, in the order the promotion head emits them and in
# `maia.py`'s piece-kind numbering: kind k of the head means piece (k + 1).
PROMO_PIECES = (5, 4, 3, 2)  # queen, rook, bishop, knight
KIND_OF_PIECE = {piece: kind for kind, piece in enumerate(PROMO_PIECES)}

CHECKPOINT = (
    r"C:\Users\Dilem\.cache\huggingface\hub"
    r"\models--UofTCSSLab--Maia3-ablate-3M\snapshots"
    r"\990dfd78e6403805dbbbb5fcfccd4b3d3e778cc1\maia3-3m.pt"
)

# The names and shapes one encoder block owns, read in this order by
# `transformer_body`. `smolgen_weight` is per-layer but tied to the same bytes,
# so it is listed where the checkpoint has it and supplied from the shared name.
LAYER_PARAMS = [
    ("self_attn.smolgen_weight", (4096, 64)),
    ("self_attn.mha.in_proj_weight", (576, 192)),
    ("self_attn.mha.out_proj.weight", (192, 192)),
    ("self_attn.sm2.weight", (64, 192)),
    ("self_attn.sm2.bias", (64,)),
    ("self_attn.ln1.weight", (64,)),
    ("self_attn.ln1.bias", (64,)),
    ("self_attn.sm3.weight", (384, 64)),
    ("self_attn.sm3.bias", (384,)),
    ("self_attn.ln2.weight", (384,)),
    ("self_attn.ln2.bias", (384,)),
    ("linear1.weight", (384, 192)),
    ("linear1.bias", (384,)),
    ("linear2.weight", (192, 384)),
    ("linear2.bias", (192,)),
    ("norm1.weight", (192,)),
    ("norm2.weight", (192,)),
]


# ---------------------------------------------------------------------------
# Reading the checkpoint: a zip of a pickle of storages.
# ---------------------------------------------------------------------------
#
# torch.save writes a directory: `data.pkl` holds an ordinary pickle whose
# values are tensor records produced by `torch._utils._rebuild_tensor_v2`, and
# each record points at a `FloatStorage` named by a persistent id. The storages
# themselves are raw float32 blobs at `data/<key>`. Only two names have to be
# understood to rebuild that faithfully, and neither is torch.

class FloatStorage:
    """The torch storage a tensor record points at: where its bytes live.

    `key` names `data/<key>` in the archive, `location` is always `cpu` here and
    `numel` is the number of float32 it holds.
    """

    def __init__(self, storage_type=None, key=None, location=None, numel=None):
        self.storage_type = storage_type
        self.key = key
        self.location = location
        self.numel = numel

    def __len__(self):
        return self.numel

    def __repr__(self):
        return "FloatStorage(key=%r, numel=%r)" % (self.key, self.numel)


class TensorRecord:
    """One `_rebuild_tensor_v2` value: a view of a storage, plus its metadata.

    A tensor is `storage.flatten()[storage_offset : storage_offset + size]`
    read through `stride`, which every checkpoint here leaves contiguous, so
    the slice can be reshaped directly.
    """

    def __init__(self, storage, storage_offset, size, stride,
                 requires_grad=False, backward_hooks=None):
        self.storage = storage
        self.storage_offset = int(storage_offset)
        self.size = tuple(size) if size is not None else ()
        self.stride = None if stride is None else tuple(int(s) for s in stride)
        self.requires_grad = requires_grad

    def numel(self):
        n = 1
        for d in self.size:
            n *= d
        return n


def _rebuild_tensor_v2(storage, storage_offset, size, stride, requires_grad,
                       backward_hooks=None, **kwargs):
    """The one torch name the pickle stream actually calls a function."""
    return TensorRecord(storage, storage_offset, size, stride, requires_grad,
                        backward_hooks)


class _CheckpointUnpickler(pickle.Unpickler):
    """A pickle loader that knows torch's four names and no torch module.

    `storage_type` is a legacy storage class: the pickle builds it with its
    arguments as a tuple and hands that tuple to `persistent_load`, which is
    where a real torch would fetch the storage by key.
    """

    def find_class(self, module, name):
        if module == "torch._utils" and name == "_rebuild_tensor_v2":
            return _rebuild_tensor_v2
        if name in ("FloatStorage", "HalfStorage", "DoubleStorage"):
            return FloatStorage
        if name in ("OrderedDict", "dict"):
            return dict
        raise pickle.UnpicklingError(
            "the checkpoint names %s.%s, which this loader does not know"
            % (module, name))

    def persistent_load(self, pid):
        # ('storage', FloatStorage, key, 'cpu', numel), or the storage already
        # rebuilt when the pickle stored it by value.
        if isinstance(pid[1], FloatStorage):
            return pid[1]
        storage_type, key, location, numel = pid[1:]
        return FloatStorage(storage_type, key, location, numel)


def load(path=CHECKPOINT):
    """Every parameter as a numpy array: name -> array of the checkpoint shape.

    The 156 names are `state_dict` names, so `smolgen_shared_weight` is the one
    (4096, 64) array the eight layers tie, and each layer's own
    `self_attn.smolgen_weight` is the same bytes under the same storage key.
    Storage bytes are read once and cached, so the tied weight is materialised
    once per name and the 12.6 MiB file is never held twice.
    """
    archive = zipfile.ZipFile(path)
    pickles = [n for n in archive.namelist() if n.endswith("/data.pkl")]
    if len(pickles) != 1:
        raise ValueError("%s holds %d pickles, expected one" % (path, len(pickles)))
    prefix = pickles[0][: -len("data.pkl")]
    state = _CheckpointUnpickler(io.BytesIO(archive.read(pickles[0]))).load()

    blobs = {}

    def blob(storage):
        if storage.key not in blobs:
            name = prefix + "data/" + str(storage.key)
            raw = archive.read(name)
            blobs[storage.key] = np.frombuffer(raw, dtype="<f4")
        return blobs[storage.key]

    params = {}
    for name in state:
        record = state[name]
        flat = blob(record.storage)
        end = record.storage_offset + record.numel()
        params[name] = flat[record.storage_offset:end].reshape(record.size).copy()
    return params


# ---------------------------------------------------------------------------
# The pieces every block needs: activations, norms, a linear layer.
# ---------------------------------------------------------------------------

def gelu(x):
    """`F.gelu`, the exact erf form and not its tanh approximation."""
    return 0.5 * x * (1.0 + np.vectorize(math.erf)(x / math.sqrt(2.0)))


def relu(x):
    return np.maximum(x, 0.0)


def linear(x, weight, bias=None):
    """`nn.Linear`: the weight is (out, in), so it multiplies from the right."""
    y = x @ weight.T
    return y if bias is None else y + bias


def layer_norm(x, weight, bias, eps=1e-5):
    """`nn.LayerNorm`: normalise over the last axis, then scale and shift."""
    mean = x.mean(axis=-1, keepdims=True)
    var = x.var(axis=-1, keepdims=True)
    return (x - mean) / np.sqrt(var + eps) * weight + bias


def rms_norm(x, weight, eps=None):
    """`nn.RMSNorm`: divide by the root mean square, then scale.

    `eps=None` is what the reference module is built with, and `F.rms_norm`
    reads that as the float32 epsilon itself — not DIM times it, which is what
    `nn.RMSNorm`'s constructor would substitute for a module built on the CPU
    with no eps. With the larger value all sixteen of these norms ran with an
    epsilon 192 times too big and the policy drifted by about 1e-4 of scale.
    """
    if eps is None:
        eps = np.finfo(np.float32).eps
    return x / np.sqrt(np.mean(x * x, axis=-1, keepdims=True) + eps) * weight


def softmax(x):
    """A numerically stable softmax over the last axis, as torch computes it."""
    e = np.exp(x - x.max(axis=-1, keepdims=True))
    return e / e.sum(axis=-1, keepdims=True)


def attention_bias(x, layer):
    """The smolgen bias, one (HEADS, 64, 64) matrix per position.

    `models.py: MHA._sq_bias` with `gab_per_square_dim = 0`: the 64 tokens are
    mean pooled into one vector, pushed up to GEN and then to `HEADS * GEN`,
    and every head's GEN values become its 4096 pair weights through the one
    tied `smolgen_shared_weight` (`einsum("hi,oi->ho")`).
    """
    pooled = x.mean(axis=0)
    y = gelu(linear(pooled, layer["self_attn.sm2.weight"], layer["self_attn.sm2.bias"]))
    y = layer_norm(y, layer["self_attn.ln1.weight"], layer["self_attn.ln1.bias"])
    y = gelu(linear(y, layer["self_attn.sm3.weight"], layer["self_attn.sm3.bias"]))
    y = layer_norm(y, layer["self_attn.ln2.weight"], layer["self_attn.ln2.bias"])
    y = y.reshape(HEADS, GEN)
    bias = np.einsum("hi,oi->ho", y, layer["smolgen_shared_weight"])
    return bias.reshape(HEADS, SQUARES, SQUARES)


def self_attention(x, layer):
    """`nn.MultiheadAttention(192, 6, bias=False)`, one sequence, no mask.

    `in_proj_weight` is q, k and v stacked, each (192, 192). Every head takes
    its own third of a projection, scaled by 1/sqrt(32), and the GAB bias is
    the `attn_mask` the reference passes -- a bias added to the scores, not a
    mask. `out_proj` mixes the heads back together and has no bias.
    """
    qkv = layer["self_attn.mha.in_proj_weight"]
    q = (x @ qkv[:DIM].T).reshape(TOKENS, HEADS, HEAD_DIM)
    k = (x @ qkv[DIM:2 * DIM].T).reshape(TOKENS, HEADS, HEAD_DIM)
    v = (x @ qkv[2 * DIM:].T).reshape(TOKENS, HEADS, HEAD_DIM)

    # (tokens, heads, head_dim) -> (heads, tokens, head_dim)
    q = q.transpose(1, 0, 2) / math.sqrt(HEAD_DIM)
    k = k.transpose(1, 0, 2)
    v = v.transpose(1, 0, 2)

    scores = np.einsum("hid,hjd->hij", q, k) + attention_bias(x, layer)
    attended = softmax(scores) @ v
    attended = attended.transpose(1, 0, 2).reshape(TOKENS, DIM)
    return linear(attended, layer["self_attn.mha.out_proj.weight"])


def encoder_block(x, layer):
    """One `EncoderOnlyBlock`: post-LayerNorm attention, then post-LN MLP."""
    x = rms_norm(x + self_attention(x, layer), layer["norm1.weight"])
    hidden = gelu(linear(x, layer["linear1.weight"], layer["linear1.bias"]))
    hidden = linear(hidden, layer["linear2.weight"], layer["linear2.bias"])
    return rms_norm(x + hidden, layer["norm2.weight"])


# ---------------------------------------------------------------------------
# The input: 64 tokens of 96 planes, plus two ELO embeddings.
# ---------------------------------------------------------------------------

def tokenize(board_state):
    """The (64, 8 * 12) piece planes for the side to move, oldest first.

    `dataset.tokenize_board` writes 12 planes per position -- 6 piece kinds for
    white then the same 6 for black -- and mirrors the board when black is to
    move, which is a rank flip (a1 becomes a8) that also swaps the colours, so
    the side to move is always the "white" planes at the bottom. `history` is
    oldest to newest, one (board, side) per ply, and a game shorter than 8
    plies repeats its oldest position at the front (`get_historical_tokens`).
    """
    if isinstance(board_state, maia.Position):
        history = [(list(slot[0]), slot[1])
                   for slot in board_state.history[-HISTORY:]]
    else:
        history = [(list(board), side) for board, side in board_state]
        history = history[-HISTORY:]

    while len(history) < HISTORY:
        history.insert(0, history[0])

    tokens = np.zeros((TOKENS, HISTORY * PLANES), dtype=np.float64)
    for step, (board, side) in enumerate(history):
        mirror = side == 1
        for rank in range(8):
            for file in range(8):
                piece = board[maia.idx(rank, file)]
                if piece == 0 or piece == maia.WALL:
                    continue
                kind = (piece - 1) % 6
                white = piece <= 6
                if mirror:
                    # A rank flip: the square moves and the colours swap.
                    square = (7 - rank) * 8 + file
                    white = not white
                else:
                    square = rank * 8 + file
                plane = step * PLANES + (kind if white else kind + 6)
                tokens[square, plane] = 1.0
    return tokens


def elo_embedding(params, elo):
    """One ELO as a 128 vector, `MAIA3Model.interpolate_elo`.

    Both `nn.Embedding(1, 128)` tables have a single row, so an ELO is only a
    mix of the two rows: clamped to 0..5000, the low row weighted by the
    fraction of 5000 and the high row by the rest.
    """
    clamped = min(max(float(elo), 0.0), 5000.0)
    low_weight = clamped / 5000.0
    high_weight = 1.0 - low_weight
    low = params["elo_embedding_low.weight"][0]
    high = params["elo_embedding_high.weight"][0]
    return low_weight * low + high_weight * high


# ---------------------------------------------------------------------------
# The forward pass.
# ---------------------------------------------------------------------------

def transformer_body(params, board, self_elo, oppo_elo, trace=None):
    """The tokens through the projection and the eight encoder blocks: (64, 192).

    Split out of `forward` because the policy head is read twice by the checks,
    once as the 4352 logits and once as the 64 x 64 score matrix they flatten.

    `trace`, when a dict is passed, collects the intermediates the port is
    bisected with: `x0` the block input after `token_projection`, `blk` the
    block input after each of the eight blocks in turn, and `xnorm` after
    `transformer.norm`. Each is a (64, 192) array, so token 0's 192 values are
    its first 192 flat elements.
    """
    tokens = tokenize(board)
    self_emb = np.tile(elo_embedding(params, self_elo), (TOKENS, 1))
    oppo_emb = np.tile(elo_embedding(params, oppo_elo), (TOKENS, 1))
    # Concat along the last axis: 96 planes, self ELO, opponent ELO -> 352.
    x = np.concatenate([tokens, self_emb, oppo_emb], axis=-1)
    x = linear(x, params["token_projection.weight"], params["token_projection.bias"])
    if trace is not None:
        trace["x0"] = x
        trace["blk"] = []

    # All eight layers tie one parameter, so each reads the same array.
    tied = params["smolgen_shared_weight"]
    for index in range(8):
        prefix = "transformer.layers.%d." % index
        layer = {"smolgen_shared_weight": tied}
        for suffix, _ in LAYER_PARAMS:
            layer[suffix] = params[prefix + suffix]
        x = encoder_block(x, layer)
        if trace is not None:
            trace["blk"].append(x)

    x = layer_norm(x, params["transformer.norm.weight"],
                   params["transformer.norm.bias"])
    if trace is not None:
        trace["xnorm"] = x
    return x


def score_matrix(params, x):
    """The 64 x 64 (from, to) policy scores of one transformer output."""
    sq_from = linear(x, params["proj_sq_from.weight"])
    sq_to = linear(x, params["proj_sq_to.weight"])
    return (sq_from @ sq_to.T) / math.sqrt(DIM), sq_to


def forward(params, board, self_elo, oppo_elo, trace=None):
    """Run the network once: (policy logits 4352, wdl logits 3, ponder scalar).

    `board` is a `maia.Position` (or the same (board, side) history list
    `tokenize` accepts) for the side to move. The policy logits are in the
    side-to-move frame, so `move_index` is how a legal move finds its score.
    `trace` is passed straight to `transformer_body`.
    """
    x = transformer_body(params, board, self_elo, oppo_elo, trace)

    # --- policy head ------------------------------------------------------
    # One 192 vector per square, projected twice, scored by a scaled dot
    # product. Row i is the from-square, column j the to-square, and the
    # flattening is row major, so logit i * 64 + j is the move i -> j.
    scores, sq_to = score_matrix(params, x)
    policy = scores.reshape(BASE_MOVES).astype(np.float64)

    # A promotion is a move from rank 7 to rank 8 and, when black is mirrored,
    # a white move, so the eight promotion rows are squares 48..55 and the
    # eight target rows 56..63. Each target square carries four biases, one per
    # promotion kind, and only the target's bias is added.
    promo_biases = linear(sq_to[56:64], params["promo_bias_proj.weight"]) * math.sqrt(DIM)
    promotions = np.empty(PROMO_MOVES, dtype=np.float64)
    slot = 0
    for from_file in range(8):
        for to_file in range(8):
            base = scores[48 + from_file, 56 + to_file]
            for kind in range(4):
                promotions[slot] = base + promo_biases[to_file, kind]
                slot += 1
    policy = np.concatenate([policy, promotions])

    # --- value and ponder heads -------------------------------------------
    pooled = layer_norm(x.mean(axis=0), params["last_ln.weight"], params["last_ln.bias"])
    value_hidden = relu(linear(pooled, params["fc_value_hid.weight"],
                               params["fc_value_hid.bias"]))
    wdl = linear(value_hidden, params["fc_value.weight"], params["fc_value.bias"])
    ponder_hidden = relu(linear(pooled, params["fc_ponder_hid.weight"],
                                params["fc_ponder_hid.bias"]))
    ponder = float(linear(ponder_hidden, params["fc_ponder.weight"],
                          params["fc_ponder.bias"])[0])
    return policy, wdl, ponder


def stage_items(trace):
    """The traced intermediates in pipeline order: x0, blk[0] .. blk[7], xnorm."""
    items = [("x0", trace["x0"])]
    items.extend(("blk[%d]" % index, block)
                 for index, block in enumerate(trace["blk"]))
    items.append(("xnorm", trace["xnorm"]))
    return items


# ---------------------------------------------------------------------------
# Maia3's move indexing, both ways.
# ---------------------------------------------------------------------------

def move_index(move):
    """The policy index of a `maia.py` move, a mailbox (from, to, promote).

    A quiet move is `square(from) * 64 + square(to)`; a promotion is 4096 plus
    the from-file, the to-file and the kind, in that order. The squares are the
    plain a1 = 0 ones -- the network's own frame is already the side to move's
    -- and the kind is `index in PROMO_PIECES`, so a queen promotion is 0.
    """
    from_m, to_m, promote = move
    from_sq = maia.sq64(from_m)
    to_sq = maia.sq64(to_m)
    if promote == 0:
        return from_sq * 64 + to_sq
    return (BASE_MOVES
            + maia.file_of(from_m) * 32
            + maia.file_of(to_m) * 4
            + KIND_OF_PIECE[promote])


def move_of_index(index):
    """The inverse of `move_index`, as a (from, to, promote) mailbox move.

    The result uses the same squares and the same promotion codes as
    `maia.py`: a kind comes back as the piece it promotes to (5 queen,
    4 rook, 3 bishop, 2 knight).
    """
    if not 0 <= index < POLICY:
        raise ValueError("policy index out of range: %d" % index)
    if index < BASE_MOVES:
        from_sq, to_sq = divmod(index, 64)
        return (maia.cell(from_sq), maia.cell(to_sq), 0)
    rest = index - BASE_MOVES
    from_file, rest = divmod(rest, 32)
    to_file, kind = divmod(rest, 4)
    return (maia.idx(6, from_file), maia.idx(7, to_file), PROMO_PIECES[kind])


def policy_move(board, policy):
    """The legal move the policy likes best: ((from, to, promote), probability).

    Ties break on the order `maia.Position.legal` produces, which is the same
    order `maia.py`'s own search walks, so the two engines pick the same move
    when the policy is the same.
    """
    probabilities = softmax(policy)
    best, best_probability = None, -1.0
    for move in board.legal():
        index = move_index(move)
        if probabilities[index] > best_probability:
            best, best_probability = move, probabilities[index]
    if best is None:
        raise ValueError("no legal moves; the game is over")
    return best, float(best_probability)


def best_move(params, board, self_elo, oppo_elo):
    """The legal move the policy likes best: ((from, to, promote), prob, wdl)."""
    policy, wdl, _ = forward(params, board, self_elo, oppo_elo)
    move, probability = policy_move(board, policy)
    return move, probability, softmax(wdl)


# ---------------------------------------------------------------------------
# Checks.
# ---------------------------------------------------------------------------

# The state_dict names and shapes the loader expects, so a checkpoint that does
# not match is a printed mismatch and not a stack trace halfway through.
TOP_PARAMS = [
    ("smolgen_shared_weight", (4096, 64)),
    ("elo_embedding_low.weight", (1, 128)),
    ("elo_embedding_high.weight", (1, 128)),
    ("token_projection.weight", (192, 352)),
    ("token_projection.bias", (192,)),
    ("transformer.norm.weight", (192,)),
    ("transformer.norm.bias", (192,)),
    ("last_ln.weight", (192,)),
    ("last_ln.bias", (192,)),
    ("fc_value_hid.weight", (192, 192)),
    ("fc_value_hid.bias", (192,)),
    ("fc_value.weight", (3, 192)),
    ("fc_value.bias", (3,)),
    ("fc_ponder_hid.weight", (192, 192)),
    ("fc_ponder_hid.bias", (192,)),
    ("fc_ponder.weight", (1, 192)),
    ("fc_ponder.bias", (1,)),
    ("proj_sq_from.weight", (192, 192)),
    ("proj_sq_to.weight", (192, 192)),
    ("promo_bias_proj.weight", (4, 192)),
]


def expected_shapes():
    """Every name and shape the code above reads, from the two lists."""
    shapes = {}
    for name, shape in TOP_PARAMS:
        shapes[name] = shape
    for layer in range(8):
        for suffix, shape in LAYER_PARAMS:
            shapes["transformer.layers.%d.%s" % (layer, suffix)] = shape
    return shapes


def check_shapes(params):
    """Print every shape the checkpoint and the code disagree on. True if none."""
    expected = expected_shapes()
    problems = []
    for name in sorted(expected):
        want = expected[name]
        if name not in params:
            problems.append("%-52s missing" % name)
        elif tuple(params[name].shape) != want:
            problems.append("%-52s %s, want %s"
                            % (name, tuple(params[name].shape), want))
    for name in sorted(params):
        if name not in expected:
            problems.append("%-52s unexpected %s" % (name, tuple(params[name].shape)))
    for line in problems:
        print("MISMATCH " + line)
    print("shapes: %d names expected, %d in the checkpoint, %d mismatched"
          % (len(expected), len(params), len(problems)))
    return not problems


def check_tied_weight(params):
    """The eight `smolgen_weight` copies are the one tied array. True if so."""
    tied = params["smolgen_shared_weight"]
    same = True
    for layer in range(8):
        name = "transformer.layers.%d.self_attn.smolgen_weight" % layer
        copied = params[name]
        if copied.shape != tied.shape or not np.array_equal(copied, tied):
            print("TIE %s differs from smolgen_shared_weight" % name)
            same = False
    print("tied smolgen weight: %s (4096 x 64 shared by 8 layers)"
          % ("identical in all 8" if same else "NOT identical"))
    return same


def check_index_round_trip(pos):
    """`move_index` and `move_of_index` agree, and quiet moves match squares."""
    moves = pos.legal()
    for move in moves:
        index = move_index(move)
        if move_of_index(index) != move:
            print("INDEX %s -> %d -> %s"
                  % (maia.uci(move), index, maia.uci(move_of_index(index))))
            return False
    print("index round trip: %d legal moves, all (from, to, promote) -> index -> move"
          % len(moves))
    return True


def check_start_position(params):
    """The moves a 1500 player should like at the start, and the index mapping.

    `e2e4` is index 12 * 64 + 28 = 796, and the logit there has to be the score
    matrix's own (e2, e4) entry -- the flat index and the matrix are two views
    of one tensor. The leading legal moves at 1500 are checked to be opening
    moves, which is what says the port is wired the right way round.
    """
    start = maia.Position()
    policy, wdl, ponder = forward(params, start, 1500, 1500)
    probabilities = softmax(policy)

    logits = []
    for move in start.legal():
        logits.append((probabilities[move_index(move)], maia.uci(move)))
    logits.sort(reverse=True)

    print("start, elo 1500, first legal move by policy:")
    for probability, name in logits[:5]:
        print("  %-6s %.6f" % (name, probability))
    print("  wdl %s  ponder %.4f"
          % (np.round(softmax(wdl), 4).tolist(), ponder))

    # The other direction: a known move's index must address the same number
    # the score matrix holds at (from, to).
    trace = {}
    x = transformer_body(params, start, 1500, 1500, trace)
    scores, _ = score_matrix(params, x)

    e2e4 = move_index((maia.idx(1, 4), maia.idx(3, 4), 0))
    want = float(scores[maia.sq64(maia.idx(1, 4)), maia.sq64(maia.idx(3, 4))])
    got = float(policy[e2e4])
    print("index 12*64+28 = %d: score matrix %.9f, policy logit %.9f, equal %s"
          % (e2e4, want, got, want == got))

    sums = (abs(probabilities.sum() - 1.0), abs(softmax(wdl).sum() - 1.0))
    print("sums: policy %.3e off 1.0, wdl %.3e off 1.0" % sums)

    names = [name for _, name in logits[:5]]
    opening = [n for n in names if n in ("e2e4", "d2d4", "g1f3", "c2c4")]
    print("leading moves %s, of the four expected opening moves %d present"
          % (names, len(opening)))

    # The intermediates the port is bisected with, printed here so the reference
    # says what they are before anything is compared against them. This is the
    # checkpoint, not the quantised table `--dump` and `--probe` read.
    print("start, elo 1500, stage intermediates (checkpoint):")
    for name, values in stage_items(trace):
        print("  %-7s %s"
              % (name, " ".join("%.17g" % v for v in values.reshape(-1)[:6])))
    want_norm = layer_norm(trace["blk"][7], params["transformer.norm.weight"],
                           params["transformer.norm.bias"])
    norm_error = float(np.abs(want_norm - trace["xnorm"]).max())
    print("  xnorm rebuilt from blk[7] by transformer.norm: worst difference %.3e"
          % norm_error)
    return (bool(opening) and want == got and max(sums) < 1e-9
            and norm_error == 0.0)


# A few positions to run the self test on: the start, an open Italian, a
# middlegame with castling still to happen, and a black-to-move position.
SELFTEST_FENS = [
    maia.START_FEN,
    "r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R b KQkq - 4 4",
    "r2q1rk1/pp1bbppp/2n1pn2/2pp4/8/1PN1PN2/PBPPBPPP/R2Q1RK1 w - - 0 9",
    "rnbq1rk1/pp2ppbp/2pp1np1/8/2PP4/2N2NP1/PP2PPBP/R1BQ1RK1 b - - 0 7",
]


def selftest(argv):
    """Shapes, the tied weight, the move mapping, then the forward pass."""
    path = CHECKPOINT
    if "--checkpoint" in argv:
        path = argv[argv.index("--checkpoint") + 1]
    started = time.time()
    params = load(path)
    print("loaded %s: %d parameters in %.2fs"
          % (os.path.basename(path), len(params), time.time() - started))

    ok = check_shapes(params)
    ok = check_tied_weight(params) and ok
    start = maia.Position()
    ok = check_index_round_trip(start) and ok
    ok = check_start_position(params) and ok

    for fen in SELFTEST_FENS:
        pos = maia.Position(fen)
        for elo in (1100, 1500, 1900):
            began = time.time()
            policy, wdl, ponder = forward(params, pos, elo, elo)
            elapsed = time.time() - began
            probabilities = softmax(policy)
            rows = sorted((probabilities[move_index(move)], maia.uci(move))
                          for move in pos.legal())
            print("")
            print("%s" % pos.fen())
            print("  elo %d  %s to move  forward pass %.3fs"
                  % (elo, "white" if pos.side == 0 else "black", elapsed))
            for probability, name in reversed(rows[-5:]):
                print("    %-6s %.6f" % (name, probability))
            print("    wdl %s  ponder %.4f"
                  % (np.round(softmax(wdl), 4).tolist(), ponder))
            print("    policy sums to %.9f" % probabilities.sum())
    print("")
    print("selftest %s" % ("ok" if ok else "FAILED"))
    return 0 if ok else 1


# ---------------------------------------------------------------------------
# The raven data module: src/net3.rav, and what tools/checkdata3.json records.
# ---------------------------------------------------------------------------

# `maia.py` is imported above for the board, the FEN parser and the legal move
# walk; it found the repository root on the way in.
ROOT = maia.ROOT

# The twenty tensors outside the encoder blocks, in table order. A name becomes
# the constant `T3_<NAME>`, and that constant is the table index. The 4096 x 64
# smolgen weight is listed once, as SMOLGEN: the checkpoint ships the same bytes
# under nine names, all eight blocks read that one window, and every offset
# after it records the window being stored once rather than eight times.
T3_GLOBALS = [
    ("ELO_LOW", "elo_embedding_low.weight"),
    ("ELO_HIGH", "elo_embedding_high.weight"),
    ("TOKEN_W", "token_projection.weight"),
    ("TOKEN_B", "token_projection.bias"),
    ("SMOLGEN", "smolgen_shared_weight"),
    ("NORM_W", "transformer.norm.weight"),
    ("NORM_B", "transformer.norm.bias"),
    ("LASTLN_W", "last_ln.weight"),
    ("LASTLN_B", "last_ln.bias"),
    ("VALUE_HID_W", "fc_value_hid.weight"),
    ("VALUE_HID_B", "fc_value_hid.bias"),
    ("VALUE_W", "fc_value.weight"),
    ("VALUE_B", "fc_value.bias"),
    ("PONDER_HID_W", "fc_ponder_hid.weight"),
    ("PONDER_HID_B", "fc_ponder_hid.bias"),
    ("PONDER_W", "fc_ponder.weight"),
    ("PONDER_B", "fc_ponder.bias"),
    ("SQ_FROM", "proj_sq_from.weight"),
    ("SQ_TO", "proj_sq_to.weight"),
    ("PROMO", "promo_bias_proj.weight"),
]

# The sixteen tensors one encoder block owns, repeated for each of the eight
# blocks with the suffix `_L<layer>`. The block's `self_attn.smolgen_weight` is
# not one of them: it is the shared window at index 4, which is why a block
# owns sixteen names here and seventeen in `LAYER_PARAMS`.
T3_LAYER = [
    ("INPROJ_W", "self_attn.mha.in_proj_weight"),
    ("OUTPROJ_W", "self_attn.mha.out_proj.weight"),
    ("SM2_W", "self_attn.sm2.weight"),
    ("SM2_B", "self_attn.sm2.bias"),
    ("LN1_W", "self_attn.ln1.weight"),
    ("LN1_B", "self_attn.ln1.bias"),
    ("SM3_W", "self_attn.sm3.weight"),
    ("SM3_B", "self_attn.sm3.bias"),
    ("LN2_W", "self_attn.ln2.weight"),
    ("LN2_B", "self_attn.ln2.bias"),
    ("LIN1_W", "linear1.weight"),
    ("LIN1_B", "linear1.bias"),
    ("LIN2_W", "linear2.weight"),
    ("LIN2_B", "linear2.bias"),
    ("NORM1_W", "norm1.weight"),
    ("NORM2_W", "norm2.weight"),
]

T3L0 = len(T3_GLOBALS)                  # 20: where layer 0's table starts
T3LS = len(T3_LAYER)                    # 16: tensors per layer
T3NL = 8                                # 8: encoder blocks, one per layer
T3N = T3L0 + T3LS * T3NL                # 148: quantised tensors in the table

NET3 = os.path.join(HERE, "..", "src", "net3.rav")
CHECK3 = os.path.join(HERE, "checkdata3.json")


def quantise(values):
    """One tensor as uint16, and the range it is a fraction of.

    Every stored tensor goes through this, and the whole of `w3` comes back out
    of it: `min + span * q / 65535`, lc0's LINEAR16 idea, so one weight costs
    two bytes instead of eight and three of them fit in one Scratch list item
    exactly.
    """
    flat = np.asarray(values, dtype=np.float64).reshape(-1)
    lo = float(flat.min())
    span = float(flat.max()) - lo
    if span > 0.0:
        q = np.rint((flat - lo) / span * 65535.0).astype(np.int64)
        q = np.clip(q, 0, 65535).astype(np.uint16)
    else:
        # A one element tensor -- ponder's bias -- has no range to be a
        # fraction of, so its single value sits at the bottom of it.
        q = np.zeros(flat.size, dtype=np.uint16)
    return q, lo, span


def dequantise(q, lo, span):
    """What a uint16 payload stands for, element for element."""
    return lo + span * np.asarray(q, dtype=np.float64) / 65535.0


def t3_tensor(suffix, layer, key, values):
    """One table record: what the constant is called and where the tensor lives."""
    shape = tuple(values.shape)
    q, lo, span = quantise(values)
    return {"suffix": suffix, "layer": layer, "key": key,
            "const": "T3_" + suffix + ("" if layer is None else "_L%d" % layer),
            "num": int(q.size), "row": int(shape[0]),
            "col": int(shape[1]) if len(shape) > 1 else 1,
            "q": q, "min": lo, "span": span, "at": 0}


def t3_records(params):
    """The 148 stored tensors in table order, quantised and placed.

    A record carries the four numbers the engine reads a tensor through: the
    offset `t3at` it starts at in the packed stream, its element count `t3num`
    and its shape `t3row` x `t3col`. A weight is stored row major, so element
    (o, i) is at `t3at + o * t3col + i`.
    """
    records = []
    for name, key in T3_GLOBALS:
        records.append(t3_tensor(name, None, key, params[key]))
    for layer in range(T3NL):
        for suffix, key in T3_LAYER:
            full = "transformer.layers.%d.%s" % (layer, key)
            records.append(t3_tensor(suffix, layer, full, params[full]))
    at = 0
    for rec in records:
        rec["at"] = at
        at += rec["num"]
    return records


def t3_biases(records):
    """`b3`, every bias concatenated in table order, and where each one sits.

    A `_W` reads the `_B` beside it and a `_B` is a bias of its own, so
    `t3bat[k]` is where the bias of tensor k starts in `b3` and `t3bn[k]` is how
    many values there are; a tensor with no bias gets -1 and 0. The values are
    the dequantised ones, so the engine and these checks agree to the bit.
    """
    b3, start, count = [], {}, {}
    for rec in records:
        if rec["suffix"].endswith("_B"):
            start[(rec["suffix"], rec["layer"])] = len(b3)
            count[(rec["suffix"], rec["layer"])] = rec["num"]
            b3.extend(dequantise(rec["q"], rec["min"], rec["span"]).tolist())
    bat, bn = [], []
    for rec in records:
        partner = (rec["suffix"], rec["layer"])
        if rec["suffix"].endswith("_W") and not rec["suffix"].endswith("_B"):
            partner = (rec["suffix"][:-2] + "_B", rec["layer"])
        bat.append(start.get(partner, -1))
        bn.append(count.get(partner, 0))
    return b3, bat, bn


def t3_dequantised(params, records):
    """The network as the engine will hold it: every stored tensor read back out
    of its own uint16 payload, and the eight smolgen weights the one window.

    The checks run on this and not on the checkpoint. Sixteen bits per weight is
    a real change to the numbers, and this is what `src/engine3.rav` computes,
    so this is what the checks have to agree with.
    """
    stored = {rec["key"]: rec for rec in records}
    out = {}
    for name in params:
        key = ("smolgen_shared_weight"
               if name.endswith(".self_attn.smolgen_weight") else name)
        rec = stored[key]
        out[name] = dequantise(rec["q"], rec["min"], rec["span"]).reshape(params[name].shape)
    return out


# ---------------------------------------------------------------------------
# --export: src/net3.rav.
# ---------------------------------------------------------------------------

def t3_body(text, name):
    """The text between the brackets of `pub var <name>: list<num> = [...];`."""
    tag = "pub var %s: list<num> = [" % name
    i = text.index(tag) + len(tag)
    return text[i:text.index("];", i)]


def t3_verify(path, params, records):
    """The round trip, out of the file the engine reads and not out of memory.

    The packed stream, the offsets, the counts and the ranges come back out of
    `net3.rav` as text, are unpacked the way `load3` has to unpack them, and are
    dequantised with `min + span * q / 65535`. A 16 bit fraction is wrong by
    half a step at most, so every tensor's worst error has to be under
    `span / 65535 / 2`, and `w3` has to be exactly as long as the table says.
    """
    text = open(path).read()
    words = np.array([int(x) for x in t3_body(text, "wq3").split(",")], dtype=np.int64)
    # Every table counts from zero here and from one in Scratch, so the record
    # this file calls `i` is item `i + 1` of the list.
    at = [int(x) for x in t3_body(text, "t3at").split(",")]
    num = [int(x) for x in t3_body(text, "t3num").split(",")]
    lo = [float(x) for x in t3_body(text, "t3min").split(",")]
    span = [float(x) for x in t3_body(text, "t3span").split(",")]

    q = np.empty(words.size * 3, dtype=np.uint16)
    q[0::3] = words % 65536
    q[1::3] = (words // 65536) % 65536
    q[2::3] = (words // 65536 // 65536) % 65536

    worst, where, limit = 0.0, "", 0.0
    for i, rec in enumerate(records):
        got = lo[i] + span[i] * q[at[i]:at[i] + num[i]].astype(np.float64) / 65535.0
        want = np.asarray(params[rec["key"]], dtype=np.float64).reshape(-1)
        error = float(np.abs(got - want).max())
        if error > worst:
            worst, where = error, rec["const"]
        limit = max(limit, span[i] / 65535.0 / 2.0)

    length = t3_body(text, "w3").count(",") + 1
    print("read back %s: %d tensors, %d weights, %d packed items, w3 %d long"
          % (os.path.basename(path), len(num), sum(num), words.size, length))
    print("round trip: worst error %.9g at %s, half a step is %.9g"
          % (worst, where, limit))
    return 0 if length == sum(num) and worst <= limit else 1


def export():
    """Write `src/net3.rav`, then read its numbers back and redo the round trip."""
    params = load()
    records = t3_records(params)
    assert len(records) == T3N, len(records)
    b3, bat, bn = t3_biases(records)

    # One stream, packed once: `tensor k` element `i` is at `t3at[k] + i` of it,
    # which is item `(t3at[k] + i) // 3` at position `(t3at[k] + i) % 3`. Packing
    # tensor by tensor instead would pad every one of them to three.
    total = sum(rec["num"] for rec in records)
    packed = maia.pack_weights(np.concatenate([rec["q"] for rec in records]))
    assert len(packed) == (total + 2) // 3, (len(packed), total)
    # The one tied window: index 4 is the smolgen weight and no layer has a
    # second copy of it, so 148 tensors cover all 156 `state_dict` names.
    assert records[4]["const"] == "T3_SMOLGEN" and records[4]["num"] == 4096 * 64
    for rec in records:
        assert rec["layer"] is None or "smolgen" not in rec["key"], rec["key"]

    out = ["// Maia3 3M, generated by tools/maia3.py --export. Do not edit.",
           "//",
           "// The checkpoint holds 156 tensors; it is 148 of them that are",
           "// stored, because the 4096 x 64 smolgen weight ships eight identical",
           "// times, once per encoder block, and all eight blocks read the one",
           "// window at T3_SMOLGEN. Each of the 148 is quantised to two bytes",
           "// read as a fraction of its own range, so three of them fit in one",
           "// Scratch list item and a network this size is one file. `w3` is the",
           "// working list `load3` fills, `b3` holds every bias already",
           "// dequantised, and the `t3*` lists place each tensor: `t3at` is where",
           "// it starts in the packed stream, `t3row` x `t3col` is its shape as a",
           "// matrix, and `t3bat`/`t3bn` are the bias it reads out of `b3`.",
           ""]

    def var(name, values, ints=False):
        # Seventeen digits, because every one of these has to come back as the
        # same float64: a range is a float32 value widened, and nine digits
        # would round it to a different float64 than the one it came from.
        fmt = "%d" if ints else "%.17g"
        out.append("pub var %s: list<num> = [%s];" % (name, ",".join(fmt % v for v in values)))
        out.append("")

    out.append("pub const T3N: num = %d;" % len(records))
    out.append("pub const T3W: num = %d;" % total)
    out.append("pub const T3PACK: num = %d;" % len(packed))
    # A Scratch list counts from one and this file counts from zero, so every
    # index is written one higher than the record it names: item `T3_TOKEN_W`
    # of `t3at` is the token projection's offset, and reading the table at `k`
    # is reading the record this file calls `k - 1`.
    out.append("pub const T3L0: num = %d;" % (T3L0 + 1))
    out.append("pub const T3LS: num = %d;" % T3LS)
    out.append("pub const T3NL: num = %d;" % T3NL)
    out.append("")
    for i, rec in enumerate(records):
        out.append("pub const %s: num = %d;" % (rec["const"], i + 1))
    out.append("")
    # The working weight list, and the right length. Scratch refuses to
    # `add to list` past 200,000 items, so a list this long cannot be built by
    # appending: it is written here, once, and `load3` overwrites it in place.
    var("w3", [0] * total, True)
    var("wq3", packed, True)
    var("t3min", [rec["min"] for rec in records])
    var("t3span", [rec["span"] for rec in records])
    var("t3at", [rec["at"] for rec in records], True)
    var("t3num", [rec["num"] for rec in records], True)
    var("t3row", [rec["row"] for rec in records], True)
    var("t3col", [rec["col"] for rec in records], True)
    var("b3", b3)
    var("t3bat", bat, True)
    var("t3bn", bn, True)
    var("elo3low", dequantise(records[0]["q"], records[0]["min"], records[0]["span"]))
    var("elo3high", dequantise(records[1]["q"], records[1]["min"], records[1]["span"]))

    with open(NET3, "w") as f:
        f.write("\n".join(out))
    print("tensors %d, weights %d, packed %d, biases %d"
          % (len(records), total, len(packed), len(b3)))
    print("wrote %s (%.1f MiB)" % (os.path.relpath(NET3, ROOT),
                                   os.path.getsize(NET3) / 1048576))
    return t3_verify(NET3, params, records)


# ---------------------------------------------------------------------------
# --dump: tools/checkdata3.json.
# ---------------------------------------------------------------------------

T3_ELOS = (1100, 1500, 1900)
T3_SAMPLE_W = 8   # weight values taken from each weight tensor
T3_SAMPLE_B = 4   # and from each bias tensor

# The two cases that carry the stage intermediates. `blk` alone is 8 x 12288
# numbers per case, so recording it for all twelve would take checkdata3.json
# from about 1 MiB to about 25 MiB. The first is the case `tools/check.mjs` runs
# the whole network on; the second is the deepest line, whose blocks see a board
# furthest from the start.
T3_TRACED = (
    ("start", 1500),
    ("e2e4 e7e5 g1f3 b8c6 f1c4 g8f6 f3g5 d7d5 e4d5 c6a5", 1500),
)


def t3_weights():
    """The checkpoint, its table, the bias offsets, and the quantised network.

    The quantised network is the one the raven engine computes, so it is what
    `--dump` records and what `--probe` reads.
    """
    params = load()
    records = t3_records(params)
    _, bat, _ = t3_biases(records)
    return params, records, bat, t3_dequantised(params, records)


def t3_sample(records, bat):
    """A spread of dequantised values, each with the table index it came from
    and the offset of the element inside that tensor.

    A weight sample also carries its index in `w3` and a bias sample its index
    in `b3`, so the engine's unpacking can be checked one element at a time
    rather than only end to end.
    """
    weights, biases = [], []
    for i, rec in enumerate(records):
        bias = rec["suffix"].endswith("_B")
        step = max(1, rec["num"] // (T3_SAMPLE_B if bias else T3_SAMPLE_W))
        values = dequantise(rec["q"], rec["min"], rec["span"])
        base = bat[i] if bias else rec["at"]
        for off in range(0, rec["num"], step):
            (biases if bias else weights).append([i, off, base + off, float(values[off])])
    return {"w": weights, "b": biases}


def t3_jdump(value):
    """JSON whose every float is written with `%.17g`.

    `json` writes the shortest decimal that round trips instead, which is the
    same number and not the same text; the file writes floats the way
    `net3.rav` writes its ranges, so a reader can compare the two as text.
    """
    if isinstance(value, str):
        return json.dumps(value)
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return "%d" % value
    if isinstance(value, float):
        return "%.17g" % value
    if isinstance(value, (list, tuple)):
        return "[" + ",".join(t3_jdump(v) for v in value) + "]"
    if isinstance(value, dict):
        return "{" + ",".join(json.dumps(k) + ":" + t3_jdump(v)
                              for k, v in value.items()) + "}"
    raise TypeError("cannot write %r as JSON" % (type(value),))


def dump():
    """What the raven port is checked against.

    The start and three more of `maia.py --dump`'s positions, each at three
    ELOs, through the quantised network -- the one the raven engine computes --
    recording the whole 4352 logit policy, the three WDL logits, the ponder
    logit and the legal move it picks. The unquantised model's move is printed
    beside it: 16 bits per weight moves a logit, and a reader has to see
    whether it moved the choice. One position also records its 64 x 96 token
    block, so the encoder can be checked on its own.

    The two cases in `T3_TRACED` also record `x0`, `blk` and `xnorm`, each a
    flat row-major list (token major, then the 192 values), so the port can be
    bisected stage by stage rather than only at the move.
    """
    params, records, bat, net = t3_weights()

    cases, agreed = [], 0
    for moves in maia.NET_CASES:
        pos = maia.Position()
        for text in moves:
            maia.play_uci(pos, text)
        name = " ".join(moves) or "start"
        for elo in T3_ELOS:
            trace = {} if (name, elo) in T3_TRACED else None
            policy, wdl, ponder = forward(net, pos, elo, elo, trace)
            move, _ = policy_move(pos, policy)
            unquantised, _ = policy_move(pos, forward(params, pos, elo, elo)[0])
            agreed += move == unquantised
            case = {"name": name, "fen": pos.fen(), "elo": elo,
                    "state": maia.state_of(pos),
                    "policy": [float(x) for x in policy],
                    "wdl": [float(x) for x in wdl],
                    "ponder": float(ponder),
                    "move": [int(x) for x in move],
                    "move_packed": maia.pack(move),
                    "uci": maia.uci(move)}
            if not cases:
                case["tokens"] = tokenize(pos).astype(int).tolist()
            if trace is not None:
                case["x0"] = [float(v) for v in trace["x0"].reshape(-1)]
                case["blk"] = [[float(v) for v in block.reshape(-1)]
                               for block in trace["blk"]]
                case["xnorm"] = [float(v) for v in trace["xnorm"].reshape(-1)]
            cases.append(case)
            print("%-44s elo %d  quantised %-6s unquantised %-6s %s"
                  % (name[:44], elo, maia.uci(move), maia.uci(unquantised),
                     "same move" if move == unquantised else "DIFFERENT MOVE"))

    weights = t3_sample(records, bat)
    with open(CHECK3, "w") as f:
        f.write(t3_jdump({"cases": cases, "weights": weights,
                          "traced": [[name, elo] for name, elo in T3_TRACED]}))
    print("chosen moves: %d of %d the quantised model and the unquantised one agree on"
          % (agreed, len(cases)))
    print("stages: %s carry x0, blk[0]..blk[7] and xnorm"
          % ", ".join("%s @ %d" % (name, elo) for name, elo in T3_TRACED))
    print("wrote %s: %d cases, %d weight samples, %d bias samples, %.1f MiB"
          % (os.path.relpath(CHECK3, ROOT), len(cases), len(weights["w"]),
             len(weights["b"]), os.path.getsize(CHECK3) / 1048576))
    return 0 if agreed == len(cases) else 1


def probe(argv):
    """One flat index of every traced stage, for the position being bisected.

    `--probe <index> [--fen <fen>] [--elo <elo>]` runs the quantised network --
    the one `--dump` records -- and prints `x0`, `blk[0]` .. `blk[7]` and
    `xnorm` at that flat index, ten numbers one per line, so one cell can be
    read off a shell and compared with the port's own list. The default is the
    start position at ELO 1500, the case `tools/check.mjs` runs.
    """
    index = int(argv[argv.index("--probe") + 1])
    fen = argv[argv.index("--fen") + 1] if "--fen" in argv else maia.START_FEN
    elo = int(argv[argv.index("--elo") + 1]) if "--elo" in argv else 1500
    pos = maia.Position(fen)
    _, _, _, net = t3_weights()

    trace = {}
    forward(net, pos, elo, elo, trace)
    width = trace["x0"].size
    if not 0 <= index < width:
        raise ValueError("flat index %d is outside a stage (%d values)"
                         % (index, width))
    print("%s  elo %d  %s to move  flat index %d"
          % (pos.fen(), elo, "white" if pos.side == 0 else "black", index))
    for stage, values in stage_items(trace):
        print("%s[%d] = %.17g" % (stage, index, float(values.reshape(-1)[index])))
    return 0


def main(argv):
    if "--export" in argv:
        return export()
    if "--dump" in argv:
        return dump()
    if "--probe" in argv:
        return probe(argv)
    return selftest(argv)


if __name__ == "__main__":
    if ("--selftest" in sys.argv or "--export" in sys.argv or "--dump" in sys.argv
            or "--probe" in sys.argv):
        sys.exit(main(sys.argv[1:]))
    print(__doc__.strip())
    sys.exit(0)
