---
name: guiraffe-ui-implementation
description: Implement or refactor a GUIraffe desktop interface in Rust using GPUI, shared theme and layout context, Controls, and interaction tests.
---

# GUIraffe UI Implementation

Read the repository instructions, the spec overview, then the assigned crate spec. Consult the [GUIraffe recommendations](../../../docs/ui-ux/guiraffe-recommendations.md) for the affected area.

1. Preserve the pinned workspace Kit version. Reuse Controls and the current GUIraffe theme and focus layers. Put outer spacing in the parent and intrinsic control padding in the widget.
2. Verify behavior through the real entry point. Keep resolution logic GPUI-free where practical. Test focus, dispatch and recovery in TestAppContext, and capture representative theme and density states when appearance changes.
3. Use the crate gates and report the revision, commands, exercised backends and remaining native-platform checks. Coordinate cross-crate changes through the foreman.

Deliver the scoped implementation, acceptance evidence and any remaining verification gaps.
