//! Command ribbon at application scale (design `command-ribbon.md` §5): 2,000 commands in 200
//! categories. Builds once in setup; the measured steps are opening (root projection),
//! navigating two chords deep and filtering (a context change hides half of a menu).
use guiraffe_commands::{
    Command, CommandId, ContextStack, Registry, When, WhenContext,
    ribbon::{Navigator, RibbonCategory, RibbonEntry, RibbonLayout, RibbonMenu, RibbonModel},
};
use std::collections::BTreeMap;

pub const COMMANDS: usize = 2_000;
pub const CATEGORIES: usize = 200;
const PER_CATEGORY: usize = COMMANDS / CATEGORIES;
/// Group menus at the root, each opening 50 category menus (keys unique per menu).
const GROUPS: usize = 4;
const KEYS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

pub struct RibbonFixture {
    registry: Registry,
    model: RibbonModel,
    all: ContextStack,
    filtered: ContextStack,
}

fn key(index: usize) -> String {
    KEYS.chars()
        .nth(index % KEYS.len())
        .map(String::from)
        .unwrap_or_default()
}

impl RibbonFixture {
    pub fn new() -> anyhow::Result<Self> {
        let mut registry = Registry::default();
        let mut layout = RibbonLayout::new("root");
        let mut root = RibbonMenu::new("root", "Commands");
        let per_group = CATEGORIES / GROUPS;
        let odd = When::parse("Odd")?;
        for group in 0..GROUPS {
            let group_id = format!("group{group}");
            root = root.category(RibbonCategory::new(format!("Group {group}")).entry(
                RibbonEntry::menu(format!("g{group}"), key(group), group_id.clone()),
            ));
            let mut group_menu = RibbonMenu::new(group_id, format!("Group {group}"));
            // Half of each group's categories show only in the "Odd" context: the filter step.
            let (mut always, mut sometimes) = (
                RibbonCategory::new("Always"),
                RibbonCategory::new("Odd").visible(odd.clone()),
            );
            for slot in 0..per_group {
                let c = group * per_group + slot;
                let menu_id = format!("c{c}");
                let entry = RibbonEntry::menu(format!("open{c}"), key(slot), menu_id.clone());
                if slot % 2 == 0 {
                    always = always.entry(entry);
                } else {
                    sometimes = sometimes.entry(entry);
                }
                let mut category = RibbonCategory::new(format!("Category {c}"));
                for e in 0..PER_CATEGORY {
                    let id = CommandId::parse(&format!("c{c}.e{e}"))?;
                    registry.register(Command::new(id.clone(), format!("Command {c}.{e}"), ()))?;
                    category =
                        category.entry(RibbonEntry::command(format!("e{c}-{e}"), key(e), id));
                }
                layout = layout
                    .menu(RibbonMenu::new(menu_id, format!("Category {c}")).category(category));
            }
            group_menu = group_menu.category(always).category(sometimes);
            layout = layout.menu(group_menu);
        }
        layout = layout.menu(root);
        let model = RibbonModel::build(&layout, &registry, &BTreeMap::new())?;
        let mut all = ContextStack::default();
        all.push(WhenContext::parse("Workspace Odd")?);
        let mut filtered = ContextStack::default();
        filtered.push(WhenContext::parse("Workspace")?);
        Ok(Self {
            registry,
            model,
            all,
            filtered,
        })
    }

    /// Opening: project the root menu. Returns the number of rows.
    pub fn open(&self) -> usize {
        let nav = Navigator::default();
        rows(&self.model.sections(&nav, &self.registry, &self.all))
    }

    /// Two chords (group, category) and the projection of the category menu.
    pub fn navigate(&self) -> anyhow::Result<usize> {
        let mut nav = Navigator::default();
        nav.press("c", &self.model, &self.registry, &self.all)?;
        nav.press("b", &self.model, &self.registry, &self.all)?;
        Ok(rows(&self.model.sections(&nav, &self.registry, &self.all)))
    }

    /// A context change while a group menu is open: reconcile and reproject, half hidden.
    pub fn filter(&self) -> anyhow::Result<usize> {
        let mut nav = Navigator::default();
        nav.press("c", &self.model, &self.registry, &self.all)?;
        nav.reconcile(&self.model, &self.filtered);
        Ok(rows(&self.model.sections(
            &nav,
            &self.registry,
            &self.filtered,
        )))
    }
}

fn rows(sections: &[guiraffe_commands::ribbon::Section]) -> usize {
    sections.iter().map(|s| s.items.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_has_application_scale_and_each_step_does_what_it_says() -> anyhow::Result<()> {
        let fixture = RibbonFixture::new()?;
        assert_eq!(fixture.registry.len(), COMMANDS);
        assert_eq!(fixture.open(), GROUPS);
        assert_eq!(fixture.navigate()?, PER_CATEGORY);
        assert_eq!(fixture.filter()?, CATEGORIES / GROUPS / 2);
        Ok(())
    }
}
