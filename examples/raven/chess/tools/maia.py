#!/usr/bin/env python3
"""Maia in Python: the engine the raven port is checked against.

This reads a lc0 `.pb.gz` network, folds its batch norm the way lc0 folds it,
turns a position into the classical 112 input planes, runs the SE residual
forward pass and picks the legal move its policy likes best. There is no
training here and no search: it is weights in, move out, and every step is
written to be translatable, statement for statement, into `src/`.

    python examples/raven/chess/tools/maia.py            # the checks
    python examples/raven/chess/tools/maia.py --export   # regenerate src/net.rav

The policy tables (which conv cell means which move) are lc0's own: the first
run parses them out of `ref/_ptmp/lc0/` and caches them in
`tools/policy_tables.json`, which is what later runs read.
"""

import gzip
import json
import os
import re
import struct
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
LC0 = os.path.join(ROOT, "ref", "_ptmp", "lc0")
WEIGHTS = os.path.join(ROOT, "ref", "maia-chess", "maia_weights")
TABLES = os.path.join(HERE, "policy_tables.json")

START_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
KIND = {"P": 1, "N": 2, "B": 3, "R": 4, "Q": 5, "K": 6}
LETTERS = " PNBRQK"
PROMO_CHAR = {0: 0, "n": 2, "b": 3, "r": 4, "q": 5}

WALL = 99
# Mailbox offsets on a 10 wide board: a step of +10 is a rank up.
KNIGHT_D = (-21, -19, -12, -8, 8, 12, 19, 21)
KING_D = (-11, -10, -9, -1, 1, 9, 10, 11)
BISHOP_D = (-11, -9, 9, 11)
ROOK_D = (-10, -1, 1, 10)


def idx(rank, file):
    """The mailbox cell of a rank and file, both 0 based.

    The board sits on rows 2..9 of a 12 row table, so the knight's two-rank
    jump and the slider's one-rank step both land on a border cell rather than
    off the end of the list.
    """
    return (rank + 2) * 10 + file + 1


def rank_of(m):
    return m // 10 - 2


def file_of(m):
    return m % 10 - 1


def sq64(m):
    """The 0..63 square of a mailbox cell, a1 = 0, h8 = 63."""
    return rank_of(m) * 8 + file_of(m)


def cell(sq):
    """The mailbox cell of a 0..63 square."""
    return idx(sq // 8, sq % 8)


def flip64(sq):
    """What was on rank 1 is on rank 8."""
    return (7 - sq // 8) * 8 + sq % 8


def name(m):
    return "abcdefgh"[file_of(m)] + str(rank_of(m) + 1)


def mine(piece, side):
    return piece != 0 and piece != WALL and (piece <= 6) == (side == 0)


# ---------------------------------------------------------------------------
# The .pb.gz: a protobuf, read without protoc.
# ---------------------------------------------------------------------------

def fields(buf):
    i = 0
    while i < len(buf):
        key = 0
        shift = 0
        while True:
            b = buf[i]
            i += 1
            key |= (b & 0x7F) << shift
            shift += 7
            if not b & 0x80:
                break
        number, wire = key >> 3, key & 7
        if wire == 0:
            value = 0
            shift = 0
            while True:
                b = buf[i]
                i += 1
                value |= (b & 0x7F) << shift
                shift += 7
                if not b & 0x80:
                    break
            yield number, value
        elif wire == 1:
            yield number, buf[i:i + 8]
            i += 8
        elif wire == 2:
            length = 0
            shift = 0
            while True:
                b = buf[i]
                i += 1
                length |= (b & 0x7F) << shift
                shift += 7
                if not b & 0x80:
                    break
            yield number, buf[i:i + length]
            i += length
        elif wire == 5:
            yield number, buf[i:i + 4]
            i += 4
        else:
            raise ValueError("wire type %d" % wire)


CONV = {1: "weights", 2: "biases", 3: "bn_means", 4: "bn_stddivs",
        5: "bn_gammas", 6: "bn_betas"}


def layer(buf):
    """One Layer, dequantised. `layer_raw` keeps the payload; this is what it means."""
    return layer_values(layer_raw(buf))


def conv_block(buf):
    """A ConvBlock, its batch norm folded in, as lc0 folds it at load time."""
    layers = {}
    for number, value in fields(buf):
        if number in CONV:
            layers[CONV[number]] = layer(value)
    w = layers.get("weights", np.zeros(0))
    if w.size == 0:
        return {"weights": w, "biases": np.zeros(0)}
    outs = layers.get("bn_means", np.zeros(0)).size
    if outs == 0:
        return {"weights": w, "biases": layers.get("biases", np.zeros(outs))}
    gamma = layers.get("bn_gammas", np.ones(outs)).copy()
    beta = layers.get("bn_betas", np.zeros(outs))
    mean = layers.get("bn_means").copy()
    std = layers.get("bn_stddivs", np.ones(outs))
    bias = layers.get("biases", np.zeros(outs))
    gamma = gamma * (1.0 / np.sqrt(std + 1e-5))
    mean = mean - bias
    w = w.reshape(outs, -1) * gamma[:, None]
    return {"weights": w.reshape(-1),
            "biases": -gamma * mean + beta}


def layer_raw(buf):
    """A Layer as the file holds it: the LINEAR16 payload, and the range it is a
    fraction of. This is the form the export keeps, because three of those fit in
    one Scratch list item where one of the floats they stand for does not."""
    info = {}
    for number, value in fields(buf):
        if number == 1:
            info["min"] = struct.unpack("<f", value)[0]
        elif number == 2:
            info["max"] = struct.unpack("<f", value)[0]
        elif number == 3:
            info["data"] = value
    if "data" not in info:
        return {"q": np.zeros(0, dtype=np.uint16), "min": 0.0, "max": 0.0}
    lo = info.get("min", 0.0)
    hi = info.get("max", 0.0)
    if lo == 0.0 and hi == 0.0:
        # A layer that is all zero is written as raw floats, not as a fraction.
        return {"q": np.zeros(len(info["data"]) // 4, dtype=np.uint16), "min": 0.0, "max": 0.0}
    return {"q": np.frombuffer(info["data"], dtype="<u2").copy(), "min": lo, "max": hi}


def layer_values(layer):
    """What a payload stands for: lc0 reads the two bytes as a fraction of the
    layer's own range and interpolates between min and max."""
    if layer is None:
        return np.zeros(0)
    return layer["min"] + (layer["max"] - layer["min"]) * layer["q"].astype(np.float64) / 65535.0


CONV_LAYERS = {1: "weights", 2: "biases", 3: "bn_means", 4: "bn_stddivs",
               5: "bn_gammas", 6: "bn_betas"}
SE_LAYERS = {1: "w1", 2: "b1", 3: "w2", 4: "b2"}


def conv_block_raw(buf):
    out = {}
    for number, value in fields(buf):
        if number in CONV_LAYERS:
            out[CONV_LAYERS[number]] = layer_raw(value)
    return out


def se_unit_raw(buf):
    out = {}
    for number, value in fields(buf):
        if number in SE_LAYERS:
            out[SE_LAYERS[number]] = layer_raw(value)
    return out


def add_conv(layers, buf, outs, inch, kernel=3):
    """A convolution, folded if it has a batch norm and copied out if it has not."""
    block = conv_block_raw(buf)
    per = inch * kernel * kernel
    if "bn_means" not in block:
        layers.append({"w": block["weights"], "fold": False, "out": outs, "per": per,
                       "beta": block["biases"]})
        return
    layers.append({"w": block["weights"], "fold": True, "out": outs, "per": per,
                   "gamma": layer_values(block["bn_gammas"]),
                   "mean": layer_values(block["bn_means"]),
                   "std": layer_values(block["bn_stddivs"]),
                   "beta": layer_values(block["bn_betas"])})


def add_flat(layers, weights, bias, outs, per):
    """A layer with no batch norm at all: a squeeze-excitation bottleneck, or a
    fully connected head, where the file's own bias is the whole story. `bias`
    may be missing, which lc0 reads as a layer of zeros."""
    if bias is None:
        bias = {"q": np.zeros(outs, dtype=np.uint16), "min": 0.0, "max": 0.0}
    layers.append({"w": weights, "fold": False, "out": outs, "per": per, "beta": bias})


def load_raw(path):
    """Every weight layer of a network, in the order the forward pass reads them,
    raw, and with the batch norm each one has to be folded into it.

    That is the order `src/engine.rav` indexes `w` and `b` in, so the two have to
    stay in step.
    """
    raw = gzip.open(path, "rb").read()
    net = {}
    residual = []
    for number, value in fields(raw):
        if number != 10:
            continue
        for wn, wv in fields(value):
            if wn == 2:
                residual.append(wv)
            else:
                net[wn] = wv

    layers = []
    add_conv(layers, net[1], 64, 112)
    for rv in residual:
        block = {}
        for rn, rw in fields(rv):
            if rn == 3:
                block["se"] = se_unit_raw(rw)
            else:
                block[rn] = rw
        add_conv(layers, block[1], 64, 64)
        add_conv(layers, block[2], 64, 64)
        se = block["se"]
        add_flat(layers, se["w1"], se["b1"], 8, 64)
        add_flat(layers, se["w2"], se["b2"], 128, 8)
    add_conv(layers, net[11], 64, 64)
    add_conv(layers, net[3], 80, 64)
    add_conv(layers, net[6], 32, 64, kernel=1)
    add_flat(layers, layer_raw(net[7]), layer_raw(net[8]), 128, 2048)
    add_flat(layers, layer_raw(net[9]), layer_raw(net[10]), 3, 128)
    return layers


def expand(layers):
    """The raw layers turned back into the two lists the forward pass reads: the
    batch norm folded in, the way `load_net` folds it at load time. This is what
    `src/engine.rav`'s `load_bot` has to reproduce."""
    w, b = [], []
    for rec in layers:
        vals = layer_values(rec["w"])
        if rec["fold"]:
            gamma = rec["gamma"] * (1.0 / np.sqrt(rec["std"] + 1e-5))
            vals = vals.reshape(rec["out"], -1) * gamma[:, None]
            b.append(-gamma * rec["mean"] + rec["beta"])
        else:
            b.append(layer_values(rec["beta"]))
        w.append(vals.reshape(-1))
    return np.concatenate(w), np.concatenate(b)


def se_unit(buf):
    out = {}
    for number, value in fields(buf):
        out[{1: "w1", 2: "b1", 3: "w2", 4: "b2"}[number]] = layer(value)
    return out


def load_net(path):
    """Every weight of a lc0 network, batch norm already folded."""
    raw = gzip.open(path, "rb").read()
    net = {"residual": []}
    for number, value in fields(raw):
        if number != 10:
            continue
        for wn, wv in fields(value):
            if wn == 1:
                net["input"] = conv_block(wv)
            elif wn == 2:
                block = {}
                for rn, rv in fields(wv):
                    if rn == 1:
                        block["conv1"] = conv_block(rv)
                    elif rn == 2:
                        block["conv2"] = conv_block(rv)
                    elif rn == 3:
                        block["se"] = se_unit(rv)
                net["residual"].append(block)
            elif wn == 3:
                net["policy"] = conv_block(wv)
            elif wn == 6:
                net["value"] = conv_block(wv)
            elif wn == 7:
                net["ip1_val_w"] = layer(wv)
            elif wn == 8:
                net["ip1_val_b"] = layer(wv)
            elif wn == 9:
                net["ip2_val_w"] = layer(wv)
            elif wn == 10:
                net["ip2_val_b"] = layer(wv)
            elif wn == 11:
                net["policy1"] = conv_block(wv)
    return net


# ---------------------------------------------------------------------------
# The move tables, out of lc0's own sources.
# ---------------------------------------------------------------------------

def parse_tables():
    text = open(os.path.join(LC0, "policy_map.h")).read()
    body = text[text.index("kConvPolicyMap[] = {") + len("kConvPolicyMap[] = {"):]
    body = body[:body.index("};")]
    table = [int(x) for x in re.findall(r"-?\d+", body)]
    assert len(table) == 73 * 64, len(table)

    text = open(os.path.join(LC0, "encoder.cc")).read()
    body = text[text.index("kMoveStrs[] = {") + len("kMoveStrs[] = {"):]
    body = body[:body.index("};")]
    moves = re.findall(r'"([a-h][1-8][a-h][1-8][nbrq]?)"', body)
    assert len(moves) == 1858, len(moves)
    return {"conv_policy_map": table, "moves": moves}


def policy_tables():
    if os.path.exists(TABLES):
        return json.load(open(TABLES))
    data = parse_tables()
    json.dump(data, open(TABLES, "w"))
    return data


def move_tables(tables):
    """The tables the engine needs to turn a legal move into a policy score.

    A move is named by lc0 squares and by a promotion code, both in lc0's own
    frame: the board of the side to move, so for black they count from h8.
    `frame()` below turns a square back into a real one.

    `cell_of_move[j]` is the conv cell `head` score for move `j` lives in: lc0's
    kConvPolicyMap is a bijection from its 1858 non-negative entries onto
    0..1857, so the inverse exists and is what lets the engine walk its legal
    moves instead of all 4672 cells.
    """
    n = len(tables["moves"])
    src = [0] * n
    dst = [0] * n
    promo = [0] * n
    for i, text in enumerate(tables["moves"]):
        src[i] = (ord(text[1]) - 49) * 8 + ord(text[0]) - 97
        dst[i] = (ord(text[3]) - 49) * 8 + ord(text[2]) - 97
        promo[i] = PROMO_CHAR[text[4]] if len(text) == 5 else 0

    cell_of_move = [-1] * n
    for i, j in enumerate(tables["conv_policy_map"]):
        if j >= 0:
            assert cell_of_move[j] < 0, j
            cell_of_move[j] = i

    mv_base = [-1] * 4096
    promo_base = [-1] * 4096
    for i in range(n):
        key = src[i] * 64 + dst[i]
        if promo[i] == 0:
            mv_base[key] = i
        elif promo_base[key] < 0:
            promo_base[key] = i
    # lc0 writes the three under-promotions of a pair consecutively, queen
    # first, and never a knight: a knight promotion has no policy index at all.
    for key in range(4096):
        if promo_base[key] >= 0:
            first = promo_base[key]
            assert promo[first:first + 3] == [5, 4, 3], (key, promo[first:first + 3])
    return {"src": src, "dst": dst, "promo": promo, "cell_of_move": cell_of_move,
            "mv_base": mv_base, "promo_base": promo_base}


PROMO_SLOT = {5: 0, 4: 1, 3: 2}


def policy_index(pos, move, tables):
    """Where `move` scores in the policy, or -1 when lc0 encodes no such move."""
    from_m, to_m, promote = move
    key = frame(pos, sq64(from_m)) * 64 + frame(pos, sq64(to_m))
    if promote == 0:
        return tables["mv_base"][key]
    if promote not in PROMO_SLOT:
        return -1
    base = tables["promo_base"][key]
    return -1 if base < 0 else base + PROMO_SLOT[promote]


# ---------------------------------------------------------------------------
# The board. A 10x10 mailbox, the same one the raven port keeps.
# ---------------------------------------------------------------------------

class Position:
    def __init__(self, fen=START_FEN):
        self.blank()
        self.history = []
        self.set_fen(fen)

    def blank(self):
        self.board = [0 if 20 <= m < 100 and m % 10 not in (0, 9) else WALL
                      for m in range(120)]

    def set_fen(self, fen):
        parts = fen.split()
        self.blank()
        for r, row in enumerate(parts[0].split("/")):
            f = 0
            for ch in row:
                if ch.isdigit():
                    f += int(ch)
                else:
                    self.board[idx(7 - r, f)] = KIND[ch.upper()] + (0 if ch.isupper() else 6)
                    f += 1
        self.side = 0 if parts[1] == "w" else 1
        cast = parts[2]
        self.castle = (1 if "K" in cast else 0) | (2 if "Q" in cast else 0) | \
                      (4 if "k" in cast else 0) | (8 if "q" in cast else 0)
        self.ep = cell(idx(int(parts[3][1]) - 1, ord(parts[3][0]) - 97)) if parts[3] != "-" else 0
        self.rule50 = int(parts[4]) if len(parts) > 4 else 0
        self.history = [self.snapshot(0)]

    def snapshot(self, reps):
        return [tuple(self.board), self.side, self.castle, self.ep, self.rule50, reps]

    def same_board(self, slot):
        return slot[0] == tuple(self.board) and slot[1] == self.side and \
            slot[2] == self.castle and slot[3] == self.ep

    def repetitions(self):
        """lc0's PositionHistory::ComputeLastMoveRepetitions, for the new tail."""
        if self.rule50 < 4:
            return 0
        i = len(self.history) - 4
        while i >= 0:
            if self.same_board(self.history[i]):
                return 1 + self.history[i][5]
            if self.history[i][4] < 2:
                return 0
            i -= 2
        return 0

    def fen(self):
        rows = []
        for r in range(7, -1, -1):
            row, empty = "", 0
            for f in range(8):
                p = self.board[idx(r, f)]
                if p == 0:
                    empty += 1
                else:
                    if empty:
                        row += str(empty)
                        empty = 0
                    ch = LETTERS[(p - 1) % 6 + 1]
                    row += ch if p <= 6 else ch.lower()
            if empty:
                row += str(empty)
            rows.append(row)
        cast = "".join(ch for bit, ch in ((1, "K"), (2, "Q"), (4, "k"), (8, "q")) if self.castle & bit)
        return "%s %s %s %s %d" % ("/".join(rows), "wb"[self.side], cast or "-",
                                   name(self.ep) if self.ep else "-", self.rule50)

    def king(self, side):
        target = 6 if side == 0 else 12
        for r in range(8):
            for f in range(8):
                if self.board[idx(r, f)] == target:
                    return idx(r, f)
        raise ValueError("no king")

    def attacked(self, m, by):
        """Whether side `by` attacks the mailbox cell m."""
        board = self.board
        pawn = 1 if by == 0 else 7
        if by == 0:
            if board[m - 9] == pawn or board[m - 11] == pawn:
                return True
        else:
            if board[m + 9] == pawn or board[m + 11] == pawn:
                return True
        knight = 2 if by == 0 else 8
        for d in KNIGHT_D:
            if board[m + d] == knight:
                return True
        king = 6 if by == 0 else 12
        for d in KING_D:
            if board[m + d] == king:
                return True
        rook = 4 if by == 0 else 10
        queen = 5 if by == 0 else 11
        for d in ROOK_D:
            q = m + d
            while board[q] == 0:
                q += d
            if board[q] == rook or board[q] == queen:
                return True
        bishop = 3 if by == 0 else 9
        for d in BISHOP_D:
            q = m + d
            while board[q] == 0:
                q += d
            if board[q] == bishop or board[q] == queen:
                return True
        return False

    def in_check(self, side):
        return self.attacked(self.king(side), 1 - side)

    def pseudo(self):
        """Every move that follows the piece rules, checks left to `legal`."""
        board = self.board
        side = self.side
        out = []

        def free(q):
            return q != WALL and not mine(q, side)

        def enemy(q):
            return q != 0 and q != WALL and not mine(q, side)

        for r in range(8):
            for f in range(8):
                m = idx(r, f)
                p = board[m]
                if not mine(p, side):
                    continue
                kind = (p - 1) % 6 + 1
                if kind == 1:
                    up = 10 if side == 0 else -10
                    home = 1 if side == 0 else 6
                    last = 6 if side == 0 else 1
                    if board[m + up] == 0:
                        if r == last:
                            for q in (5, 4, 3, 2):
                                out.append((m, m + up, q))
                        else:
                            out.append((m, m + up, 0))
                            if r == home and board[m + 2 * up] == 0:
                                out.append((m, m + 2 * up, 0))
                    for d in (up - 1, up + 1):
                        q = board[m + d]
                        if enemy(q):
                            if r == last:
                                for promote in (5, 4, 3, 2):
                                    out.append((m, m + d, promote))
                            else:
                                out.append((m, m + d, 0))
                        elif self.ep and m + d == self.ep:
                            out.append((m, m + d, 0))
                elif kind == 2 or kind == 6:
                    for d in (KNIGHT_D if kind == 2 else KING_D):
                        if free(board[m + d]):
                            out.append((m, m + d, 0))
                else:
                    if kind == 3:
                        dirs = BISHOP_D
                    elif kind == 4:
                        dirs = ROOK_D
                    else:
                        dirs = BISHOP_D + ROOK_D
                    for d in dirs:
                        q = m + d
                        while board[q] == 0:
                            out.append((m, q, 0))
                            q += d
                        if free(board[q]):
                            out.append((m, q, 0))

        home = 1 if side == 0 else 8
        rank = home - 1
        other = 1 - side
        king = 6 if side == 0 else 12
        rook = 4 if side == 0 else 10
        if board[idx(rank, 4)] == king:
            if (self.castle & (1 if side == 0 else 4)) and board[idx(rank, 7)] == rook:
                if board[idx(rank, 5)] == 0 and board[idx(rank, 6)] == 0 and \
                        not any(self.attacked(idx(rank, f), other) for f in (4, 5, 6)):
                    out.append((idx(rank, 4), idx(rank, 6), 0))
            if (self.castle & (2 if side == 0 else 8)) and board[idx(rank, 0)] == rook:
                if board[idx(rank, 1)] == 0 and board[idx(rank, 2)] == 0 and \
                        board[idx(rank, 3)] == 0 and \
                        not any(self.attacked(idx(rank, f), other) for f in (4, 3, 2)):
                    out.append((idx(rank, 4), idx(rank, 2), 0))
        return out

    def apply(self, move):
        """Make a move and append the position it reaches to the history."""
        from_m, to_m, promote = move
        board = self.board
        piece = board[from_m]
        kind = (piece - 1) % 6 + 1
        side = self.side
        captured = board[to_m]
        ep = self.ep
        castle = self.castle
        rule50 = self.rule50
        # Castling bits: 1 white kingside, 2 white queenside, 4 black kingside,
        # 8 black queenside, so one factor names a side and one a wing.
        right = 1 if side == 0 else 4
        theirs = 1 if side == 1 else 4
        home = 1 if side == 0 else 8

        self.rule50 += 1
        self.ep = 0
        if kind == 1:
            self.rule50 = 0
        elif captured != 0 or (ep and to_m == ep):
            self.rule50 = 0

        if kind == 6:
            self.castle &= ~(3 << (0 if side == 0 else 2))
            if abs(file_of(to_m) - file_of(from_m)) == 2:
                rank = home - 1
                if file_of(to_m) == 6:
                    board[idx(rank, 5)] = board[idx(rank, 7)]
                    board[idx(rank, 7)] = 0
                else:
                    board[idx(rank, 3)] = board[idx(rank, 0)]
                    board[idx(rank, 0)] = 0
        if kind == 4 and rank_of(from_m) == home - 1:
            if file_of(from_m) == 0:
                self.castle &= ~(2 * right)
            if file_of(from_m) == 7:
                self.castle &= ~(1 * right)
        if captured != 0 and rank_of(to_m) == (7 if side == 0 else 0):
            # A captured rook on its home rank takes the right with it.
            if file_of(to_m) == 0:
                self.castle &= ~(2 * theirs)
            if file_of(to_m) == 7:
                self.castle &= ~(1 * theirs)

        if kind == 1 and to_m == ep and file_of(from_m) != file_of(to_m):
            board[to_m + (-10 if side == 0 else 10)] = 0
        # A promotion code is a piece kind, so black's is six higher.
        board[to_m] = promote + (6 if side == 1 else 0) if promote else piece
        board[from_m] = 0
        if kind == 1 and abs(rank_of(to_m) - rank_of(from_m)) == 2:
            self.ep = (from_m + to_m) // 2
        self.side = 1 - side
        self.history.append(self.snapshot(self.repetitions()))
        return (from_m, to_m, captured, piece, ep, castle, rule50)

    def undo(self, move, saved):
        from_m, to_m, captured, piece, ep, castle, rule50 = saved
        board = self.board
        kind = (piece - 1) % 6 + 1
        side = 1 - self.side
        if kind == 6 and abs(file_of(to_m) - file_of(from_m)) == 2:
            home = 1 if side == 0 else 8
            rank = home - 1
            if file_of(to_m) == 6:
                board[idx(rank, 7)] = board[idx(rank, 5)]
                board[idx(rank, 5)] = 0
            else:
                board[idx(rank, 0)] = board[idx(rank, 3)]
                board[idx(rank, 3)] = 0
        if kind == 1 and to_m == ep and file_of(from_m) != file_of(to_m):
            # The pawn taken en passant belongs to the side that is not moving.
            board[to_m + (-10 if side == 0 else 10)] = 7 if side == 0 else 1
        board[from_m] = piece
        board[to_m] = captured
        self.ep = ep
        self.castle = castle
        self.rule50 = rule50
        self.side = side
        self.history.pop()

    def legal(self):
        side = self.side
        out = []
        for move in self.pseudo():
            saved = self.apply(move)
            if not self.in_check(side):
                out.append(move)
            self.undo(move, saved)
        return out


# ---------------------------------------------------------------------------
# The 112 planes, lc0's classical encoder.
# ---------------------------------------------------------------------------

def encode(pos):
    """112 planes of 64, plane major, in the frame of the side to move.

    lc0 keeps every Position mirrored for black, and flips every second history
    board as it walks back, so all eight boards arrive in one frame: the pieces
    named "our" are always the side that is to move now, and the board is
    mirrored when that side is black.
    """
    planes = np.zeros((112, 64), dtype=np.float32)
    swap = pos.side == 1
    if pos.castle & 2:
        planes[104] = 1.0
    if pos.castle & 1:
        planes[105] = 1.0
    if pos.castle & 8:
        planes[106] = 1.0
    if pos.castle & 4:
        planes[107] = 1.0
    if swap:
        planes[108] = 1.0
    planes[109] = pos.rule50
    planes[111] = 1.0

    total = len(pos.history)
    ours = pos.side
    for i in range(8):
        if i >= total:
            break  # FEN_ONLY history fill: the oldest slot is the start position
        board, _, _, _, _, reps = pos.history[total - 1 - i]
        base = i * 13
        for r in range(8):
            for f in range(8):
                piece = board[idx(r, f)]
                if piece == 0:
                    continue
                kind = (piece - 1) % 6
                sq = r * 8 + f
                if swap:
                    sq = flip64(sq)
                if (piece <= 6) == (ours == 0):
                    planes[base + kind, sq] = 1.0
                else:
                    planes[base + 6 + kind, sq] = 1.0
        if reps >= 1:
            planes[base + 12] = 1.0
    return planes


def frame(pos, sq):
    """lc0's square for a real square: mirrored when black is to move."""
    return flip64(sq) if pos.side == 1 else sq


# ---------------------------------------------------------------------------
# The forward pass.
# ---------------------------------------------------------------------------

def relu(x):
    return np.maximum(x, 0.0)


def conv3(x, w, b, outs):
    """3x3, pad 1, channels first. `x` is (C, 8, 8)."""
    p = np.zeros((x.shape[0], 10, 10), dtype=np.float64)
    p[:, 1:9, 1:9] = x
    w = w.reshape(outs, x.shape[0], 3, 3).astype(np.float64)
    out = np.zeros((outs, 8, 8), dtype=np.float64)
    for ky in range(3):
        for kx in range(3):
            out += np.einsum("oc,cyx->oyx", w[:, :, ky, kx], p[:, ky:ky + 8, kx:kx + 8])
    return out + b.astype(np.float64)[:, None, None]


def conv1(x, w, b, outs):
    """1x1: the value head's squeeze."""
    w = w.reshape(outs, x.shape[0]).astype(np.float64)
    return np.einsum("oc,cyx->oyx", w, x) + b.astype(np.float64)[:, None, None]


def tower(net, planes, stop):
    """The value running through the tower, after `stop` residual blocks."""
    x = relu(conv3(planes.reshape(112, 8, 8), net["input"]["weights"], net["input"]["biases"], 64))
    for block in net["residual"][:stop]:
        h = relu(conv3(x, block["conv1"]["weights"], block["conv1"]["biases"], 64))
        h = conv3(h, block["conv2"]["weights"], np.zeros(64, dtype=np.float32), 64)
        bias = block["conv2"]["biases"].astype(np.float64)
        pool = h.sum(axis=(1, 2)) / 64.0 + bias
        se = block["se"]
        fc1 = np.maximum(se["w1"].reshape(-1, 64).astype(np.float64) @ pool + se["b1"], 0.0)
        fc2 = se["w2"].reshape(128, -1).astype(np.float64) @ fc1 + se["b2"]
        gamma = 1.0 / (1.0 + np.exp(-fc2[:64]))
        beta = fc2[64:] + gamma * bias
        x = np.maximum(x + h * gamma[:, None, None] + beta[:, None, None], 0.0)
    return x


def forward(net, planes):
    """The policy head (80 planes of 8 x 8) and the WDL value."""
    x = tower(net, planes, len(net["residual"]))

    p = relu(conv3(x, net["policy1"]["weights"], net["policy1"]["biases"], 64))
    head = conv3(p, net["policy"]["weights"], net["policy"]["biases"], 80)

    v = relu(conv1(x, net["value"]["weights"], net["value"]["biases"], 32))
    v = np.maximum(net["ip1_val_w"].reshape(128, 2048).astype(np.float64) @ v.reshape(-1) + net["ip1_val_b"], 0.0)
    wdl = net["ip2_val_w"].reshape(3, -1).astype(np.float64) @ v + net["ip2_val_b"]
    e = np.exp(wdl - wdl.max())
    return head, e / e.sum()


def engine_move(pos, net, tables):
    """The legal move the policy likes best, as (move, score, wdl)."""
    head, wdl = forward(net, encode(pos))
    flat = head[:73].reshape(-1)
    best, best_score = None, -1e30
    for move in pos.legal():
        j = policy_index(pos, move, tables)
        if j < 0:
            continue
        score = flat[tables["cell_of_move"][j]]
        if score > best_score:
            best, best_score = move, score
    if best is None:
        best, best_score = pos.legal()[0], 0.0
    return best, best_score, wdl


def uci(move):
    return name(move[0]) + name(move[1]) + LETTERS[move[2]].lower() if move[2] else \
        name(move[0]) + name(move[1])


# ---------------------------------------------------------------------------
# Checks.
# ---------------------------------------------------------------------------

PERFT = [
    (START_FEN, [20, 400, 8902, 197281]),
    ("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1", [48, 2039, 97862]),
    ("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1", [14, 191, 2812, 43238]),
    ("r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1", [6, 264, 9467]),
    ("rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8", [44, 1486, 62379]),
    ("r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10", [46, 2079, 89890]),
]


def perft(pos, depth):
    if depth == 0:
        return 1
    total = 0
    for move in pos.legal():
        saved = pos.apply(move)
        total += perft(pos, depth - 1)
        pos.undo(move, saved)
    return total


def check_perft():
    for fen, counts in PERFT:
        pos = Position(fen)
        for depth, want in enumerate(counts, start=1):
            got = perft(pos, depth)
            if got != want:
                print("perft FAIL %s depth %d: %d, want %d" % (fen, depth, got, want))
                return 1
        print("perft ok   %-42s %s" % (fen[:42], counts))
    return 0


def main(argv):
    if "--export" in argv:
        return export()
    if "--dump" in argv:
        return dump()

    tables = policy_tables()
    tables = move_tables(tables)
    if check_perft():
        return 1

    net = load_net(os.path.join(WEIGHTS, "maia-1100.pb.gz"))
    print("net:  %d residual blocks, %d filters" %
          (len(net["residual"]), net["input"]["weights"].size // (112 * 9)))

    pos = Position()
    move, score, wdl = engine_move(pos, net, tables)
    got = uci(move)
    print("maia-1100 plays %s (%.4f)   wdl %.3f %.3f %.3f" % (got, score, wdl[0], wdl[1], wdl[2]))
    if got != "e2e4":
        print("expected e2e4")
        return 1

    for _ in range(8):
        move, score, wdl = engine_move(pos, net, tables)
        print("  %-5s %s   wdl %.3f %.3f %.3f" % (uci(move), pos.fen(), wdl[0], wdl[1], wdl[2]))
        pos.apply(move)
    return 0


# ---------------------------------------------------------------------------
# The raven data module.
# ---------------------------------------------------------------------------

def numbers(values):
    return "[" + ",".join("%.9g" % v for v in np.asarray(values, dtype=np.float64).reshape(-1)) + "]"


# The order the forward pass reads the weights in, and the offsets `src/engine.rav`
# indexes them with. One list of weights and one of biases, so a convolution is a
# window into `w` rather than a list of its own.
LAYERS = [
    ("input", 64512, 64),
    ("block", 75264, 264, 6),
    ("policy1", 36864, 64),
    ("policy", 46080, 80),
    ("value", 2048, 32),
    ("ip1_val", 262144, 128),
    ("ip2_val", 384, 3),
]


def layout():
    """(w off, b off) per layer, checked against src/engine.rav's constants."""
    wo = bo = 0
    out = {}
    for layer in LAYERS:
        name, wn, bn = layer[0], layer[1], layer[2]
        if name == "block":
            for k in range(layer[3]):
                out["block%d" % k] = (wo, bo)
                wo += wn
                bo += bn
        else:
            out[name] = (wo, bo)
            wo += wn
            bo += bn
    return out, wo, bo


BOTS = ["maia-%d.pb.gz" % e for e in range(1100, 2000, 100)]


def pack_weights(q):
    """Three 16 bit weights in one number. A Scratch list item is a float64, so
    53 bits of integer survive exactly; packing is what makes nine networks fit
    in a file that held one."""
    out = []
    for k in range(0, len(q), 3):
        a = int(q[k])
        b = int(q[k + 1]) if k + 1 < len(q) else 0
        c = int(q[k + 2]) if k + 2 < len(q) else 0
        out.append(a + b * 65536 + c * 65536 * 65536)
    return out


def bot_tables():
    """Everything `src/engine.rav`'s `load_bot` needs for all nine bots.

    The layout it expands into is the one the forward pass indexes: one weight
    list and one bias list, concatenated in layer order. A layer is a
    convolution, a squeeze-excitation bottleneck or a fully connected head, and
    `lper` is how many weights one of its output channels owns.
    """
    first = load_raw(os.path.join(WEIGHTS, BOTS[0]))
    lk, lout, lper, lnum, lp, lgo = [], [], [], [], [], []
    packed = 0
    slot = 0
    for rec in first:
        lk.append(0 if rec["fold"] else 1)
        lout.append(rec["out"])
        lper.append(rec["per"])
        lnum.append(len(rec["w"]["q"]))
        lp.append(packed)
        lgo.append(slot)
        packed += (len(rec["w"]["q"]) + 2) // 3
        slot += rec["out"]

    wq, wlmin, wlmax = [], [], []
    gain, mean, std, beta = [], [], [], []
    for name in BOTS:
        raw = load_raw(os.path.join(WEIGHTS, name))
        assert len(raw) == len(first), name
        for rec in raw:
            wq.extend(pack_weights(rec["w"]["q"]))
            wlmin.append(rec["w"]["min"])
            wlmax.append(rec["w"]["max"])
            if rec["fold"]:
                gain.extend(rec["gamma"])
                mean.extend(rec["mean"])
                std.extend(rec["std"])
                beta.extend(rec["beta"])
            else:
                gain.extend([1.0] * rec["out"])
                mean.extend([0.0] * rec["out"])
                std.extend([1.0] * rec["out"])
                beta.extend(layer_values(rec["beta"]))
    return {"lk": lk, "lout": lout, "lper": lper, "lnum": lnum, "lp": lp, "lgo": lgo,
            "wq": wq, "wlmin": wlmin, "wlmax": wlmax,
            "botg": gain, "botm": mean, "bots": std, "bott": beta,
            "pack": packed, "slot": slot}


def export():
    tables = move_tables(policy_tables())
    bots = bot_tables()
    assert len(bots["wq"]) == len(BOTS) * bots["pack"], len(bots["wq"])
    assert len(bots["botg"]) == len(BOTS) * bots["slot"], len(bots["botg"])
    for name in BOTS:
        w, b = expand(load_raw(os.path.join(WEIGHTS, name)))
        assert len(w) == 863616 and len(b) == bots["slot"], name

    out = ["// The nine bots, generated by tools/maia.py --export. Do not edit.",
           "//",
           "// A lc0 network's weights are LINEAR16: two bytes each, read as a",
           "// fraction of a range the layer carries. Three of those fit in one",
           "// Scratch list item, so `wq` holds every network packed three to an",
           "// item and `wlmin`/`wlmax` hold the ranges. `load_bot` in",
           "// src/engine.rav unpacks one of them, folds its batch norm in, and",
           "// leaves `w` and `b` in the layout the forward pass reads: nine times",
           "// 863,616 weights, which unpacked would be 104 MiB, in one file.",
           ""]

    def var(name, values, ints=False):
        # Seventeen digits, because every one of these has to come back as the
        # same float64: a layer range is a float32 value widened, and nine digits
        # would round it to a different float64 than the one it came from.
        fmt = "%d" if ints else "%.17g"
        out.append("pub var %s: list<num> = [%s];" % (name, ",".join(fmt % v for v in values)))
        out.append("")

    out.append("pub const BNUM: num = %d;" % len(BOTS))
    out.append("pub const BLAYERS: num = %d;" % len(bots["lk"]))
    out.append("pub const BSLOT: num = %d;" % bots["slot"])
    out.append("pub const BPACK: num = %d;" % bots["pack"])
    out.append("")
    var("elos", [int(n[5:9]) for n in BOTS], True)
    var("lk", bots["lk"], True)
    var("lout", bots["lout"], True)
    var("lper", bots["lper"], True)
    var("lnum", bots["lnum"], True)
    var("lp", bots["lp"], True)
    var("lgo", bots["lgo"], True)
    var("wlmin", bots["wlmin"])
    var("wlmax", bots["wlmax"])
    var("botg", bots["botg"])
    var("botm", bots["botm"])
    var("botsd", bots["bots"])
    var("bott", bots["bott"])
    # The working weight list, empty and the right length. Scratch refuses to
    # `add to list` past 200,000 items, so a list this long cannot be built by
    # appending: it is written here, once, and `load_bot` overwrites it in place.
    var("w", [0] * 863616, True)
    var("wq", bots["wq"], True)
    var("pol_map", policy_tables()["conv_policy_map"], True)
    var("cell_of_move", tables["cell_of_move"], True)
    var("mv_base", tables["mv_base"], True)
    var("promo_base", tables["promo_base"], True)

    path = os.path.join(HERE, "..", "src", "net.rav")
    with open(path, "w") as f:
        f.write("\n".join(out))
    print("bots %d, layers %d, slots %d, packed items per bot %d" %
          (len(BOTS), len(bots["lk"]), bots["slot"], bots["pack"]))
    print("wrote %s (%.1f MiB)" % (os.path.relpath(path, ROOT), os.path.getsize(path) / 1048576))
    return 0


SLOT = 69
SLOTS = 8

NET_CASES = [
    [],
    ["e2e4"],
    ["e2e4", "e7e5", "g1f3"],
    ["e2e4", "e7e5", "g1f3", "b8c6", "f1c4", "g8f6", "f3g5", "d7d5", "e4d5", "c6a5"],
]


def pack(move):
    """A move as one number: from, to and promotion kind, the way raven packs it."""
    from_m, to_m, promote = move
    return (from_m * 100 + to_m) * 10 + promote


def play_uci(pos, text):
    for move in pos.legal():
        if uci(move) == text:
            return pos.apply(move)
    raise ValueError("not legal here: %s" % text)


def state_of(pos):
    """The engine's lists as the checker writes them: 120 board cells, the four
    numbers that describe the position, and the last eight snapshots."""
    slots = pos.history[-SLOTS:]
    hist = [0] * (SLOTS * SLOT)
    for k, slot in enumerate(slots):
        base = k * SLOT
        for r in range(8):
            for f in range(8):
                hist[base + r * 8 + f] = slot[0][idx(r, f)]
        hist[base + 64] = slot[1]
        hist[base + 65] = slot[2]
        hist[base + 66] = slot[3]
        hist[base + 67] = slot[4]
        hist[base + 68] = slot[5]
    return {"board": list(pos.board), "side": pos.side, "castle": pos.castle,
            "ep": pos.ep, "rule50": pos.rule50, "hist": hist, "histn": len(slots)}


def planes_padded(planes):
    """The 112 input planes in a 10x10 frame, as `planes` holds them."""
    out = [0.0] * (112 * 100)
    grid = planes.reshape(112, 8, 8)
    for p in range(112):
        for r in range(8):
            for f in range(8):
                out[p * 100 + r * 10 + f + 11] = float(grid[p, r, f])
    return out


def head_padded(head):
    """The policy head at 80 planes of 8x8 in a 10x10 frame, as `bufl2` holds it."""
    out = [0.0] * (80 * 100)
    for p in range(80):
        for r in range(8):
            for f in range(8):
                out[p * 100 + r * 10 + f + 11] = float(head[p, r, f])
    return out


def dump():
    """What `tools/check.mjs` compares the built project against: the six perft
    positions, a few games for one bot and one for another, and a sample of every
    bot's weights, which is the only way to prove that all nine unpack the way
    this file folds them."""
    tables = move_tables(policy_tables())
    cases = []
    for fen, counts in PERFT:
        pos = Position(fen)
        cases.append({"name": fen, "perft": counts,
                      "state": state_of(pos), "legal": [pack(m) for m in pos.legal()]})
    for elo in (0, 8):
        net = load_net(os.path.join(WEIGHTS, BOTS[elo]))
        for moves in (NET_CASES if elo == 0 else NET_CASES[:1]):
            pos = Position()
            for text in moves:
                play_uci(pos, text)
            move, score, wdl = engine_move(pos, net, tables)
            head, _ = forward(net, encode(pos))
            cases.append({"name": " ".join(moves) or "start", "elo": elo,
                          "state": state_of(pos), "move": pack(move),
                          "wdl": [float(x) for x in wdl],
                          "planes": planes_padded(encode(pos)),
                          "head": head_padded(head)})

    weights = []
    for e, name in enumerate(BOTS):
        w, b = expand(load_raw(os.path.join(WEIGHTS, name)))
        weights.append({"elo": e,
                        "w": [[i, float(w[i])] for i in range(0, len(w), 1687)],
                        "b": [[i, float(b[i])] for i in range(0, len(b), 31)]})

    path = os.path.join(HERE, "checkdata.json")
    json.dump({"cases": cases, "weights": weights}, open(path, "w"))
    print("wrote %s: %d cases, %d sampled bots, %.1f MiB" %
          (os.path.relpath(path, ROOT), len(cases), len(weights),
           os.path.getsize(path) / 1048576))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
