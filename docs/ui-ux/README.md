# UI and UX field guide

Start with a real task and representative data. Define the desired outcome, make the task easy to complete and recover, then refine the visual hierarchy. Apply the same sequence to a new screen or a refactor.

| Deliverable | Use it for |
|---|---|
| [Visual design](visual-design.md) | Hierarchy, typography, spacing, color, density and finishing |
| [Interaction patterns](interaction-patterns.md) | Navigation, editing, forms, feedback, data views and overlays |
| [Accessibility](accessibility.md) | Keyboard, assistive technology, perceptual checks and adaptation |
| [UX research and evaluation](ux-research.md) | Choosing methods, running studies and measuring outcomes |
| [Refactoring playbook](refactoring-playbook.md) | Taking one workflow from baseline to reviewed change |
| [GUIraffe recommendations](guiraffe-recommendations.md) | Framework backlog, acceptance evidence and rollout order |
| [Research notes](sources.md) | Provenance, scope and limits of the evidence |

The repository ships five role-specific agent skills under [`.agents/skills`](../../.agents/skills). Invoke the relevant role when a task crosses design and engineering: [research](../../.agents/skills/ux-research/SKILL.md), [design](../../.agents/skills/ui-design/SKILL.md), [product](../../.agents/skills/ux-product/SKILL.md), [implementation](../../.agents/skills/guiraffe-ui-implementation/SKILL.md), or [review](../../.agents/skills/ui-ux-review/SKILL.md).

Use repository specs and platform conventions for implementation decisions. Record deviations from these guidelines with the user need and the evidence behind the decision.
