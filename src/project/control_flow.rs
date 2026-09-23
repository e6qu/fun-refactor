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
}

pub(super) struct Block<'t> {
    pub syntax: Option<Node<'t>>,
    pub operation: Operation,
    pub successors: Vec<(usize, &'static str)>,
}

pub(super) struct Graph<'t> {
    pub blocks: Vec<Block<'t>>,
    pub entry: usize,
}

impl<'t> Graph<'t> {
    pub fn lower(function: Node<'t>) -> Result<Self> {
        let mut graph = Self {
            blocks: Vec::new(),
            entry: 0,
        };
        graph.add(None, Operation::Exit, vec![])?;
        graph.add(None, Operation::ExceptionalExit, vec![])?;
        let first = match function.child_by_field_name("body") {
            Some(body) => graph.sequence(body, 0, None)?,
            None => 0,
        };
        graph.entry = graph.add(Some(function), Operation::Entry, vec![(first, "entry")])?;
        Ok(graph)
    }

    fn add(
        &mut self,
        syntax: Option<Node<'t>>,
        operation: Operation,
        successors: Vec<(usize, &'static str)>,
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
        });
        Ok(id)
    }

    fn sequence(
        &mut self,
        body: Node<'t>,
        mut next: usize,
        loop_targets: Option<(usize, usize)>,
    ) -> Result<usize> {
        let statements: Vec<_> = body.named_children(&mut body.walk()).collect();
        for statement in statements.into_iter().rev() {
            next = self.statement(statement, next, loop_targets)?;
        }
        Ok(next)
    }

    fn branch(
        &mut self,
        node: Node<'t>,
        next: usize,
        otherwise: usize,
        loop_targets: Option<(usize, usize)>,
    ) -> Result<usize> {
        let yes = match node.child_by_field_name("consequence") {
            Some(body) => self.sequence(body, next, loop_targets)?,
            None => next,
        };
        self.add(
            node.child_by_field_name("condition"),
            Operation::Condition,
            vec![(yes, "true"), (otherwise, "false")],
        )
    }

    fn statement(
        &mut self,
        node: Node<'t>,
        next: usize,
        loop_targets: Option<(usize, usize)>,
    ) -> Result<usize> {
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
                            otherwise = self.sequence(body, next, loop_targets)?;
                        }
                    } else {
                        otherwise = self.branch(clause, next, otherwise, loop_targets)?;
                    }
                }
                self.branch(node, next, otherwise, loop_targets)
            }
            "while_statement" => {
                let head = self.add(
                    node.child_by_field_name("condition"),
                    Operation::Condition,
                    vec![],
                )?;
                let body = match node.child_by_field_name("body") {
                    Some(body) => self.sequence(body, head, Some((next, head)))?,
                    None => head,
                };
                let otherwise = match node
                    .child_by_field_name("alternative")
                    .and_then(|n| n.child_by_field_name("body"))
                {
                    Some(body) => self.sequence(body, next, loop_targets)?,
                    None => next,
                };
                self.blocks[head].successors = vec![(body, "true"), (otherwise, "false")];
                Ok(head)
            }
            "return_statement" => self.add(Some(node), Operation::Return, vec![(0, "return")]),
            "raise_statement" => self.add(Some(node), Operation::Raise, vec![(1, "raise")]),
            "break_statement" if loop_targets.is_some() => self.add(
                Some(node),
                Operation::Jump,
                vec![(loop_targets.unwrap().0, "break")],
            ),
            "continue_statement" if loop_targets.is_some() => self.add(
                Some(node),
                Operation::Jump,
                vec![(loop_targets.unwrap().1, "continue")],
            ),
            _ => self.add(Some(node), Operation::Statement, vec![(next, "next")]),
        }
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
                };
                let origins = match block.syntax {
                    Some(node) => {
                        project.occurrence_origins(file, Span::from(node), "control-flow")
                    }
                    None => SourceOrigins::Absent {
                        reason: "synthetic function exit".into(),
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
                block
                    .successors
                    .iter()
                    .map(move |(to, rule)| json!({"from":from,"to":to,"rule":rule}))
            })
            .collect();
        json!({"function":name,"entry":self.entry,"normal_exit":0,"exceptional_exit":1,
            "nodes":nodes,"edges":edges,"claim":"syntactic control flow; branch feasibility unchecked"})
    }
}
