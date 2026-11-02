induction xs generalizing acc with
| nil => rfl
| cons x xs ih =>
  simp only [reverseInto, ih, List.reverse_cons, List.append_assoc, List.singleton_append]
