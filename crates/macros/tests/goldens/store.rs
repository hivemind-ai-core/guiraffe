#[derive(Clone, PartialEq, Debug)]
pub struct Settings {
    pub theme: Theme,
    pub font_size: f32,
    pub recent: Vec<PathBuf>,
    pub draft: Draft,
}
impl ::core::default::Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            font_size: 14.0,
            recent: vec![],
            draft: Draft::default(),
        }
    }
}
impl Settings {
    /// A store holding the default state.
    pub fn store(cx: &mut ::gpui_kit::App) -> ::guiraffe_state::Store<Settings> {
        ::guiraffe_state::Store::new(
            <Settings as ::core::default::Default>::default(),
            cx,
        )
    }
}
impl Settings {
    #[allow(non_upper_case_globals)]
    #[doc = concat!("Lens onto `", stringify!(theme), "`.")]
    pub const theme: ::guiraffe_state::Field<Settings, Theme> = ::guiraffe_state::Field::<
        Settings,
        Theme,
    >::new("theme", |s| &s.theme, |s| &mut s.theme);
    #[allow(non_upper_case_globals)]
    #[doc = concat!("Lens onto `", stringify!(font_size), "`.")]
    pub const font_size: ::guiraffe_state::Field<Settings, f32> = ::guiraffe_state::Field::<
        Settings,
        f32,
    >::new("font_size", |s| &s.font_size, |s| &mut s.font_size)
        .with_check(|v: f32| ::core::option::Option::Some(
            ::guiraffe_state::lens::clamp(v, 8.0..=48.0),
        ));
    #[allow(non_upper_case_globals)]
    #[doc = concat!("Lens onto `", stringify!(recent), "`.")]
    pub const recent: ::guiraffe_state::Field<Settings, Vec<PathBuf>> = ::guiraffe_state::Field::<
        Settings,
        Vec<PathBuf>,
    >::new("recent", |s| &s.recent, |s| &mut s.recent)
        .persist();
    #[allow(non_upper_case_globals)]
    #[doc = concat!("Lens onto `", stringify!(draft), "`.")]
    pub const draft: ::guiraffe_state::Field<Settings, Draft> = ::guiraffe_state::Field::<
        Settings,
        Draft,
    >::new("draft", |s| &s.draft, |s| &mut s.draft)
        .no_undo();
}
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct SettingsPath<L>(L);
impl<L: ::guiraffe_state::Lens<Target = Settings>> SettingsPath<L> {
    pub fn theme(
        self,
    ) -> ::guiraffe_state::Compose<L, ::guiraffe_state::Field<Settings, Theme>> {
        ::guiraffe_state::Lens::then(self.0, Settings::theme)
    }
    pub fn font_size(
        self,
    ) -> ::guiraffe_state::Compose<L, ::guiraffe_state::Field<Settings, f32>> {
        ::guiraffe_state::Lens::then(self.0, Settings::font_size)
    }
    pub fn recent(
        self,
    ) -> ::guiraffe_state::Compose<L, ::guiraffe_state::Field<Settings, Vec<PathBuf>>> {
        ::guiraffe_state::Lens::then(self.0, Settings::recent)
    }
    pub fn draft(
        self,
    ) -> ::guiraffe_state::Compose<L, ::guiraffe_state::Field<Settings, Draft>> {
        ::guiraffe_state::Lens::then(self.0, Settings::draft)
    }
    /// The lens built so far.
    pub fn lens(self) -> L {
        self.0
    }
}
impl ::guiraffe_state::Lensed for Settings {
    type Path<L: ::guiraffe_state::Lens<Target = Self>> = SettingsPath<L>;
    fn wrap<L: ::guiraffe_state::Lens<Target = Self>>(lens: L) -> SettingsPath<L> {
        SettingsPath(lens)
    }
}
#[doc = concat!(
    "The `Set` events of [`", stringify!(Settings), "`], one per field: `",
    stringify!(Settings), "::set().field(value)`."
)]
#[derive(Clone, Copy, Debug)]
pub struct SettingsSet;
impl SettingsSet {
    #[doc = concat!(
        "Set `", stringify!(theme),
        "`: validated, keyed to the field, tagged `Source::Host`."
    )]
    pub fn theme(
        self,
        value: Theme,
    ) -> ::guiraffe_state::Set<::guiraffe_state::Field<Settings, Theme>> {
        ::guiraffe_state::Set::host(Settings::theme, value)
    }
    #[doc = concat!(
        "Set `", stringify!(font_size),
        "`: validated, keyed to the field, tagged `Source::Host`."
    )]
    pub fn font_size(
        self,
        value: f32,
    ) -> ::guiraffe_state::Set<::guiraffe_state::Field<Settings, f32>> {
        ::guiraffe_state::Set::host(Settings::font_size, value)
    }
    #[doc = concat!(
        "Set `", stringify!(recent),
        "`: validated, keyed to the field, tagged `Source::Host`."
    )]
    pub fn recent(
        self,
        value: Vec<PathBuf>,
    ) -> ::guiraffe_state::Set<::guiraffe_state::Field<Settings, Vec<PathBuf>>> {
        ::guiraffe_state::Set::host(Settings::recent, value)
    }
    #[doc = concat!(
        "Set `", stringify!(draft),
        "`: validated, keyed to the field, tagged `Source::Host`."
    )]
    pub fn draft(
        self,
        value: Draft,
    ) -> ::guiraffe_state::Set<::guiraffe_state::Field<Settings, Draft>> {
        ::guiraffe_state::Set::host(Settings::draft, value)
    }
}
impl Settings {
    /// The generated per-field `Set` events: `store.dispatch(Self::set().field(value), cx)`.
    pub fn set() -> SettingsSet {
        SettingsSet
    }
}
#[derive(Debug)]
pub struct ResetFonts;
impl ::guiraffe_state::Event<Settings> for ResetFonts {
    #[allow(unused_variables)]
    fn handle(self, __guiraffe_fx: &mut ::guiraffe_state::Fx<'_, Settings>) {
        __guiraffe_fx
            .update(move |s: &mut Settings| {
                let _ = s.font_size = 14.0;
            });
    }
}
#[derive(Debug)]
pub struct Open(pub PathBuf);
impl ::guiraffe_state::Event<Settings> for Open {
    #[allow(unused_variables)]
    fn handle(self, __guiraffe_fx: &mut ::guiraffe_state::Fx<'_, Settings>) {
        let Open(path) = self;
        let mut __guiraffe_next = ::core::clone::Clone::clone(__guiraffe_fx.state());
        {
            let s: &mut Settings = &mut __guiraffe_next;
            let fx: &mut ::guiraffe_state::Fx<'_, Settings> = &mut *__guiraffe_fx;
            let _ = {
                s.recent.insert(0, path.clone());
                fx.effect(OpenFile(path))
            };
        }
        __guiraffe_fx.set(__guiraffe_next);
    }
}
