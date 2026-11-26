# guiraffe-geometry

GPUI-free geometry shared across GUIraffe crates, so headless models (such as
`guiraffe-flow-core`) and GPUI widgets agree on it. Today: `Point` and `AxisLock`,
the optional lock for drag gestures. Enable the `serde` feature to serialize them.
