use anyhow::{ensure, Result};
use clap::{Args, ValueEnum};
use serde::Serialize;
use serde_json::{json, Value};

pub const BODY_SCHEMA: &str = "fr-semantic-body-1";

pub const TYPE_KINDS: &[&str] = &[
    "unit", "bool", "int", "float", "string", "list", "set", "map", "optional", "tuple", "named",
    "fn",
];
pub const STATEMENT_KINDS: &[&str] = &[
    "return",
    "let",
    "assign",
    "tuple-assign",
    "if",
    "if-present",
    "while",
    "counted-for",
    "for-each-indexed",
    "defer",
    "err-defer",
    "switch",
    "match-variants",
    "while-present",
    "for-each",
    "expr",
    "assert",
    "comment",
    "local-function",
    "block",
    "throw",
    "try",
    "break",
    "break-with",
    "continue",
    "unsupported",
];
pub const EXPRESSION_KINDS: &[&str] = &[
    "int",
    "float",
    "str",
    "bool",
    "null",
    "name",
    "field",
    "index",
    "call",
    "binary",
    "unary",
    "await",
    "propagate",
    "keyword",
    "cast",
    "instance-of",
    "new",
    "record-lit",
    "coalesce",
    "ternary",
    "variant",
    "tuple",
    "list-lit",
    "map-lit",
    "template",
    "lambda",
    "set-lit",
    "comprehension",
    "unsupported",
];
pub const TEMPLATE_KINDS: &[&str] = &["text", "expr"];
pub const BINARY_OPERATORS: &[&str] = &[
    "add",
    "sub",
    "mul",
    "div",
    "floor-div",
    "true-div",
    "rem",
    "floor-rem",
    "eq",
    "ne",
    "lt",
    "le",
    "gt",
    "ge",
    "and",
    "or",
    "xor",
];
pub const UNARY_OPERATORS: &[&str] = &["not", "neg", "unwrap"];

#[derive(Clone, Copy, Debug, ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum Section {
    Body,
    Type,
    Statement,
    Expression,
    Template,
    BinaryOperator,
    UnaryOperator,
    Record,
}

impl Section {
    fn name(self) -> &'static str {
        match self {
            Self::Body => "body",
            Self::Type => "type",
            Self::Statement => "statement",
            Self::Expression => "expression",
            Self::Template => "template",
            Self::BinaryOperator => "binary-operator",
            Self::UnaryOperator => "unary-operator",
            Self::Record => "record",
        }
    }
}

#[derive(Args)]
pub struct SchemaOptions {
    #[arg(value_enum, help = "Optional contract section.")]
    pub section: Option<Section>,
    #[arg(long, help = "One kind within the selected node section.")]
    pub kind: Option<String>,
}

#[derive(Clone, Copy, Serialize)]
pub struct KindSpec {
    pub kind: &'static str,
    pub value: &'static str,
    pub authorable: bool,
}

const fn spec(kind: &'static str, value: &'static str) -> KindSpec {
    KindSpec {
        kind,
        value,
        authorable: true,
    }
}

const fn refused(kind: &'static str, value: &'static str) -> KindSpec {
    KindSpec {
        kind,
        value,
        authorable: false,
    }
}

pub const TYPE_SPECS: &[KindSpec] = &[
    spec("unit", "absent"),
    spec("bool", "absent"),
    spec("int", "absent"),
    spec("float", "absent"),
    spec("string", "absent"),
    spec("list", "type"),
    spec("set", "type"),
    spec("map", "[type,type]"),
    spec("optional", "type"),
    spec("tuple", "type[]"),
    spec("named", "{name:string,args:type[]}"),
    spec("fn", "{params:type[],returns:type}"),
];

pub const STATEMENT_SPECS: &[KindSpec] = &[
    spec("return", "expr|null"),
    spec(
        "let",
        "{name:string,ty:type|null,value:expr|null,mutable:bool}",
    ),
    spec("assign", "{target:expr,value:expr}"),
    spec("tuple-assign", "{names:string[],value:expr,declares:bool}"),
    spec(
        "if",
        "{condition:expr,then:statement[],otherwise:statement[]}",
    ),
    spec(
        "if-present",
        "{binding:string,value:expr,then:statement[],otherwise:statement[]}",
    ),
    spec("while", "{condition:expr,body:statement[]}"),
    spec(
        "counted-for",
        "{init:statement|null,condition:expr|null,update:statement|null,body:statement[]}",
    ),
    spec(
        "for-each-indexed",
        "{index:string,binding:string,iterable:expr,body:statement[]}",
    ),
    spec("defer", "statement[]"),
    spec("err-defer", "statement[]"),
    spec(
        "switch",
        "{subject:expr,arms:[[expr[],statement[]]],default:statement[]}",
    ),
    spec(
        "match-variants",
        "{subject:expr,sum:string,arms:variant-arm[],default:statement[]}",
    ),
    spec(
        "while-present",
        "{binding:string,value:expr,body:statement[]}",
    ),
    spec(
        "for-each",
        "{binding:string,iterable:expr,body:statement[]}",
    ),
    spec("expr", "expr"),
    spec("assert", "{condition:expr,message:expr|null}"),
    spec("comment", "string"),
    spec("local-function", "function"),
    spec("block", "statement[]"),
    spec("throw", "expr"),
    spec(
        "try",
        "{body:statement[],catches:catch[],finally:statement[]}",
    ),
    spec("break", "absent"),
    spec("break-with", "{label:string,value:expr|null}"),
    spec("continue", "absent"),
    refused("unsupported", "{construct:string,source:string,line:uint}"),
];

pub const EXPRESSION_SPECS: &[KindSpec] = &[
    spec("int", "string"),
    spec("float", "string"),
    spec("str", "string"),
    spec("bool", "bool"),
    spec("null", "absent"),
    spec("name", "string"),
    spec("field", "{of:expr,name:string}"),
    spec("index", "{of:expr,index:expr}"),
    spec("call", "{callee:expr,args:expr[]}"),
    spec("binary", "{op:binary-operator,left:expr,right:expr}"),
    spec("unary", "{op:unary-operator,operand:expr}"),
    spec("await", "expr"),
    spec("propagate", "expr"),
    spec("keyword", "{name:string,value:expr}"),
    spec("cast", "{ty:expr,value:expr}"),
    spec("instance-of", "{value:expr,ty:expr}"),
    spec("new", "{callee:expr,args:expr[]}"),
    spec("record-lit", "{ty:string,fields:[[string,expr]]}"),
    spec("coalesce", "{value:expr,fallback:expr}"),
    spec("ternary", "{condition:expr,then:expr,otherwise:expr}"),
    spec("variant", "{sum:string,name:string,fields:[[string,expr]]}"),
    spec("tuple", "expr[]"),
    spec("list-lit", "expr[]"),
    spec("map-lit", "[[expr,expr]]"),
    spec("template", "template[]"),
    spec("lambda", "{params:param[],returns:type|null,body:expr}"),
    spec("set-lit", "expr[]"),
    spec(
        "comprehension",
        "{element:expr,binding:string,iterable:expr,condition:expr|null}",
    ),
    refused("unsupported", "{construct:string,source:string,line:uint}"),
];

pub const TEMPLATE_SPECS: &[KindSpec] = &[spec("text", "string"), spec("expr", "expr")];

fn specs(section: Section) -> Option<&'static [KindSpec]> {
    match section {
        Section::Type => Some(TYPE_SPECS),
        Section::Statement => Some(STATEMENT_SPECS),
        Section::Expression => Some(EXPRESSION_SPECS),
        Section::Template => Some(TEMPLATE_SPECS),
        _ => None,
    }
}

fn operators(section: Section) -> Option<&'static [&'static str]> {
    match section {
        Section::BinaryOperator => Some(BINARY_OPERATORS),
        Section::UnaryOperator => Some(UNARY_OPERATORS),
        _ => None,
    }
}

fn section_report(section: Section) -> Value {
    if let Some(specs) = specs(section) {
        return json!({"section":section.name(),"encoding":{"tag":"kind","content":"value","case":"kebab-case"},"variants":specs});
    }
    if let Some(values) = operators(section) {
        return json!({"section":section.name(),"encoding":"kebab-case string","values":values});
    }
    match section {
        Section::Body => {
            json!({"section":"body","shape":{"schema":BODY_SCHEMA,"body":"statement[]"},"limits":{"bytes":65536,"statements":512,"semantic_nodes":4096}})
        }
        Section::Record => json!({"section":"record","records":{
            "param":"{name:string,ty:type|null,default:expr|null,kind:param-kind}",
            "function":"{doc:string[],name:string,receiver:string|null,receiver_binding:string|null,params:param[],returns:type|null,body:statement[],exported:bool,is_async:bool,is_property:bool,is_constructor:bool,is_private:bool}",
            "catch":"{binding:string|null,ty:type|null,body:statement[]}",
            "variant-arm":"{variant:string,bindings:[[string,string]],body:statement[]}"},
            "enums":{"param-kind":["normal","var-args","keyword-args","marker"]}}),
        _ => unreachable!(),
    }
}

fn python_constructor(section: Section, kind: &str) -> String {
    let namespace = match section {
        Section::Type => "Type",
        Section::Statement => "Stmt",
        Section::Expression => "Expr",
        Section::Template => "TemplatePart",
        _ => unreachable!(),
    };
    let name = kind
        .split('-')
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<String>();
    format!("{namespace}.{name}")
}

pub fn catalog(options: &SchemaOptions) -> Result<Value> {
    if let Some(kind) = options.kind.as_deref() {
        let section = options
            .section
            .ok_or_else(|| anyhow::anyhow!("--kind requires a section."))?;
        let entries = specs(section).ok_or_else(|| {
            anyhow::anyhow!("--kind requires type, statement, expression or template.")
        })?;
        let entry = entries.iter().find(|entry| entry.kind == kind);
        ensure!(
            entry.is_some(),
            "unknown {} kind '{}'.",
            section.name(),
            kind
        );
        return Ok(json!({
            "schema":"fr-semantic-catalog-1",
            "semantic_schema":BODY_SCHEMA,
            "section":section.name(),
            "variant":entry,
            "python":{"package":"fr_ir","constructor":python_constructor(section, kind)}
        }));
    }
    if let Some(section) = options.section {
        return Ok(
            json!({"schema":"fr-semantic-catalog-1","semantic_schema":BODY_SCHEMA,"contract":section_report(section)}),
        );
    }
    Ok(json!({
        "schema":"fr-semantic-catalog-1",
        "semantic_schema":BODY_SCHEMA,
        "encoding":{"enum":"adjacently-tagged","tag":"kind","content":"value","case":"kebab-case"},
        "sections":[
            {"name":"body","entries":1}, {"name":"type","entries":TYPE_SPECS.len()},
            {"name":"statement","entries":STATEMENT_SPECS.len()},
            {"name":"expression","entries":EXPRESSION_SPECS.len()},
            {"name":"template","entries":TEMPLATE_SPECS.len()},
            {"name":"binary-operator","entries":BINARY_OPERATORS.len()},
            {"name":"unary-operator","entries":UNARY_OPERATORS.len()},
            {"name":"record","entries":4}
        ],
        "commands":{
            "section":"fr author semantic-schema <SECTION>",
            "variant":"fr author semantic-schema <SECTION> --kind <KIND>",
            "validate":"fr author validate-semantic --from <FILE>"
        }
    }))
}

pub fn kind_authorable(category: usize, kind: usize) -> bool {
    let specs = match category {
        0 => TYPE_SPECS,
        1 => STATEMENT_SPECS,
        2 => EXPRESSION_SPECS,
        3 => TEMPLATE_SPECS,
        _ => return false,
    };
    specs.get(kind).is_some_and(|entry| entry.authorable)
}

pub fn semantic_node_source_free(
    has_source_field: bool,
    unsupported_kind: bool,
    children_source_free: bool,
) -> bool {
    !has_source_field && !unsupported_kind && children_source_free
}

pub fn source_free(value: &Value) -> bool {
    match value {
        Value::Array(values) => values.iter().all(source_free),
        Value::Object(object) => semantic_node_source_free(
            object.contains_key("source"),
            object.get("kind").and_then(Value::as_str) == Some("unsupported"),
            object.values().all(source_free),
        ),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_free_walk_checks_every_nested_value() {
        assert!(source_free(
            &json!({"body":[{"kind":"comment","value":"ok"}]})
        ));
        assert!(!source_free(
            &json!({"body":[{"kind":"comment","value":{"source":"hidden"}}]})
        ));
        assert!(!source_free(
            &json!({"body":[{"kind":"unsupported","value":null}]})
        ));
    }
}
