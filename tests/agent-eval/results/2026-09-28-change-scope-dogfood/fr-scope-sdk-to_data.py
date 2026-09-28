return {
    "schema": TASK_CHANGE_SCHEMA,
    "requests": [request.to_data() for request in self.requests],
    "targets": [target.to_data() for target in self.targets],
    "postconditions": dict(self.postconditions),
    "checks": list(self.checks),
    "delivery": self.delivery.to_data(),
    **({"acceptance_checks": list(self.acceptance_checks)} if self.acceptance_checks else {}),
    **({"change_scope": dict(self.change_scope)} if self.change_scope is not None else {}),
}