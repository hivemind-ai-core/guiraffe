//! The checks: the findings an agent gets about a screen.
//!
//! Every check answers with structured findings, so an agent can tell which
//! node is wrong and why, and a test can fail when a defect is introduced.

use guiraffe_core::GuiraffeTheme;
use serde::{Deserialize, Serialize};

use crate::drive::{Action, Driver};
use crate::inspect::{A11yReport, NodeFacts, NodeId, NodeIndex};

/// How bad a finding is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// The screen is wrong for a person using it.
    Error,
    /// The screen is off, but nobody is blocked.
    Warning,
}

/// One finding of a check.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    /// The name of the check that found it.
    pub check: String,
    /// The node it is about.
    pub node: Option<NodeId>,
    /// How bad it is.
    pub severity: Severity,
    /// What is wrong.
    pub message: String,
}

/// The findings of a run of the checks.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CheckReport {
    /// The findings.
    pub findings: Vec<Finding>,
}

impl CheckReport {
    /// The findings of one check.
    pub fn of(&self, check: &str) -> Vec<&Finding> {
        self.findings
            .iter()
            .filter(|finding| finding.check == check)
            .collect()
    }

    /// Whether a finding is an error.
    pub fn has_errors(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.severity == Severity::Error)
    }
}

/// The smallest target a pointer can hit (WCAG 2.2 target size).
pub const MIN_HIT_TARGET: f32 = 24.;

/// The roles a pointer can act on.
fn is_interactive(role: Option<&str>) -> bool {
    matches!(
        role,
        Some(
            "Button"
                | "Link"
                | "Checkbox"
                | "RadioButton"
                | "Switch"
                | "TextBox"
                | "MenuItem"
                | "Slider"
                | "Tab"
                | "ToggleButton"
        )
    )
}

/// Runs the checks over a running app.
///
/// The checks that need the app (the layout jumps on focus and hover) drive
/// it; the rest read the frame.
pub fn run<T: crate::inspect::InspectTarget>(
    driver: &mut Driver<'_, T>,
    theme: &GuiraffeTheme,
) -> CheckReport {
    let index = driver.nodes();
    let a11y = driver.a11y();
    let mut findings = Vec::new();
    findings.extend(clipped_text(&index));
    findings.extend(hit_targets(&index));
    findings.extend(contrast(theme));
    findings.extend(unlabeled(&index));
    findings.extend(focus_reachable(&index, &a11y));
    findings.extend(focus_traps(&a11y));
    findings.extend(tab_order(&a11y));
    findings.extend(layout_jumps(driver));
    CheckReport { findings }
}

/// The nodes whose text does not fit their box.
fn clipped_text(index: &NodeIndex) -> Vec<Finding> {
    index
        .nodes()
        .iter()
        .filter(|node| node.layout.truncated || node.layout.clipped)
        .map(|node| Finding {
            check: "clipped-text".into(),
            node: Some(node.id.clone()),
            severity: Severity::Error,
            message: format!(
                "the text of the node does not fit its box ({:?})",
                node.layout.bounds
            ),
        })
        .collect()
}

/// The interactive nodes a pointer cannot hit.
fn hit_targets(index: &NodeIndex) -> Vec<Finding> {
    index
        .nodes()
        .iter()
        .filter(|node| is_interactive(node.a11y.role.as_deref()))
        .filter(|node| {
            node.layout.bounds.width < MIN_HIT_TARGET || node.layout.bounds.height < MIN_HIT_TARGET
        })
        .map(|node| Finding {
            check: "hit-target".into(),
            node: Some(node.id.clone()),
            severity: Severity::Error,
            message: format!(
                "the target is {}x{} pixels, smaller than {MIN_HIT_TARGET}x{MIN_HIT_TARGET}",
                node.layout.bounds.width, node.layout.bounds.height
            ),
        })
        .collect()
}

/// The colours of the theme that do not reach the floor.
///
/// The boundary of the default theme is exempt in the standard contrast, the
/// same exemption the contrast report of the theme documents.
fn contrast(theme: &GuiraffeTheme) -> Vec<Finding> {
    guiraffe_core::state::contrast_checks(theme)
        .into_iter()
        .filter(|check| check.ratio < check.floor)
        .filter(|check| {
            !(theme.contrast == guiraffe_core::Contrast::Standard
                && check.label == "Default boundary"
                && check.floor <= 3.0)
        })
        .map(|check| Finding {
            check: "contrast".into(),
            node: None,
            severity: Severity::Error,
            message: format!(
                "{} is {:.2}:1, below the floor of {:.1}:1",
                check.label, check.ratio, check.floor
            ),
        })
        .collect()
}

/// The interactive nodes without a name.
fn unlabeled(index: &NodeIndex) -> Vec<Finding> {
    index
        .nodes()
        .iter()
        .filter(|node| is_interactive(node.a11y.role.as_deref()))
        .filter(|node| {
            node.a11y
                .name
                .as_deref()
                .is_none_or(|name| name.trim().is_empty())
        })
        .map(|node| Finding {
            check: "unlabeled".into(),
            node: Some(node.id.clone()),
            severity: Severity::Error,
            message: format!(
                "the {} is not named, so a screen reader announces nothing",
                node.a11y.role.as_deref().unwrap_or("node")
            ),
        })
        .collect()
}

/// The focusable nodes the Tab key never reaches, and the traps in between.
fn focus_reachable(index: &NodeIndex, a11y: &A11yReport) -> Vec<Finding> {
    let focusable: Vec<&NodeFacts> = index
        .nodes()
        .iter()
        .filter(|node| node.a11y.focusable)
        .collect();
    let mut findings = Vec::new();
    for node in &focusable {
        if !a11y.focus_order.contains(&node.id) {
            findings.push(Finding {
                check: "unreachable-focusable".into(),
                node: Some(node.id.clone()),
                severity: Severity::Error,
                message: "the Tab key never reaches the node".into(),
            });
        }
    }
    // A cycle is a node the Tab order visits twice.
    let mut seen = Vec::new();
    for id in &a11y.focus_order {
        if seen.contains(id) {
            findings.push(Finding {
                check: "focus-cycle".into(),
                node: Some(id.clone()),
                severity: Severity::Error,
                message: "the Tab order visits the node twice".into(),
            });
        } else {
            seen.push(id.clone());
        }
    }
    findings
}

/// The nodes the Tab key cannot leave.
///
/// A node that keeps the focus while other focusable nodes are still to be
/// visited is a trap: the keyboard cannot get out of it.
fn focus_traps(a11y: &A11yReport) -> Vec<Finding> {
    let focusable = a11y.nodes.iter().filter(|node| node.a11y.focusable).count();
    let mut visited: Vec<&NodeId> = Vec::new();
    let mut findings = Vec::new();
    for id in &a11y.tab_walk {
        if visited.contains(&id) {
            if visited.len() < focusable
                && !findings
                    .iter()
                    .any(|finding: &Finding| finding.node.as_ref() == Some(id))
            {
                findings.push(Finding {
                    check: "focus-trap".into(),
                    node: Some(id.clone()),
                    severity: Severity::Error,
                    message: "the Tab key cannot leave the node".into(),
                });
            }
            continue;
        }
        visited.push(id);
    }
    findings
}

/// The Tab order against the order the nodes are seen in.
fn tab_order(a11y: &A11yReport) -> Vec<Finding> {
    let mut visual: Vec<&NodeFacts> = a11y.nodes.iter().collect();
    visual.sort_by(|a, b| {
        a.layout
            .bounds
            .y
            .partial_cmp(&b.layout.bounds.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.layout
                    .bounds
                    .x
                    .partial_cmp(&b.layout.bounds.x)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    let visual: Vec<NodeId> = visual
        .iter()
        .filter(|node| node.a11y.focusable)
        .map(|node| node.id.clone())
        .collect();
    let mut findings = Vec::new();
    if visual.len() == a11y.focus_order.len() && visual != a11y.focus_order {
        findings.push(Finding {
            check: "tab-order".into(),
            node: None,
            severity: Severity::Warning,
            message: format!(
                "the Tab order {:?} is not the order the nodes are seen in {:?}",
                a11y.focus_order, visual
            ),
        });
    }
    findings
}

/// The nodes that move when the pointer or the focus touches them.
fn layout_jumps<T: crate::inspect::InspectTarget>(driver: &mut Driver<'_, T>) -> Vec<Finding> {
    let index = driver.nodes();
    let before: Vec<(NodeId, crate::inspect::Rect)> = index
        .nodes()
        .iter()
        .map(|node| (node.id.clone(), node.layout.bounds))
        .collect();
    let mut findings = Vec::new();
    for node in index.nodes() {
        if !node.a11y.focusable && !is_interactive(node.a11y.role.as_deref()) {
            continue;
        }
        driver.drive(&Action::Hover {
            id: node.id.clone(),
        });
        let hovered = driver.nodes();
        for (id, bounds) in &before {
            let Some(node) = hovered.node(id) else {
                continue;
            };
            if node.layout.bounds != *bounds {
                findings.push(Finding {
                    check: "layout-jump".into(),
                    node: Some(id.clone()),
                    severity: Severity::Error,
                    message: format!(
                        "the node moves when it is hovered (from {bounds:?} to {:?})",
                        node.layout.bounds
                    ),
                });
            }
        }
    }
    findings
}
