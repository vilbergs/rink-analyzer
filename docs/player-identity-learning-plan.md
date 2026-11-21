# Improve player identification from review corrections

## Goal

A reviewer identifies a skater once, corrects an ID switch when it happens, and gets a better suggestion the next time that skater appears. Start within one footage session. Treat a session as one game for the first release. A jersey number or display name can be reused in another game.

The first milestone is a complete interaction: label a player in clip A, confirm the same player in clip B, undo a mistaken link, and reopen both clips with the correct labels and study moments intact.

## Starting point

The worker already records clip-local track IDs, boxes, timestamps, predicted positions, and gear colors when available. The hosted service saves confirmed “Your skater” selections in each clip’s `focus.json`. Other player names and study notes live in browser storage. The current tracker uses motion and gear to associate detections, and the browser can suggest a match from a confirmed gear sample.

Existing selections can establish identity at their recorded frames. They cannot tell us what the system originally suggested or whether the reviewer rejected it. Browser aliases can prefill a confirmation prompt. Two tracks with the same typed name still need an explicit link.

## 1. Capture corrections for any player

- Give each confirmed player a session-scoped ID and an editable display name. Use the ID in links and study moments so a rename preserves their meaning.
- Add **Same player as…**, **Different player**, and **Undo** to Player study. Show the current frame alongside a previously confirmed view. Keep routine playback free of prompts.
- Save each action on the server with a unique edit ID, clip, source time, tracking-run ID, local track ID, player ID, and the suggestion the reviewer saw, if any. Save enough frame and box information to recover the reviewed example. Distinguish a first label, an accepted suggestion, a rejected suggestion, and a correction.
- Validate the edit against a detected box at that time. Record the effective time of an ID switch so earlier appearances of that ID keep their previous assignment. Make retries idempotent, detect conflicting edits from two tabs, and make undo an explicit reversal.
- Keep evidence with its session so session deletion also removes the correction records and derived examples. Bind labels to a specific tracking run so rerunning the tracker cannot silently assign an old label to a different skater.

**Acceptance:** Correct a switch halfway through a clip, reload from another browser, and inspect both sides of the switch. Undo restores the previous assignment. Retrying the save creates one correction. Existing coaching selections still resolve to the same skater.

## 2. Improve the next suggestion immediately

- Build a small collection of confirmed appearances per player. Sample across different times and views so a long burst of nearly identical frames does not dominate. Keep machine predictions out of the confirmed collection.
- Rank candidates using the existing helmet, jersey, and sock measurements. Use motion continuity within a continuous shot. Across clips, start from appearance. Respect explicit rejections in the context where the reviewer made them.
- Show a single suggestion only when it clearly outranks the alternatives. For a close match, offer the top candidates with reference frames. Allow **Can’t tell** and manual selection when gear is missing or the player is obscured.
- Recompute suggestions after confirmation or undo. Preserve the original tracker output and explicit reviewer assignments. The first release can improve suggestions on existing clips without re-encoding the video or retraining the detector.

**Acceptance:** In a three-clip fixture with similar jerseys, confirming a second view changes the ranking for a later appearance. Undo removes its influence. With insufficient evidence, the app leaves the identity open.

## 3. Use the identity in film study

- Let **Follow player** and **Compare with** use confirmed player identities across linked track fragments. Keep unconfirmed tracks individually selectable. Preserve gaps when a player is out of view.
- Offer a short before-and-after replay at crossings and reappearances. A single confirmation can link the reviewed stretches. If the same ID changes identity within a stretch, let the reviewer set the boundary.
- Reopen saved observations at their original clip and timestamp. Surface a changed identity assignment when it affects a saved comparison or coaching export.

**Acceptance:** A reviewer can watch two players together, correct one ID switch, and replay their next shared appearance without selecting the players again.

## 4. Verify that accuracy improves

Build a Rust replay evaluator using frozen track data and adjudicated identity examples. Compare the current matcher with the correction-aware matcher on the same footage. Hold out whole games when tuning thresholds. For same-game learning, reveal confirmations in chronological order and score each suggestion before revealing its answer.

| Measure | What to count |
| --- | --- |
| Wrong suggestions | Incorrect first choices divided by reviewed suggestions. Report suggestion coverage alongside this rate so abstaining on every difficult frame cannot look like progress. |
| ID switches | Wrong identity transitions per reviewed player-minute on continuously annotated stretches. Sparse clicks alone cannot supply this metric. |
| Reviewer effort | Corrections and confirmation prompts per reviewed minute, reported separately. |
| Benefit from use | Compare later-clip results with and without earlier confirmations from the same game. Break out crossings, reappearances, and similar jerseys. |

**Release gate:** Establish the baseline first and agree on thresholds before tuning. Enable automatic suggestions by default only after the held-out comparison shows fewer wrong suggestions at comparable coverage and acceptable reviewer effort. Keep manual linking available independently.

## Delivery order

1. Ship server-side identity edits, undo, and the cross-clip review interaction. Collect a baseline with the existing matcher.
2. Add confirmed appearance examples and run the new ranking in the background. Compare its proposed answers with actual reviewer decisions.
3. Enable assisted suggestions for reviewed sessions, then connect player identities to paired playback and study moments.
4. Consider training a new appearance model once there are enough independently reviewed games to evaluate it. Export only explicitly confirmed examples with their provenance. Missed detections, inaccurate boxes, and lost pucks require their own annotation actions and evaluation sets.

## Implementation entry points

- Hosted edit validation and existing skater selections: [focus.rs](../crates/rink-footage/src/web/focus.rs)
- Player selection, comparison, and gear suggestions: [identity.js](../crates/rink-footage/src/web/identity.js)
- Rust track association: [association.rs](../crates/rink-footage/src/annotate/auto/association.rs)
- Coaching export selection checks: [coaching.rs](../crates/rink-footage/src/web/coaching.rs)
