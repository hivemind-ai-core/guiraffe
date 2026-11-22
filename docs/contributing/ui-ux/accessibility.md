# Accessible desktop interfaces

Use WCAG 2.2 as a reference for measurable criteria and map them to native software with WCAG2ICT. Desktop logical pixels and CSS pixels require an explicit mapping when borrowing target criteria. Test actual platform accessibility behavior before making a platform support claim.

| Concern | Design and test |
|---|---|
| Contrast | Use 4.5:1 for typical text and 3:1 for text meeting WCAG's large-text definition. Reference 3:1 for required non-text boundaries and state indicators against adjacent colors. Test compositing, dimmed surfaces and focus on selected rows. |
| Targets | Use WCAG 2.2 AA's 24 CSS pixel minimum or its spacing and other exceptions as reference points. Check actual hit regions and neighboring controls. Prefer generous hit regions for frequent or high-risk actions. |
| Focus | Verify forward and backward traversal, visible focus, no obscured target and a meaningful place to return after dismissal. In composite widgets, use the established arrow and Tab conventions for the chosen role. |
| Semantics | Expose accessible name, role, value and state. Test label association, expanded and selected states, and status announcements with assistive technology. Verify that icon-only actions admit a specific spoken name, such as “Remove Draft 2.” |
| Alternatives | Provide alternatives to dragging, color-only status, hover-only disclosure and pointer-only activation. Expose essential chart values or summaries in an accessible textual form. |
| Adaptation | Test text scale and high DPI, narrow windows, light/dark and high contrast, reduced motion, localization and right-to-left layout where supported. Verify content and actions under each condition. |

Run the task keyboard-only from startup through completion and recovery. Verify at least one screen-reader journey per supported platform when making support claims. Record platform, version, backend, input, theme and steps. Snapshots can catch visual regression. Interaction tests can establish focus order and event handling. Native assistive-technology testing establishes what users can actually perceive and operate.

Sources: [WCAG 2.2](https://www.w3.org/TR/WCAG22/), [WCAG2ICT](https://www.w3.org/TR/wcag2ict-22/), [WAI keyboard guidance](https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/).
