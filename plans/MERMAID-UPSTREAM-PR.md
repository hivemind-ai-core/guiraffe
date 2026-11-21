# Index collision candidates in flowchart routing and cleanup

On a 500-node flowchart with tree edges and periodic cross links, sampling put
82.62% of the run in post-route cleanup. The crossing sweep alone accounted for
68.55%. It scans all nodes and all earlier segments for each path candidate.

Use a 96-unit uniform grid to find candidate obstacles during initial routing
and during the crossing and deoverlap sweeps. Apply the existing exact
predicates to those candidates. Preserve insertion order when adding overlap
lengths, since changing floating-point addition order can change route choices.
Keep near-collinear diagonal segments on the full-scan path because their
projected overlap can reach outside their axis-aligned bounds.

The index lives for a routing stage or sweep. An object is referenced by at most
128 grid cells, with longer objects held once in the fallback list. A bounded
cell traversal falls back to checking the full index for large query bounds.

The comparison harness runs the patched and released 0.3.1 renderers side by
side and compares full layouts, edge coordinate bits and SVG bytes. It covers
the packaged upstream fixtures, both GUIraffe showcase diagrams in light and
dark, and the 500-node tree with cross links. Grid tests cover local candidate
counts as distant data grows, deduplication and fallback order.

On the 500-node workload at font size 14, hardware counters fell from 587.5
billion to 54.5 billion retired user-space instructions. Allocator calls through
malloc, calloc and realloc fell from 831.4 million to 9.0 million. Peak process
RSS grew from roughly 14 MiB to 39 MiB in the standalone harness, which also
renders and serializes the returned layout. All 47 packaged fixtures, the
showcase cases and the 500-node case matched the released renderer.
