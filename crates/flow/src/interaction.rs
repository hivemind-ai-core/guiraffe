//! Pointer state machine, independent of GPUI and directly testable.
use guiraffe_flow_core::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
#[derive(Clone, Debug)]
pub enum Gesture {
    Pan {
        start: Point,
        origin: Point,
    },
    Move {
        start: Point,
        origins: Arc<BTreeMap<NodeId, Point>>,
        roots: Arc<BTreeMap<NodeId, Point>>,
        delta: Point,
    },
    Box {
        start: Point,
        end: Point,
        additive: bool,
    },
    Connect {
        source: Endpoint,
        at: Point,
    },
}
impl Gesture {
    pub(crate) fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Pan {
                    start: a,
                    origin: b,
                },
                Self::Pan {
                    start: c,
                    origin: d,
                },
            ) => a == c && b == d,
            (
                Self::Move {
                    start: a,
                    origins: b,
                    delta: c,
                    ..
                },
                Self::Move {
                    start: d,
                    origins: e,
                    delta: f,
                    ..
                },
            ) => a == d && Arc::ptr_eq(b, e) && c == f,
            (
                Self::Box {
                    start: a,
                    end: b,
                    additive: c,
                },
                Self::Box {
                    start: d,
                    end: e,
                    additive: f,
                },
            ) => a == d && b == e && c == f,
            (Self::Connect { source: a, at: b }, Self::Connect { source: c, at: d }) => {
                a == c && b == d
            }
            _ => false,
        }
    }
    pub(crate) fn preview(&self, id: NodeId, _graph: &Graph, grid: Option<f32>) -> Option<Point> {
        if let Self::Move {
            origins,
            roots,
            delta,
            ..
        } = self
        {
            let origin = *origins.get(&id)?;
            if *delta == Point::default() {
                return Some(origin);
            }
            let root_origin = *roots.get(&id)?;
            let target = grid.map_or(root_origin + *delta, |g| (root_origin + *delta).snap(g));
            Some(origin + (target - root_origin))
        } else {
            None
        }
    }
}
pub struct Interaction {
    pub editor: Editor,
    viewport: Viewport,
    pub gesture: Option<Gesture>,
    pub grid: Option<f32>,
    scratch: Vec<NodeId>,
}
impl Interaction {
    pub fn new(graph: Graph) -> Result<Self, FlowError> {
        Ok(Self::from_editor(Editor::new(graph)?))
    }
    pub(crate) fn from_editor(editor: Editor) -> Self {
        Self {
            editor,
            viewport: Viewport::default(),
            gesture: None,
            grid: Some(20.),
            scratch: Vec::new(),
        }
    }
    pub fn cancel(&mut self) {
        self.gesture = None;
    }
    pub fn viewport(&self) -> Viewport {
        self.viewport
    }
    pub fn set_viewport(&mut self, mut viewport: Viewport) -> Result<(), FlowError> {
        if !viewport.pan.finite() || !viewport.zoom.is_finite() || viewport.zoom <= 0. {
            return Err(FlowError::InvalidGeometry);
        }
        viewport.zoom = viewport.zoom.clamp(Viewport::MIN_ZOOM, Viewport::MAX_ZOOM);
        self.cancel();
        self.viewport = viewport;
        Ok(())
    }
    pub fn pan_to(&mut self, pan: Point) {
        if pan.finite() {
            self.viewport.pan = pan;
        }
    }
    pub fn zoom_at(&mut self, at: Point, factor: f32) {
        self.cancel();
        self.viewport.zoom_at(at, factor);
    }
    pub fn fit(&mut self, size: Point) {
        self.cancel();
        if let Some(bounds) = self.editor.scene().bounds() {
            self.viewport.fit(bounds, size, 40.);
        }
    }
    pub fn delete_selected(&mut self) -> Result<bool, FlowError> {
        self.cancel();
        self.editor.delete_selected()
    }
    pub fn preview(&self, id: NodeId) -> Option<Point> {
        self.gesture
            .as_ref()
            .and_then(|g| g.preview(id, self.editor.graph(), self.grid))
    }
    pub fn handle_at(&mut self, at: Point, kind: HandleKind) -> Option<Endpoint> {
        let radius = 9. / self.viewport.zoom;
        self.editor.scene().query_nodes(
            Rect {
                origin: at,
                size: Point::default(),
            }
            .expand(radius),
            &mut self.scratch,
        );
        self.scratch.sort_by_key(|id| self.editor.z_key(*id));
        for id in self.scratch.iter().rev() {
            let Some(node) = self.editor.graph().nodes.get(id) else {
                continue;
            };
            let rect = Rect {
                origin: self.preview(*id).unwrap_or(node.pos),
                size: node.size,
            };
            if let Some(h) = node
                .handles
                .iter()
                .find(|h| h.kind == kind && h.position(rect).distance(at) <= radius)
            {
                return Some(Endpoint::new(*id, h.id.clone()));
            }
        }
        None
    }
    pub fn press(&mut self, screen: Point, additive: bool, boxing: bool) {
        self.cancel();
        let world = self.viewport.world(screen);
        if let Some(source) = self.handle_at(world, HandleKind::Source) {
            self.gesture = Some(Gesture::Connect { source, at: world });
            return;
        }
        let hit = self.editor.hit_node(world, &mut self.scratch);
        if let Some(id) = hit {
            if additive || boxing {
                self.editor.toggle(id);
            } else if !self.editor.is_selected(id) {
                self.editor.select(&BTreeSet::from([id]), false);
            }
            let positions = self
                .editor
                .selected_nodes()
                .iter()
                .filter_map(|id| {
                    self.editor
                        .graph()
                        .nodes
                        .get(id)
                        .filter(|n| n.draggable)
                        .map(|n| (*id, n.pos))
                })
                .collect();
            let origins = self
                .editor
                .move_positions(&positions, None)
                .unwrap_or_default();
            let roots = origins
                .iter()
                .map(|(id, pos)| {
                    let mut root = *pos;
                    let mut parent = self.editor.graph().nodes.get(id).and_then(|n| n.parent);
                    while let Some(id) = parent {
                        if let Some(pos) = origins.get(&id) {
                            root = *pos;
                        }
                        parent = self.editor.graph().nodes.get(&id).and_then(|n| n.parent);
                    }
                    (*id, root)
                })
                .collect();
            self.gesture = Some(Gesture::Move {
                start: screen,
                origins: Arc::new(origins),
                roots: Arc::new(roots),
                delta: Point::default(),
            });
        } else if let Some(id) =
            self.editor
                .scene()
                .hit_edge(world, 6. / self.viewport.zoom, &mut self.scratch)
        {
            self.editor.select_edge(id, additive);
        } else if boxing {
            self.gesture = Some(Gesture::Box {
                start: world,
                end: world,
                additive,
            });
        } else {
            if !additive {
                self.editor.select(&BTreeSet::new(), false);
            }
            self.gesture = Some(Gesture::Pan {
                start: screen,
                origin: self.viewport.pan,
            });
        }
    }
    pub fn motion(&mut self, screen: Point) {
        if !screen.finite() {
            return;
        }
        let world = self.viewport.world(screen);
        match &mut self.gesture {
            Some(Gesture::Pan { start, origin }) => {
                let pan = *origin + screen - *start;
                if pan.finite() {
                    self.viewport.pan = pan;
                }
            }
            Some(Gesture::Move { start, delta, .. }) => {
                if *delta != Point::default() || screen.distance(*start) >= 3. {
                    *delta = (screen - *start) * (1. / self.viewport.zoom);
                }
            }
            Some(Gesture::Box { end, .. }) => *end = world,
            Some(Gesture::Connect { at, .. }) => *at = world,
            None => {}
        }
    }
    pub fn release(
        &mut self,
        validator: Option<&dyn ConnectionValidator>,
    ) -> Result<bool, FlowError> {
        match self.gesture.take() {
            Some(Gesture::Move { origins, delta, .. }) if delta != Point::default() => {
                let positions = origins.iter().map(|(id, p)| (*id, *p + delta)).collect();
                self.editor.move_nodes(&positions, self.grid)
            }
            Some(Gesture::Box {
                start,
                end,
                additive,
            }) => {
                if start.distance(end) > 3. / self.viewport.zoom {
                    self.editor.select_box(Rect::between(start, end), additive);
                }
                Ok(false)
            }
            Some(Gesture::Connect { source, at }) => {
                if let Some(target) = self.handle_at(at, HandleKind::Target) {
                    self.editor.connect_endpoints(source, target, validator)
                } else {
                    Ok(false)
                }
            }
            _ => Ok(false),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn interaction() -> Interaction {
        Interaction::new(
            Graph::from_parts(
                [
                    Node::new(1, "A", Point::default()),
                    Node::new(2, "B", Point::new(400., 0.)),
                ],
                [],
            )
            .expect("fixture"),
        )
        .expect("interaction")
    }
    #[test]
    fn drag_preview_cancel_and_one_history_entry() {
        let mut i = interaction();
        i.press(Point::new(30., 30.), false, false);
        i.motion(Point::new(76., 62.));
        assert_eq!(i.preview(1), Some(Point::new(40., 40.)));
        assert_eq!(i.editor.graph().nodes[&1].pos, Point::default());
        i.cancel();
        assert!(!i.editor.can_undo());
        i.press(Point::new(30., 30.), false, false);
        i.motion(Point::new(76., 62.));
        assert!(i.release(None).expect("release"));
        assert_eq!(i.editor.graph().nodes[&1].pos, Point::new(40., 40.));
        i.editor.undo().expect("undo");
        assert!(!i.editor.can_undo());
    }
    #[test]
    fn click_does_not_snap_or_create_history() {
        let mut i = interaction();
        i.press(Point::new(30., 30.), false, false);
        i.motion(Point::new(31., 31.));
        assert!(!i.release(None).expect("release"));
        assert!(!i.editor.can_undo());
    }
    #[test]
    fn zoomed_pan_and_group_drag() {
        let mut i = interaction();
        i.viewport.zoom = 2.;
        i.editor.select(&BTreeSet::from([1, 2]), false);
        i.press(Point::new(60., 60.), false, false);
        i.motion(Point::new(140., 60.));
        i.release(None).expect("release");
        assert_eq!(i.editor.graph().nodes[&1].pos.x, 40.);
        assert_eq!(i.editor.graph().nodes[&2].pos.x, 440.);
        i.press(Point::new(900., 900.), false, false);
        i.motion(Point::new(920., 910.));
        assert_eq!(i.viewport.pan, Point::new(20., 10.));
    }
    #[test]
    fn handle_drag_validates_and_connects() {
        let mut i = interaction();
        i.press(Point::new(160., 36.), false, false);
        assert!(matches!(i.gesture, Some(Gesture::Connect { .. })));
        i.motion(Point::new(400., 36.));
        assert!(i.release(None).expect("connect"));
        i.press(Point::new(160., 36.), false, false);
        i.motion(Point::new(400., 36.));
        assert_eq!(i.release(None), Err(FlowError::DuplicateConnection));
        assert_eq!(i.editor.graph().edges.len(), 1);
    }
    #[test]
    fn box_selection_and_additive_toggle() {
        let mut i = interaction();
        i.press(Point::new(-20., -20.), true, true);
        i.motion(Point::new(600., 100.));
        i.release(None).expect("box");
        assert!(i.editor.selected_nodes().len() == 2);
        i.press(Point::new(30., 30.), true, false);
        i.release(None).expect("toggle");
        assert!(!i.editor.is_selected(1));
        assert!(i.editor.is_selected(2));
    }
    #[test]
    fn review_5_off_grid_preview_matches_commit_and_click() {
        let mut i = interaction();
        i.editor
            .move_nodes(&BTreeMap::from([(1, Point::new(13., 17.))]), None)
            .expect("origin");
        i.press(Point::new(40., 40.), false, false);
        assert_eq!(i.preview(1), Some(Point::new(13., 17.)));
        i.motion(Point::new(87., 73.));
        let preview = i.preview(1).expect("preview");
        i.release(None).expect("release");
        assert_eq!(i.editor.graph().nodes[&1].pos, preview);
    }
    #[test]
    fn review_2_parent_preview_moves_descendants() {
        let mut i = interaction();
        i.editor
            .edit(|g| {
                g.nodes.get_mut(&2).expect("child").parent = Some(1);
                Ok(())
            })
            .expect("parent");
        i.press(Point::new(30., 30.), false, false);
        i.motion(Point::new(70., 50.));
        assert_eq!(i.preview(2), Some(Point::new(440., 20.)));
    }
    #[test]
    fn review_8_edge_click_selects_and_deletes() {
        let mut i = interaction();
        i.editor.connect(Edge::new(1, 1, 2), None).expect("edge");
        let at = i.editor.scene().paths()[&1].points[16];
        i.press(at, false, false);
        assert!(i.editor.delete_selected().expect("delete edge"));
        assert!(i.editor.graph().edges.is_empty());
        assert_eq!(i.editor.graph().nodes.len(), 2);
    }
    #[test]
    fn review_7_invalid_camera_cannot_be_installed() {
        let mut i = interaction();
        let old = i.viewport();
        for zoom in [0., f32::NAN, -1., f32::INFINITY] {
            assert!(i.set_viewport(Viewport { zoom, ..old }).is_err());
            assert_eq!(i.viewport(), old);
        }
        i.set_viewport(Viewport { zoom: 100., ..old })
            .expect("clamp");
        assert_eq!(i.viewport().zoom, Viewport::MAX_ZOOM);
    }
    #[test]
    fn review_16_allow_self_connects_via_handles() {
        let mut i = interaction();
        i.editor.policy.allow_self = true;
        i.press(Point::new(160., 36.), false, false);
        i.motion(Point::new(0., 36.));
        assert!(i.release(None).expect("self connect"));
        assert_eq!(i.editor.graph().edges[&1].target.node, 1);
    }
    #[test]
    fn review2_11_box_replace_and_additive_modes() {
        let mut i = interaction();
        i.editor.select(&BTreeSet::from([2]), false);
        i.press(Point::new(-20., -20.), false, true);
        i.motion(Point::new(180., 100.));
        i.release(None).expect("box");
        assert_eq!(i.editor.selected_nodes(), &BTreeSet::from([1]));
        i.press(Point::new(380., -20.), true, true);
        i.motion(Point::new(580., 100.));
        i.release(None).expect("add box");
        assert_eq!(i.editor.selected_nodes(), &BTreeSet::from([1, 2]));
    }
}
