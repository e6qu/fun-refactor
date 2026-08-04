//! Translate a Next.js API route into a FastAPI module.
use fun_refactor::transpile::nextjs;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).expect("usage: nextapi <route.ts>");
    let plan = nextjs::plan(std::path::Path::new(&path))?;
    println!("{}", plan.output);
    eprintln!("route {} -> {}", plan.route, plan.destination.display());
    eprintln!("methods: {}", plan.methods.join(", "));
    eprintln!("carried: {}", plan.fidelity.carried_verbatim);
    for note in plan.fidelity.notes.iter().take(8) {
        eprintln!("  {note}");
    }
    Ok(())
}
