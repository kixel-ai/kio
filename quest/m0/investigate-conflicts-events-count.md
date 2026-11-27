# [M] Investigate conflicts/events (count), reviews/findings/reviewer:Codex, gpt-6.2-sol, xhigh effort (count), and 1…

## Goal

Kixel Quest measured what the Plan lists and filed it here on its own. For each finding:

1. Diagnose its cause, starting from the culprit's diff and sessions where it has them.
2. Have a second session, one that did not write the diagnosis, try to refute it, and diagnose again when it does.
3. Fix it, accept it with the reason recorded in this Plan, or improve on it further. A fix carries the measurement that shows it.

Then look past the finding: make the related improvements the diagnosis turns up where they fit here, and add the rest as follow-up quests.

## Plan

Each finding keeps the comment Kixel Quest finds it by, so a later run that sees it again adds its sighting here instead of filing it anew.

### `conflicts/events (count)`: bottleneck

<!-- kixel.quest finding {"series":"conflicts/events (count)","culprit":null} -->

- Series: [`conflicts/events (count)`](https://kixel.quest/factory#bottlenecks-1472741617).
- 2026-11-27: 0 against a baseline of 0, +0. The largest share of conflicts, 6.667% of PRs over the 30 days to 2026-11-27; too few measurements for a trend.

### `reviews/findings/reviewer:Codex, gpt-6.2-sol, xhigh effort (count)`: bottleneck

<!-- kixel.quest finding {"series":"reviews/findings/reviewer:Codex, gpt\u002d6.2\u002dsol, xhigh effort (count)","culprit":null} -->

- Series: [`reviews/findings/reviewer:Codex, gpt-6.2-sol, xhigh effort (count)`](https://kixel.quest/factory#bottlenecks-1472741617).
- 2026-11-27: 3 against a baseline of 3, +0 (+0%). The largest share of reviews, 68% of its measure over the 30 days to 2026-11-27; too few measurements for a trend.

### `ship/pick (h)`: bottleneck

<!-- kixel.quest finding {"series":"ship/pick (h)","culprit":null} -->

- Series: [`ship/pick (h)`](https://kixel.quest/factory#bottlenecks-1472741617), in h.
- 2026-11-27: 2.477 h against a baseline of 2.477 h, +0 h (+0%). The largest share of time to ship, 46.32% of its phases over the 30 days to 2026-11-27; too few measurements for a trend.
