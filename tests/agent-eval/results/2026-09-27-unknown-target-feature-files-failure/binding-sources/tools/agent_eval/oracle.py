"""Independent behavioral oracles for the pinned acceptance tasks."""

from pathlib import Path
import subprocess
import tempfile

COMMON = r'''
extern crate strsim;
fn inputs(alphabet: &[char], depth: usize) -> Vec<String> {
    let mut all = vec![String::new()];
    let mut layer = all.clone();
    for _ in 0..depth {
        layer = layer.iter().flat_map(|s| alphabet.iter().map(move |c| format!("{}{}", s, c))).collect();
        all.extend(layer.clone());
    }
    all
}
'''

DICE = r'''
fn reference(a: &str, b: &str) -> f64 {
    let a: Vec<_> = a.chars().filter(|c| !c.is_whitespace()).collect();
    let b: Vec<_> = b.chars().filter(|c| !c.is_whitespace()).collect();
    if a == b { return 1.0; }
    if a.len() < 2 || b.len() < 2 { return 0.0; }
    let mut left: Vec<_> = a.windows(2).collect();
    let mut right: Vec<_> = b.windows(2).collect();
    left.sort(); right.sort();
    let (mut i, mut j, mut intersection) = (0, 0, 0);
    while i < left.len() && j < right.len() {
        match left[i].cmp(right[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => { i += 1; j += 1; intersection += 1; }
        }
    }
    2.0 * intersection as f64 / (left.len() + right.len()) as f64
}
fn main() {
    let values = inputs(&['a', 'b', 'é', '🙂', '\u{2003}', '\t'], 3);
    for a in &values { for b in &values {
        let got = strsim::sorensen_dice(a, b);
        let expected = reference(a, b);
        assert!(got.is_finite() && (got - expected).abs() < 1e-12,
            "a={:?} b={:?}: {} != {}", a, b, got, expected);
    }}
    println!("{} independent pairs passed", values.len() * values.len());
}
'''

OSA = r'''
fn reference(a: &str, b: &str) -> f64 {
    let a: Vec<_> = a.chars().collect();
    let b: Vec<_> = b.chars().collect();
    let length = a.len().max(b.len());
    if length == 0 { return 1.0; }
    let mut table = vec![vec![0; b.len() + 1]; a.len() + 1];
    for i in 0..=a.len() { table[i][0] = i; }
    for j in 0..=b.len() { table[0][j] = j; }
    for i in 1..=a.len() { for j in 1..=b.len() {
        table[i][j] = (table[i-1][j] + 1).min(table[i][j-1] + 1)
            .min(table[i-1][j-1] + usize::from(a[i-1] != b[j-1]));
        if i > 1 && j > 1 && a[i-1] == b[j-2] && a[i-2] == b[j-1] {
            table[i][j] = table[i][j].min(table[i-2][j-2] + 1);
        }
    }}
    1.0 - table[a.len()][b.len()] as f64 / length as f64
}
fn main() {
    let mut values = inputs(&['a', 'b', 'é', '🙂'], 3);
    values.extend(["ca".to_owned(), "abc".to_owned()]);
    for a in &values { for b in &values {
        let got = strsim::normalized_osa_distance(a, b);
        let expected = reference(a, b);
        assert!(got.is_finite() && (got - expected).abs() < 1e-12,
            "a={:?} b={:?}: {} != {}", a, b, got, expected);
    }}
    println!("{} independent pairs passed", values.len() * values.len());
}
'''


def verify(root, task):
    with tempfile.TemporaryDirectory(prefix="fr-agent-oracle-") as tmp:
        tmp = Path(tmp)
        oracle = tmp / "oracle.rs"
        oracle.write_text(COMMON + {"unicode-dice": DICE, "normalized-osa": OSA}[task])
        commands = [
            ["rustc", "--crate-name", "strsim", "--crate-type", "rlib", "--edition=2015",
             str(root / "src/lib.rs"), "-o", str(tmp / "libstrsim.rlib")],
            ["rustc", "--edition=2015", str(oracle), "--extern", f"strsim={tmp / 'libstrsim.rlib'}",
             "-o", str(tmp / "oracle")],
            [str(tmp / "oracle")],
        ]
        for command in commands:
            result = subprocess.run(command, capture_output=True, timeout=120)
            if result.returncode:
                return {"passed": False, "stage": commands.index(command),
                        "detail": result.stderr.decode(errors="replace")[:4096]}
        return {"passed": True, "detail": result.stdout.decode().strip()}
