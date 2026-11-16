//! Optional desktop delivery. Call from a background executor; delivery errors are returned.
//! Linux notification bodies are markup: escape untrusted text as in Switchboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    Critical,
}
#[derive(Debug, Clone)]
pub struct DesktopNotification {
    pub app_name: String,
    pub title: String,
    pub body: String,
    pub icon: String,
    pub urgency: Urgency,
    pub timeout_ms: i32,
}
impl DesktopNotification {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            app_name: "GUIraffe".into(),
            title: title.into(),
            body: body.into(),
            icon: String::new(),
            urgency: Urgency::Normal,
            timeout_ms: 10_000,
        }
    }
    /// Deliver off the foreground thread; await the returned task for delivery errors.
    pub fn show_async(&self, cx: &gpui_kit::App) -> gpui_kit::Task<Result<(), String>> {
        let note = self.clone();
        deliver_async(cx, move || note.show())
    }
    /// Blocking platform delivery; callers should run it on their background executor.
    #[cfg(not(any(target_os = "ios", target_os = "android", target_family = "wasm")))]
    pub fn show(&self) -> Result<(), String> {
        #[cfg(all(unix, not(target_os = "macos")))]
        let body = escape_markup(&self.body);
        #[cfg(not(all(unix, not(target_os = "macos"))))]
        let body = self.body.clone();
        let mut notification = notify_rust::Notification::new();
        notification
            .appname(&self.app_name)
            .summary(&self.title)
            .body(&body)
            .icon(&self.icon)
            .timeout(self.timeout_ms);
        #[cfg(target_os = "linux")]
        notification.urgency(match self.urgency {
            Urgency::Low => notify_rust::Urgency::Low,
            Urgency::Normal => notify_rust::Urgency::Normal,
            Urgency::Critical => notify_rust::Urgency::Critical,
        });
        notification.show().map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(any(target_os = "ios", target_os = "android", target_family = "wasm"))]
    pub fn show(&self) -> Result<(), String> {
        Err(
            "Desktop notifications are unavailable on this platform; use OverlayHost::notify"
                .into(),
        )
    }
}
pub fn escape_markup(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn untrusted_body_is_plain_text() {
        assert_eq!(
            escape_markup("<b>A & B</b>"),
            "&lt;b&gt;A &amp; B&lt;/b&gt;"
        );
    }
    #[test]
    fn defaults() {
        let n = DesktopNotification::new("Hello", "Body");
        assert_eq!(n.urgency, Urgency::Normal);
        assert_eq!(n.timeout_ms, 10_000);
    }
}

fn deliver_async(
    cx: &gpui_kit::App,
    delivery: impl FnOnce() -> Result<(), String> + Send + 'static,
) -> gpui_kit::Task<Result<(), String>> {
    cx.background_executor().spawn(async move { delivery() })
}
#[cfg(test)]
mod review_tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    #[gpui_kit::test]
    async fn review_11_delivery_is_deferred_to_background(cx: &mut gpui_kit::TestAppContext) {
        let calls = Arc::new(AtomicUsize::new(0));
        let delivery = calls.clone();
        let task = cx.update(|cx| {
            deliver_async(cx, move || {
                delivery.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        });
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "delivery must not execute inline on the foreground"
        );
        assert_eq!(task.await, Ok(()));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
