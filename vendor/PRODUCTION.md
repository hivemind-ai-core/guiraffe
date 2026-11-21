# GPUI rendering storage bounds

| Storage | Bound and lifetime |
| --- | --- |
| Tessellated path vertices | One owned buffer per distinct tessellation or changed clone. Reference counts release buffers when the existing scene, path or layer owners release them. The patch adds no vertex cache. |
| Recent element IDs in `gpui-pre` | At most 64 paths per window. Each entry retains one observed stack of at most 32 element IDs. An entry is replaced on a collision and all entries drop with the window. |
| Default font features in `gpui-pre` | One empty vector for the process. |
| Dispatch replay stack in `gpui-pre` | 32 entries inline. Deeper nesting can spill for that replay and frees the buffer when replay returns. |
| Arena validity token | One token owned by the arena. Escaped boxes keep their own invalidated token until dropped, as before. |

The `cached_paths` benchmark integration test counts allocations during CPU
draw and arena clear after warming both frames. It verifies that the cached
canvas stays unpainted and all 1,000 paths remain in the rendered scene.
GPU submission, deep dispatch trees, new tessellations and focusable tab-stop
replay are outside this test.
