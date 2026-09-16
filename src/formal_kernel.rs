use crate::transpile::ir::{BinaryOp, Expr, Function, Stmt, UnaryOp};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA: &str = "fr-pure-kernel-1";
pub const LEAN_SOURCE: &str = include_str!("../kernels/FrKernels/PureKernel.lean");

pub fn quote_string(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            character if character.is_control() && (character as u32) <= 0xffff => {
                output.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

pub fn quote_value(value: &Value) -> String {
    let sequence = |values: &[Value]| {
        values
            .iter()
            .map(quote_value)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let string = quote_string;
    let body = match value {
        Value::Unit => "unit".into(),
        Value::Bool(value) => format!("bool {value}"),
        Value::Int(value) => format!("int ({value})"),
        Value::String(value) => format!("string {}", string(value)),
        Value::Tuple(items) => format!("tuple [{}]", sequence(items)),
        Value::List(items) => format!("list [{}]", sequence(items)),
        Value::Record(fields) => format!(
            "record [{}]",
            fields
                .iter()
                .map(|(name, value)| format!("({}, {})", string(name), quote_value(value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::Option(None) => "option none".into(),
        Value::Option(Some(value)) => format!("option (some {})", quote_value(value)),
        Value::Result { ok, value } => format!("result {ok} {}", quote_value(value)),
    };
    format!("(FrPureKernel.Value.{body})")
}

pub fn quote_term(term: &Term) -> String {
    let sequence = |terms: &[Term]| terms.iter().map(quote_term).collect::<Vec<_>>().join(", ");
    let body = match term {
        Term::Value { value } => format!("value {}", quote_value(value)),
        Term::Bound { index } => format!("bound {index}"),
        Term::Let { value, body } => format!("letE {} {}", quote_term(value), quote_term(body)),
        Term::If {
            condition,
            then,
            otherwise,
        } => format!(
            "ite {} {} {}",
            quote_term(condition),
            quote_term(then),
            quote_term(otherwise)
        ),
        Term::Binary {
            operator,
            left,
            right,
        } => format!(
            "binary FrPureKernel.Operator.{} {} {}",
            serde_json::to_value(operator)
                .expect("operator serialization")
                .as_str()
                .expect("operator tag"),
            quote_term(left),
            quote_term(right)
        ),
        Term::Not { operand } => format!("notE {}", quote_term(operand)),
        Term::Neg { operand } => format!("neg {}", quote_term(operand)),
        Term::Tuple { items } => format!("tuple [{}]", sequence(items)),
        Term::List { items } => format!("list [{}]", sequence(items)),
        Term::Record { fields } => format!(
            "record [{}]",
            fields
                .iter()
                .map(|(name, term)| format!("({}, {})", quote_string(name), quote_term(term)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Term::Option { value: None } => "option none".into(),
        Term::Option { value: Some(value) } => format!("option (some {})", quote_term(value)),
        Term::Result { ok, value } => format!("result {ok} {}", quote_term(value)),
        Term::Field { value, name } => {
            format!("field {} {}", quote_term(value), quote_string(name))
        }
        Term::Index { value, index } => format!("index {} {index}", quote_term(value)),
    };
    format!("(FrPureKernel.Term.{body})")
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum Value {
    Unit,
    Bool(bool),
    Int(i64),
    String(String),
    Tuple(Vec<Value>),
    List(Vec<Value>),
    Record(BTreeMap<String, Value>),
    Option(Option<Box<Value>>),
    Result { ok: bool, value: Box<Value> },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Term {
    Value {
        value: Value,
    },
    Bound {
        index: usize,
    },
    Let {
        value: Box<Term>,
        body: Box<Term>,
    },
    If {
        condition: Box<Term>,
        then: Box<Term>,
        otherwise: Box<Term>,
    },
    Binary {
        operator: Operator,
        left: Box<Term>,
        right: Box<Term>,
    },
    Not {
        operand: Box<Term>,
    },
    Neg {
        operand: Box<Term>,
    },
    Tuple {
        items: Vec<Term>,
    },
    List {
        items: Vec<Term>,
    },
    Record {
        fields: BTreeMap<String, Term>,
    },
    Option {
        value: Option<Box<Term>>,
    },
    Result {
        ok: bool,
        value: Box<Term>,
    },
    Field {
        value: Box<Term>,
        name: String,
    },
    Index {
        value: Box<Term>,
        index: usize,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Operator {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Xor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Failure {
    Fuel,
    Unbound,
    Type,
    Overflow,
    DivisionByZero,
    MissingField,
    Index,
    Limit,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub term: Term,
    #[serde(default)]
    pub environment: Vec<Value>,
    #[serde(default = "default_fuel")]
    pub fuel: usize,
}

pub fn default_fuel() -> usize {
    64
}

pub fn kernel_limits_admitted(fuel: usize, environment: usize, nodes: usize, depth: usize) -> bool {
    (1..=256).contains(&fuel) && environment <= 64 && nodes <= 4096 && depth <= 64
}

pub fn evaluate(term: &Term, environment: &[Value], fuel: usize) -> Result<Value, Failure> {
    if !kernel_limits_admitted(fuel, environment.len(), 0, 0) {
        return Err(Failure::Limit);
    }
    let mut nodes = 0;
    validate_term(term, 0, &mut nodes)?;
    for value in environment {
        validate_value(value, 0, &mut nodes)?;
    }
    eval(term, environment, fuel)
}

fn count(depth: usize, nodes: &mut usize) -> Result<(), Failure> {
    *nodes += 1;
    if *nodes > 4096 || depth > 64 {
        return Err(Failure::Limit);
    }
    Ok(())
}

fn validate_value(value: &Value, depth: usize, nodes: &mut usize) -> Result<(), Failure> {
    count(depth, nodes)?;
    match value {
        Value::Tuple(values) | Value::List(values) => {
            for value in values {
                validate_value(value, depth + 1, nodes)?;
            }
        }
        Value::Record(fields) => {
            for (name, value) in fields {
                if name.len() > 256 {
                    return Err(Failure::Limit);
                }
                validate_value(value, depth + 1, nodes)?;
            }
        }
        Value::Option(Some(value)) | Value::Result { value, .. } => {
            validate_value(value, depth + 1, nodes)?
        }
        Value::String(value) if value.len() > 65_536 => return Err(Failure::Limit),
        _ => {}
    }
    Ok(())
}

fn validate_term(term: &Term, depth: usize, nodes: &mut usize) -> Result<(), Failure> {
    count(depth, nodes)?;
    let next = depth + 1;
    match term {
        Term::Value { value } => validate_value(value, next, nodes)?,
        Term::Let { value, body } => {
            validate_term(value, next, nodes)?;
            validate_term(body, next, nodes)?;
        }
        Term::If {
            condition,
            then,
            otherwise,
        } => {
            validate_term(condition, next, nodes)?;
            validate_term(then, next, nodes)?;
            validate_term(otherwise, next, nodes)?;
        }
        Term::Binary { left, right, .. } => {
            validate_term(left, next, nodes)?;
            validate_term(right, next, nodes)?;
        }
        Term::Not { operand } | Term::Neg { operand } => validate_term(operand, next, nodes)?,
        Term::Tuple { items } | Term::List { items } => {
            for item in items {
                validate_term(item, next, nodes)?;
            }
        }
        Term::Record { fields } => {
            for (name, item) in fields {
                if name.len() > 256 {
                    return Err(Failure::Limit);
                }
                validate_term(item, next, nodes)?;
            }
        }
        Term::Option { value: Some(value) }
        | Term::Result { value, .. }
        | Term::Index { value, .. } => validate_term(value, next, nodes)?,
        Term::Field { value, name } => {
            if name.len() > 256 {
                return Err(Failure::Limit);
            }
            validate_term(value, next, nodes)?;
        }
        _ => {}
    }
    Ok(())
}

fn eval(term: &Term, environment: &[Value], fuel: usize) -> Result<Value, Failure> {
    let next = fuel.checked_sub(1).ok_or(Failure::Fuel)?;
    let run = |term| eval(term, environment, next);
    Ok(match term {
        Term::Value { value } => value.clone(),
        Term::Bound { index } => environment.get(*index).cloned().ok_or(Failure::Unbound)?,
        Term::Let { value, body } => {
            let mut nested = vec![run(value)?];
            nested.extend_from_slice(environment);
            eval(body, &nested, next)?
        }
        Term::If {
            condition,
            then,
            otherwise,
        } => match run(condition)? {
            Value::Bool(true) => run(then)?,
            Value::Bool(false) => run(otherwise)?,
            _ => return Err(Failure::Type),
        },
        Term::Binary {
            operator,
            left,
            right,
        } => {
            let left = run(left)?;
            match (operator, &left) {
                (Operator::And, Value::Bool(false)) => Value::Bool(false),
                (Operator::Or, Value::Bool(true)) => Value::Bool(true),
                _ => binary(*operator, left, run(right)?)?,
            }
        }
        Term::Not { operand } => match run(operand)? {
            Value::Bool(value) => Value::Bool(!value),
            _ => return Err(Failure::Type),
        },
        Term::Neg { operand } => match run(operand)? {
            Value::Int(value) => Value::Int(value.checked_neg().ok_or(Failure::Overflow)?),
            _ => return Err(Failure::Type),
        },
        Term::Tuple { items } => Value::Tuple(items.iter().map(run).collect::<Result<_, _>>()?),
        Term::List { items } => Value::List(items.iter().map(run).collect::<Result<_, _>>()?),
        Term::Record { fields } => Value::Record(
            fields
                .iter()
                .map(|(name, term)| Ok((name.clone(), run(term)?)))
                .collect::<Result<_, Failure>>()?,
        ),
        Term::Option { value } => Value::Option(
            value
                .as_ref()
                .map(|term| run(term).map(Box::new))
                .transpose()?,
        ),
        Term::Result { ok, value } => Value::Result {
            ok: *ok,
            value: Box::new(run(value)?),
        },
        Term::Field { value, name } => match run(value)? {
            Value::Record(fields) => fields.get(name).cloned().ok_or(Failure::MissingField)?,
            _ => return Err(Failure::Type),
        },
        Term::Index { value, index } => match run(value)? {
            Value::List(items) | Value::Tuple(items) => {
                items.get(*index).cloned().ok_or(Failure::Index)?
            }
            _ => return Err(Failure::Type),
        },
    })
}

fn binary(operator: Operator, left: Value, right: Value) -> Result<Value, Failure> {
    use Operator::*;
    if matches!(operator, Eq | Ne) {
        return Ok(Value::Bool((left == right) == matches!(operator, Eq)));
    }
    Ok(match (left, right) {
        (Value::Bool(left), Value::Bool(right)) => Value::Bool(match operator {
            And => left && right,
            Or => left || right,
            Xor => left ^ right,
            _ => return Err(Failure::Type),
        }),
        (Value::Int(left), Value::Int(right)) => match operator {
            Lt => Value::Bool(left < right),
            Le => Value::Bool(left <= right),
            Gt => Value::Bool(left > right),
            Ge => Value::Bool(left >= right),
            Add | Sub | Mul | Div | Rem => {
                if matches!(operator, Div | Rem) && right == 0 {
                    return Err(Failure::DivisionByZero);
                }
                Value::Int(
                    match operator {
                        Add => left.checked_add(right),
                        Sub => left.checked_sub(right),
                        Mul => left.checked_mul(right),
                        Div => left.checked_div(right),
                        Rem => left.checked_rem(right),
                        _ => unreachable!(),
                    }
                    .ok_or(Failure::Overflow)?,
                )
            }
            _ => return Err(Failure::Type),
        },
        _ => return Err(Failure::Type),
    })
}

pub fn compile(function: &Function) -> anyhow::Result<Term> {
    if function.params.len() > 64 {
        anyhow::bail!("formal kernel accepts at most 64 parameters.");
    }
    let names = function
        .params
        .iter()
        .map(|parameter| parameter.name.clone())
        .collect::<Vec<_>>();
    if names
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != names.len()
    {
        anyhow::bail!("formal kernel parameters must have unique names.");
    }
    let term = compile_block(&function.body, &names, 0)?;
    validate_term(&term, 0, &mut 0)
        .map_err(|failure| anyhow::anyhow!("formal kernel exceeds its {failure:?} boundary"))?;
    let types = function
        .params
        .iter()
        .map(|parameter| {
            parameter
                .ty
                .clone()
                .ok_or_else(|| anyhow::anyhow!("formal kernel parameters need explicit types"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let inferred = infer(&term, &types)?;
    if function
        .returns
        .as_ref()
        .is_none_or(|expected| !compatible(&inferred, expected))
    {
        anyhow::bail!("formal kernel body type does not match its explicit return type.");
    }
    Ok(term)
}

fn compatible(actual: &crate::transpile::ir::Type, expected: &crate::transpile::ir::Type) -> bool {
    use crate::transpile::ir::Type;
    match (actual, expected) {
        (Type::Named { name, args }, _) if name == "__fr_empty_element__" && args.is_empty() => {
            true
        }
        (Type::List(actual), Type::List(expected))
        | (Type::Optional(actual), Type::Optional(expected)) => compatible(actual, expected),
        (Type::Tuple(actual), Type::Tuple(expected)) => {
            actual.len() == expected.len()
                && actual
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| compatible(actual, expected))
        }
        _ => actual == expected,
    }
}

fn infer(
    term: &Term,
    environment: &[crate::transpile::ir::Type],
) -> anyhow::Result<crate::transpile::ir::Type> {
    use crate::transpile::ir::Type;
    let run = |term| infer(term, environment);
    Ok(match term {
        Term::Value { value: Value::Unit } => Type::Unit,
        Term::Value {
            value: Value::Bool(_),
        } => Type::Bool,
        Term::Value {
            value: Value::Int(_),
        } => Type::Int,
        Term::Value {
            value: Value::String(_),
        } => Type::String,
        Term::Bound { index } => environment
            .get(*index)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unbound formal kernel type"))?,
        Term::Let { value, body } => {
            let mut nested = vec![run(value)?];
            nested.extend_from_slice(environment);
            infer(body, &nested)?
        }
        Term::If {
            condition,
            then,
            otherwise,
        } => {
            let yes = run(then)?;
            let no = run(otherwise)?;
            if run(condition)? != Type::Bool || !(compatible(&yes, &no) || compatible(&no, &yes)) {
                anyhow::bail!(
                    "formal conditionals need a Bool condition and matching branch types."
                );
            }
            if compatible(&yes, &no) {
                no
            } else {
                yes
            }
        }
        Term::Not { operand } => {
            if run(operand)? != Type::Bool {
                anyhow::bail!("formal logical negation requires Bool.");
            }
            Type::Bool
        }
        Term::Neg { operand } => {
            if run(operand)? != Type::Int {
                anyhow::bail!("formal numeric negation requires Int.");
            }
            Type::Int
        }
        Term::Binary {
            operator,
            left,
            right,
        } => {
            let left = run(left)?;
            let right = run(right)?;
            let (required, output) = match operator {
                Operator::And | Operator::Or | Operator::Xor => (Type::Bool, Type::Bool),
                Operator::Add | Operator::Sub | Operator::Mul | Operator::Div | Operator::Rem => {
                    (Type::Int, Type::Int)
                }
                Operator::Lt | Operator::Le | Operator::Gt | Operator::Ge => {
                    (Type::Int, Type::Bool)
                }
                Operator::Eq | Operator::Ne => {
                    if !compatible(&left, &right) {
                        anyhow::bail!("formal equality needs matching operand types.");
                    }
                    return Ok(Type::Bool);
                }
            };
            if left != required || right != required {
                anyhow::bail!("formal operator requires {required} operands.");
            }
            output
        }
        Term::Tuple { items } => Type::Tuple(items.iter().map(run).collect::<anyhow::Result<_>>()?),
        Term::List { items } => {
            let types = items.iter().map(run).collect::<anyhow::Result<Vec<_>>>()?;
            let inner = types
                .first()
                .cloned()
                .unwrap_or_else(|| Type::named("__fr_empty_element__"));
            if types.iter().any(|ty| !compatible(ty, &inner)) {
                anyhow::bail!("formal lists require one element type.");
            }
            Type::List(Box::new(inner))
        }
        _ => anyhow::bail!(
            "source kernel type inference needs an explicit model for this value or construct."
        ),
    })
}

fn compile_block(statements: &[Stmt], names: &[String], depth: usize) -> anyhow::Result<Term> {
    if depth > 64 {
        anyhow::bail!("formal kernel scope depth exceeds 64.");
    }
    let statements = statements
        .iter()
        .filter(|statement| !matches!(statement, Stmt::Comment(_)))
        .collect::<Vec<_>>();
    if statements.len() > 4096 {
        anyhow::bail!("formal kernel block exceeds 4096 statements.");
    }
    Ok(match statements.as_slice() {
        [Stmt::Return(Some(expression))] | [Stmt::Expr(expression)] => compile_expr(expression, names, depth + 1)?,
        [Stmt::Return(None)] => Term::Value { value: Value::Unit },
        [Stmt::Block(body)] => compile_block(body, names, depth + 1)?,
        [Stmt::If { condition, then, otherwise }] if !otherwise.is_empty() => Term::If {
            condition: Box::new(compile_expr(condition, names, depth + 1)?),
            then: Box::new(compile_block(then, names, depth + 1)?),
            otherwise: Box::new(compile_block(otherwise, names, depth + 1)?),
        },
        [Stmt::Let { name, value: Some(value), mutable: false, .. }, rest @ ..] if !rest.is_empty() => {
            let mut nested = vec![name.clone()];
            nested.extend_from_slice(names);
            Term::Let { value: Box::new(compile_expr(value, names, depth + 1)?),
                body: Box::new(compile_block(&rest.iter().map(|statement| (*statement).clone()).collect::<Vec<_>>(), &nested, depth + 1)?) }
        },
        _ => anyhow::bail!("formal kernel statements require immutable bindings, returns and complete conditionals; effects and mutation remain outside the contract."),
    })
}

fn compile_expr(expression: &Expr, names: &[String], depth: usize) -> anyhow::Result<Term> {
    if depth > 64 {
        anyhow::bail!("formal kernel expression depth exceeds 64.");
    }
    let run = |expression| compile_expr(expression, names, depth + 1);
    Ok(match expression {
        Expr::Bool(value) => Term::Value { value: Value::Bool(*value) },
        Expr::Int(value) => Term::Value { value: Value::Int(value.parse().map_err(|_| anyhow::anyhow!("integer literal needs the checked signed-64 policy."))?) },
        Expr::Str(value) => Term::Value { value: Value::String(value.clone()) },
        Expr::Name(name) => Term::Bound { index: names.iter().position(|bound| bound == name).ok_or_else(|| anyhow::anyhow!("unbound or external name `{name}` needs an explicit environment."))? },
        Expr::Unary { op: UnaryOp::Not, operand } => Term::Not { operand: Box::new(run(operand)?) },
        Expr::Unary { op: UnaryOp::Neg, operand } => Term::Neg { operand: Box::new(run(operand)?) },
        Expr::Binary { op, left, right } => Term::Binary {
            operator: match op {
                BinaryOp::Add => Operator::Add, BinaryOp::Sub => Operator::Sub, BinaryOp::Mul => Operator::Mul,
                BinaryOp::Div => Operator::Div, BinaryOp::Rem => Operator::Rem, BinaryOp::Eq => Operator::Eq,
                BinaryOp::Ne => Operator::Ne, BinaryOp::Lt => Operator::Lt, BinaryOp::Le => Operator::Le,
                BinaryOp::Gt => Operator::Gt, BinaryOp::Ge => Operator::Ge, BinaryOp::And => Operator::And,
                BinaryOp::Or => Operator::Or, BinaryOp::Xor => Operator::Xor,
                BinaryOp::FloorDiv | BinaryOp::TrueDiv | BinaryOp::FloorRem => anyhow::bail!("floor and floating-point arithmetic need a separate formal kernel policy."),
            }, left: Box::new(run(left)?), right: Box::new(run(right)?),
        },
        Expr::Ternary { condition, then, otherwise } => Term::If { condition: Box::new(run(condition)?), then: Box::new(run(then)?), otherwise: Box::new(run(otherwise)?) },
        Expr::Tuple(items) if items.is_empty() => Term::Value { value: Value::Unit },
        Expr::Tuple(items) => Term::Tuple { items: items.iter().map(run).collect::<anyhow::Result<_>>()? },
        Expr::ListLit(items) => Term::List { items: items.iter().map(run).collect::<anyhow::Result<_>>()? },
        _ => anyhow::bail!("formal kernel expression needs an explicit model for calls, effects, partial access or this construct."),
    })
}
