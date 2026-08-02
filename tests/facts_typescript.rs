// TEMPORARY smoke harness — replaced by real tests before finishing.
use fun_refactor::{extract::Extractor, lang::Language, model::*, parse::Parsers};
use std::path::Path;

fn facts(lang: Language, src: &str) -> FileFacts {
    let parsed = Parsers::new().parse(lang, src).unwrap();
    assert!(!parsed.has_errors(), "sample failed to parse as {lang}");
    Extractor::new()
        .extract(&parsed, Path::new("t"), src)
        .unwrap()
}

const TS: &str = r#"import def from 'm1';
import { a, b as c } from 'm2';
import * as ns from 'm3';
import type { T2 } from 'm4';
import 'm5';
const req = require('m6');

export function exported(p: number, q?: string, r = 1, ...rest: any[]) {}
function plain() {}
export const arrow = (x: number): void => {};
const inner = function () {};
export default class Def {}
export class Klass extends Base implements Iface {
  private field: number = 1;
  static SFIELD = 2;
  handler = () => {};
  constructor(private y: string) {}
  get val(): number { return this.field; }
  method(z: string) { return z; }
}
export interface Iface { m(): void; p: string; }
export type Alias = { k: string };
export enum Colour { Red = 1, Blue }
export namespace Space { export const inside = 1; }
namespace Plain { }
export const CONST_X = 1;
let mutable = 2;
var older = 3;
let uninit: Iface;
const { da, db: alias, ...drest } = someObj;
const [e1, ...e2] = someArr;
export { arrow as arrowAlias, CONST_X };
export * from './all';
function uses() {
  plain();
  obj.method(1);
  new Klass();
  const local: Iface = value as Alias;
  for (const it of list) { use(it); }
  try { risky(); } catch (err) { log(err); }
  return local;
}
"#;

const TSX: &str = r#"import { Helper } from './helper';

export function Card({ title, onClick }: Props) {
  return (
    <div className="card wide" id="main">
      <span class="label">{title}</span>
      <Helper onSelect={onClick} />
      <ns.Widget />
    </div>
  );
}

const Small = () => <p className="tiny">hi</p>;
"#;

fn show(f: &FileFacts, src: &str) {
    println!("--- symbols ({})", f.symbols.len());
    for s in &f.symbols {
        println!(
            "  {:?} {:?} qual={:?} exported={} name_span={:?} full={:?}",
            s.kind,
            s.name,
            s.qualifier,
            s.exported,
            s.name_span.text(src),
            {
                let t = s.full_span.text(src);
                if t.len() > 46 { format!("{}…", &t[..46]) } else { t.to_string() }
            }
        );
    }
    println!("--- references ({})", f.references.len());
    for r in &f.references {
        println!("  {:?} {:?} {:?}", r.kind, r.name, r.span);
    }
    println!("--- imports ({})", f.imports.len());
    for i in &f.imports {
        println!(
            "  path={:?} glob={} alias={:?} names={:?}",
            i.path,
            i.is_glob,
            i.alias,
            i.names
                .iter()
                .map(|n| format!("{}<-{}", n.local, n.original))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn smoke_ts() {
    let f = facts(Language::TypeScript, TS);
    show(&f, TS);
}

#[test]
fn smoke_tsx() {
    let f = facts(Language::Tsx, TSX);
    show(&f, TSX);
}

#[test]
fn tmp_dump_tsx_string() {
    let p = Parsers::new().parse(Language::Tsx, TSX).unwrap();
    fn dump(node: tree_sitter::Node, src: &str, depth: usize) {
        if node.is_named() {
            println!("{}{} {:?} {:?}", "  ".repeat(depth), node.kind(), node.byte_range(), &src[node.byte_range()].replace('\n', "\\n"));
        }
        let mut c = node.walk();
        for ch in node.children(&mut c) { dump(ch, src, depth + 1); }
    }
    dump(p.root(), TSX, 0);
}
