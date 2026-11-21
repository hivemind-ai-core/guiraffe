//! Structured document input for native disclosure bodies.

use gpui_kit::{IntoElement, SharedString};
use guiraffe_core::widget_value::Value;

use crate::DisclosureItem;

pub(crate) fn items(values: &Value) -> Vec<DisclosureItem> {
    let title = ["title".into()];
    let body = ["body".into()];
    let disabled = ["disabled".into()];
    (0..values.list_len().unwrap_or_default())
        .filter_map(|i| values.list_get(i))
        .filter_map(|value| {
            let title = value.field(&title).as_str()?;
            let body: SharedString = value
                .field(&body)
                .as_str()
                .unwrap_or_default()
                .to_owned()
                .into();
            let mut item =
                DisclosureItem::new(title.to_owned(), move || body.clone().into_any_element());
            item.disabled = value.field(&disabled).as_bool().unwrap_or_default();
            Some(item)
        })
        .collect()
}

pub(crate) fn indices(value: &Value) -> Vec<usize> {
    (0..value.list_len().unwrap_or_default())
        .filter_map(|i| index(value.list_get(i)?))
        .collect()
}

pub(crate) fn index(value: &Value) -> Option<usize> {
    value
        .as_f64()
        .filter(|value| (0.0..=f64::from(u32::MAX)).contains(value) && value.fract() == 0.0)
        .map(|value| value as usize)
}
