# UX research and evaluation

Frame research around a decision. Record the user segment, task and environment, the observed problem or untested assumption, and what finding would change the decision. Include expertise, access needs, input devices and real interruptions in recruitment. Use consent, collect only necessary data, and choose a retention period before recording anything.

| Question | Method | Evidence and limits |
|---|---|---|
| What work happens today? | Contextual observation, artifact walkthrough, interviews | Observe real examples and workarounds. Treat recalled behavior as self-report. |
| How do people group and find things? | Open card sort followed by tree testing | Discover categories, then test findability in a proposed hierarchy. Confirm in the complete interface. |
| Can a new user find the next step? | Cognitive walkthrough and task-based moderated testing | Walkthrough identifies candidate problems. Observe real users to establish where problems occur. |
| Can users complete and recover from the task? | Representative prototype or live usability sessions | Observe unaided behavior, errors, help and recovery. Choose fidelity that supports the question. |
| How common is a problem? | Defined telemetry, survey or quantitative benchmark | Document denominators, missing data, recruitment and uncertainty. Use observation to investigate causes. |
| Did the change improve outcomes? | Comparable benchmark or controlled experiment | Predefine metrics and guardrails. Randomize when feasible, plan sample size, and account for learning or novelty. |

## Run a study

1. Recruit for the decision, including people using relevant assistive technology. Use iterative qualitative rounds to uncover problems and quantitative sampling when estimating rates.
2. Write a neutral scenario with a starting state, realistic data, a goal and a success condition. For example, “Send a copy of this project to a colleague who only needs to read it.” Avoid exposing exact control labels.
3. Pilot the script. Let the participant work without coaching. Record actions, outcomes and interventions. For neutral benchmark timing, avoid adding a concurrent think-aloud burden.
4. Synthesize by task and evidence. Record observation, impact, frequency within the study, plausible cause, and proposed change. Rank blocked completion and risk of lost work before repeated friction or visual polish.
5. Retest the affected journey and compare against the baseline using equivalent tasks and conditions.

## Measure outcomes

Define a task goal, an observable signal and a metric. For exports, measure unaided correct export among valid attempts, elapsed time for completed attempts, rates of retry and duplicate export, and a post-task ease rating. Report abandonment and completion time separately. Segment by expertise and access needs only when counts support interpretation. Record elapsed time from a defined start to visible completion, and report a tail percentile only with adequate observations. Set acceptance targets from the baseline and expected variation. Include guardrails for data loss, assistive-technology regressions and input latency.

Sources: [GOV.UK Service Manual user research](https://www.gov.uk/service-manual/user-research), [Google HEART paper](https://research.google/pubs/measuring-the-user-experience-on-a-large-scale-user-centered-metrics-for-web-applications/).

## Move from evidence to a design decision

- **Discover.** Observe the existing journey from the trigger through handoffs and downstream work. A journey map should show user goal, action, dependencies, friction and the evidence behind each step. If different groups follow materially different paths, map them separately.
- **Define.** Write the problem as an outcome that people struggle to achieve under specific conditions. For example, “Editors need to send a readable copy while keeping their working project editable.” Check assumptions with participants and people who support the workflow.
- **Develop.** Try competing layouts or interaction models for the riskiest decision. Use a sketch for vocabulary and grouping, a clickable prototype for navigation, and a working slice for focus, real latency and data recovery.
- **Deliver.** Test before rollout and inspect the same outcomes after release. Treat repeated friction or new failure modes as inputs to the next research round.

Map navigation to the mental model revealed by observed tasks. Start with user vocabulary, run a card sort where categories are uncertain, and tree-test competing labels with realistic retrieval goals. Observe the first branch chosen, destination, backtracking and requests for help. Then test the full UI, where search, visual prominence and past experience can change behavior. [Design Council Framework for Innovation](https://www.designcouncil.org.uk/resources/framework-for-innovation) · [GOV.UK experience mapping](https://www.gov.uk/service-manual/user-research/creating-an-experience-map) · [NN/G card sorting and tree testing](https://www.nngroup.com/articles/card-sorting-tree-testing-differences/).

## Reusable session record

| Field | Record |
|---|---|
| Setup | Participant code, relevant experience or access needs, build, device and task |
| Behavior | Starting state, action sequence, hesitations, errors and any moderator help |
| Outcome | Unaided, assisted, abandoned or incorrect completion, with relevant time and recovery |
| Interpretation | Likely cause, alternate explanation, strength of evidence and proposed change |
| Follow-up | Design owner, affected workflow and a task that can confirm the change |
