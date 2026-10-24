use super::{occurrence::SourceOrigins, Project};
use crate::span::Span;
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use std::path::Path;
use tree_sitter::Node;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Operation {
    Entry,
    Exit,
    ExceptionalExit,
    Condition,
    Statement,
    Return,
    Raise,
    Jump,
    ResumeReturn,
    ResumeRaise,
}

#[derive(Clone, Copy)]
pub(super) struct CompletionTarget {
    pub node: usize,
    pub slot: usize,
}

#[derive(Clone, Copy)]
struct Exits {
    returned: CompletionTarget,
    raised: CompletionTarget,
    loops: Option<(usize, usize)>,
}

pub(super) struct Block<'t> {
    pub syntax: Option<Node<'t>>,
    pub operation: Operation,
    pub successors: Vec<(usize, &'static str)>,
    pub raised: CompletionTarget,
    pub completion: Option<(usize, usize)>,
}

pub(super) struct Graph<'t> {
    pub blocks: Vec<Block<'t>>,
    pub entry: usize,
    completion_slots: usize,
}

impl<'t> Graph<'t> {
    pub fn lower(function: Node<'t>) -> Result<Self> {
        let mut graph = Self {
            blocks: Vec::new(),
            entry: 0,
            completion_slots: 1,
        };
        let exits = Exits {
            returned: CompletionTarget { node: 0, slot: 0 },
            raised: CompletionTarget { node: 1, slot: 0 },
            loops: None,
        };
        graph.add(None, Operation::Exit, vec![], exits)?;
        graph.add(None, Operation::ExceptionalExit, vec![], exits)?;
        let first = match function.child_by_field_name("body") {
            Some(body) => graph.sequence(body, 0, exits)?,
            None => 0,
        };
        graph.entry = graph.add(
            Some(function),
            Operation::Entry,
            vec![(first, "entry")],
            exits,
        )?;
        Ok(graph)
    }

    fn add(
        &mut self,
        syntax: Option<Node<'t>>,
        operation: Operation,
        successors: Vec<(usize, &'static str)>,
        exits: Exits,
    ) -> Result<usize> {
        ensure!(
            self.blocks.len() < 512,
            "control-flow graph exceeds 512 blocks"
        );
        let id = self.blocks.len();
        self.blocks.push(Block {
            syntax,
            operation,
            successors,
            raised: exits.raised,
            completion: match operation {
                Operation::Return => Some((exits.returned.slot, exits.returned.slot)),
                Operation::Raise => Some((exits.raised.slot, exits.raised.slot)),
                _ => None,
            },
        });
        Ok(id)
    }

    fn sequence(&mut self, body: Node<'t>, mut next: usize, exits: Exits) -> Result<usize> {
        let statements: Vec<_> = body.named_children(&mut body.walk()).collect();
        for statement in statements.into_iter().rev() {
            next = self.statement(statement, next, exits)?;
        }
        Ok(next)
    }

    fn branch(
        &mut self,
        node: Node<'t>,
        next: usize,
        otherwise: usize,
        exits: Exits,
    ) -> Result<usize> {
        let yes = match node.child_by_field_name("consequence") {
            Some(body) => self.sequence(body, next, exits)?,
            None => next,
        };
        self.add(
            node.child_by_field_name("condition"),
            Operation::Condition,
            vec![(yes, "true"), (otherwise, "false")],
            exits,
        )
    }

    fn statement(&mut self, node: Node<'t>, next: usize, exits: Exits) -> Result<usize> {
        match node.kind() {
            "if_statement" => {
                let clauses: Vec<_> = node
                    .named_children(&mut node.walk())
                    .filter(|child| matches!(child.kind(), "elif_clause" | "else_clause"))
                    .collect();
                let mut otherwise = next;
                for clause in clauses.into_iter().rev() {
                    if clause.kind() == "else_clause" {
                        if let Some(body) = clause.child_by_field_name("body") {
                            otherwise = self.sequence(body, next, exits)?;
                        }
                    } else {
                        otherwise = self.branch(clause, next, otherwise, exits)?;
                    }
                }
                self.branch(node, next, otherwise, exits)
            }
            "while_statement" => {
                let head = self.add(
                    node.child_by_field_name("condition"),
                    Operation::Condition,
                    vec![],
                    exits,
                )?;
                let body = match node.child_by_field_name("body") {
                    Some(body) => self.sequence(
                        body,
                        head,
                        Exits {
                            loops: Some((next, head)),
                            ..exits
                        },
                    )?,
                    None => head,
                };
                let otherwise = match node
                    .child_by_field_name("alternative")
                    .and_then(|n| n.child_by_field_name("body"))
                {
                    Some(body) => self.sequence(body, next, exits)?,
                    None => next,
                };
                self.blocks[head].successors = vec![(body, "true"), (otherwise, "false")];
                Ok(head)
            }
            "try_statement" if finally_body(node).is_some() => self.finally(node, next, exits),
            "return_statement" => self.add(
                Some(node),
                Operation::Return,
                vec![(exits.returned.node, "return")],
                exits,
            ),
            "raise_statement" => self.add(
                Some(node),
                Operation::Raise,
                vec![(exits.raised.node, "raise")],
                exits,
            ),
            "break_statement" if exits.loops.is_some() => self.add(
                Some(node),
                Operation::Jump,
                vec![(exits.loops.unwrap().0, "break")],
                exits,
            ),
            "continue_statement" if exits.loops.is_some() => self.add(
                Some(node),
                Operation::Jump,
                vec![(exits.loops.unwrap().1, "continue")],
                exits,
            ),
            _ => self.add(
                Some(node),
                Operation::Statement,
                vec![(next, "next")],
                exits,
            ),
        }
    }

    fn resume(
        &mut self,
        operation: Operation,
        slot: usize,
        target: CompletionTarget,
        exits: Exits,
    ) -> Result<usize> {
        let edge = if operation == Operation::ResumeReturn {
            "return"
        } else {
            "raise"
        };
        let id = self.add(None, operation, vec![(target.node, edge)], exits)?;
        self.blocks[id].completion = Some((slot, target.slot));
        Ok(id)
    }

    fn finally(&mut self, node: Node<'t>, next: usize, exits: Exits) -> Result<usize> {
        let final_body = finally_body(node).expect("checked finally shape");
        let slot = self.completion_slots;
        self.completion_slots += 1;
        let returned = self.resume(Operation::ResumeReturn, slot, exits.returned, exits)?;
        let returned = self.sequence(final_body, returned, exits)?;
        let raised = self.resume(Operation::ResumeRaise, slot, exits.raised, exits)?;
        let raised = self.sequence(final_body, raised, exits)?;
        let normal = self.sequence(final_body, next, exits)?;
        let loops = match exits.loops {
            Some((stop, again)) => Some((
                self.sequence(final_body, stop, exits)?,
                self.sequence(final_body, again, exits)?,
            )),
            None => None,
        };
        self.sequence(
            node.child_by_field_name("body").expect("try body"),
            normal,
            Exits {
                returned: CompletionTarget {
                    node: returned,
                    slot,
                },
                raised: CompletionTarget { node: raised, slot },
                loops,
            },
        )
    }

    pub fn report(&self, project: &Project<'_>, file: &Path, name: &str) -> Value {
        let nodes: Vec<_> = self
            .blocks
            .iter()
            .enumerate()
            .map(|(id, block)| {
                let operation = match block.operation {
                    Operation::Entry => "entry",
                    Operation::Exit => "exit",
                    Operation::ExceptionalExit => "exceptional-exit",
                    Operation::Condition => "condition",
                    Operation::Statement => "statement",
                    Operation::Return => "return",
                    Operation::Raise => "raise",
                    Operation::Jump => "jump",
                    Operation::ResumeReturn => "resume-return",
                    Operation::ResumeRaise => "resume-raise",
                };
                let origins = match block.syntax {
                    Some(node) => {
                        project.occurrence_origins(file, Span::from(node), "control-flow")
                    }
                    None => SourceOrigins::Absent {
                        reason: if matches!(
                            block.operation,
                            Operation::ResumeReturn | Operation::ResumeRaise
                        ) {
                            "synthetic completion continuation"
                        } else {
                            "synthetic function exit"
                        }
                        .into(),
                    },
                };
                json!({"id":id, "operation":operation, "origins":origins})
            })
            .collect();
        let edges: Vec<_> = self
            .blocks
            .iter()
            .enumerate()
            .flat_map(|(from, block)| {
                let mut edges: Vec<_> = block
                    .successors
                    .iter()
                    .map(move |(to, rule)| json!({"from":from,"to":to,"rule":rule}))
                    .collect();
                if matches!(
                    block.operation,
                    Operation::Condition
                        | Operation::Statement
                        | Operation::Return
                        | Operation::Raise
                ) && block.raised.node != 1
                    && block.syntax.is_some_and(contains_call)
                {
                    edges.push(json!({"from":from,"to":block.raised.node,"rule":"call-cleanup"}));
                }
                edges
            })
            .collect();
        json!({"function":name,"entry":self.entry,"normal_exit":0,"exceptional_exit":1,
            "nodes":nodes,"edges":edges,"claim":"syntactic control flow; branch feasibility unchecked"})
    }
}

fn finally_body(node: Node<'_>) -> Option<Node<'_>> {
    let clauses: Vec<_> = node
        .named_children(&mut node.walk())
        .filter(|child| child.kind() != "block" && child.kind() != "comment")
        .collect();
    if clauses.len() != 1 || clauses[0].kind() != "finally_clause" {
        return None;
    }
    clauses[0]
        .named_children(&mut clauses[0].walk())
        .find(|child| child.kind() == "block")
}

fn contains_call(node: Node<'_>) -> bool {
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        if node.kind() == "call" {
            return true;
        }
        pending.extend(node.named_children(&mut node.walk()));
    }
    false
}
