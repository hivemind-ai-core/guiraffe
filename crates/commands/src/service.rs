use crate::{Command, CommandId, ContextStack, Error, Keybinding, model};
use gpui_kit::{
    App, AppContext, BorrowAppContext, Entity, Global, KeyBinding, KeyBindingContextPredicate,
    Window,
};
use std::{collections::HashSet, rc::Rc};

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
type Run = dyn Fn(&Payload, &mut Window, &mut App);
#[derive(Clone)]
pub struct Handler(Rc<Run>);
impl Handler {
    pub fn new(f: impl Fn(&Payload, &mut Window, &mut App) + 'static) -> Self {
        Self(Rc::new(f))
    }
}
impl PartialEq for Handler {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for Handler {}
#[derive(Clone)]
pub(crate) enum MenuRow {
    Heading(gpui_kit::SharedString),
    Item {
        id: CommandId,
        title: gpui_kit::SharedString,
        hint: gpui_kit::SharedString,
        enabled: bool,
    },
}
/// Mutable service state lives in an entity, never in the configuration global.
#[derive(Default)]
pub struct RegistryState {
    catalog: model::Registry<Handler>,
    contexts: ContextStack,
    revision: u64,
    pub(crate) rows: Rc<Vec<MenuRow>>,
    pub(crate) flat_rows: Rc<Vec<MenuRow>>,
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
    fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
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
                id: command.id.clone(),
                title: command.title.clone().into(),
                hint: command
                    .keybinding
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default()
                    .into(),
                enabled: command.enabled,
            };
            grouped.push(row.clone());
            flat.push(row);
        }
        self.rows = Rc::new(grouped);
        self.flat_rows = Rc::new(flat);
    }
}
#[derive(Clone)]
pub struct Commands {
    state: Entity<RegistryState>,
}
struct Configuration {
    installed: HashSet<Keybinding>,
    predicate: Rc<KeyBindingContextPredicate>,
    default: Commands,
}
impl Global for Configuration {}
gpui_kit::actions!(guiraffe_commands, [ShowCommands]);
#[derive(Clone, PartialEq, serde::Deserialize, gpui_kit::Action)]
#[action(namespace=guiraffe_commands,no_json)]
pub(crate) struct InvokeBinding {
    pub binding: String,
}
/// Register the `GrCommand` scope and its Ctrl+Shift+P palette action once.
/// Dynamic shortcuts use native `KeyBinding::load`, supporting chords without leaks.
pub fn init(cx: &mut App) {
    if cx.has_global::<Configuration>() {
        return;
    }
    guiraffe_overlays::init(cx);
    let default = Commands::empty(cx);
    cx.set_global(Configuration {
        installed: HashSet::new(),
        predicate: Rc::new(KeyBindingContextPredicate::Identifier("GrCommand".into())),
        default,
    });
    cx.bind_keys([KeyBinding::new(
        "ctrl-shift-p",
        ShowCommands,
        Some("GrCommand"),
    )]);
}
pub fn registry(cx: &App) -> Result<Commands, Error> {
    cx.try_global::<Configuration>()
        .map(|config| config.default.clone())
        .ok_or(Error::NotInitialized)
}
fn install(binding: &Keybinding, cx: &mut App) -> Result<(), Error> {
    if cx.global::<Configuration>().installed.contains(binding) {
        return Ok(());
    }
    let predicate = cx.global::<Configuration>().predicate.clone();
    let native = KeyBinding::load(
        binding.gpui_keystrokes(),
        Box::new(InvokeBinding {
            binding: binding.to_string(),
        }),
        Some(predicate),
        false,
        None,
        cx,
    )
    .map_err(|error| Error::NativeBinding(error.to_string()))?;
    cx.update_global::<Configuration, _>(|config, _| {
        config.installed.insert(binding.clone());
    });
    cx.bind_keys([native]);
    Ok(())
}
impl Commands {
    fn empty(cx: &mut App) -> Self {
        Self {
            state: cx.new(|_| RegistryState::default()),
        }
    }
    /// Prefer a distinct service per window when application contexts differ.
    pub fn new(cx: &mut App) -> Self {
        init(cx);
        Self::empty(cx)
    }
    pub fn entity(&self) -> &Entity<RegistryState> {
        &self.state
    }
    pub fn register<F>(&self, command: Command<F>, cx: &mut App) -> Result<(), Error>
    where
        F: Fn(&Payload, &mut Window, &mut App) + 'static,
    {
        command.validate()?;
        if self.state.read(cx).catalog.lookup(&command.id).is_some() {
            return Err(Error::Duplicate(command.id.to_string()));
        }
        if let Some(binding) = &command.keybinding {
            install(binding, cx)?;
        }
        let command = command.map_run(Handler::new);
        self.state.update(cx, |state, _| {
            state.catalog.register(command)?;
            state.changed();
            Ok::<(), Error>(())
        })?;
        cx.notify(self.state.entity_id());
        Ok(())
    }
    pub fn unregister(&self, id: &CommandId, cx: &mut App) -> bool {
        if self.state.read(cx).catalog.lookup(id).is_none() {
            return false;
        }
        self.state.update(cx, |state, _| {
            state.catalog.unregister(id);
            state.changed();
        });
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
        if let Some(binding) = &command.keybinding {
            install(binding, cx)?;
        }
        self.state.update(cx, |state, _| {
            state.catalog.replace(command)?;
            state.changed();
            Ok::<(), Error>(())
        })?;
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
        if let Some(binding) = &binding {
            install(binding, cx)?;
        }
        self.state.update(cx, |state, _| {
            state.catalog.set_keybinding(id, binding)?;
            state.changed();
            Ok::<(), Error>(())
        })?;
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
            let command = self
                .state
                .read(cx)
                .catalog
                .lookup(id)
                .ok_or_else(|| Error::NotFound(id.to_string()))?;
            if !command.enabled {
                return Err(Error::Disabled(id.to_string()));
            }
            if !command.when.matches(contexts) {
                return Err(Error::Inactive(id.to_string()));
            }
            command.run.clone()
        };
        (handler.0)(
            &Payload {
                id: id.clone(),
                source,
            },
            window,
            cx,
        );
        Ok(())
    }
    pub(crate) fn dispatch_binding(
        &self,
        binding: &str,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let Ok(binding) = Keybinding::parse(binding) else {
            return false;
        };
        let contexts = self.window_contexts(window, cx);
        let id = self
            .state
            .read(cx)
            .catalog
            .resolve_binding(&binding, &contexts)
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
    /// Snapshot projection for the existing overlays API; its id/label/keywords/
    /// disabled Command type is unchanged. Refresh via Spotlight::set_commands.
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
    pub fn open_spotlight(
        &self,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Entity<guiraffe_overlays::Spotlight>, guiraffe_overlays::OverlayError> {
        let origin = native_contexts(window);
        let mut contexts = self.state.read(cx).contexts.clone();
        contexts.extend(&origin);
        let commands = self.spotlight_commands(&contexts, cx);
        let weak = self.state.downgrade();
        let palette = cx.new(|cx| {
            guiraffe_overlays::Spotlight::new(commands, window, cx, move |value, window, cx| {
                let (Some(state), Ok(id)) = (weak.upgrade(), CommandId::parse(value)) else {
                    return;
                };
                let service = Commands { state };
                let mut contexts = service.state.read(cx).contexts.clone();
                contexts.extend(&origin);
                let _ = service.execute(&id, &contexts, InvocationSource::Spotlight, window, cx);
            })
        });
        guiraffe_overlays::Spotlight::open(&palette, window, cx)?;
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
