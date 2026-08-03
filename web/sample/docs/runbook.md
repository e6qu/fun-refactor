# Runbook

## A sensor stops reporting

1. Check `/sensors` — if the name is missing, nothing has arrived for it.
2. Check the refused list at `/rejects`. A sensor sending `900.0` is not silent,
   it is being rejected by `validate`.
3. If the reading is genuine, the ceiling in `Limits` is wrong, not the sensor.

## The dashboard shows stale means

`Averages` is computed per request over whatever is in the ring. If the ring is
smaller than the window the dashboard claims, the mean is over less than it says.
`Ring.init` takes the backing slice, so the size is decided by the caller.

## Rolling back

    helm rollback collector --namespace signals

The chart's `appVersion` and the image tag move together; a rollback of one
without the other leaves the dashboard reporting a version that is not running.

## Retention

`retention.days` in the chart becomes `RETENTION_DAYS` in the container and
`var.retention_days` in Terraform. All three have to agree — see [README](README.md).
