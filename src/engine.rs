use crate::parse;
use send_wrapper::SendWrapper;
use std::cmp::PartialEq;
use std::{
    collections::HashMap,
    ops::{Add, Div, Mul, Sub},
};

//TODO: dialogue options inside conditionals

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NodeName(pub String);
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VariableName(pub String);

struct Variables(HashMap<VariableName, Value>);
impl Variables {
    fn set(&mut self, name: VariableName, value: Value) {
        self.0.insert(name, value);
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct Choice {
    text: String,
    kind: ChoiceKind,
}

impl Choice {
    pub(crate) fn external(text: String, name: NodeName) -> Choice {
        Choice {
            text,
            kind: ChoiceKind::External(name),
        }
    }

    pub(crate) fn inline(text: String, steps: Vec<Step>, condition: Option<Expr>) -> Choice {
        Choice {
            text,
            kind: ChoiceKind::Inline(steps, condition),
        }
    }
}

#[derive(Debug, PartialEq)]
enum ChoiceKind {
    External(NodeName),
    Inline(Vec<Step>, Option<Expr>),
}

#[derive(Debug, PartialEq)]
pub(crate) enum Step {
    Dialogue(String, Vec<Choice>),
    Command(String),
    Assign(VariableName, Expr),
    Conditional(Expr, Vec<Step>, Vec<(Expr, Vec<Step>)>, Vec<Step>),
    Jump(NodeName),
}

#[derive(Debug, PartialEq)]
pub(crate) enum Expr {
    Unary(UnaryOp, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Term(Term),
    Parentheses(Box<Expr>),
}

#[derive(Debug, PartialEq)]
pub(crate) enum UnaryOp {
    Not,
    Negate,
}

#[derive(Debug, PartialEq)]
pub(crate) enum BinaryOp {
    And,
    Or,
    Plus,
    Minus,
    Multiply,
    Divide,
    Equals,
    NotEquals,
    GreaterThan,
    LessThan,
    GreaterThanEqual,
    LessThanEqual,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Term {
    Number(f32),
    Boolean(bool),
    String(String),
    Variable(VariableName),
    Function(String, Vec<Expr>),
}

#[derive(Debug, PartialEq)]
pub struct Node {
    pub title: NodeName,
    pub extra: HashMap<String, String>,
    pub(crate) steps: Vec<Step>,
    pub visited: bool,
}

struct Conversation {
    node: NodeName,
    base_index: usize,
    indexes: Vec<StepIndex>,
}

impl Conversation {
    fn new(node: NodeName) -> Conversation {
        Conversation {
            node,
            base_index: 0,
            indexes: vec![],
        }
    }
}

#[derive(Copy, Clone)]
enum StepIndex {
    Dialogue(usize, usize),
    If(usize),
    ElseIf(usize, usize),
    Else(usize),
}

impl StepIndex {
    fn advance(&mut self) {
        let idx = match *self {
            StepIndex::Dialogue(_, ref mut idx)
            | StepIndex::If(ref mut idx)
            | StepIndex::ElseIf(_, ref mut idx)
            | StepIndex::Else(ref mut idx) => idx,
        };
        *idx += 1;
    }
}
/// A primitive value .
#[derive(Clone)]
pub enum Value {
    /// A string value.
    String(String),
    /// A floating point value.
    Number(f32),
    /// A boolean value.
    Boolean(bool),
    //TODO: null
}

impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        match (self, other) {
            (&Value::String(ref s1), ref v) => s1 == &v.as_string(),
            (ref v, &Value::String(ref s2)) => s2 == &v.as_string(),
            (&Value::Number(f1), ref v) => f1 == v.as_num(),
            (ref v, &Value::Number(f2)) => f2 == v.as_num(),
            (&Value::Boolean(b1), &Value::Boolean(b2)) => b1 == b2,
        }
    }
}

impl Add for Value {
    type Output = Value;
    fn add(self, other: Value) -> Value {
        match (self, other) {
            (Value::String(s1), v) => Value::String(format!("{}{}", s1, v.as_string())),
            (v, Value::String(s2)) => Value::String(format!("{}{}", v.as_string(), s2)),
            (Value::Number(f1), v) => Value::Number(f1 + v.as_num()),
            (v, Value::Number(f2)) => Value::Number(v.as_num() + f2),
            (v1, v2) => Value::Number(v1.as_num() + v2.as_num()),
        }
    }
}

impl Sub for Value {
    type Output = Value;
    fn sub(self, other: Value) -> Value {
        Value::Number(self.as_num() - other.as_num())
    }
}

impl Mul for Value {
    type Output = Value;
    fn mul(self, other: Value) -> Value {
        Value::Number(self.as_num() * other.as_num())
    }
}

impl Div for Value {
    type Output = Value;
    fn div(self, other: Value) -> Value {
        Value::Number(self.as_num() / other.as_num())
    }
}

impl Value {
    /// The contained value represented as a string.
    pub fn as_string(&self) -> String {
        match *self {
            Value::Boolean(b) => b.to_string(),
            Value::String(ref s) => (*s).clone(),
            Value::Number(f) => f.to_string(),
        }
    }

    /// The contained value represented as a boolean.
    /// If not already a boolean, true if a non-empty string or non-zero number, false otherwise.
    fn as_bool(&self) -> bool {
        match *self {
            Value::Boolean(b) => b,
            Value::String(ref s) => !s.is_empty(),
            Value::Number(f) => f != 0.0,
        }
    }

    /// The contained value represented as a floating point number.
    /// If not already a number, 0 if a string, 0 or 1 if a boolean.
    fn as_num(&self) -> f32 {
        match *self {
            Value::Boolean(b) => b as isize as f32,
            Value::String(ref _s) => 0.,
            Value::Number(f) => f,
        }
    }
}

struct Function {
    num_args: usize,
    callback: SendWrapper<Box<FunctionCallback>>,
}

/// A closure that will be invoked when a particular function is called in a Yarn expression.
pub type FunctionCallback = dyn Fn(Vec<Value>, &Nodes) -> Result<Value, ()>;

/// The engine that stores all conversation-related state.
pub struct YarnEngine {
    state: NodeState,
    engine_state: EngineState,
    conversion_ended: bool,
}

struct EngineState {
    variables: Variables,
    functions: HashMap<String, Function>,
}

impl EngineState {
    fn evaluate(&self, expr: &Expr, state: &Nodes) -> Result<Value, ()> {
        match expr {
            Expr::Parentheses(expr) => self.evaluate(expr, state),
            Expr::Term(Term::Number(f)) => Ok(Value::Number(*f)),
            Expr::Term(Term::Boolean(b)) => Ok(Value::Boolean(*b)),
            Expr::Term(Term::String(ref s)) => Ok(Value::String((*s).clone())),
            Expr::Term(Term::Variable(ref n)) => self.variables.0.get(n).cloned().ok_or(()),
            Expr::Term(Term::Function(ref name, ref args)) => {
                let mut eval_args = vec![];
                for arg in args {
                    let v = self.evaluate(arg, state)?;
                    eval_args.push(v);
                }
                let f = self.functions.get(name).ok_or(())?;
                if f.num_args != args.len() {
                    return Err(());
                }
                (f.callback)(eval_args, state)
            }

            Expr::Unary(UnaryOp::Not, expr) => self
                .evaluate(expr, state)
                .map(|v| Value::Boolean(!v.as_bool())),
            Expr::Unary(UnaryOp::Negate, expr) => self
                .evaluate(expr, state)
                .map(|v| Value::Number(-v.as_num())),

            Expr::Binary(BinaryOp::And, left, right) => {
                let left = self.evaluate(left, state)?.as_bool();
                let right = self.evaluate(right, state)?.as_bool();
                Ok(Value::Boolean(left && right))
            }
            Expr::Binary(BinaryOp::Or, left, right) => {
                let left = self.evaluate(left, state)?.as_bool();
                let right = self.evaluate(right, state)?.as_bool();
                Ok(Value::Boolean(left || right))
            }

            Expr::Binary(BinaryOp::Plus, left, right) => {
                let left = self.evaluate(left, state)?;
                let right = self.evaluate(right, state)?;
                Ok(left + right)
            }
            Expr::Binary(BinaryOp::Minus, left, right) => {
                let left = self.evaluate(left, state)?;
                let right = self.evaluate(right, state)?;
                Ok(left - right)
            }
            Expr::Binary(BinaryOp::Multiply, left, right) => {
                let left = self.evaluate(left, state)?;
                let right = self.evaluate(right, state)?;
                Ok(left * right)
            }
            Expr::Binary(BinaryOp::Divide, left, right) => {
                let left = self.evaluate(left, state)?;
                let right = self.evaluate(right, state)?;
                Ok(left / right)
            }

            Expr::Binary(BinaryOp::Equals, left, right) => {
                let left = self.evaluate(left, state)?;
                let right = self.evaluate(right, state)?;
                Ok(Value::Boolean(left == right))
            }
            Expr::Binary(BinaryOp::NotEquals, left, right) => {
                let left = self.evaluate(left, state)?;
                let right = self.evaluate(right, state)?;
                Ok(Value::Boolean(!(left == right)))
            }

            Expr::Binary(BinaryOp::GreaterThan, left, right) => {
                let left = self.evaluate(left, state)?;
                let right = self.evaluate(right, state)?;
                Ok(Value::Boolean(left.as_num() > right.as_num()))
            }
            Expr::Binary(BinaryOp::GreaterThanEqual, left, right) => {
                let left = self.evaluate(left, state)?;
                let right = self.evaluate(right, state)?;
                Ok(Value::Boolean(left.as_num() >= right.as_num()))
            }
            Expr::Binary(BinaryOp::LessThan, left, right) => {
                let left = self.evaluate(left, state)?;
                let right = self.evaluate(right, state)?;
                Ok(Value::Boolean(left.as_num() < right.as_num()))
            }
            Expr::Binary(BinaryOp::LessThanEqual, left, right) => {
                let left = self.evaluate(left, state)?;
                let right = self.evaluate(right, state)?;
                Ok(Value::Boolean(left.as_num() <= right.as_num()))
            }
        }
    }
}

/// A collection of Yarn nodes.
pub struct Nodes(pub HashMap<NodeName, Node>);

struct NodeState {
    nodes: Nodes,
    conversation: Option<Conversation>,
}

impl NodeState {
    fn set_conversation(&mut self, conversation: Option<NodeName>) {
        self.conversation = conversation.map(|x| Conversation::new(x));
    }

    fn push_step(&mut self, index: StepIndex) {
        self.conversation.as_mut().unwrap().indexes.push(index);
    }
    fn advance(&mut self) {
        let conversation = self.conversation.as_mut().unwrap();
        match conversation.indexes.last_mut() {
            Some(index) => index.advance(),
            None => conversation.base_index += 1,
        }
    }
    fn get_current_step(&self) -> Option<&Step> {
        let conversation = self
            .conversation
            .as_ref()
            .expect("No active conversation found");
        let mut steps = {
            let current = self.nodes.0.get(&conversation.node).expect("missing node");
            &current.steps
        };
        let mut current_step_index = conversation.base_index;

        for index in &conversation.indexes {
            match (&steps[current_step_index], *index) {
                (&Step::Dialogue(_, ref choices), StepIndex::Dialogue(choice, step_index)) => {
                    let choice = &choices[choice];
                    match choice.kind {
                        ChoiceKind::Inline(ref choice_steps, _) => {
                            steps = choice_steps;
                            current_step_index = step_index;
                        }
                        ChoiceKind::External(..) => unreachable!(),
                    }
                }
                (&Step::Conditional(_, ref if_steps, ..), StepIndex::If(step_index)) => {
                    steps = if_steps;
                    current_step_index = step_index;
                }
                (
                    &Step::Conditional(_, _, ref else_ifs, ..),
                    StepIndex::ElseIf(index, step_index),
                ) => {
                    steps = &else_ifs[index].1;
                    current_step_index = step_index;
                }
                (&Step::Conditional(_, _, _, ref else_steps), StepIndex::Else(step_index)) => {
                    steps = else_steps;
                    current_step_index = step_index;
                }
                _ => unreachable!(),
            }
        }

        steps.get(current_step_index)
    }
}

impl YarnEngine {
    /// Create a new YarnEngine instance associated with the given handler.
    pub fn new() -> Self {
        let mut engine = YarnEngine {
            state: NodeState {
                nodes: Nodes(HashMap::new()),
                conversation: None,
            },
            engine_state: EngineState {
                variables: Variables(HashMap::new()),
                functions: HashMap::new(),
            },
            conversion_ended: false,
            // handler,
        };

        // Define built-in functions.
        engine.register_function(
            "visited".to_string(),
            1,
            Box::new(|args, state| match args[0] {
                Value::String(ref s) => state
                    .0
                    .get(&NodeName(s.to_string()))
                    .map(|node| Value::Boolean(node.visited))
                    .ok_or(()),
                _ => return Err(()),
            }),
        );

        engine
    }

    /// Parse the provided string as a series of Yarn nodes, appending the results to
    /// the internal node storage. Returns Ok if parsing succeeded, Err otherwise.
    pub fn load_from_string(&mut self, s: &str) -> Result<(), ()> {
        let nodes = parse::parse_nodes_from_string(s)?;
        for node in nodes {
            self.state.nodes.0.insert(node.title.clone(), node);
        }
        Ok(())
    }

    /// Register a native function for use in Yarn expressions.
    pub fn register_function(
        &mut self,
        name: String,
        num_args: usize,
        callback: Box<FunctionCallback>,
    ) {
        self.engine_state.functions.insert(
            name,
            Function {
                num_args,
                callback: SendWrapper::new(callback),
            },
        );
    }
    /// Set a given variable to the provided value. Any Yarn expressions evaluated
    /// after this call will observe the new value when using the variable.
    pub fn set_variable(&mut self, name: VariableName, value: Value) {
        self.engine_state.variables.set(name, value);
    }

    /// Begin evaluating the provided Yarn node.
    pub fn activate(&mut self, node: NodeName) {
        self.state.conversation = Some(Conversation::new(node));
        self.conversion_ended = false;
    }

    /// Make a choice between a series of options for the current Yarn node's active step.
    /// Execution will resume immediately based on the choice provided.
    pub fn choose(&mut self, choice: usize) -> Result<(), ()> {
        let step = self.state.get_current_step();
        match step {
            Some(Step::Dialogue(_, ref choices)) => match choices[choice].kind {
                ChoiceKind::External(ref node) => {
                    let node = node.clone();
                    self.state.set_conversation(Some(node));
                    Ok(())
                }
                ChoiceKind::Inline(..) => {
                    self.state.push_step(StepIndex::Dialogue(choice, 0));
                    Ok(())
                }
            },
            None => Ok(()),
            Some(Step::Command(..))
            | Some(Step::Assign(..))
            | Some(Step::Conditional(..))
            | Some(Step::Jump(..)) => unreachable!(),
        }
    }
}

/// A handler for Yarn actions that require integration with the embedder.
/// Invoked synchronously during Yarn execution when matching steps are
/// evaluated.
// pub trait YarnHandler {
//     type Data;

//     fn say(&mut self, text: String, data: Option<&mut Self::Data>);

//     fn choose(&mut self, text: String, choices: Vec<String>, data: Option<&mut Self::Data>);

//     fn command(&mut self, action: String, data: Option<&mut Self::Data>) -> Result<(), ()>;

//     fn end_conversation(&mut self, data: Option<&mut Self::Data>);
// }

#[derive(Eq, PartialEq, Debug)]
pub enum YarnEntry {
    /// Present a line of dialogue without any choices. Execution will not
    /// resume until `YarnEngine::proceed` is invoked.
    Say(String),
    /// Present a line of dialogue with subsequent choices. Execution will not
    /// resume until `YarnEngine::choose` is invoked.
    Choose { text: String, choices: Vec<String> },
    /// Instruct the embedder to perform some kind of action. The given action
    /// string is passed unmodified from the node source.
    Command { action: String },
    /// End the current conversation. Execution will not resume until a new
    /// node is made active with `YarnEngine::activate`.
    EndConversation,
}

impl<'a> Iterator for YarnEngine {
    type Item = YarnEntry;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.state.conversation.is_none() {
                return None;
            }
            if self.conversion_ended {
                return None;
            }
            let step = self.state.get_current_step();
            if step.is_none() {
                self.conversion_ended = true;
                return Some(YarnEntry::EndConversation);
            }

            match step.unwrap() {
                Step::Dialogue(text, choices) => {
                    if choices.is_empty() {
                        let text = text.clone();
                        self.state.advance();
                        return Some(YarnEntry::Say(text));
                    } else {
                        return Some(YarnEntry::Choose {
                            text: text.clone(),
                            choices: choices.iter().map(|c| c.text.clone()).collect(),
                        });
                    }
                }
                Step::Command(command) => {
                    let command = command.clone();
                    self.state.advance();
                    return Some(YarnEntry::Command {
                        action: command,
                    });
                }
                Step::Assign(name, expr) => {
                    let value = self.engine_state.evaluate(expr, &self.state.nodes).unwrap();
                    self.engine_state.variables.set((*name).clone(), value);
                    self.state.advance();
                }
                Step::Jump(name) => {
                    let name = name.clone();
                    self.state.set_conversation(Some(name));
                }
                Step::Conditional(expr, _if_steps, else_ifs, _else_steps) => {
                    let value = self.engine_state.evaluate(expr, &self.state.nodes).unwrap();
                    if value.as_bool() {
                        self.state.push_step(StepIndex::If(0));
                    } else {
                        let mut matched = false;
                        for (else_if_index, else_ifs) in else_ifs.iter().enumerate() {
                            let value = self
                                .engine_state
                                .evaluate(&else_ifs.0, &self.state.nodes)
                                .unwrap();
                            if value.as_bool() {
                                self.state.push_step(StepIndex::ElseIf(else_if_index, 0));
                                matched = true;
                                break;
                            }
                        }
                        if !matched {
                            self.state.push_step(StepIndex::Else(0));
                        }
                    }
                }
            }
        }
    }
}
