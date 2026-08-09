#!/usr/bin/env python3
"""Generate a Daft Punk *Get Lucky*–style nu-disco gold-standard project.

`sound_bank: "house"` drums + the recorded SF2 bank for bass/e-piano/guitar,
expressing the nu-disco blueprint (see `docs/planning/music-reference-catalog.md`
§2):

- 116 BPM, B Dorian, 4/4, 44 bars (~1:31).
- Dorian loop **Bm7 - D - F#m7 - E** (i7-III-v7-IV), four 1-bar patterns
  transposed per bar ([0, +3, -5, -7]).
- Note-based drums: 4-on-floor kick, snare+clap backbeat, closed 8th hats
  with open-hat off-beats (the disco pocket).
- Bass: octave-disco bounce (root/octave 8ths). Guitar: off-beat power-
  chord stabs (the Nile Rodgers "chuck"). Epiano: chord stabs on 1 & 3.
- Structure: intro -> verse -> break (filter moment) -> chorus -> verse ->
  chorus -> outro fade.

Deterministic: uuid5 ids, byte-identical output every run.
Run from the repo root: `python3 scripts/gen_nu_disco.py`.
"""

import datetime
import json
import os
import uuid

PPQ = 480
BAR = PPQ * 4
EIGHTH = PPQ // 2
SIXTEENTH = PPQ // 4

# Bm7 - D - F#m7 - E: semitone offsets from the B-minor pattern base.
CHORDS = [0, 3, -5, -7]

G_ROOT, G_FIFTH = 47, 54  # B2 + F#3
B_ROOT = 35  # B1

NS = uuid.UUID("6ba7b810-9dad-11d1-80b4-00c04fd430c8")


def uid(seed: str) -> str:
    return str(uuid.uuid5(NS, seed))


def note(seed: str, pitch: int, start: int, duration: int, velocity: int) -> dict:
    return {
        "id": uid(seed), "pitch": pitch, "start": start, "duration": duration,
        "velocity": velocity, "tied": False, "articulation": None,
    }


def pattern(seed: str, name: str, notes) -> dict:
    return {
        "id": uid(seed), "name": name, "bars": 1,
        "time_signature": {"numerator": 4, "denominator": 4},
        "notes": notes, "steps": [],
    }


def instance(seed: str, pattern_id: str, bar: int, transpose: int, vm: float) -> dict:
    return {
        "id": uid(seed), "pattern_id": pattern_id, "start_tick": bar * BAR,
        "transpose": transpose, "velocity_multiplier": vm,
    }


def layer(seed: str, name: str, instrument: str, instances, register=None,
          automation=None, gain=None, pan=None) -> dict:
    return {
        "id": uid(seed), "name": name, "instrument": instrument,
        "pattern_instances": instances, "register": register,
        "automation": automation or [], "fx": None,
        "mix_gain_db": gain, "mix_pan": pan, "group": None,
    }


def segment(seed: str, name: str, role: str, start_bar: int, bars: int, layers) -> dict:
    return {
        "id": uid(seed), "name": name, "role": role, "start_bar": start_bar,
        "bars": bars, "layers": layers,
    }


def reg(lo: int, hi: int) -> dict:
    return {"min_octave": lo, "max_octave": hi}


def place(seed_prefix: str, pattern_id: str, bars: int, cycle, vm: float,
          offset: int = 0, fade: list | None = None) -> list:
    """One pattern instance per bar; `cycle` = transposes; `fade` = trailing
    velocity-multiplier taper (applied to the final `len(fade)` bars)."""
    out = []
    for b in range(offset, offset + bars):
        mult = vm
        if fade and b >= offset + bars - len(fade):
            mult *= fade[b - (bars - len(fade))]
        out.append(instance(f"{seed_prefix}#{b}", pattern_id, b,
                            cycle[b % len(cycle)], round(mult, 3)))
    return out


def build_patterns() -> dict:
    p = {}

    # ── Drums (note-based — full disco kit: 36 kick, 38 snare, 42 closed,
    #    46 open hat, 49 clap) ────────────────────────────────────────────
    hats_c = [note(f"D_hc{i}", 42, i * EIGHTH, 60, 80) for i in range(8)]
    hats_o = [note(f"D_ho{i}", 46, i * EIGHTH + SIXTEENTH, 160, 66) for i in range(4)]
    kick = [note(f"D_k{i}", 36, i * PPQ, 160, 112) for i in range(4)]
    snare = [note(f"D_s{i}", 38, i * PPQ, 120, 102) for i in (1, 3)]
    clap = [note(f"D_c{i}", 49, i * PPQ, 80, 92) for i in (1, 3)]
    p["Drums_Disco"] = pattern("P_drums", "Drums_Disco", kick + snare + clap + hats_c + hats_o)

    # ── Bass — octave-disco bounce on B1 / B2 ────────────────────────────
    p["Bass_Bounce"] = pattern("P_bass", "Bass_Bounce", [
        note("N_bb0", 35, 0, 200, 100), note("N_bb1", 47, 240, 200, 90),
        note("N_bb2", 35, 480, 200, 98), note("N_bb3", 35, 720, 200, 86),
        note("N_bb4", 35, 960, 200, 100), note("N_bb5", 47, 1200, 200, 90),
        note("N_bb6", 35, 1440, 200, 96), note("N_bb7", 47, 1680, 200, 88),
    ])
    p["Bass_Root"] = pattern("P_bass_root", "Bass_Root", [
        note("N_br0", 35, 0, 880, 96), note("N_br1", 35, 960, 880, 92),
    ])

    # ── Guitar — off-beat power-chord stabs (Nile Rodgers chuck) ────────
    p["Guitar_Stabs"] = pattern("P_guitar", "Guitar_Stabs", [
        n for i, vel in enumerate([96, 88, 96, 90]) for n in (
            note(f"N_gs{i}a", G_ROOT, i * EIGHTH + EIGHTH // 2, 160, vel),
            note(f"N_gs{i}b", G_FIFTH, i * EIGHTH + EIGHTH // 2, 160, vel),
        )
    ])

    # ── Epiano — chord stabs on beats 1 & 3 with a passing octave ───────
    p["Epiano_Stabs"] = pattern("P_epiano", "Epiano_Stabs", [
        note("N_es0a", 59, 0, 240, 88), note("N_es0b", 62, 0, 240, 88), note("N_es0c", 66, 0, 240, 88),
        note("N_es1a", 59, 960, 240, 84), note("N_es1b", 62, 960, 240, 84), note("N_es1c", 66, 960, 240, 84),
        note("N_es2", 71, 1680, 120, 78),
    ])

    # ── Lead — B dorian pentatonic riff (chorus only) ────────────────────
    p["Lead_Riff"] = pattern("P_lead", "Lead_Riff", [
        note("N_lr0", 71, 0, 120, 96), note("N_lr1", 74, 240, 120, 88),
        note("N_lr2", 76, 480, 240, 92), note("N_lr3", 78, 720, 120, 86),
        note("N_lr4", 76, 960, 240, 90), note("N_lr5", 74, 1200, 120, 86),
        note("N_lr6", 71, 1440, 240, 84),
    ])

    return p


def build_segments(patterns: dict) -> list:
    pid = {name: p["id"] for name, p in patterns.items()}
    segs = []

    # Intro (0-3): epiano stabs + drums/bass enter bar 2.
    intro_layers = [
        layer("L_i_epi", "Epiano", "epiano_bell",
              place("I_epi", pid["Epiano_Stabs"], 4, CHORDS, 0.7), reg(3, 4),
              gain=-7.0, pan=0.3),
        layer("L_i_dr", "Drums", "drum_sampler",
              place("I_dr", pid["Drums_Disco"], 2, [0, 0], 0.75, offset=2)),
        layer("L_i_ba", "Bass", "bass_synth",
              place("I_ba", pid["Bass_Bounce"], 2, CHORDS[2:], 0.8, offset=2),
              reg(1, 3), gain=-1.5),
    ]
    segs.append(segment("S_intro", "Intro", "intro", 0, 4, intro_layers))

    # Verse (4-11): the full groove.
    verse_layers = [
        layer("L_v_dr", "Drums", "drum_sampler",
              place("V_dr", pid["Drums_Disco"], 8, [0, 0, 0, 0], 0.9)),
        layer("L_v_ba", "Bass", "bass_synth",
              place("V_ba", pid["Bass_Bounce"], 8, CHORDS, 0.9), reg(1, 3), gain=-1.5),
        layer("L_v_gu", "Guitar", "distorted_guitar",
              place("V_gu", pid["Guitar_Stabs"], 8, CHORDS, 0.8), reg(2, 3),
              gain=-2.0, pan=-0.2),
        layer("L_v_epi", "Epiano", "epiano_bell",
              place("V_epi", pid["Epiano_Stabs"], 8, CHORDS, 0.6), reg(3, 4),
              gain=-7.0, pan=0.3),
    ]
    segs.append(segment("S_v1", "Verse", "verse", 4, 8, verse_layers))

    # Break (12-15): drums + epiano only — the filter/open moment.
    break_layers = [
        layer("L_k_dr", "Drums", "drum_sampler",
              place("K_dr", pid["Drums_Disco"], 4, [0, 0, 0, 0], 0.95)),
        layer("L_k_epi", "Epiano", "epiano_bell",
              place("K_epi", pid["Epiano_Stabs"], 4, CHORDS, 0.9), reg(3, 4),
              gain=-7.0, pan=0.3),
    ]
    segs.append(segment("S_brk", "Break", "build", 12, 4, break_layers))

    # Chorus (16-23): add the lead riff.
    chorus_layers = [
        layer("L_c_dr", "Drums", "drum_sampler",
              place("C_dr", pid["Drums_Disco"], 8, [0, 0, 0, 0], 1.0)),
        layer("L_c_ba", "Bass", "bass_synth",
              place("C_ba", pid["Bass_Bounce"], 8, CHORDS, 1.0), reg(1, 3), gain=-1.5),
        layer("L_c_gu", "Guitar", "distorted_guitar",
              place("C_gu", pid["Guitar_Stabs"], 8, CHORDS, 1.0), reg(2, 3),
              gain=-2.0, pan=-0.2),
        layer("L_c_epi", "Epiano", "epiano_bell",
              place("C_epi", pid["Epiano_Stabs"], 8, CHORDS, 0.7), reg(3, 4),
              gain=-7.0, pan=0.3),
        layer("L_c_ld", "Lead", "lead_synth",
              place("C_ld", pid["Lead_Riff"], 8, CHORDS, 0.9), reg(4, 5),
              gain=-3.0, pan=0.25),
    ]
    segs.append(segment("S_c1", "Chorus", "chorus", 16, 8, chorus_layers))

    # Verse 2 (24-31).
    segs.append(segment("S_v2", "Verse", "verse", 24, 8, verse_layers))

    # Chorus 2 (32-39).
    segs.append(segment("S_c2", "Chorus", "chorus", 32, 8, chorus_layers))

    # Outro (40-43): full groove with a volume-automation fade.
    outro_layers = [
        layer("L_o_dr", "Drums", "drum_sampler",
              place("O_dr", pid["Drums_Disco"], 4, [0, 0, 0, 0], 1.0)),
        layer("L_o_ba", "Bass", "bass_synth",
              place("O_ba", pid["Bass_Bounce"], 4, CHORDS, 1.0), reg(1, 3), gain=-1.5),
        layer("L_o_gu", "Guitar", "distorted_guitar",
              place("O_gu", pid["Guitar_Stabs"], 4, CHORDS, 1.0), reg(2, 3),
              gain=-2.0, pan=-0.2),
        layer("L_o_epi", "Epiano", "epiano_bell",
              place("O_epi", pid["Epiano_Stabs"], 4, CHORDS, 0.7), reg(3, 4),
              gain=-7.0, pan=0.3),
    ]
    # Wire the volume fade to every outro layer (segment-relative ticks).
    for lyr in outro_layers:
        lyr["automation"] = [{"layer_id": lyr["id"], "param": "Volume",
                              "points": [{"tick": 0, "value": 1.0},
                                         {"tick": 4 * BAR, "value": 0.2}]}]
    segs.append(segment("S_outro", "Outro", "outro", 40, 4, outro_layers))

    return segs


def main() -> None:
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    out_dir = os.path.join(root, "demos", "nu_disco")
    os.makedirs(out_dir, exist_ok=True)

    patterns_map = build_patterns()
    patterns = list(patterns_map.values())
    segments = build_segments(patterns_map)
    total_bars = sum(s["bars"] for s in segments)
    assert total_bars == 44, f"expected 44 bars, got {total_bars}"

    now = datetime.datetime.now(datetime.timezone.utc).isoformat()
    project = {
        "id": uid("project"),
        "name": "nu_disco",
        "created_at": now,
        "modified_at": now,
        "brief": {
            "raw_text": (
                "Nu-disco at 116 BPM in B Dorian, 44 bars — Daft Punk Get Lucky-style: "
                "4-on-floor kick with open-hat disco pocket, octave-bounce bass on "
                "Bm7-D-F#m7-E, off-beat guitar stabs, epiano chord comps, pentatonic "
                "lead riff in the chorus"
            ),
            "genre": "funk",
            "mood": ["upbeat", "groovy"],
            "references": [],
            "delivery_target": "streaming",
            "constraints": [],
            "confidence": 0.8,
        },
        "config": {
            "name": "nu_disco",
            "genre": "funk",
            "bpm": 116,
            "key": "B minor",
            "time_signature": {"numerator": 4, "denominator": 4},
            "duration_bars": 44,
            "delivery_target": "streaming",
            "mood": ["upbeat", "groovy"],
            "references": [],
        },
        "arrangement": {
            "total_bars": 44,
            "time_signature": {"numerator": 4, "denominator": 4},
            "segments": segments,
            "energy_curve": [],
        },
        "tracks": [],
        "patterns": patterns,
        "spectral_reference": None,
        "mixer": {"master_gain_db": 0.0, "stereo_width": 1.0, "limiter_ceiling_db": -1.0},
        "sound_bank": "house",
        "melodic_bank": "sf2",
        "bus_graph": None,
        "humanize": {
            "enabled": True, "seed": 116, "velocity_sigma": 0.04,
            "timing_sigma_ms": 3.0, "gate_length_jitter": 0.04,
            "groove": None, "groove_amount": 0.0,
        },
        "master_chain": {
            "enabled": True,
            "multiband": {
                "enabled": True, "low_crossover_hz": 120.0, "high_crossover_hz": 3000.0,
                "low_gain_db": -1.0, "mid_gain_db": 0.5, "high_gain_db": 1.0,
                "low_width": 1.0, "mid_width": 1.0, "high_width": 1.0,
                "mono_below_hz": 200.0,
            },
            "saturation": {"drive_db": 4.0, "softness": 0.55, "oversample": 2, "trim_db": -3.0},
            "dither": {"bits": 24, "enabled": True},
            "target_lufs": -14.0,
        },
        "status": "config_approved",
    }

    out_path = os.path.join(out_dir, "project.json")
    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(project, f, indent=2)
        f.write("\n")
    print(f"Wrote {out_path} ({len(patterns)} patterns, {len(segments)} segments, {total_bars} bars)")


if __name__ == "__main__":
    main()
