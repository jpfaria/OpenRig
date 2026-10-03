#!/usr/bin/env python3
"""Converts Groove MIDI Dataset performances into OpenRig groove files.

The Groove MIDI Dataset (Magenta, CC BY 4.0) holds real drummers playing to a
click. For each chosen performance this script finds where the bar starts,
cuts a two-bar loop that repeats cleanly, and pulls the fills the drummer
played in that same performance (a busy bar that lands on a crash), so every
groove comes with fills in its own style and feel. Micro-timing and velocity
are kept: that is what makes it sound played, not programmed.

Usage:
    gmd_to_grooves.py <groove-dir-with-info.csv> <out-dir>

Writes one `<genre>.yaml` per genre. Standard library only.
"""

import collections
import csv
import os
import struct
import sys

# GM percussion note -> OpenRig role key (mirrors DrumRole::from_general_midi).
GM_ROLES = {
    35: "kick", 36: "kick", 37: "side_stick", 38: "snare", 39: "clap",
    # The dataset's kit sends rimshots on 40: a loud snare hit, not an edge tap.
    40: "snare", 41: "tom_floor", 43: "tom_floor", 22: "hat_closed",
    42: "hat_closed", 44: "hat_pedal", 26: "hat_open", 46: "hat_open",
    45: "tom_mid", 47: "tom_mid", 48: "tom_high", 50: "tom_high",
    49: "crash", 58: "crash", 51: "ride", 59: "ride", 52: "china",
    53: "ride_bell", 54: "tambourine", 55: "splash", 56: "cowbell",
    57: "crash2",
}
TOMS = {"tom_floor", "tom_mid", "tom_high"}
CRASHES = {"crash", "crash2", "china", "splash"}

BEATS_PER_BAR = 4
LOOP_BARS = 2
# Hits this early still belong to the next beat (a drummer pushing the time).
EARLY = 0.125
MAX_GROOVES_PER_GENRE = 6
GROOVES_PER_GENRE = {"rock": 10, "funk": 8, "pop": 6}
# Genres whose grooves must keep the snare on two and four.
BACKBEAT_GENRES = {
    "rock", "pop", "punk", "country", "blues", "soul", "gospel", "hiphop",
    "funk", "dance",
}
MAX_FILLS = 4
MIN_BARS = 8

# Genre folder name in the dataset -> display name.
GENRES = {
    "rock": "Rock", "funk": "Funk", "pop": "Pop", "punk": "Punk",
    "hiphop": "Hip-Hop", "soul": "Soul", "jazz": "Jazz", "latin": "Latin",
    "reggae": "Reggae", "country": "Country", "blues": "Blues",
    "neworleans": "New Orleans", "afrobeat": "Afrobeat", "dance": "Dance",
    "gospel": "Gospel", "afrocuban": "Afro-Cuban", "highlife": "Highlife",
}


def read_vlq(data, i):
    value = 0
    while True:
        byte = data[i]
        i += 1
        value = (value << 7) | (byte & 0x7F)
        if not byte & 0x80:
            return value, i


def read_midi(path):
    """Returns (ticks_per_beat, [(tick, note, velocity)]) for note-ons."""
    data = open(path, "rb").read()
    if data[:4] != b"MThd":
        raise ValueError(f"{path}: not a MIDI file")
    _, tracks, ppq = struct.unpack(">HHH", data[8:14])
    i = 14
    notes = []
    for _ in range(tracks):
        length = struct.unpack(">I", data[i + 4:i + 8])[0]
        i += 8
        end = i + length
        tick = 0
        running = None
        while i < end:
            delta, i = read_vlq(data, i)
            tick += delta
            status = data[i]
            if status == 0xFF:
                size, j = read_vlq(data, i + 2)
                i = j + size
                continue
            if status in (0xF0, 0xF7):
                size, j = read_vlq(data, i + 1)
                i = j + size
                continue
            if status & 0x80:
                running = status
                i += 1
            kind = running & 0xF0
            argc = 1 if kind in (0xC0, 0xD0) else 2
            args = data[i:i + argc]
            i += argc
            if kind == 0x90 and args[1] > 0:
                notes.append((tick, args[0], args[1]))
        i = end
    notes.sort()
    return ppq, notes


def to_hits(ppq, notes):
    hits = []
    for tick, note, velocity in notes:
        role = GM_ROLES.get(note)
        if role:
            hits.append((tick / ppq, role, velocity))
    return hits


def nearest_beat(beat):
    return round(beat), beat - round(beat)


def bar_phase(hits):
    """Beat offset (0-3) of the bar start: kicks and crashes on the one,
    snares on two and four."""
    scores = [0.0] * BEATS_PER_BAR
    for beat, role, velocity in hits:
        if velocity < 50:
            continue
        index, error = nearest_beat(beat)
        if abs(error) > 0.15:
            continue
        for phase in range(BEATS_PER_BAR):
            position = (index - phase) % BEATS_PER_BAR
            if role == "kick" and position == 0:
                scores[phase] += 1
            elif role in ("snare", "snare_rim") and position in (1, 3):
                scores[phase] += 1
            elif role in CRASHES and position == 0:
                scores[phase] += 2
    return max(range(BEATS_PER_BAR), key=lambda p: scores[p])


def window(hits, start, length):
    """Hits in [start - EARLY, start + length - EARLY), rebased to start."""
    return [
        (beat - start, role, velocity)
        for beat, role, velocity in hits
        if start - EARLY <= beat < start + length - EARLY
    ]


def signature(hits):
    """The set of (sixteenth, role) a window plays, for comparing bars."""
    return {(round(beat * 4), role) for beat, role, velocity in hits if velocity >= 30}


def similarity(a, b):
    sa, sb = signature(a), signature(b)
    if not sa and not sb:
        return 0.0
    return len(sa & sb) / len(sa | sb)


def is_fill_bar(bar_hits, next_bar_hits):
    busy = sum(1 for _, role, v in bar_hits if role in TOMS and v >= 30)
    snares_late = sum(
        1 for beat, role, v in bar_hits
        if role in ("snare", "snare_rim") and beat >= 2 and v >= 40
    )
    lands = any(
        role in CRASHES and abs(beat) <= EARLY for beat, role, _ in next_bar_hits
    )
    return lands and (busy >= 2 or snares_late >= 4)


def backbeat(loop):
    """Share of the loop's backbeats (two and four) that carry a snare."""
    beats = range(1, LOOP_BARS * BEATS_PER_BAR, 2)
    hit = sum(
        1 for b in beats
        if any(role == "snare" and v >= 50 and abs(beat - b) <= 0.1 for beat, role, v in loop)
    )
    return hit / len(beats)


def best_loop(hits, first_bar, bars):
    length = LOOP_BARS * BEATS_PER_BAR
    best = None
    for bar in range(2, bars - 2 * LOOP_BARS):
        start = first_bar + bar * BEATS_PER_BAR
        loop = window(hits, start, length)
        if not loop:
            continue
        roles = {role for _, role, _ in loop}
        if roles & TOMS or roles & CRASHES - {"crash"}:
            continue
        score = similarity(loop, window(hits, start + length, length))
        score += 0.5 * similarity(loop, window(hits, start - length, length))
        if best is None or score > best[0]:
            best = (score, loop)
    return best


def fills_in(hits, first_bar, bars):
    fills = []
    for bar in range(1, bars - 1):
        start = first_bar + bar * BEATS_PER_BAR
        bar_hits = window(hits, start, BEATS_PER_BAR)
        next_hits = window(hits, start + BEATS_PER_BAR, BEATS_PER_BAR)
        if not is_fill_bar(bar_hits, next_hits):
            continue
        landing = [
            (beat + BEATS_PER_BAR, role, v)
            for beat, role, v in next_hits
            if abs(beat) <= EARLY and role in CRASHES
        ]
        fill = [(max(beat, 0.0), role, v) for beat, role, v in bar_hits] + landing
        fills.append(fill)
    return fills


def convert(row, root):
    ppq, notes = read_midi(os.path.join(root, row["midi_filename"]))
    hits = to_hits(ppq, notes)
    if not hits:
        return None
    phase = bar_phase(hits)
    first = hits[0][0]
    first_bar = phase + BEATS_PER_BAR * int((first - phase + EARLY) // BEATS_PER_BAR)
    bars = int((hits[-1][0] - first_bar) // BEATS_PER_BAR)
    if bars < MIN_BARS:
        return None
    loop = best_loop(hits, first_bar, bars)
    fills = fills_in(hits, first_bar, bars)
    if loop is None or loop[0] < 0.5:
        return None
    # Keep the most varied fills: different tom/snare signatures first.
    chosen = []
    for fill in sorted(fills, key=lambda f: -len(signature(f))):
        if all(similarity(fill, c) < 0.6 for c in chosen):
            chosen.append(fill)
        if len(chosen) == MAX_FILLS:
            break
    return {"score": loop[0], "loop": loop[1], "fills": chosen}


def fmt_hits(hits, indent):
    lines = []
    for beat, role, velocity in sorted(hits):
        lines.append(f"{indent}- [{beat:.4f}, {role}, {velocity}]")
    return "\n".join(lines)


def main():
    root, out = sys.argv[1], sys.argv[2]
    rows = list(csv.DictReader(open(os.path.join(root, "info.csv"))))
    by_genre = collections.defaultdict(list)
    for row in rows:
        if row["beat_type"] != "beat" or row["time_signature"] != "4-4":
            continue
        genre = row["style"].split("/")[0]
        if genre in GENRES:
            by_genre[genre].append(row)

    os.makedirs(out, exist_ok=True)
    for genre, genre_rows in sorted(by_genre.items()):
        results = []
        for row in genre_rows:
            result = convert(row, root)
            if not result:
                continue
            if genre in BACKBEAT_GENRES:
                strength = backbeat(result["loop"])
                kicks = sum(1 for _, role, v in result["loop"] if role == "kick" and v >= 25)
                if strength < 0.75 or kicks < 2:
                    continue
                result["score"] += strength
            results.append((row, result))
        # Prefer clean loops, one per drummer/style before repeating.
        results.sort(key=lambda r: -r[1]["score"])
        picked, seen = [], set()
        for row, result in results:
            variant = row["style"].split("/")[1] if "/" in row["style"] else ""
            if variant.startswith("groove"):
                variant = ""
            key = (row["drummer"], variant, row["bpm"])
            if key in seen:
                continue
            seen.add(key)
            picked.append((row, result))
            if len(picked) == GROOVES_PER_GENRE.get(genre, MAX_GROOVES_PER_GENRE):
                break
        if not picked:
            continue
        # A groove whose performance had no clean fill borrows the fills of
        # the closest-tempo groove of the same genre.
        donors = [r for r in results if r[1]["fills"]]
        if donors:
            for row, result in picked:
                if not result["fills"]:
                    donor = min(donors, key=lambda d: abs(int(d[0]["bpm"]) - int(row["bpm"])))
                    result["fills"] = donor[1]["fills"]
        picked = [r for r in picked if r[1]["fills"]]
        if not picked:
            continue
        picked.sort(key=lambda r: int(r[0]["bpm"]))
        name = GENRES[genre]
        lines = [
            "# Converted from the Groove MIDI Dataset (Magenta, CC BY 4.0)",
            "# by tools/drums/gmd_to_grooves.py. Hits: [beat, role, velocity 0-127].",
            f"genre: {genre}",
            "grooves:",
        ]
        for n, (row, result) in enumerate(picked, 1):
            style = row["style"].split("/")
            variant = f" ({style[1]})" if len(style) > 1 and not style[1].startswith("groove") else ""
            lines += [
                f"  - id: {genre}-{n:02d}",
                f"    name: {name} {n}{variant}",
                f"    beats_per_bar: {BEATS_PER_BAR}",
                f"    tempo: {row['bpm']}",
                f"    source: {row['id']}",
                "    beat:",
                f"      beats: {LOOP_BARS * BEATS_PER_BAR}",
                "      hits:",
                fmt_hits(result["loop"], "        "),
                "    fills:",
            ]
            for fill in result["fills"]:
                lines += [
                    f"      - beats: {BEATS_PER_BAR + 1}",
                    "        hits:",
                    fmt_hits(fill, "          "),
                ]
        with open(os.path.join(out, f"{genre}.yaml"), "w") as handle:
            handle.write("\n".join(lines) + "\n")
        print(f"{genre}: {len(picked)} grooves")


if __name__ == "__main__":
    main()
