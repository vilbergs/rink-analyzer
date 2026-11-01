"""Rebuild the "wrong" variants of the curriculum plays from their main plays.

    python3 scripts/gen-wrong-variants.py curriculum/plays

Each variant changes `me`'s keyframes (and, where the cost needs it, an
opponent's) from the decision point on, and rewrites the captions to describe
the cost. Ids are derived from the stem, so a rerun changes nothing.
"""
import json, sys, uuid
from pathlib import Path

PLAYS = Path(sys.argv[1])

def kf(t, x, y):
    return {"t_ms": t, "pos": [x, y]}

VARIANTS = {
    "01-dzone-strong-side-wall": {
        "tracks": {
            "home_RW": [kf(0, -15.0, -9.0), kf(1200, -20.0, -11.0), kf(2500, -25.0, -12.0),
                        kf(3200, -22.0, -11.0), kf(4200, -17.0, -8.5), kf(5000, -15.5, -6.5)],
        },
        "captions": [
            (0, "Their C has it in our right corner, my corner. I leave the lane up the wall and drop down to help in the corner."),
            (2000, "I'm down on the wall at the battle, next to our RD. Nobody is between the puck and their LD."),
            (2500, "Pass to the point. It's clean: I'm below the puck and have to chase uphill."),
            (3000, "Their LD has it with the whole top of the zone. I'm more than ten meters away, below the puck."),
            (4000, "He walks the line toward the middle. I'm chasing from behind him, not in his lane."),
            (4200, "Shot from the point through an open lane."),
            (4500, "Save. Rebound to the far corner, our LD picks it up. I'm nowhere near my wall for the breakout."),
        ],
    },
    "02-dzone-weak-side-slot": {
        "tracks": {
            "home_RW": [kf(0, -16.0, -3.0), kf(2500, -18.5, 1.0), kf(3500, -18.0, 0.5),
                        kf(4000, -17.5, -0.5), kf(4800, -15.0, -3.5), kf(5500, -13.5, -5.0)],
        },
        "captions": [
            (0, "Their RW has it in our left corner, the far side. I start drifting toward the puck to help."),
            (2500, "I'm in the middle of the slot, puck side, watching the puck. Their LD sneaks in behind my shoulder and I can't see him."),
            (3000, "Pass from the corner to their RD at the left point. I'm still watching the puck."),
            (3800, "D-to-D to their LD. I see it late and turn back across the slot."),
            (4200, "Their LD has it at my point with nobody in his shot lane."),
            (4800, "Free shot. The step I took toward the puck is the step I owed back. Save, rebound to the near corner."),
            (5800, "Our C recovers on the far side. I'm in the middle, not on my wall for the breakout."),
        ],
    },
    "03-breakout-strong-side-wall": {
        "tracks": {
            "home_RW": [kf(0, -15.0, -5.0), kf(1000, -17.0, -8.5), kf(2000, -18.5, -11.0),
                        kf(2600, -19.3, -12.5), kf(3200, -18.0, -12.6), kf(4000, -14.5, -12.5),
                        kf(5000, -10.0, -12.0), kf(6000, -5.5, -11.0)],
        },
        "move_events": {"zone_exit": 5600},
        "captions": [
            (0, "Our RD recovers behind the net. Their C forechecks him. I'm late, still skating out of the middle toward my wall."),
            (1500, "RD rims it up my wall. I'm not at the hash marks yet."),
            (2000, "The rim reaches me while I'm still moving. I take it on my skates, back to the play, with no idea who's coming."),
            (3000, "I'm still turning. Their LW is above me on the wall and their LD is coming down."),
            (3500, "I come up the wall slow, their LW in front of me and their LD stepping down. No room to carry, no time to look."),
            (5500, "It gets out, late and slow. Our C and LW had to wait for me: no 3-on-2."),
        ],
    },
    "04-breakout-weak-side-middle-lane": {
        "tracks": {
            "home_RW": [kf(0, -19.0, -11.5), kf(1000, -17.0, -6.5), kf(2000, -14.0, -1.5),
                        kf(3000, -11.0, -0.5), kf(3700, -10.5, 0.0), kf(4500, -12.0, -0.5),
                        kf(5500, -16.0, -1.0), kf(7000, -20.0, -1.5)],
            "away_LW": [kf(0, -14.0, -8.0), kf(1000, -15.0, -5.5), kf(2000, -13.0, -2.0),
                        kf(3000, -11.5, 1.0), kf(3600, -11.0, 2.0), kf(4500, -14.0, 2.0),
                        kf(5500, -18.0, 1.5), kf(7000, -21.0, 1.0)],
        },
        "drop_events": {"zone_exit", "zone_entry"},
        "add_events": [
            {"t_ms": 3600, "kind": "puck_recovery", "actors": ["away_LW"], "note": "Interception in the middle lane"},
        ],
        "captions": [
            (0, "Our LD recovers behind the net on the far side. I leave my hash marks right away for the middle. Their LW comes with me."),
            (1500, "LD goes up the far wall to our LW. I'm already in the middle, and their LW is on my hip."),
            (2500, "The puck is on the far wall, moving up. The middle lane is covered: their LW is standing in it with me."),
            (3000, "Our LW is under pressure and has nobody open. He forces it to the middle anyway."),
            (3600, "Their LW steps in front of me and picks it off. My wall is empty behind me."),
            (4500, "They're coming back at our net and I'm turning around. No breakout."),
        ],
    },
}

for stem, v in VARIANTS.items():
    play = json.loads((PLAYS / f"{stem}.rink.json").read_text())
    play["id"] = str(uuid.uuid5(uuid.NAMESPACE_URL, f"rink-replay:{stem}.wrong"))
    play["title"] = play["title"] + " (wrong)"
    play["tracks"].update(v["tracks"])
    drop = v.get("drop_events", set())
    events = [e for e in play["events"] if e["kind"] not in drop] + v.get("add_events", [])
    for e in events:
        if e["kind"] in v.get("move_events", {}):
            e["t_ms"] = v["move_events"][e["kind"]]
    play["events"] = sorted(events, key=lambda e: e["t_ms"])
    play["captions"] = [{"t_ms": t, "text": text} for t, text in v["captions"]]
    out = PLAYS / f"{stem}.wrong.rink.json"
    out.write_text(json.dumps(play, indent=2, ensure_ascii=False) + "\n")
    print(out)
