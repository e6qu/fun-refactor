open FrPureKernel

example : FrRustScalar.applyOperator false .sub (.int 0) (.int 1) = none := by rfl
example : FrRustScalar.applyOperator false .add (.int 18446744073709551615) (.int 1) = none := by rfl
example : FrRustScalar.applyOperator true .add (.int 9223372036854775807) (.int 1) = none := by rfl
example : FrRustScalar.applyOperator true .div (.int 1) (.int 0) = none := by rfl
example : FrRustScalar.applyOperator false .sub (.int 18446744073709551615) (.int 1) =
    some (.int 18446744073709551614) := by rfl
example : FrRustScalar.eval false 8 []
    (.binary .and (.value (.bool false))
      (.binary .eq (.binary .sub (.value (.int 0)) (.value (.int 1))) (.value (.int 0)))) =
    some (.bool false) := by rfl
