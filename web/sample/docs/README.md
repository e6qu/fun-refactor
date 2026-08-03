# Signals

A sensor collector, a dashboard, and the infrastructure that runs them. This
workspace exists so every grammar the tool has can be exercised against something
that reads like a service rather than a syntax sample.

## Layout

| Path | Language | What it is |
| --- | --- | --- |
| `src/ingest.rs`, `src/main.rs` | Rust | Validation and the mean per sensor |
| `src/buffer.zig` | Zig | The fixed-size ring readings land in |
| `cmd/collector.go`, `cmd/serve.go` | Go | The same rules, and the HTTP surface |
| `web/dashboard.ts` | TypeScript | The dashboard's data layer |
| `web/Panel.tsx` | TSX | The table the dashboard renders |
| `web/index.html` | HTML | The page |
| `web/dashboard.css`, `web/theme.scss` | CSS, SCSS | Light and dark |
| `scripts/report.py` | Python | Offline reporting |
| `scripts/deploy.sh` | Bash | Build, push, roll |
| `infra/main.tf`, `infra/prod.tfvars` | HCL | The VPC and its subnets |
| `ops/pipeline.yaml` | YAML | CI |
| `chart/` | Helm | The chart that deploys it |
| `ops/dashboards.xml` | XML | Dashboard definitions |
| `docs/README.md`, `docs/runbook.md` | Markdown | These |

## The same rules, four times

`validate` appears in Rust, Go, TypeScript and Python, and `averages`/`rejects`
alongside it. That repetition is deliberate: it is what a service looks like once
the same policy has to hold in the collector, the dashboard and the nightly
report, and it is what makes a cross-language rename worth having.

## Running it

    cargo run                 # the collector
    python scripts/report.py  # last night's numbers
    ./scripts/deploy.sh       # build, push, roll

## Known rough edges

- `fahrenheit` exists in Rust, Go, Zig and TypeScript and is called from none of
  them.
- `handleAverages` and `handleRejects` in `cmd/serve.go` differ by one line.
- `legacy.bufferSize` in the chart is read by no template.
