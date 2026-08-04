//! Translate a file and print the result with its fidelity report.
use fun_refactor::lang::Language;
use fun_refactor::transpile;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .expect("usage: xlate <file> <language>");
    let to = std::env::args()
        .nth(2)
        .expect("usage: xlate <file> <language>");
    let to = Language::from_name(&to).expect("unknown language");
    let plan = transpile::plan(std::path::Path::new(&path), to)?;
    println!("{}", plan.output);
    eprintln!("--- fidelity ---");
    eprintln!(
        "  {} functions ({} complete signatures, {} with foreign types)",
        plan.fidelity.functions,
        plan.fidelity.signatures_complete,
        plan.fidelity.signatures_with_foreign_types
    );
    eprintln!(
        "  {} records, {} constants",
        plan.fidelity.records, plan.fidelity.constants
    );
    eprintln!("  {} carried verbatim", plan.fidelity.carried_verbatim);
    for note in plan.fidelity.notes.iter().take(12) {
        eprintln!("    {note}");
    }
    Ok(())
}
