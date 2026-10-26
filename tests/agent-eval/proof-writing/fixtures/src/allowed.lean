def allowed (staff : Bool) (owner : Bool) : Bool :=
  if staff then true else owner
