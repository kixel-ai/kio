--------------------------- MODULE SendCreditTrace ---------------------------
(***************************************************************************)
(* Trace validation: each of kio-lab's explored runs, `Traces` in the      *)
(* generated SendCreditTraces.tla, must be a behavior of SendCreditSplit:  *)
(* it starts in `Init`, and each state follows the one before by a split   *)
(* step or a stutter. TLC walks every trace at once, one per initial       *)
(* state.                                                                  *)
(***************************************************************************)
EXTENDS SendCreditSplit, SendCreditTraces, Sequences

VARIABLES t, i

\* A trace's state, its `st` written as a tuple of rows.
ToV(r) == [r EXCEPT !.st = [p \in Pairs |-> r.st[p[1]][p[2]]]]

TraceInit ==
    /\ t \in 1..Len(Traces)
    /\ i = 1
    /\ v = ToV(Traces[t][1])

TraceNext ==
    /\ i < Len(Traces[t])
    /\ i' = i + 1
    /\ t' = t
    /\ v' = ToV(Traces[t][i + 1])
    /\ SplitNext \/ UNCHANGED v

Done == i = Len(Traces[t]) /\ UNCHANGED <<v, t, i>>

TraceSpec == TraceInit /\ [][TraceNext \/ Done]_<<v, t, i>>

\* Every trace starts in `Init`: an invariant rather than part of `TraceInit`, which would drop
\* such a trace instead of failing it.
StartsInit == i = 1 => Init

\* Every state but a trace's last has its successor.
Accepted == i < Len(Traces[t]) => ENABLED TraceNext

\* A trace ends quiet, as the explorer stops.
EndsQuiet == i = Len(Traces[t]) => Quiet
=============================================================================
