use fun_refactor::{
    index::Index,
    scan::{scan, ScanOptions},
};

fn probe(name: &str, src: &str) {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(name);
    std::fs::write(&path, src).unwrap();
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let index = Index::build_from_scan(&scanned).unwrap();
    let info = index.file(&path).unwrap();
    println!("===== {name} =====");
    for imp in &info.imports {
        println!(
            "  span {:?} glob={} path={:?} alias={:?} names={:?} text={:?}",
            imp.span,
            imp.is_glob,
            imp.path,
            imp.alias,
            imp.names
                .iter()
                .map(|n| (n.original.clone(), n.local.clone()))
                .collect::<Vec<_>>(),
            imp.span.text(src)
        );
    }
    println!("  -- refs --");
    for r in &info.references {
        let r = &index.references[*r];
        println!(
            "  {:?} {:?} kind={:?} conf={:?} target={:?}",
            r.name, r.span, r.kind, r.confidence, r.target
        );
    }
    println!("  -- syms --");
    for s in &info.symbols {
        let s = index.symbol(*s).unwrap();
        println!("  {:?} {:?} full={:?} name={:?}", s.name, s.kind, s.full_span, s.name_span);
    }
}

#[test]
fn probe_all() {
    probe(
        "a.rs",
        "use std::collections::{HashMap, HashSet};\nuse std::fmt;\nuse zed::*;\n\nuse abc::Thing as T;\n\nfn main() {\n    let m: HashMap<u8, u8> = HashMap::new();\n}\n",
    );
    probe(
        "b.go",
        "package main\n\nimport (\n\t\"os\"\n\t\"fmt\"\n)\n\nimport \"strings\"\n\nfunc main() {\n\tfmt.Println(os.Args)\n}\n",
    );
    probe(
        "c.py",
        "import os\nimport sys\nfrom typing import List, Dict\nfrom foo import *\n\nimport re\n\ndef f(x: List) -> None:\n    print(os.path)\n",
    );
    probe(
        "d.ts",
        "import { b, a } from './m';\nimport def from 'other';\nimport * as ns from 'ns';\n\nimport 'side-effect';\n\nexport function go() { return a; }\n",
    );
    probe("e.zig", "const std = @import(\"std\");\nconst mem = @import(\"std\").mem;\n\npub fn f() void { std.debug.print(\"x\", .{}); }\n");
    probe("f.css", "@import \"a.css\";\n@import \"b.css\";\n\n.btn { color: red; }\n");
}
