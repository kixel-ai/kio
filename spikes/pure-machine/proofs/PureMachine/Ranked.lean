import PureMachine.Funs
open Aeneas Aeneas.Std Result

/-!
# Warm pulls rank downward

`warm_settle::Relay::options`, translated by Aeneas: every warm copy a relay may pull
feeds a reader ranked strictly below every reader the relay feeds itself. So along any
chain of warm pulls ranks strictly fall, and warm pulls never close a loop: the rule
`WarmSettle.tla` calls "order".

Relays are numbered from 1 to `N`, and a path is a `u16` bit set, so `N ≤ 16`.
-/

namespace pure_machine.warm_settle

/-- A computation that meets a spec returns. -/
theorem ok_of_spec {α : Type} {m : Result α} {p : α → Prop} (h : WP.spec m p) :
    ∃ v, m = ok v ∧ p v := by
  cases m with
  | ret v => exact ⟨v, rfl, by simpa using h⟩
  | vis i k => simp at h
  | div => simp at h

/-- What a relay's number and size need for its helpers not to fail. -/
def Numbered {N : Usize} (self : Relay N) : Prop :=
  N.val ≤ 16 ∧ 1 ≤ self.id.val ∧ self.id.val ≤ 16

/-- A relay whose route, if any, leads through one of its `N` relays. -/
def Routed {N : Usize} (self : Relay N) : Prop :=
  ∀ hop cost, self.route = some (hop, cost) → 1 ≤ hop.val ∧ hop.val ≤ N.val

@[step]
theorem bit_spec (r : U8) (h1 : 1 ≤ r.val) (h2 : r.val ≤ 16) : bit r ⦃ _ => True ⦄ := by
  unfold bit
  step*

@[step]
theorem is_spec (x : Option U8) (v : U8) : is x v ⦃ _ => True ⦄ := by
  unfold is
  split <;> simp

@[step]
theorem pull_eq_spec (a b : Pull) : Pull.Insts.CoreCmpPartialEqPull.eq a b ⦃ _ => True ⦄ := by
  unfold Pull.Insts.CoreCmpPartialEqPull.eq
  simp

@[step]
theorem link_spec {N : Usize} (self : Relay N) (r : U8) (h1 : 1 ≤ r.val) (h2 : r.val ≤ N.val) :
    Relay.link self r ⦃ _ => True ⦄ := by
  unfold Relay.link
  step*

@[step]
theorem heard_spec {N : Usize} (self : Relay N) (r : U8) (h1 : 1 ≤ r.val) (h2 : r.val ≤ N.val) :
    Relay.impl.heard self r ⦃ _ => True ⦄ := by
  unfold Relay.impl.heard
  step*

@[step]
theorem is_open_spec {N : Usize} (self : Relay N) (r : U8) (h1 : 1 ≤ r.val) (h2 : r.val ≤ N.val) :
    Relay.is_open self r ⦃ _ => True ⦄ := by
  unfold Relay.is_open
  step*

@[step]
theorem down_spec {N : Usize} (self : Relay N) (c : U8) (h1 : 1 ≤ c.val) (h2 : c.val ≤ N.val) :
    Relay.down self c ⦃ _ => True ⦄ := by
  unfold Relay.down
  step*

@[step]
theorem pull_spec {N : Usize} (self : Relay N) (s : U8) (hn : Numbered self) (h1 : 1 ≤ s.val)
    (h2 : s.val ≤ N.val) : Relay.pull self s ⦃ _ => True ⦄ := by
  obtain ⟨hN, hid1, hid2⟩ := hn
  unfold Relay.pull
  step*

/-- The fields of a relay, as Aeneas passes them to its loops. -/
abbrev fields {N : Usize} (i : U8) (o : Option U32) (o1 : Option (U8 × U32))
    (a : Array (Option Link) N) (a1 : Array Heard N) (o2 o3 : Option U8) (i1 : U16) : Relay N :=
  { id := i, reader := o, route := o1, links := a, heard := a1, up := o2, next := o3, answered := i1 }

/-- `mine`'s loop always finishes. -/
theorem mine_loop_spec {N : Usize} (hN : N.val ≤ 16) (i : U8) (o : Option U32)
    (o1 : Option (U8 × U32)) (a : Array (Option Link) N) (a1 : Array Heard N) (o2 o3 : Option U8)
    (i1 : U16) (rank : U32) (d : U8) (hd : 1 ≤ d.val) :
    Relay.mine_loop i o o1 a a1 o2 o3 i1 rank d ⦃ _ => True ⦄ := by
  unfold Relay.mine_loop
  apply loop.spec_decr_nat (fun x => N.val + 1 - x.2.val) (fun x => 1 ≤ x.2.val)
  · rintro ⟨r, t⟩ ht
    simp only at ht
    unfold Relay.mine_loop.body
    step*
    all_goals (repeat' (first | simp only [bind_ok] | step | split))
    all_goals scalar_tac
  · exact hd

@[step]
theorem mine_spec {N : Usize} (self : Relay N) (hN : N.val ≤ 16) : Relay.mine self ⦃ _ => True ⦄ := by
  unfold Relay.mine
  split <;> simp only [bind_ok] <;> exact mine_loop_spec hN _ _ _ _ _ _ _ _ _ _ (by simp)

/-- The options loop, from relay `s` on, keeps every warm copy below `mine`. -/
theorem options_loop_spec {N : Usize} (hN : N.val ≤ 16) (i : U8) (hi : 1 ≤ i.val ∧ i.val ≤ 16)
    (o : Option U32) (o1 : Option (U8 × U32)) (a : Array (Option Link) N) (a1 : Array Heard N)
    (o2 o3 : Option U8) (i1 : U16) (u : Option U8) (mine : U32) (choices : alloc.vec.Vec Choice)
    (s : U8) (hs : 1 ≤ s.val) (hl : choices.val.length ≤ s.val)
    (hc : ∀ c ∈ choices.val, c.copy → c.rank.val < mine.val) :
    Relay.options_loop i o o1 a a1 o2 o3 i1 u mine choices s
      ⦃ v => ∀ c ∈ v.val, c.copy → c.rank.val < mine.val ⦄ := by
  have hn : Numbered (fields i o o1 a a1 o2 o3 i1) := ⟨hN, hi⟩
  unfold Relay.options_loop
  apply loop.spec_decr_nat (fun x => N.val + 1 - x.2.val)
    (fun x => 1 ≤ x.2.val ∧ x.1.val.length ≤ x.2.val ∧ ∀ c ∈ x.1.val, c.copy → c.rank.val < mine.val)
  · rintro ⟨cs, t⟩ ⟨ht, hlt, hcs⟩
    simp only at ht hlt hcs
    unfold Relay.options_loop.body
    step*
    all_goals (repeat' (first | simp only [bind_ok] | step | split))
    all_goals (try (refine ⟨by scalar_tac, ?_, ?_, by scalar_tac⟩))
    all_goals (try (first | scalar_tac | exact hcs))
    -- What remains is a pushed copy, which the code admits only below `mine`.
    all_goals
      intro c hmem hcopy
      rw [choices1_post, List.mem_append, List.mem_singleton] at hmem
      rcases hmem with hmem | rfl
      · exact hcs c hmem hcopy
      · scalar_tac
  · exact ⟨hs, hl, hc⟩

/-- Every warm copy a relay may pull feeds a reader ranked strictly below every reader it
feeds itself: `options` returns, and each copy among its choices ranks below `mine`. -/
theorem options_rank_below {N : Usize} (self : Relay N) (hn : Numbered self) (hr : Routed self)
    (u : Option U8) :
    ∃ mine, Relay.mine self = ok mine ∧
      Relay.options self u ⦃ v => ∀ c ∈ v.val, c.copy → c.rank.val < mine.val ⦄ := by
  obtain ⟨hN, hi⟩ := hn
  obtain ⟨mine, hmine, -⟩ := ok_of_spec (mine_spec self hN)
  refine ⟨mine, hmine, ?_⟩
  unfold Relay.options
  rw [hmine]
  simp only [bind_ok]
  split
  · simp only [bind_ok]
    apply options_loop_spec hN _ hi <;> simp
  · rename_i hop_cost hroute
    obtain ⟨hop, cost⟩ := hop_cost
    obtain ⟨h1, h2⟩ := hr hop cost hroute
    step*
    all_goals (repeat' (first | simp only [bind_ok] | step | split))
    all_goals (apply options_loop_spec hN _ hi <;> simp [*])

end pure_machine.warm_settle
