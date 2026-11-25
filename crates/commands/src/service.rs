use crate::{Command, CommandId, ContextStack, Error, Keybinding, Platform, model};
use gpui_kit::{App, AppContext, BorrowAppContext, Entity, Global, KeyBinding, Window};
use guiraffe_core::keys;
use guiraffe_core::protocol::{Action, Source};
use std::{cell::OnceCell, collections::HashMap, rc::Rc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvocationSource {
    Keyboard,
    Menu,
    Spotlight,
    Programmatic,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload {
    pub id: CommandId,
    pub source: InvocationSource,
}
impl From<InvocationSource> for Source {
    fn from(source: InvocationSource) -> Self {
        match source {
            InvocationSource::Keyboard => Source::Shortcut,
            InvocationSource::Spotlight => Source::Spotlight,
            InvocationSource::Menu => Source::Pointer,
            InvocationSource::Programmatic => Source::Host,
        }
    }
}
type Run = dyn Fn(&Payload, &mut Window, &mut App);
/// What a command does: a closure, or a declarative "apply action X to control Y".
#[derive(Clone)]
pub struct Handler(HandlerKind);
#[derive(Clone)]
enum HandlerKind {
    Run(Rc<Run>),
    Apply(Rc<crate::control::ControlAction>),
}
impl Handler {
    pub fn new(f: impl Fn(&Payload, &mut Window, &mut App) + 'static) -> Self {
        Self(HandlerKind::Run(Rc::new(f)))
    }
    /// Apply `action` to the control at `path` on the window's action bus. The source is
    /// the invocation's: [`Source::Shortcut`] from the keymap, [`Source::Spotlight`] from
    /// the palette. No closure, so the binding is data (serialisable, inspectable).
    pub fn apply(path: impl Into<gpui_kit::SharedString>, action: Action) -> Self {
        Self(HandlerKind::Apply(Rc::new(crate::control::ControlAction {
            path: path.into(),
            action,
        })))
    }
    /// The declarative target, when this handler is one.
    pub fn control_action(&self) -> Option<&crate::control::ControlAction> {
        match &self.0 {
            HandlerKind::Apply(apply) => Some(apply),
            HandlerKind::Run(_) => None,
        }
    }
    fn invoke(&self, payload: &Payload, window: &mut Window, cx: &mut App) -> Result<(), Error> {
        match &self.0 {
            HandlerKind::Run(run) => {
                run(payload, window, cx);
                Ok(())
            }
            HandlerKind::Apply(apply) => apply.dispatch(payload.source.into(), window, cx),
        }
    }
}
impl PartialEq for Handler {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (HandlerKind::Run(a), HandlerKind::Run(b)) => Rc::ptr_eq(a, b),
            (HandlerKind::Apply(a), HandlerKind::Apply(b)) => a == b,
            _ => false,
        }
    }
}
impl Eq for Handler {}
/// Every conflict in the active keymap: same binding on several commands, and bindings
/// that are a chord prefix of another (the shorter one waits for a possible second key).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConflictReport {
    pub exact: Vec<model::Conflict>,
    pub prefixes: Vec<model::PrefixConflict>,
}
impl ConflictReport {
    pub fn is_empty(&self) -> bool {
        self.exact.is_empty() && self.prefixes.is_empty()
    }
}
#[derive(Clone)]
pub(crate) enum MenuRow {
    Heading(gpui_kit::SharedString),
    Item {
        id: CommandId,
        /// The button's element id, built once per row instead of per frame.
        element_id: gpui_kit::SharedString,
        title: gpui_kit::SharedString,
        hint: gpui_kit::SharedString,
        enabled: bool,
    },
}
/// Mutable service state lives in an entity, never in the configuration global.
pub struct RegistryState {
    catalog: model::Registry<Handler>,
    contexts: ContextStack,
    revision: u64,
    /// Menu projections, built lazily on first render after a change.
    rows: OnceCell<Rc<Vec<MenuRow>>>,
    flat_rows: OnceCell<Rc<Vec<MenuRow>>>,
    /// Recently run Spotlight commands, most recent first (kept across palette openings).
    spotlight_recents: Vec<gpui_kit::SharedString>,
}
impl RegistryState {
    pub fn catalog(&self) -> &model::Registry<Handler> {
        &self.catalog
    }
    pub fn contexts(&self) -> &ContextStack {
        &self.contexts
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    fn new(platform: Platform) -> Self {
        Self {
            catalog: model::Registry::with_platform(platform),
            contexts: ContextStack::default(),
            revision: 0,
            rows: OnceCell::new(),
            flat_rows: OnceCell::new(),
            spotlight_recents: Vec::new(),
        }
    }
    /// Command ids recently run from Spotlight, most recent first.
    pub fn spotlight_recents(&self) -> &[gpui_kit::SharedString] {
        &self.spotlight_recents
    }
    fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.rows = OnceCell::new();
        self.flat_rows = OnceCell::new();
    }
    /// Menu rows for the current catalog and host contexts; computed once per revision.
    pub(crate) fn rows(&self, grouped: bool) -> Rc<Vec<MenuRow>> {
        let cell = if grouped { &self.rows } else { &self.flat_rows };
        cell.get_or_init(|| Rc::new(self.build_rows(grouped)))
            .clone()
    }
    fn build_rows(&self, with_headings: bool) -> Vec<MenuRow> {
        let platform = self.catalog.platform();
        let mut grouped = Vec::new();
        let mut flat = Vec::new();
        let mut category = None;
        for command in self.catalog.visible(&self.contexts) {
            if category != Some(command.category.as_str()) {
                category = Some(&command.category);
                grouped.push(MenuRow::Heading(if command.category.is_empty() {
                    "General".into()
                } else {
                    command.category.clone().into()
                }));
            }
            let row = MenuRow::Item {
                element_id: command.id.as_str().to_owned().into(),
                id: command.id.clone(),
                title: command.title.clone().into(),
                hint: command
                    .keybinding
                    .as_ref()
                    .map(|b| b.label(platform))
                    .unwrap_or_default()
                    .into(),
                enabled: command.enabled,
            };
            grouped.push(row.clone());
            flat.push(row);
        }
        if with_headings { grouped } else { flat }
    }
}
#[derive(Clone)]
pub struct Commands {
    state: Entity<RegistryState>,
}
struct Configuration {
    /// Native route (platform-resolved canonical binding) -> number of commands, across
    /// every live service, bound to it. A route is removed from the GPUI keymap when its
    /// count drops to zero, so unmounted views leave no stale bindings behind.
    routes: HashMap<String, usize>,
    default: Commands,
}
impl Global for Configuration {}
gpui_kit::actions!(guiraffe_commands, [ShowCommands]);
#[derive(Clone, PartialEq, serde::Deserialize, gpui_kit::Action)]
#[action(namespace=guiraffe_commands,no_json)]
pub(crate) struct InvokeBinding {
    /// Platform-resolved canonical binding; an index key, never re-parsed per keystroke.
    pub binding: String,
}
/// The palette shortcut: ⌘⇧P on macOS, Ctrl+Shift+P elsewhere.
pub const PALETTE_SHORTCUT: &str = "secondary-shift-p";
/// The keyboard convention GPUI reports for this app (tests may emulate another OS).
pub fn platform(cx: &App) -> Platform {
    if cx.platform_info().os == gpui_kit::OperatingSystem::Mac {
        Platform::Mac
    } else {
        Platform::Other
    }
}
/// Register the `GrCommand` scope and its palette action once. Returns bindings GPUI
/// rejected (none for the built-in shortcut); dynamic shortcuts use the same fallible path.
pub fn init(cx: &mut App) -> Vec<guiraffe_core::keys::InvalidBinding> {
    if cx.has_global::<Configuration>() {
        return Vec::new();
    }
    guiraffe_overlays::init(cx);
    let default = Commands::empty(cx);
    cx.set_global(Configuration {
        routes: HashMap::new(),
        default,
    });
    crate::document::register(cx);
    let palette = keys::binding(PALETTE_SHORTCUT, ShowCommands, Some("GrCommand"), cx);
    keys::bind_keys(vec![palette], cx)
}
pub fn registry(cx: &App) -> Result<Commands, Error> {
    cx.try_global::<Configuration>()
        .map(|config| config.default.clone())
        .ok_or(Error::NotInitialized)
}
/// Parse a native route without installing it, so a bad binding fails before any write.
fn native_route(resolved: &Keybinding, cx: &App) -> Result<Option<KeyBinding>, Error> {
    let config = cx
        .try_global::<Configuration>()
        .ok_or(Error::NotInitialized)?;
    if config.routes.contains_key(resolved.as_str()) {
        return Ok(None);
    }
    let action = InvokeBinding {
        binding: resolved.as_str().to_owned(),
    };
    keys::binding(resolved.gpui_keystrokes(), action, Some("GrCommand"), cx)
        .map(Some)
        .map_err(|error| Error::NativeBinding(error.to_string()))
}
fn is_route(binding: &KeyBinding, resolved: &str) -> bool {
    binding
        .action()
        .as_any()
        .downcast_ref::<InvokeBinding>()
        .is_some_and(|route| route.binding == resolved)
}
/// Take one reference on a route, installing it on the first.
fn acquire(resolved: &Keybinding, native: Option<KeyBinding>, cx: &mut App) {
    let first = cx.update_global::<Configuration, _>(|config, _| {
        let count = config
            .routes
            .entry(resolved.as_str().to_owned())
            .or_insert(0);
        *count += 1;
        *count == 1
    });
    if first {
        // `native_route` built it when the route was absent; rebuild only if it raced.
        let native = native.or_else(|| {
            let action = InvokeBinding {
                binding: resolved.as_str().to_owned(),
            };
            keys::binding(resolved.gpui_keystrokes(), action, Some("GrCommand"), cx).ok()
        });
        let keymap = cx.key_bindings();
        let present = keymap
            .borrow()
            .bindings()
            .any(|b| is_route(b, resolved.as_str()));
        if !present && let Some(native) = native {
            cx.bind_keys([native]);
        }
    }
}
/// Drop one reference on a route; the last one removes it from the GPUI keymap while
/// keeping every other (application) binding and its precedence order.
fn release(resolved: &str, cx: &mut App) {
    let Some(config) = cx.try_global::<Configuration>() else {
        return;
    };
    if !config.routes.contains_key(resolved) {
        return;
    }
    let last = cx.update_global::<Configuration, _>(|config, _| {
        let Some(count) = config.routes.get_mut(resolved) else {
            return false;
        };
        *count = count.saturating_sub(1);
        if *count == 0 {
            config.routes.remove(resolved);
            true
        } else {
            false
        }
    });
    if last {
        let keymap = cx.key_bindings();
        let kept = keymap
            .borrow()
            .bindings()
            .filter(|b| !is_route(b, resolved))
            .cloned()
            .collect::<Vec<_>>();
        cx.clear_key_bindings();
        cx.bind_keys(kept);
    }
}
fn resolved(binding: &Option<Keybinding>, platform: Platform) -> Option<Keybinding> {
    binding.as_ref().map(|b| b.resolve(platform))
}
impl Commands {
    fn empty(cx: &mut App) -> Self {
        let platform = platform(cx);
        let state = cx.new(|_| RegistryState::new(platform));
        // A dropped service (an unmounted view's registry) releases all of its routes.
        cx.observe_release(&state, |state: &mut RegistryState, cx| {
            let held = state
                .catalog
                .ordered()
                .filter_map(|c| resolved(&c.keybinding, state.catalog.platform()))
                .collect::<Vec<_>>();
            for binding in held {
                release(binding.as_str(), cx);
            }
        })
        .detach();
        Self { state }
    }
    /// Prefer a distinct service per window when application contexts differ.
    pub fn new(cx: &mut App) -> Self {
        init(cx);
        Self::empty(cx)
    }
    pub fn entity(&self) -> &Entity<RegistryState> {
        &self.state
    }
    /// The keyboard convention this service resolves `secondary` for.
    pub fn platform(&self, cx: &App) -> Platform {
        self.state.read(cx).catalog.platform()
    }
    /// Swap the route held for `old` to `new` (both platform-resolved), validating `new`
    /// first. Returns the parsed native binding to pass to `acquire`.
    fn prepare(&self, new: &Option<Keybinding>, cx: &App) -> Result<Option<KeyBinding>, Error> {
        match new {
            Some(binding) => native_route(binding, cx),
            None => Ok(None),
        }
    }
    fn swap_routes(
        old: Option<Keybinding>,
        new: Option<Keybinding>,
        native: Option<KeyBinding>,
        cx: &mut App,
    ) {
        if old == new {
            return;
        }
        if let Some(new) = &new {
            acquire(new, native, cx);
        }
        if let Some(old) = &old {
            release(old.as_str(), cx);
        }
    }
    pub fn register<F>(&self, command: Command<F>, cx: &mut App) -> Result<(), Error>
    where
        F: Fn(&Payload, &mut Window, &mut App) + 'static,
    {
        self.register_handler(command.map_run(Handler::new), cx)
    }
    /// Register a command whose handler is already built, such as [`Handler::apply`]
    /// or a [`Commands::control_command`].
    pub fn register_handler(&self, command: Command<Handler>, cx: &mut App) -> Result<(), Error> {
        command.validate()?;
        if self.state.read(cx).catalog.lookup(&command.id).is_some() {
            return Err(Error::Duplicate(command.id.to_string()));
        }
        let new = resolved(&command.keybinding, self.platform(cx));
        let native = self.prepare(&new, cx)?;
        self.state.update(cx, |state, _| {
            state.catalog.register(command)?;
            state.changed();
            Ok::<(), Error>(())
        })?;
        Self::swap_routes(None, new, native, cx);
        cx.notify(self.state.entity_id());
        Ok(())
    }
    pub fn unregister(&self, id: &CommandId, cx: &mut App) -> bool {
        let Some(old) = self
            .state
            .read(cx)
            .catalog
            .lookup(id)
            .map(|c| c.keybinding.clone())
        else {
            return false;
        };
        let old = resolved(&old, self.platform(cx));
        self.state.update(cx, |state, _| {
            state.catalog.unregister(id);
            state.changed();
        });
        Self::swap_routes(old, None, None, cx);
        cx.notify(self.state.entity_id());
        true
    }
    pub fn lookup(&self, id: &CommandId, cx: &App) -> Option<Command<Handler>> {
        self.state.read(cx).catalog.lookup(id).cloned()
    }
    /// Clone a stored command, edit metadata, then replace. Handler clones preserve
    /// identity, so replacing an unchanged descriptor performs no entity write.
    pub fn replace(&self, command: Command<Handler>, cx: &mut App) -> Result<bool, Error> {
        command.validate()?;
        let old = self
            .state
            .read(cx)
            .catalog
            .lookup(&command.id)
            .ok_or_else(|| Error::NotFound(command.id.to_string()))?;
        if old == &command {
            return Ok(false);
        }
        let platform = self.platform(cx);
        let old = resolved(&old.keybinding, platform);
        let new = resolved(&command.keybinding, platform);
        let native = self.prepare(&new, cx)?;
        self.state.update(cx, |state, _| {
            state.catalog.replace(command)?;
            state.changed();
            Ok::<(), Error>(())
        })?;
        Self::swap_routes(old, new, native, cx);
        cx.notify(self.state.entity_id());
        Ok(true)
    }
    pub fn set_enabled(&self, id: &CommandId, enabled: bool, cx: &mut App) -> Result<bool, Error> {
        let old = self
            .state
            .read(cx)
            .catalog
            .lookup(id)
            .ok_or_else(|| Error::NotFound(id.to_string()))?;
        if old.enabled == enabled {
            return Ok(false);
        }
        self.state.update(cx, |state, _| {
            state.catalog.set_enabled(id, enabled)?;
            state.changed();
            Ok::<(), Error>(())
        })?;
        cx.notify(self.state.entity_id());
        Ok(true)
    }
    pub fn set_keybinding(
        &self,
        id: &CommandId,
        binding: Option<Keybinding>,
        cx: &mut App,
    ) -> Result<bool, Error> {
        let old = self
            .state
            .read(cx)
            .catalog
            .lookup(id)
            .ok_or_else(|| Error::NotFound(id.to_string()))?;
        if old.keybinding == binding {
            return Ok(false);
        }
        let platform = self.platform(cx);
        let old = resolved(&old.keybinding, platform);
        let new = resolved(&binding, platform);
        let native = self.prepare(&new, cx)?;
        self.state.update(cx, |state, _| {
            state.catalog.set_keybinding(id, binding)?;
            state.changed();
            Ok::<(), Error>(())
        })?;
        Self::swap_routes(old, new, native, cx);
        cx.notify(self.state.entity_id());
        Ok(true)
    }
    pub fn set_contexts(&self, contexts: ContextStack, cx: &mut App) -> bool {
        if self.state.read(cx).contexts == contexts {
            return false;
        }
        self.state.update(cx, |state, _| {
            state.contexts = contexts;
            state.changed();
        });
        cx.notify(self.state.entity_id());
        true
    }
    /// Exact and chord-prefix conflicts among the commands active in `contexts`.
    pub fn conflict_report(&self, contexts: &ContextStack, cx: &App) -> ConflictReport {
        let bindings = self.state.read(cx).catalog.bindings(contexts);
        ConflictReport {
            exact: bindings.conflicts,
            prefixes: bindings.prefixes,
        }
    }
    pub fn binding_conflicts(&self, contexts: &ContextStack, cx: &App) -> Vec<model::Conflict> {
        self.state.read(cx).catalog.bindings(contexts).conflicts
    }
    pub fn execute(
        &self,
        id: &CommandId,
        contexts: &ContextStack,
        source: InvocationSource,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<(), Error> {
        // Release the registry borrow before calling user code. A handler may
        // register/unregister commands or invoke another command reentrantly.
        let handler = {
            let Some(command) = self.state.read(cx).catalog.lookup(id) else {
                // Every registered control action is a command without being registered:
                // `control.<path>.<action>` resolves on the bus only when invoked.
                return match crate::control::ControlAction::parse(id.as_str()) {
                    Some(apply) => apply.dispatch(source.into(), window, cx),
                    None => Err(Error::NotFound(id.to_string())),
                };
            };
            if !command.enabled {
                return Err(Error::Disabled(id.to_string()));
            }
            if !command.when.matches(contexts) {
                return Err(Error::Inactive(id.to_string()));
            }
            command.run.clone()
        };
        handler.invoke(
            &Payload {
                id: id.clone(),
                source,
            },
            window,
            cx,
        )
    }
    pub(crate) fn dispatch_binding(
        &self,
        binding: &str,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        // Fast path: a route this service has no command for costs one hash lookup.
        if self
            .state
            .read(cx)
            .catalog
            .bound_to_native(binding)
            .next()
            .is_none()
        {
            return false;
        }
        let contexts = self.window_contexts(window, cx);
        let id = self
            .state
            .read(cx)
            .catalog
            .resolve_native(binding, &contexts)
            .map(|command| command.id.clone());
        id.is_some_and(|id| {
            self.execute(&id, &contexts, InvocationSource::Keyboard, window, cx)
                .is_ok()
        })
    }
    /// Reuse the focused window's actual GPUI contexts, plus explicit host contexts.
    pub fn window_contexts(&self, window: &Window, cx: &App) -> ContextStack {
        let mut result = self.state.read(cx).contexts.clone();
        result.extend(&native_contexts(window));
        result
    }
    /// Projection for the existing overlays API; its id/label/keywords/disabled Command
    /// type is unchanged. A palette opened by `open_spotlight` re-projects on every
    /// registry change.
    pub fn spotlight_commands(
        &self,
        contexts: &ContextStack,
        cx: &App,
    ) -> Vec<guiraffe_overlays::search::Command> {
        self.state
            .read(cx)
            .catalog
            .visible(contexts)
            .map(|command| guiraffe_overlays::search::Command {
                id: command.id.to_string(),
                label: command.title.clone(),
                keywords: format!("{} {}", command.category, command.keywords),
                disabled: !command.enabled,
            })
            .collect()
    }
    /// Spotlight actions for the commands visible in `contexts`: the title, the category
    /// as the group, keywords, and the platform's shortcut label as keycap chips.
    pub fn spotlight_actions(
        &self,
        contexts: &ContextStack,
        cx: &App,
    ) -> Vec<guiraffe_overlays::SpotlightAction> {
        let state = self.state.read(cx);
        let platform = state.catalog.platform();
        state
            .catalog
            .visible(contexts)
            .map(|command| {
                let mut action = guiraffe_overlays::SpotlightAction::new(
                    command.id.to_string(),
                    command.title.clone(),
                )
                .keywords(command.keywords.clone())
                .disabled(!command.enabled);
                if !command.category.is_empty() {
                    action = action.group(command.category.clone());
                }
                if let Some(binding) = &command.keybinding {
                    action = action.shortcut(&binding.label(platform));
                }
                action
            })
            .collect()
    }
    /// Open the command palette over the focused window. Commands run through
    /// [`Commands::execute`] with the contexts focused when it opened, so a command that
    /// became disabled or inactive meanwhile does not run. The palette follows registry
    /// changes while open and remembers recently run commands.
    pub fn open_spotlight(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Entity<guiraffe_overlays::SpotlightState>, guiraffe_overlays::OverlayError> {
        let origin = native_contexts(window);
        let mut contexts = self.state.read(cx).contexts.clone();
        contexts.extend(&origin);
        // Control actions are enumerated only now, when the palette opens (no idle cost).
        let mut controls = crate::control::spotlight_actions(window, cx);
        // A control action already registered as a command (for its shortcut) is listed once.
        controls.retain(|action| {
            CommandId::parse(&action.id)
                .ok()
                .is_none_or(|id| self.state.read(cx).catalog.lookup(&id).is_none())
        });
        let controls = Rc::new(controls);
        let mut actions = self.spotlight_actions(&contexts, cx);
        actions.extend(controls.iter().cloned());
        let recents = self.state.read(cx).spotlight_recents.clone();
        let live_controls = controls.clone();
        let weak = self.state.downgrade();
        let recorded = self.state.downgrade();
        let observed = self.state.clone();
        let live_origin = origin.clone();
        let palette = cx.new(|cx| {
            // Keep an open palette in sync with the registry (add/remove/enable/rebind).
            cx.observe(
                &observed,
                move |palette: &mut guiraffe_overlays::SpotlightState, state, cx| {
                    let service = Commands { state };
                    let mut contexts = service.state.read(cx).contexts.clone();
                    contexts.extend(&live_origin);
                    let mut actions = service.spotlight_actions(&contexts, cx);
                    actions.extend(live_controls.iter().cloned());
                    palette.set_actions(actions, cx);
                },
            )
            .detach();
            // Remember recents on the registry (without notifying its observers).
            cx.subscribe_self(move |palette, event, cx| {
                if let guiraffe_overlays::SpotlightEvent::Activated(_) = event {
                    let recents = palette.recent_ids().to_vec();
                    if let Some(state) = recorded.upgrade() {
                        state.update(cx, |state, _| state.spotlight_recents = recents);
                    }
                }
            })
            .detach();
            guiraffe_overlays::SpotlightState::new(window, cx)
                .actions(actions)
                .recents(recents)
                .on_activate(move |value, window, cx| {
                    let Some(state) = weak.upgrade() else { return };
                    let Ok(id) = CommandId::parse(value) else {
                        // A control path that is not a valid slug still runs on the bus.
                        if let Some(apply) = crate::control::ControlAction::parse(value) {
                            let _ = apply.dispatch(Source::Spotlight, window, cx);
                        }
                        return;
                    };
                    let service = Commands { state };
                    let mut contexts = service.state.read(cx).contexts.clone();
                    contexts.extend(&origin);
                    let _ =
                        service.execute(&id, &contexts, InvocationSource::Spotlight, window, cx);
                })
        });
        guiraffe_overlays::SpotlightState::open(&palette, window, cx)?;
        Ok(palette)
    }
}
fn native_contexts(window: &Window) -> ContextStack {
    let mut result = ContextStack::default();
    for context in window.context_stack() {
        let mut frame = crate::WhenContext::default();
        for entry in context.primary().into_iter().chain(context.secondary()) {
            let _ = frame.insert(&entry.key, entry.value.as_ref().map(|value| value.as_ref()));
        }
        result.push(frame);
    }
    result
}
/// Convenience for the application-wide default service. Window-local instances
/// expose the same methods directly.
pub fn register<F>(command: Command<F>, cx: &mut App) -> Result<(), Error>
where
    F: Fn(&Payload, &mut Window, &mut App) + 'static,
{
    registry(cx)?.register(command, cx)
}
pub fn unregister(id: &CommandId, cx: &mut App) -> Result<bool, Error> {
    Ok(registry(cx)?.unregister(id, cx))
}
pub fn execute(id: &CommandId, window: &mut Window, cx: &mut App) -> Result<(), Error> {
    let service = registry(cx)?;
    let contexts = service.window_contexts(window, cx);
    service.execute(id, &contexts, InvocationSource::Programmatic, window, cx)
}
