--------------------------- MODULE SendCreditSplit ---------------------------
(***************************************************************************)
(* SendCredit.tla with `TrackRun::poll_step` split into the guarded        *)
(* actions kio-lab's `#[reactor]` fires: the waiting open (`open`) and the *)
(* cursor (`hand_out`), each one atomic step, in either order. The         *)
(* original runs the open, then the cursor, as one step; here another      *)
(* subscription or the peer may act between them, and the cursor may run   *)
(* before the open it would have followed. A task parks once neither is    *)
(* enabled.                                                                *)
(*                                                                         *)
(* Each original step is one or two of these, so every state SendCredit    *)
(* reaches is reached here too, and an invariant checked here holds there. *)
(***************************************************************************)
EXTENDS SendCredit

\* The groups the cursor may hand out.
Ready(x, s) == {k \in 1..G : x.st[s, k] = "cached" /\ Admitted(x, s, k)}

\* `open` fires: the waiting open changes the state.
OpenAct(s) ==
    /\ v.awake[s]
    /\ PollOpen(v, s) # v
    /\ v' = PollOpen(v, s)

\* `hand_out` fires on any group it may hand out.
CursorAct(s) ==
    /\ v.awake[s]
    /\ \E k \in Ready(v, s) : v' = Cursor(v, s, k)

\* Neither is enabled: the task parks.
Park(s) ==
    /\ v.awake[s]
    /\ PollOpen(v, s) = v
    /\ Ready(v, s) = {}
    /\ v' = [v EXCEPT !.awake[s] = FALSE]

SplitSystem == (\E s \in Subs : OpenAct(s) \/ CursorAct(s) \/ Park(s)) \/ (\E p \in Pairs : Finish(p) \/ Free(p))

SplitNext == SplitSystem \/ (Quiet /\ UNCHANGED vars)

SplitSpec == Init /\ [][SplitNext]_vars /\ WF_vars(SplitSystem)

\* Where some split step is enabled: `Enabled`, unchanged by the split.
SplitGuarded == Enabled = ENABLED SplitSystem
=============================================================================
