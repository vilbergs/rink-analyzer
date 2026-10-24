---
name: team-system
description: Interview the user about how their team plays and write or update system.md in the Rink Replay library, so position reviews follow their team's system instead of generic principles. Use when the user wants to set up or change their team system, or when a review needs it and there is none.
---

# Write the team system profile

Position feedback is only as good as the system it's judged against. A weak-side winger in a
box-plus-one stands somewhere different from one in a low-zone collapse. This skill writes
`system.md` in the library folder; the `review-positioning` skill reads it.

## Steps

1. **Find the library folder** with `rink list` (or `cargo run -q -p rink-cli -- list`). If
   `system.md` already exists there, read it and only ask about what's changing.
2. **Interview the user** in short rounds of two or three questions. Use hockey terms, and offer
   common options so they can just pick one:
   - **Them:** position, shooting side, level, and what they want to improve.
   - **D-zone coverage:** zone (box-plus-one, low collapse) or man-to-man down low. Also: who
     takes the point, where the weak-side winger stands, and the center's job.
   - **Breakout:** the default play (rim, D-to-D, reverse, center swing) and where the wingers set
     up.
   - **Neutral zone and regroups:** forecheck and regroup shape (1-2-2, 1-3-1, 2-1-2).
   - **Offensive zone:** forecheck roles for F1, F2 and F3; cycle and net-front rules; when the D
     pinch.

   "I don't know" is a fine answer. Write "not set; use common principles" for that part rather
   than guessing.
3. **Write `<library>/system.md`** following the structure of `docs/system-profile.example.md` in
   the repo. Keep it concrete and short: rules a reviewer can check against positions ("weak-side
   winger: high slot, between the point and the net, stick in the lane"), not general philosophy.
4. **Confirm.** Show the user a short summary of what you wrote. Tell them the next positioning
   review will use it.
