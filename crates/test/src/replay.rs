//! The replay (spec/21 §2): a recorded interaction script the agent can run
//! again after a fix.
//!
//! A script is a list of actions with the assertions that must hold after
//! each of them, so the script is both the recording and the test.

use serde::{Deserialize, Serialize};

use crate::drive::{Action, Driver};
use crate::inspect::NodeId;

/// An assertion about the state after a step.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "assert", rename_all = "snake_case")]
pub enum Assertion {
    /// The node has these bounds.
    Bounds {
        id: NodeId,
        bounds: crate::inspect::Rect,
    },
    /// The node has the keyboard focus.
    Focus { id: NodeId },
    /// The app emitted these events, in this order.
    Events { names: Vec<String> },
    /// The app emitted no events.
    NoEvents,
    /// The node is there.
    Present { id: NodeId },
    /// The node is not there.
    Absent { id: NodeId },
}

/// One step of a script: an action and what must hold after it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// The action.
    pub action: Action,
    /// The assertions after the action.
    #[serde(default)]
    pub assert: Vec<Assertion>,
}

/// A recorded interaction script.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Script {
    /// The name of the script.
    #[serde(default)]
    pub name: String,
    /// The steps.
    pub steps: Vec<Step>,
}

impl Script {
    /// A script with a name and steps.
    pub fn new(name: impl Into<String>, steps: Vec<Step>) -> Self {
        Self {
            name: name.into(),
            steps,
        }
    }

    /// The script of a JSON document.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// The script as a JSON document.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// What one step of a script did.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StepResult {
    /// The name of the action.
    pub action: String,
    /// Whether the action was performed.
    pub ok: bool,
    /// The failures of the assertions.
    pub failures: Vec<String>,
}

/// What a run of a script did.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ReplayResult {
    /// The name of the script.
    pub name: String,
    /// The steps.
    pub steps: Vec<StepResult>,
    /// The failures of the whole run.
    pub failures: Vec<String>,
}

impl ReplayResult {
    /// Whether every step did what the script says.
    pub fn ok(&self) -> bool {
        self.failures.is_empty() && self.steps.iter().all(|step| step.failures.is_empty())
    }
}

/// Runs a script against a running app.
pub fn run(driver: &mut Driver, script: &Script) -> ReplayResult {
    let mut result = ReplayResult {
        name: script.name.clone(),
        ..Default::default()
    };
    for (index, step) in script.steps.iter().enumerate() {
        let driven = driver.drive(&step.action);
        let mut step_result = StepResult {
            action: step.action.name().to_string(),
            ok: driven.ok,
            failures: Vec::new(),
        };
        if let Some(error) = driven.error.clone() {
            step_result.failures.push(error);
        }
        for assertion in &step.assert {
            if let Err(failure) = check(driver, assertion, &driven) {
                step_result.failures.push(failure);
            }
        }
        result.failures.extend(step_result.failures.iter().cloned());
        result.steps.push(step_result);
        let _ = index;
    }
    result
}

/// Checks one assertion.
fn check(
    driver: &mut Driver,
    assertion: &Assertion,
    driven: &crate::drive::DriveResult,
) -> Result<(), String> {
    match assertion {
        Assertion::Present { id } => {
            let index = driver.nodes();
            if index.node(id).is_some() {
                Ok(())
            } else {
                Err(format!("the node {id} is not there"))
            }
        }
        Assertion::Absent { id } => {
            let index = driver.nodes();
            if index.node(id).is_none() {
                Ok(())
            } else {
                Err(format!("the node {id} is there"))
            }
        }
        Assertion::Bounds { id, bounds } => {
            let index = driver.nodes();
            let node = index
                .node(id)
                .ok_or_else(|| format!("the node {id} is not there"))?;
            if node.layout.bounds == *bounds {
                Ok(())
            } else {
                Err(format!(
                    "the bounds of {id} are {:?}, not {bounds:?}",
                    node.layout.bounds
                ))
            }
        }
        Assertion::Focus { id } => {
            if driven.focus.after.as_ref() == Some(id) {
                Ok(())
            } else {
                Err(format!(
                    "the focus is on {:?}, not on {id}",
                    driven.focus.after
                ))
            }
        }
        Assertion::Events { names } => {
            let fired: Vec<&str> = driven
                .events
                .iter()
                .map(|event| event.name.as_str())
                .collect();
            let expected: Vec<&str> = names.iter().map(String::as_str).collect();
            if fired == expected {
                Ok(())
            } else {
                Err(format!("the events are {fired:?}, not {names:?}"))
            }
        }
        Assertion::NoEvents => {
            if driven.events.is_empty() {
                Ok(())
            } else {
                let fired: Vec<&str> = driven
                    .events
                    .iter()
                    .map(|event| event.name.as_str())
                    .collect();
                Err(format!("the app emitted {fired:?}"))
            }
        }
    }
}
