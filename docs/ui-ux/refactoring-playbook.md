# Refactoring a UI

1. Pick a single journey and define success. Capture an existing screenshot and interaction trace with realistic data, revision, theme, window size and backend. Record navigation, shortcuts, focus, validation, persistence and recovery that the refactor needs to preserve.
2. Fix flow first. Identify the decision and information needed at each step. Remove duplicate actions when evidence supports it, improve labels and grouping, and prototype the risky decision before styling the full screen.
3. Establish hierarchy with spacing, type, alignment and placement. Apply theme roles and default recipes. Walk through the state matrix and validate at narrow and wide widths with long content.
4. Implement in a small slice through the normal entry point. Preserve focus, stable identity and draft state. Add a regression test for a changed behavior or a specific risk that existing tests miss.
5. Review equivalent before and after captures and exercise the journey with keyboard and pointer, including error and recovery. For interaction or layout performance changes, compare the same workload and environment. Record what still needs platform or user validation.

## Example workflow

An export dialog uses three equally loud buttons and reports a write error by closing and showing a toast. Start from the goal of creating a usable copy. Group destination and output options, give “Export” visual priority, and keep “Cancel” in the expected platform position. On write failure, leave the dialog and settings intact, explain the failure beside the destination, and allow retry. Verify keyboard entry, the failure path, exactly one successful export, and focus restoration on close.

A review record should include task, revision, evidence, observed consequence, change, acceptance check and remaining uncertainty. Keep appearance findings tied to a concrete consequence such as ambiguous grouping or a hidden action.
