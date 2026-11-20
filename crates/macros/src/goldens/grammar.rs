fn snapshot() {
    let _ = layouts::Stack::new()
        .gap(Size::Md)
        .child(Title::new("Settings").order(2))
        .child(
            div()
                .flex()
                .gap_2()
                .p(px(8.))
                .when(dense, |d| d.p(px(2.)))
                .test::<u8>(1)
                .child("raw div")
                .child(built),
        )
        .children(
            if let Some(error) = &error {
                ::core::option::Option::Some(Text::new(error.clone()))
            } else {
                ::core::option::Option::None
            },
        )
        .child(
            if dense {
                Text::new("Dense")
            } else {
                if roomy { Text::new("Roomy") } else { Text::new("Normal") }
            },
        )
        .child(
            match tab {
                Tab::A => ::gpui_kit::IntoElement::into_any_element(Text::new("A")),
                Tab::B => ::gpui_kit::IntoElement::into_any_element(div().child("B")),
            },
        )
        .children(
            (&rows)
                .into_iter()
                .map(|row| {
                    let __guiraffe_key = row.id;
                    ::guiraffe_ui::__macro::keyed(__guiraffe_key, RowView::new(row))
                }),
        )
        .children((lines).into_iter().map(|line| line))
        .child(chart.clone())
        .child(
            ::guiraffe_ui::__macro::cached(style, move || HeavyTable::new(data.clone())),
        );
}
