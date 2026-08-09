#!/usr/bin/env python3
"""Generate the LP-style nu-metal gold-standard project for scorestrata.

Recreates the structural blueprint of Linkin Park's nu-metal sound as a
scorestrata project file (demos/lp_nu_metal/project.json):

- 105 BPM, A minor, 4/4, 60 bars (~2:17).
- The classic LP progression **i - VI - III - VII** (Am - F - C - G),
  expressed as four 1-bar patterns transposed per bar ([0, -4, +3, -2]).
- Production structure: intro (epiano hook, Numb-style) -> verse (chugged
  palm-mute guitar + root-8th bass + backbeat) -> build (sustained power
  chords + rising lead with a cutoff sweep) -> chorus (wall-of-sound power
  chords + syncopated drums + lead hook) -> verse 2 -> build 2 -> chorus 2
  -> bridge (sparse epiano/bass/lead, no drums) -> finale (fade-out).
- Instrumentation routes through the recorded SF2 bank
  (`melodic_bank: "sf2"`): GM Distortion Guitar, Picked Bass, E.Piano one-
  shots, plus the rock drum kit (`sound_bank: "rock"`).

Deterministic: all ids are uuid5-derived, so repeated runs emit byte-identical
JSON. Run from the repo root: `python3 scripts/gen_lp_golden.py`.
"""

import datetime
import json
import os
import uuid

PPQ = 480
BAR = PPQ * 4  # 1920 ticks per 4/4 bar
EIGHTH = PPQ // 2  # 240
SIXTEENTH = PPQ // 4  # 120

# i - VI - III - VII in A minor: semitone offsets from the A-minor pattern.
CHORDS = [0, -4, 3, -2]

# A-minor pattern bases (MIDI): power-chord root, bass root.
G_ROOT, G_FIFTH = 45, 52  # A2 + E3
B_ROOT = 33  # A1

NS = uuid.UUID("6ba7b810-9dad-11d1-80b4-00c04fd430c8")


def uid(seed: str) -> str:
    return str(uuid.uuid5(NS, seed))


def note(seed: str, pitch: int, start: int, duration: int, velocity: int) -> dict:
    return {
        "id": uid(seed),
        "pitch": pitch,
        "start": start,
        "duration": duration,
        "velocity": velocity,
        "tied": False,
        "articulation": None,
    }


def step(seed: str, index: int, velocity: int) -> dict:
    return {"id": uid(seed), "index": index, "active": velocity > 0, "velocity": velocity}


def pattern(seed: str, name: str, notes=None, steps=None) -> dict:
    return {
        "id": uid(seed),
        "name": name,
        "bars": 1,
        "time_signature": {"numerator": 4, "denominator": 4},
        "notes": notes or [],
        "steps": steps or [],
    }


def instance(seed: str, pattern_id: str, bar: int, transpose: int, vm: float) -> dict:
    return {
        "id": uid(seed),
        "pattern_id": pattern_id,
        "start_tick": bar * BAR,
        "transpose": transpose,
        "velocity_multiplier": vm,
    }


def layer(seed: str, name: str, instrument: str, instances, register=None,
          automation=None, gain=None, pan=None) -> dict:
    return {
        "id": uid(seed),
        "name": name,
        "instrument": instrument,
        "pattern_instances": instances,
        "register": register,
        "automation": automation or [],
        "fx": None,
        "mix_gain_db": gain,
        "mix_pan": pan,
        "group": None,
    }


def segment(seed: str, name: str, role: str, start_bar: int, bars: int, layers) -> dict:
    return {
        "id": uid(seed),
        "name": name,
        "role": role,
        "start_bar": start_bar,
        "bars": bars,
        "layers": layers,
    }


def reg(lo: int, hi: int) -> dict:
    return {"min_octave": lo, "max_octave": hi}


def place(seed_prefix: str, pattern_id: str, bars: int, cycle, vm: float,
          offset: int = 0, fade: list | None = None) -> list:
    """One pattern instance per bar; `cycle` = transposes; `fade` = trailing
    velocity-multiplier taper applied to the final `len(fade)` bars."""
    out = []
    for b in range(offset, offset + bars):
        mult = vm
        if fade and b >= offset + bars - len(fade):
            # Offset-independent: the fade index counts back from the last bar.
            mult *= fade[b - (bars - len(fade))]
        out.append(instance(f"{seed_prefix}#{b}", pattern_id, b,
                            cycle[b % len(cycle)], round(mult, 3)))
    return out


def drum_steps(seed_prefix: str, hits: dict) -> list:
    return [step(f"{seed_prefix}#{i}", i, hits.get(i, 0)) for i in range(16)]


def build_patterns() -> dict:
    """Every 1-bar pattern. Kick steps = velocity >= 105, snare = 95..104,
    hats < 95 (the compiler's GM drum tier mapping)."""
    p = {}

    # ── Drums ────────────────────────────────────────────────────────────
    p["Drums_Backbeat"] = pattern("P_drums_backbeat", "Drums_Backbeat", steps=drum_steps(
        "S_backbeat", {0: 110, 2: 85, 4: 100, 6: 85, 8: 110, 10: 85, 12: 100, 14: 85}))
    p["Drums_Verse"] = pattern("P_drums_verse", "Drums_Verse", steps=drum_steps(
        "S_verse", {0: 106, 2: 78, 4: 98, 6: 78, 8: 106, 10: 78, 12: 98, 14: 78}))
    p["Drums_Build"] = pattern("P_drums_build", "Drums_Build", steps=drum_steps(
        "S_build", {0: 112, 4: 100, 8: 110, 12: 100,
                    1: 62, 3: 68, 5: 70, 7: 76, 9: 80, 11: 84, 13: 88, 15: 90}))
    p["Drums_Chorus"] = pattern("P_drums_chorus", "Drums_Chorus", steps=drum_steps(
        "S_chorus", {0: 115, 2: 88, 3: 72, 4: 102, 5: 68, 6: 90, 7: 74,
                     8: 112, 9: 70, 10: 88, 11: 74, 12: 102, 13: 70, 14: 92, 15: 76}))

    # ── Bass ─────────────────────────────────────────────────────────────
    p["Bass_Verse"] = pattern("P_bass_verse", "Bass_Verse", notes=[
        note(f"N_bv{i}", B_ROOT, i * EIGHTH, EIGHTH, v)
        for i, v in enumerate([100, 88, 96, 88, 100, 88, 92, 86])])
    p["Bass_Chorus"] = pattern("P_bass_chorus", "Bass_Chorus", notes=[
        note(f"N_bc{i}", pitch, i * EIGHTH, EIGHTH, v)
        for i, (pitch, v) in enumerate(
            [(33, 104), (33, 90), (45, 100), (33, 90),
             (33, 104), (33, 90), (45, 98), (33, 88)])])
    p["Bass_Bridge"] = pattern("P_bass_bridge", "Bass_Bridge", notes=[
        note("N_bb0", B_ROOT, 0, PPQ, 90), note("N_bb1", B_ROOT, PPQ, PPQ, 88)])

    # ── Guitar ───────────────────────────────────────────────────────────
    p["Guitar_Verse"] = pattern("P_guitar_verse", "Guitar_Verse", notes=[
        note(f"N_gv{i}", G_ROOT, i * EIGHTH, 120, v)
        for i, v in enumerate([96, 84, 90, 84, 96, 84, 90, 88])])
    chorus_vel = [106, 88, 100, 88, 106, 88, 100, 90]
    p["Guitar_Chorus"] = pattern("P_guitar_chorus", "Guitar_Chorus", notes=[
        n for i in range(8) for n in (
            note(f"N_gc{i}a", G_ROOT, i * EIGHTH, 360 if i % 2 == 0 else 120, chorus_vel[i]),
            note(f"N_gc{i}b", G_FIFTH, i * EIGHTH, 360 if i % 2 == 0 else 120, chorus_vel[i]),
        )
    ])
    p["Guitar_Build"] = pattern("P_guitar_build", "Guitar_Build", notes=[
        note("N_gb0a", G_ROOT, 0, PPQ - 80, 102), note("N_gb0b", G_FIFTH, 0, PPQ - 80, 102),
        note("N_gb1a", G_ROOT, PPQ, PPQ - 80, 98), note("N_gb1b", G_FIFTH, PPQ, PPQ - 80, 98)])
    p["Guitar_Bridge"] = pattern("P_guitar_bridge", "Guitar_Bridge", notes=[
        note("N_gbr0", G_ROOT, 0, 640, 80), note("N_gbr1", G_ROOT, 960, 480, 76)])

    # ── Lead (A minor pentatonic hook, register 4..5) ────────────────────
    p["Lead_Hook"] = pattern("P_lead_hook", "Lead_Hook", notes=[
        note("N_lh0", 69, 0, 240, 100), note("N_lh1", 72, 240, 240, 92),
        note("N_lh2", 76, 480, 480, 96), note("N_lh3", 74, 960, 240, 88),
        note("N_lh4", 72, 1200, 240, 88), note("N_lh5", 69, 1440, 480, 84)])
    p["Lead_BuildRise"] = pattern("P_lead_rise", "Lead_BuildRise", notes=[
        note(f"N_lbr{i}", pitch, t, 200, 88 + 3 * i)
        for i, (pitch, t) in enumerate(
            [(69, 0), (72, 240), (74, 480), (76, 720), (77, 960), (76, 1200), (74, 1440), (72, 1680)])])
    p["Lead_Bridge"] = pattern("P_lead_bridge", "Lead_Bridge", notes=[
        note("N_lbd0", 76, 0, 880, 96), note("N_lbd1", 72, 960, 480, 88),
        note("N_lbd2", 69, 1440, 480, 84)])

    # ── Epiano (Numb-style intro arpeggio, register 3..5) ────────────────
    p["Epiano_Arp"] = pattern("P_epiano", "Epiano_Arp", notes=[
        note(f"N_ep{i}", pitch, i * EIGHTH, 200, v)
        for i, (pitch, v) in enumerate(zip(
            [57, 64, 69, 64, 57, 64, 69, 64],
            [92, 80, 88, 80, 92, 80, 88, 84]))])

    return p


def build_segments(patterns: dict) -> list:
    segs = []
    pid = {name: p["id"] for name, p in patterns.items()}

    # ── Intro (bars 0-3): epiano hook, drums + bass enter on bar 2 ───────
    intro_lead_id = uid("L_intro_lead")
    intro_layers = [
        layer("L_intro_epi", "Epiano", "epiano_bell",
              place("I_epi", pid["Epiano_Arp"], 4, CHORDS, 0.75), reg(3, 5),
              gain=-7.0, pan=0.3),
        layer("L_intro_dr", "Drums", "drum_sampler",
              place("I_dr", pid["Drums_Verse"], 2, [0, 0], 0.7, offset=2)),
        layer("L_intro_ba", "Bass", "bass_synth",
              place("I_ba", pid["Bass_Verse"], 2, CHORDS[2:], 0.8, offset=2),
              reg(1, 3), gain=-1.5),
    ]
    segs.append(segment("S_intro", "Intro", "intro", 0, 4, intro_layers))

    # ── Verse (bars 4-11): chug + backbeat foundation ────────────────────
    verse_layers = [
        layer("L_v_dr", "Drums", "drum_sampler",
              place("V_dr", pid["Drums_Backbeat"], 8, [0, 0, 0, 0], 0.9)),
        layer("L_v_ba", "Bass", "bass_synth",
              place("V_ba", pid["Bass_Verse"], 8, CHORDS, 0.9), reg(1, 3), gain=-1.5),
        layer("L_v_gu", "Guitar", "distorted_guitar",
              place("V_gu", pid["Guitar_Verse"], 8, CHORDS, 0.8), reg(2, 3),
              gain=-2.0, pan=-0.2),
        layer("L_v_epi", "Epiano", "epiano_bell",
              place("V_epi", pid["Epiano_Arp"], 8, CHORDS, 0.55), reg(3, 5),
              gain=-7.0, pan=0.3),
    ]
    segs.append(segment("S_v1", "Verse", "verse", 4, 8, verse_layers))

    # ── Build (bars 12-15): sustained chords + rising lead + cutoff sweep ─
    build_lead_id = uid("L_build_lead")
    build_layers = [
        layer("L_b_dr", "Drums", "drum_sampler",
              place("B_dr", pid["Drums_Build"], 4, [0, 0, 0, 0], 0.95)),
        layer("L_b_ba", "Bass", "bass_synth",
              place("B_ba", pid["Bass_Verse"], 4, CHORDS, 0.95), reg(1, 3), gain=-1.5),
        layer("L_b_gu", "Guitar", "distorted_guitar",
              place("B_gu", pid["Guitar_Build"], 4, CHORDS, 0.95), reg(2, 3),
              gain=-2.0, pan=-0.2),
        layer("L_b_ld", "Lead", "lead_synth",
              place("B_ld", pid["Lead_BuildRise"], 4, CHORDS, 0.9), reg(4, 5),
              automation=[{
                  "layer_id": build_lead_id,
                  "param": "Cutoff",
                  "points": [{"tick": 0, "value": 0.35}, {"tick": 4 * BAR, "value": 1.0}],
              }],
              gain=-3.0, pan=0.25),
    ]
    # Wire the automation to the real lead layer id.
    build_layers[3]["id"] = build_lead_id
    segs.append(segment("S_b1", "Build", "build", 12, 4, build_layers))

    # ── Chorus (bars 16-23): wall-of-sound ───────────────────────────────
    chorus_layers = [
        layer("L_c_dr", "Drums", "drum_sampler",
              place("C_dr", pid["Drums_Chorus"], 8, [0, 0, 0, 0], 1.0)),
        layer("L_c_ba", "Bass", "bass_synth",
              place("C_ba", pid["Bass_Chorus"], 8, CHORDS, 1.0), reg(1, 3), gain=-1.5),
        layer("L_c_gu", "Guitar", "distorted_guitar",
              place("C_gu", pid["Guitar_Chorus"], 8, CHORDS, 1.0), reg(2, 3),
              gain=-2.0, pan=-0.2),
        layer("L_c_ld", "Lead", "lead_synth",
              place("C_ld", pid["Lead_Hook"], 8, CHORDS, 1.0), reg(4, 5),
              gain=-3.0, pan=0.25),
    ]
    segs.append(segment("S_c1", "Chorus", "chorus", 16, 8, chorus_layers))

    # ── Verse 2 (bars 24-31) ─────────────────────────────────────────────
    segs.append(segment("S_v2", "Verse", "verse", 24, 8, verse_layers))

    # ── Build 2 (bars 32-35) ─────────────────────────────────────────────
    segs.append(segment("S_b2", "Build", "build", 32, 4, build_layers))

    # ── Chorus 2 (bars 36-43) ────────────────────────────────────────────
    segs.append(segment("S_c2", "Chorus", "chorus", 36, 8, chorus_layers))

    # ── Bridge (bars 44-51): sparse, no drums (In the End piano moment) ──
    bridge_layers = [
        layer("L_br_epi", "Epiano", "epiano_bell",
              place("BR_epi", pid["Epiano_Arp"], 8, CHORDS, 0.7), reg(3, 5),
              gain=-7.0, pan=0.3),
        layer("L_br_ba", "Bass", "bass_synth",
              place("BR_ba", pid["Bass_Bridge"], 8, CHORDS, 0.75), reg(1, 3), gain=-1.5),
        layer("L_br_ld", "Lead", "lead_synth",
              place("BR_ld", pid["Lead_Bridge"], 8, CHORDS, 0.7), reg(4, 5),
              gain=-3.0, pan=0.25),
        layer("L_br_gu", "Guitar", "distorted_guitar",
              place("BR_gu", pid["Guitar_Bridge"], 8, CHORDS, 0.6), reg(2, 3),
              gain=-2.0, pan=-0.2),
    ]
    segs.append(segment("S_br", "Bridge", "bridge", 44, 8, bridge_layers))

    # ── Finale (bars 52-59): full chorus with a 4-bar volume-automation
    #    fade-out (CC 7) — audible in the mastered mix, unlike a velocity-
    #    multiplier taper which the mastering chain's LUFS normalization
    #    largely compensates.
    finale_layers = [
        layer("L_f_dr", "Drums", "drum_sampler",
              place("F_dr", pid["Drums_Chorus"], 8, [0, 0, 0, 0], 1.0)),
        layer("L_f_ba", "Bass", "bass_synth",
              place("F_ba", pid["Bass_Chorus"], 8, CHORDS, 1.0),
              reg(1, 3), gain=-1.5),
        layer("L_f_gu", "Guitar", "distorted_guitar",
              place("F_gu", pid["Guitar_Chorus"], 8, CHORDS, 1.0),
              reg(2, 3), gain=-2.0, pan=-0.2),
        layer("L_f_ld", "Lead", "lead_synth",
              place("F_ld", pid["Lead_Hook"], 8, CHORDS, 1.0),
              reg(4, 5), gain=-3.0, pan=0.25),
    ]
    for lyr in finale_layers:
        lyr["automation"] = [{
            "layer_id": lyr["id"],
            "param": "Volume",
            "points": [{"tick": 0, "value": 1.0},
                       {"tick": 4 * BAR, "value": 1.0},
                       {"tick": 8 * BAR, "value": 0.2}],
        }]
    segs.append(segment("S_f", "Finale", "chorus", 52, 8, finale_layers))

    return segs


def main() -> None:
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    out_dir = os.path.join(root, "demos", "lp_nu_metal")
    os.makedirs(out_dir, exist_ok=True)

    patterns_map = build_patterns()
    patterns = list(patterns_map.values())
    segments = build_segments(patterns_map)
    total_bars = sum(s["bars"] for s in segments)
    assert total_bars == 60, f"expected 60 bars, got {total_bars}"

    now = datetime.datetime.now(datetime.timezone.utc).isoformat()
    project = {
        "id": uid("project"),
        "name": "lp_nu_metal",
        "created_at": now,
        "modified_at": now,
        "brief": {
            "raw_text": (
                "Rock nu-metal at 105 BPM in A minor, 60 bars — Linkin Park-style: "
                "palm-muted power-chord guitars on i-VI-III-VII, driving root/octave "
                "bass, backbeat-to-syncopated drums, epiano intro hook, quiet verses "
                "building to wall-of-sound choruses"
            ),
            "genre": "nu_metal",
            "mood": ["dark", "driving"],
            "references": [],
            "delivery_target": "streaming",
            "constraints": [],
            "confidence": 0.8,
        },
        "config": {
            "name": "lp_nu_metal",
            "genre": "nu_metal",
            "bpm": 105,
            "key": "A minor",
            "time_signature": {"numerator": 4, "denominator": 4},
            "duration_bars": 60,
            "delivery_target": "streaming",
            "mood": ["dark", "driving"],
            "references": [],
        },
        "arrangement": {
            "total_bars": 60,
            "time_signature": {"numerator": 4, "denominator": 4},
            "segments": segments,
            "energy_curve": [],
        },
        "tracks": [],
        "patterns": patterns,
        "spectral_reference": None,
        "mixer": {"master_gain_db": 0.0, "stereo_width": 1.0, "limiter_ceiling_db": -1.0},
        "sound_bank": "rock",
        "melodic_bank": "sf2",
        "bus_graph": None,
        "humanize": {
            "enabled": True,
            "seed": 105,
            "velocity_sigma": 0.05,
            "timing_sigma_ms": 4.0,
            "gate_length_jitter": 0.05,
            "groove": None,
            "groove_amount": 0.0,
        },
        "master_chain": {
            "enabled": True,
            "multiband": {
                "enabled": True,
                "low_crossover_hz": 120.0,
                "high_crossover_hz": 3000.0,
                "low_gain_db": -1.0,
                "mid_gain_db": 0.5,
                "high_gain_db": 1.0,
                "low_width": 1.0,
                "mid_width": 1.0,
                "high_width": 1.0,
                "mono_below_hz": 200.0,
            },
            "saturation": {"drive_db": 6.0, "softness": 0.6, "oversample": 2, "trim_db": -3.0},
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
